// utxo_common_swap — HTLC/swap operations, payment scripts, validation

use super::*;

pub const DEFAULT_SWAP_TX_SPEND_SIZE: u64 = 305;

pub const DEFAULT_SWAP_VOUT: usize = 0;

/// returns the fee required to be paid for HTLC spend transaction
pub async fn get_htlc_spend_fee<T: UtxoCommonOps>(coin: &T, tx_size: u64) -> UtxoRpcResult<u64> {
    let coin_fee = coin.get_tx_fee().await?;
    let mut fee = match coin_fee {
        // atomic swap payment spend transaction is slightly more than 300 bytes in average as of now
        ActualTxFee::Dynamic(fee_per_kb) => (fee_per_kb * tx_size) / KILO_BYTE,
        // return satoshis here as swap spend transaction size is always less than 1 kb
        ActualTxFee::FixedPerKb(satoshis) => {
            let tx_size_kb = if tx_size % KILO_BYTE == 0 {
                tx_size / KILO_BYTE
            } else {
                tx_size / KILO_BYTE + 1
            };
            satoshis * tx_size_kb
        },
    };
    if coin.as_ref().conf.force_min_relay_fee {
        let relay_fee = coin.as_ref().rpc_client.get_relay_fee().compat().await?;
        let relay_fee_sat = sat_from_big_decimal(&relay_fee, coin.as_ref().decimals).mm_err(Into::into)?;
        if fee < relay_fee_sat {
            fee = relay_fee_sat;
        }
    }
    Ok(fee)
}

/// Constructs and broadcasts the taker DEX fee transaction.
///
/// For `DexFee::Standard`: a single P2PKH output to the fee-collection address.
/// For `DexFee::WithBurn`: two outputs — fee to the DEX address, burn via
/// OP_RETURN (KMD) or P2PKH to a burn address (other coins).
pub fn send_taker_fee<T>(coin: T, dex_fee: &DexFee, fee_pub_key: &[u8]) -> TransactionFut
where
    T: UtxoCommonOps + GetUtxoListOps,
{
    let fee_address = try_tx_fus!(address_from_raw_pubkey(
        fee_pub_key,
        coin.as_ref().conf.pub_addr_prefix,
        coin.as_ref().conf.pub_t_addr_prefix,
        coin.as_ref().conf.checksum_type,
        coin.as_ref().conf.bech32_hrp.clone(),
        coin.addr_format().clone(),
    ));

    let outputs = try_tx_fus!(generate_taker_fee_tx_outputs(&coin, dex_fee, &fee_address));
    send_outputs_from_my_address(coin, outputs)
}

/// Builds the transaction outputs for a taker fee payment.
///
/// Returns 0 outputs for `NoFee`, 1 for `Standard`, or 2 for `WithBurn`.
fn generate_taker_fee_tx_outputs(
    coin: &impl UtxoCommonOps,
    dex_fee: &DexFee,
    fee_address: &Address,
) -> Result<Vec<TransactionOutput>, String> {
    match dex_fee {
        DexFee::NoFee => Ok(vec![]),
        DexFee::Standard(amount) => {
            let sat = sat_from_big_decimal(&amount.to_decimal(), coin.as_ref().decimals).map_err(|e| e.to_string())?;
            Ok(vec![TransactionOutput {
                value: sat,
                script_pubkey: Builder::build_p2pkh(&fee_address.hash).to_bytes(),
            }])
        },
        DexFee::WithBurn {
            fee_amount,
            burn_amount,
            burn_destination,
        } => {
            let fee_sat =
                sat_from_big_decimal(&fee_amount.to_decimal(), coin.as_ref().decimals).map_err(|e| e.to_string())?;
            let burn_sat =
                sat_from_big_decimal(&burn_amount.to_decimal(), coin.as_ref().decimals).map_err(|e| e.to_string())?;

            // Output 0: fee portion → DEX address (P2PKH)
            let fee_output = TransactionOutput {
                value: fee_sat,
                script_pubkey: Builder::build_p2pkh(&fee_address.hash).to_bytes(),
            };

            // Output 1: burn portion → OP_RETURN (KMD) or burn address (others)
            let burn_output = match burn_destination {
                DexFeeBurnDestination::KmdOpReturn => TransactionOutput {
                    value: burn_sat,
                    script_pubkey: Builder::default().push_opcode(Opcode::OP_RETURN).into_bytes(),
                },
                DexFeeBurnDestination::PreBurnAccount { burn_pubkey } => {
                    let burn_address = address_from_raw_pubkey(
                        burn_pubkey,
                        coin.as_ref().conf.pub_addr_prefix,
                        coin.as_ref().conf.pub_t_addr_prefix,
                        coin.as_ref().conf.checksum_type,
                        coin.as_ref().conf.bech32_hrp.clone(),
                        coin.addr_format().clone(),
                    )
                    .map_err(|e| format!("Failed to derive burn address: {}", e))?;
                    TransactionOutput {
                        value: burn_sat,
                        script_pubkey: Builder::build_p2pkh(&burn_address.hash).to_bytes(),
                    }
                },
            };

            Ok(vec![fee_output, burn_output])
        },
    }
}

pub fn send_maker_payment<T>(
    coin: T,
    time_lock: u32,
    maker_pub: &[u8],
    taker_pub: &[u8],
    secret_hash: &[u8],
    amount: BigDecimal,
) -> TransactionFut
where
    T: UtxoCommonOps + GetUtxoListOps,
{
    let SwapPaymentOutputsResult {
        payment_address,
        outputs,
    } = try_tx_fus!(generate_swap_payment_outputs(
        &coin,
        time_lock,
        maker_pub,
        taker_pub,
        secret_hash,
        amount
    ));
    let send_fut = match &coin.as_ref().rpc_client {
        UtxoRpcClientEnum::Electrum(_) => Either::A(send_outputs_from_my_address(coin, outputs)),
        UtxoRpcClientEnum::Native(client) => {
            let addr_string = try_tx_fus!(payment_address.display_address());
            Either::B(
                client
                    .import_address(&addr_string, &addr_string, false)
                    .map_err(|e| TransactionErr::Plain(ERRL!("{}", e)))
                    .and_then(move |_| send_outputs_from_my_address(coin, outputs)),
            )
        },
    };
    Box::new(send_fut)
}

pub fn send_taker_payment<T>(
    coin: T,
    time_lock: u32,
    taker_pub: &[u8],
    maker_pub: &[u8],
    secret_hash: &[u8],
    amount: BigDecimal,
) -> TransactionFut
where
    T: UtxoCommonOps + GetUtxoListOps,
{
    let SwapPaymentOutputsResult {
        payment_address,
        outputs,
    } = try_tx_fus!(generate_swap_payment_outputs(
        &coin,
        time_lock,
        taker_pub,
        maker_pub,
        secret_hash,
        amount
    ));

    let send_fut = match &coin.as_ref().rpc_client {
        UtxoRpcClientEnum::Electrum(_) => Either::A(send_outputs_from_my_address(coin, outputs)),
        UtxoRpcClientEnum::Native(client) => {
            let addr_string = try_tx_fus!(payment_address.display_address());
            Either::B(
                client
                    .import_address(&addr_string, &addr_string, false)
                    .map_err(|e| TransactionErr::Plain(ERRL!("{}", e)))
                    .and_then(move |_| send_outputs_from_my_address(coin, outputs)),
            )
        },
    };
    Box::new(send_fut)
}

pub fn send_maker_spends_taker_payment<T: UtxoCommonOps>(
    coin: T,
    taker_payment_tx: &[u8],
    time_lock: u32,
    taker_pub: &[u8],
    secret: &[u8],
    htlc_privkey: &[u8],
) -> TransactionFut {
    let key_pair = try_tx_fus!(key_pair_from_secret(htlc_privkey));
    let my_address = try_tx_fus!(coin.as_ref().derivation_method.iguana_or_err()).clone();

    let mut prev_tx: UtxoTx = try_tx_fus!(deserialize(taker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
    prev_tx.tx_hash_algo = coin.as_ref().tx_hash_algo;
    let script_data = Builder::default()
        .push_data(secret)
        .push_opcode(Opcode::OP_0)
        .into_script();
    let redeem_script = payment_script(
        time_lock,
        &*dhash160(secret),
        &try_tx_fus!(Public::from_slice(taker_pub)),
        key_pair.public(),
    );
    let fut = async move {
        let fee = try_tx_s!(coin.get_htlc_spend_fee(DEFAULT_SWAP_TX_SPEND_SIZE).await);
        let script_pubkey = output_script(&my_address, ScriptType::P2PKH).to_bytes();
        let output = TransactionOutput {
            value: prev_tx.outputs[0].value - fee,
            script_pubkey,
        };

        let transaction = try_tx_s!(
            coin.p2sh_spending_tx(
                prev_tx,
                redeem_script.into(),
                vec![output],
                script_data,
                SEQUENCE_FINAL,
                time_lock,
                &key_pair,
            )
            .await
        );

        let tx_fut = coin.as_ref().rpc_client.send_transaction(&transaction).compat();
        try_tx_s!(tx_fut.await, transaction);

        Ok(transaction.into())
    };
    Box::new(fut.boxed().compat())
}

pub fn send_taker_spends_maker_payment<T: UtxoCommonOps>(
    coin: T,
    maker_payment_tx: &[u8],
    time_lock: u32,
    maker_pub: &[u8],
    secret: &[u8],
    htlc_privkey: &[u8],
) -> TransactionFut {
    let key_pair = try_tx_fus!(key_pair_from_secret(htlc_privkey));
    let my_address = try_tx_fus!(coin.as_ref().derivation_method.iguana_or_err()).clone();

    let mut prev_tx: UtxoTx = try_tx_fus!(deserialize(maker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
    prev_tx.tx_hash_algo = coin.as_ref().tx_hash_algo;
    let script_data = Builder::default()
        .push_data(secret)
        .push_opcode(Opcode::OP_0)
        .into_script();
    let redeem_script = payment_script(
        time_lock,
        &*dhash160(secret),
        &try_tx_fus!(Public::from_slice(maker_pub)),
        key_pair.public(),
    );
    let fut = async move {
        let fee = try_tx_s!(coin.get_htlc_spend_fee(DEFAULT_SWAP_TX_SPEND_SIZE).await);
        let script_pubkey = output_script(&my_address, ScriptType::P2PKH).to_bytes();
        let output = TransactionOutput {
            value: prev_tx.outputs[0].value - fee,
            script_pubkey,
        };

        let transaction = try_tx_s!(
            coin.p2sh_spending_tx(
                prev_tx,
                redeem_script.into(),
                vec![output],
                script_data,
                SEQUENCE_FINAL,
                time_lock,
                &key_pair,
            )
            .await
        );

        let tx_fut = coin.as_ref().rpc_client.send_transaction(&transaction).compat();
        try_tx_s!(tx_fut.await, transaction);

        Ok(transaction.into())
    };
    Box::new(fut.boxed().compat())
}

pub fn send_taker_refunds_payment<T: UtxoCommonOps>(
    coin: T,
    taker_payment_tx: &[u8],
    time_lock: u32,
    maker_pub: &[u8],
    secret_hash: &[u8],
    htlc_privkey: &[u8],
) -> TransactionFut {
    let key_pair = try_tx_fus!(key_pair_from_secret(htlc_privkey));
    let my_address = try_tx_fus!(coin.as_ref().derivation_method.iguana_or_err()).clone();

    let mut prev_tx: UtxoTx =
        try_tx_fus!(deserialize(taker_payment_tx).map_err(|e| TransactionErr::Plain(format!("{:?}", e))));
    prev_tx.tx_hash_algo = coin.as_ref().tx_hash_algo;
    let script_data = Builder::default().push_opcode(Opcode::OP_1).into_script();
    let redeem_script = payment_script(
        time_lock,
        secret_hash,
        key_pair.public(),
        &try_tx_fus!(Public::from_slice(maker_pub)),
    );
    let fut = async move {
        let fee = try_tx_s!(coin.get_htlc_spend_fee(DEFAULT_SWAP_TX_SPEND_SIZE).await);
        let script_pubkey = output_script(&my_address, ScriptType::P2PKH).to_bytes();
        let output = TransactionOutput {
            value: prev_tx.outputs[0].value - fee,
            script_pubkey,
        };

        let transaction = try_tx_s!(
            coin.p2sh_spending_tx(
                prev_tx,
                redeem_script.into(),
                vec![output],
                script_data,
                SEQUENCE_FINAL - 1,
                time_lock,
                &key_pair,
            )
            .await
        );

        let tx_fut = coin.as_ref().rpc_client.send_transaction(&transaction).compat();
        try_tx_s!(tx_fut.await, transaction);

        Ok(transaction.into())
    };
    Box::new(fut.boxed().compat())
}

pub fn send_maker_refunds_payment<T: UtxoCommonOps>(
    coin: T,
    maker_payment_tx: &[u8],
    time_lock: u32,
    taker_pub: &[u8],
    secret_hash: &[u8],
    htlc_privkey: &[u8],
) -> TransactionFut {
    let key_pair = try_tx_fus!(key_pair_from_secret(htlc_privkey));
    let my_address = try_tx_fus!(coin.as_ref().derivation_method.iguana_or_err()).clone();

    let mut prev_tx: UtxoTx = try_tx_fus!(deserialize(maker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
    prev_tx.tx_hash_algo = coin.as_ref().tx_hash_algo;
    let script_data = Builder::default().push_opcode(Opcode::OP_1).into_script();
    let redeem_script = payment_script(
        time_lock,
        secret_hash,
        key_pair.public(),
        &try_tx_fus!(Public::from_slice(taker_pub)),
    );
    let fut = async move {
        let fee = try_tx_s!(coin.get_htlc_spend_fee(DEFAULT_SWAP_TX_SPEND_SIZE).await);
        let script_pubkey = output_script(&my_address, ScriptType::P2PKH).to_bytes();
        let output = TransactionOutput {
            value: prev_tx.outputs[0].value - fee,
            script_pubkey,
        };

        let transaction = try_tx_s!(
            coin.p2sh_spending_tx(
                prev_tx,
                redeem_script.into(),
                vec![output],
                script_data,
                SEQUENCE_FINAL - 1,
                time_lock,
                &key_pair,
            )
            .await
        );

        let tx_fut = coin.as_ref().rpc_client.send_transaction(&transaction).compat();
        try_tx_s!(tx_fut.await, transaction);

        Ok(transaction.into())
    };
    Box::new(fut.boxed().compat())
}

/// Extracts pubkey from script sig
fn pubkey_from_script_sig(script: &Script) -> Result<H264, String> {
    match script.get_instruction(0) {
        Some(Ok(instruction)) => match instruction.opcode {
            Opcode::OP_PUSHBYTES_70 | Opcode::OP_PUSHBYTES_71 | Opcode::OP_PUSHBYTES_72 => match instruction.data {
                Some(bytes) => try_s!(Signature::from_der(&bytes[..bytes.len() - 1])),
                None => return ERR!("No data at instruction 0 of script {:?}", script),
            },
            _ => return ERR!("Unexpected opcode {:?}", instruction.opcode),
        },
        Some(Err(e)) => return ERR!("Error {} on getting instruction 0 of script {:?}", e, script),
        None => return ERR!("None instruction 0 of script {:?}", script),
    };

    let pubkey = match script.get_instruction(1) {
        Some(Ok(instruction)) => match instruction.opcode {
            Opcode::OP_PUSHBYTES_33 => match instruction.data {
                Some(bytes) => try_s!(PublicKey::from_slice(bytes)),
                None => return ERR!("No data at instruction 1 of script {:?}", script),
            },
            _ => return ERR!("Unexpected opcode {:?}", instruction.opcode),
        },
        Some(Err(e)) => return ERR!("Error {} on getting instruction 1 of script {:?}", e, script),
        None => return ERR!("None instruction 1 of script {:?}", script),
    };

    if script.get_instruction(2).is_some() {
        return ERR!("Unexpected instruction at position 2 of script {:?}", script);
    }
    Ok(pubkey.serialize().into())
}

/// Extracts pubkey from witness script
fn pubkey_from_witness_script(witness_script: &[Bytes]) -> Result<H264, String> {
    if witness_script.len() != 2 {
        return ERR!("Invalid witness length {}", witness_script.len());
    }

    let signature = witness_script[0].clone().take();
    if signature.is_empty() {
        return ERR!("Empty signature data in witness script");
    }
    try_s!(Signature::from_der(&signature[..signature.len() - 1]));

    let pubkey = try_s!(PublicKey::from_slice(&witness_script[1]));

    Ok(pubkey.serialize().into())
}

pub async fn is_tx_confirmed_before_block<T>(coin: &T, tx: &RpcTransaction, block_number: u64) -> Result<bool, String>
where
    T: UtxoCommonOps,
{
    match tx.height {
        Some(confirmed_at) => Ok(confirmed_at <= block_number),
        // fallback to a number of confirmations
        None => {
            if tx.confirmations > 0 {
                let current_block = try_s!(coin.as_ref().rpc_client.get_block_count().compat().await);
                let confirmed_at = current_block + 1 - tx.confirmations as u64;
                Ok(confirmed_at <= block_number)
            } else {
                Ok(false)
            }
        },
    }
}

pub fn check_all_inputs_signed_by_pub(tx: &UtxoTx, expected_pub: &[u8]) -> Result<bool, String> {
    for input in &tx.inputs {
        let pubkey = if input.has_witness() {
            try_s!(pubkey_from_witness_script(&input.script_witness))
        } else {
            let script: Script = input.script_sig.clone().into();
            try_s!(pubkey_from_script_sig(&script))
        };
        if *pubkey != expected_pub {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Validates a taker DEX fee transaction against expected parameters.
///
/// For `DexFee::Standard`: checks output 0 pays the fee address the expected amount.
/// For `DexFee::WithBurn`: additionally validates the burn output (OP_RETURN or P2PKH).
pub fn validate_fee<T: UtxoCommonOps>(
    coin: T,
    tx: UtxoTx,
    output_index: usize,
    sender_pubkey: &[u8],
    dex_fee: &DexFee,
    min_block_number: u64,
    fee_addr: &[u8],
) -> Box<dyn Future<Item = (), Error = String> + Send> {
    let dex_fee = dex_fee.clone();
    let address = try_fus!(address_from_raw_pubkey(
        fee_addr,
        coin.as_ref().conf.pub_addr_prefix,
        coin.as_ref().conf.pub_t_addr_prefix,
        coin.as_ref().conf.checksum_type,
        coin.as_ref().conf.bech32_hrp.clone(),
        coin.addr_format().clone(),
    ));

    if !try_fus!(check_all_inputs_signed_by_pub(&tx, sender_pubkey)) {
        return Box::new(futures01::future::err(ERRL!("The dex fee was sent from wrong address")));
    }
    let fut = async move {
        let tx_from_rpc = try_s!(
            coin.as_ref()
                .rpc_client
                .get_verbose_transaction(&tx.hash().reversed().into())
                .compat()
                .await
        );

        if try_s!(is_tx_confirmed_before_block(&coin, &tx_from_rpc, min_block_number).await) {
            return ERR!(
                "Fee tx {:?} confirmed before min_block {}",
                tx_from_rpc,
                min_block_number,
            );
        }
        if tx_from_rpc.hex.0 != serialize(&tx).take()
            && tx_from_rpc.hex.0 != serialize_with_flags(&tx, SERIALIZE_TRANSACTION_WITNESS).take()
        {
            return ERR!(
                "Provided dex fee tx {:?} doesn't match tx data from rpc {:?}",
                tx,
                tx_from_rpc
            );
        }

        // Validate fee output(s) based on DexFee variant
        match &dex_fee {
            DexFee::NoFee => {},
            DexFee::Standard(amount) => {
                let expected_sat = try_s!(sat_from_big_decimal(&amount.to_decimal(), coin.as_ref().decimals));
                try_s!(validate_dex_output(&tx, output_index, &address, expected_sat));
            },
            DexFee::WithBurn {
                fee_amount,
                burn_amount,
                burn_destination,
            } => {
                // Validate fee output (output_index)
                let fee_sat = try_s!(sat_from_big_decimal(&fee_amount.to_decimal(), coin.as_ref().decimals));
                try_s!(validate_dex_output(&tx, output_index, &address, fee_sat));

                // Validate burn output (output_index + 1)
                let burn_sat = try_s!(sat_from_big_decimal(&burn_amount.to_decimal(), coin.as_ref().decimals));
                let expected_burn_script = match burn_destination {
                    DexFeeBurnDestination::KmdOpReturn => {
                        Builder::default().push_opcode(Opcode::OP_RETURN).into_bytes()
                    },
                    DexFeeBurnDestination::PreBurnAccount { burn_pubkey } => {
                        let burn_address = try_s!(address_from_raw_pubkey(
                            burn_pubkey,
                            coin.as_ref().conf.pub_addr_prefix,
                            coin.as_ref().conf.pub_t_addr_prefix,
                            coin.as_ref().conf.checksum_type,
                            coin.as_ref().conf.bech32_hrp.clone(),
                            coin.addr_format().clone(),
                        ));
                        Builder::build_p2pkh(&burn_address.hash).to_bytes()
                    },
                };
                try_s!(validate_burn_output(
                    &tx,
                    output_index + 1,
                    &expected_burn_script,
                    burn_sat
                ));
            },
        }
        Ok(())
    };
    Box::new(fut.boxed().compat())
}

/// Validates that a specific output pays the expected address the expected amount.
fn validate_dex_output(tx: &UtxoTx, index: usize, expected_addr: &Address, expected_sat: u64) -> Result<(), String> {
    match tx.outputs.get(index) {
        Some(out) => {
            let expected_script = Builder::build_p2pkh(&expected_addr.hash).to_bytes();
            if out.script_pubkey != expected_script {
                return ERR!(
                    "Dex fee tx output {} script_pubkey mismatch: got {:?}, expected {:?}",
                    index,
                    out.script_pubkey,
                    expected_script
                );
            }
            if out.value < expected_sat {
                return ERR!(
                    "Dex fee tx output {} value {} is less than expected {}",
                    index,
                    out.value,
                    expected_sat
                );
            }
            Ok(())
        },
        None => ERR!("Dex fee tx does not have output index {}", index),
    }
}

/// Validates that a specific output matches the expected burn script and amount.
fn validate_burn_output(tx: &UtxoTx, index: usize, expected_script: &[u8], expected_sat: u64) -> Result<(), String> {
    match tx.outputs.get(index) {
        Some(out) => {
            if out.script_pubkey.as_ref() != expected_script {
                return ERR!(
                    "Burn output {} script mismatch: got {:?}, expected {:?}",
                    index,
                    out.script_pubkey,
                    expected_script
                );
            }
            if out.value < expected_sat {
                return ERR!(
                    "Burn output {} value {} is less than expected {}",
                    index,
                    out.value,
                    expected_sat
                );
            }
            Ok(())
        },
        None => ERR!("Dex fee tx does not have burn output at index {}", index),
    }
}

pub fn validate_maker_payment<T: UtxoCommonOps>(
    coin: &T,
    input: ValidatePaymentInput,
) -> Box<dyn Future<Item = (), Error = String> + Send> {
    let my_public = try_fus!(Public::from_slice(&input.taker_pub));
    let mut tx: UtxoTx = try_fus!(deserialize(input.payment_tx.as_slice()).map_err(|e| ERRL!("{:?}", e)));
    tx.tx_hash_algo = coin.as_ref().tx_hash_algo;

    validate_payment(
        coin.clone(),
        tx,
        DEFAULT_SWAP_VOUT,
        &try_fus!(Public::from_slice(&input.maker_pub)),
        &my_public,
        &input.secret_hash,
        input.amount,
        input.time_lock,
        input.try_spv_proof_until,
        input.confirmations,
    )
}

pub fn validate_taker_payment<T: UtxoCommonOps>(
    coin: &T,
    input: ValidatePaymentInput,
) -> Box<dyn Future<Item = (), Error = String> + Send> {
    let my_public = try_fus!(Public::from_slice(&input.maker_pub));
    let mut tx: UtxoTx = try_fus!(deserialize(input.payment_tx.as_slice()).map_err(|e| ERRL!("{:?}", e)));
    tx.tx_hash_algo = coin.as_ref().tx_hash_algo;

    validate_payment(
        coin.clone(),
        tx,
        DEFAULT_SWAP_VOUT,
        &try_fus!(Public::from_slice(&input.taker_pub)),
        &my_public,
        &input.secret_hash,
        input.amount,
        input.time_lock,
        input.try_spv_proof_until,
        input.confirmations,
    )
}

pub fn check_if_my_payment_sent<T: UtxoCommonOps>(
    coin: T,
    time_lock: u32,
    my_pub: &[u8],
    other_pub: &[u8],
    secret_hash: &[u8],
) -> Box<dyn Future<Item = Option<TransactionEnum>, Error = String> + Send> {
    let my_public = try_fus!(Public::from_slice(my_pub));
    let script = payment_script(
        time_lock,
        secret_hash,
        &my_public,
        &try_fus!(Public::from_slice(other_pub)),
    );
    let hash = dhash160(&script);
    let p2sh = Builder::build_p2sh(&hash.into());
    let script_hash = electrum_script_hash(&p2sh);
    let fut = async move {
        match &coin.as_ref().rpc_client {
            UtxoRpcClientEnum::Electrum(client) => {
                let history = try_s!(client.scripthash_get_history(&hex::encode(script_hash)).compat().await);
                match history.first() {
                    Some(item) => {
                        let tx_bytes = try_s!(client.get_transaction_bytes(&item.tx_hash).compat().await);
                        let mut tx: UtxoTx = try_s!(deserialize(tx_bytes.0.as_slice()).map_err(|e| ERRL!("{:?}", e)));
                        tx.tx_hash_algo = coin.as_ref().tx_hash_algo;
                        Ok(Some(tx.into()))
                    },
                    None => Ok(None),
                }
            },
            UtxoRpcClientEnum::Native(client) => {
                let target_addr = Address {
                    t_addr_prefix: coin.as_ref().conf.p2sh_t_addr_prefix,
                    prefix: coin.as_ref().conf.p2sh_addr_prefix,
                    hash: hash.into(),
                    checksum_type: coin.as_ref().conf.checksum_type,
                    hrp: coin.as_ref().conf.bech32_hrp.clone(),
                    addr_format: coin.addr_format().clone(),
                };
                let target_addr = target_addr.to_string();
                let is_imported = try_s!(client.is_address_imported(&target_addr).await);
                if !is_imported {
                    return Ok(None);
                }
                let received_by_addr = try_s!(client.list_received_by_address(0, true, true).compat().await);
                for item in received_by_addr {
                    if item.address == target_addr && !item.txids.is_empty() {
                        let tx_bytes = try_s!(client.get_transaction_bytes(&item.txids[0]).compat().await);
                        let mut tx: UtxoTx = try_s!(deserialize(tx_bytes.0.as_slice()).map_err(|e| ERRL!("{:?}", e)));
                        tx.tx_hash_algo = coin.as_ref().tx_hash_algo;
                        return Ok(Some(tx.into()));
                    }
                }
                Ok(None)
            },
        }
    };
    Box::new(fut.boxed().compat())
}

pub async fn search_for_swap_tx_spend_my(
    coin: &UtxoCoinFields,
    time_lock: u32,
    other_pub: &[u8],
    secret_hash: &[u8],
    tx: &[u8],
    output_index: usize,
    search_from_block: u64,
) -> Result<Option<FoundSwapTxSpend>, String> {
    let my_public = try_s!(coin.priv_key_policy.key_pair_or_err()).public();
    search_for_swap_output_spend(
        coin,
        time_lock,
        my_public,
        &try_s!(Public::from_slice(other_pub)),
        secret_hash,
        tx,
        output_index,
        search_from_block,
    )
    .await
}

pub async fn search_for_swap_tx_spend_other(
    coin: &UtxoCoinFields,
    time_lock: u32,
    other_pub: &[u8],
    secret_hash: &[u8],
    tx: &[u8],
    output_index: usize,
    search_from_block: u64,
) -> Result<Option<FoundSwapTxSpend>, String> {
    let my_public = try_s!(coin.priv_key_policy.key_pair_or_err()).public();
    search_for_swap_output_spend(
        coin,
        time_lock,
        &try_s!(Public::from_slice(other_pub)),
        my_public,
        secret_hash,
        tx,
        output_index,
        search_from_block,
    )
    .await
}

/// Extract a secret from the `spend_tx`.
/// Note spender could generate the spend with several inputs where the only one input is the p2sh script.
pub fn extract_secret(secret_hash: &[u8], spend_tx: &[u8]) -> Result<Vec<u8>, String> {
    let spend_tx: UtxoTx = try_s!(deserialize(spend_tx).map_err(|e| ERRL!("{:?}", e)));
    for (input_idx, input) in spend_tx.inputs.into_iter().enumerate() {
        let script: Script = input.script_sig.clone().into();
        let instruction = match script.get_instruction(1) {
            Some(Ok(instr)) => instr,
            Some(Err(e)) => {
                log!("Warning: "[e]);
                continue;
            },
            None => {
                log!("Warning: couldn't find secret in "[input_idx]" input");
                continue;
            },
        };

        if instruction.opcode != Opcode::OP_PUSHBYTES_32 {
            log!("Warning: expected "[Opcode::OP_PUSHBYTES_32]" opcode, found "[instruction.opcode] " in "[input_idx]" input");
            continue;
        }

        let secret = match instruction.data {
            Some(data) => data.to_vec(),
            None => {
                log!("Warning: secret is empty in "[input_idx] " input");
                continue;
            },
        };

        let actual_secret_hash = &*dhash160(&secret);
        if actual_secret_hash != secret_hash {
            log!("Warning: invalid 'dhash160(secret)' "[actual_secret_hash]", expected "[secret_hash]);
            continue;
        }
        return Ok(secret);
    }
    ERR!("Couldn't extract secret")
}

#[allow(clippy::too_many_arguments)]
pub fn validate_payment<T: UtxoCommonOps>(
    coin: T,
    tx: UtxoTx,
    output_index: usize,
    first_pub0: &Public,
    second_pub0: &Public,
    priv_bn_hash: &[u8],
    amount: BigDecimal,
    time_lock: u32,
    try_spv_proof_until: u64,
    confirmations: u64,
) -> Box<dyn Future<Item = (), Error = String> + Send> {
    let amount = try_fus!(sat_from_big_decimal(&amount, coin.as_ref().decimals));

    let expected_redeem = payment_script(time_lock, priv_bn_hash, first_pub0, second_pub0);
    let fut = async move {
        let mut attempts = 0;
        loop {
            let tx_from_rpc = match coin
                .as_ref()
                .rpc_client
                .get_transaction_bytes(&tx.hash().reversed().into())
                .compat()
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    if attempts > 2 {
                        return ERR!(
                            "Got error {:?} after 3 attempts of getting tx {:?} from RPC",
                            e,
                            tx.tx_hash()
                        );
                    };
                    attempts += 1;
                    log!("Error " [e] " getting the tx " [tx.tx_hash()] " from rpc");
                    Timer::sleep(10.).await;
                    continue;
                },
            };
            if serialize(&tx).take() != tx_from_rpc.0
                && serialize_with_flags(&tx, SERIALIZE_TRANSACTION_WITNESS).take() != tx_from_rpc.0
            {
                return ERR!(
                    "Provided payment tx {:?} doesn't match tx data from rpc {:?}",
                    tx,
                    tx_from_rpc
                );
            }

            let expected_output = TransactionOutput {
                value: amount,
                script_pubkey: Builder::build_p2sh(&dhash160(&expected_redeem).into()).into(),
            };

            let actual_output = tx.outputs.get(output_index);
            if actual_output != Some(&expected_output) {
                // Distinguish amount mismatch from script mismatch so failed swaps
                // and `validate_*_payment` callers get an actionable diagnostic
                // instead of a raw struct dump.
                let kind = match actual_output {
                    Some(actual)
                        if actual.value != expected_output.value
                            && actual.script_pubkey == expected_output.script_pubkey =>
                    {
                        "amount mismatch"
                    },
                    Some(actual) if actual.script_pubkey != expected_output.script_pubkey => "script mismatch",
                    Some(_) => "output mismatch",
                    None => "missing output",
                };
                return ERR!(
                    "Provided payment tx output {}: actual {:?}, expected {:?}",
                    kind,
                    actual_output,
                    expected_output
                );
            }

            if !coin.as_ref().conf.enable_spv_proof {
                return Ok(());
            }

            return match confirmations {
                0 => Ok(()),
                _ => validate_spv_proof(coin, tx, try_spv_proof_until)
                    .await
                    .map_err(|e| format!("{:?}", e)),
            };
        }
    };
    Box::new(fut.boxed().compat())
}

#[allow(clippy::too_many_arguments)]
async fn search_for_swap_output_spend(
    coin: &UtxoCoinFields,
    time_lock: u32,
    first_pub: &Public,
    second_pub: &Public,
    secret_hash: &[u8],
    tx: &[u8],
    output_index: usize,
    search_from_block: u64,
) -> Result<Option<FoundSwapTxSpend>, String> {
    let mut tx: UtxoTx = try_s!(deserialize(tx).map_err(|e| ERRL!("{:?}", e)));
    tx.tx_hash_algo = coin.tx_hash_algo;
    let script = payment_script(time_lock, secret_hash, first_pub, second_pub);
    let expected_script_pubkey = Builder::build_p2sh(&dhash160(&script).into()).to_bytes();
    if tx.outputs[0].script_pubkey != expected_script_pubkey {
        return ERR!(
            "Transaction {:?} output 0 script_pubkey doesn't match expected {:?}",
            tx,
            expected_script_pubkey
        );
    }

    let spend = try_s!(
        coin.rpc_client
            .find_output_spend(
                tx.hash(),
                &tx.outputs[output_index].script_pubkey,
                output_index,
                BlockHashOrHeight::Height(search_from_block as i64)
            )
            .compat()
            .await
    );
    match spend {
        Some(spent_output_info) => {
            let mut tx = spent_output_info.spending_tx;
            tx.tx_hash_algo = coin.tx_hash_algo;
            let script: Script = tx.inputs[0].script_sig.clone().into();
            if let Some(Ok(ref i)) = script.iter().nth(2) {
                if i.opcode == Opcode::OP_0 {
                    return Ok(Some(FoundSwapTxSpend::Spent(tx.into())));
                }
            }

            if let Some(Ok(ref i)) = script.iter().nth(1) {
                if i.opcode == Opcode::OP_1 {
                    return Ok(Some(FoundSwapTxSpend::Refunded(tx.into())));
                }
            }

            ERR!(
                "Couldn't find required instruction in script_sig of input 0 of tx {:?}",
                tx
            )
        },
        None => Ok(None),
    }
}

pub(crate) struct SwapPaymentOutputsResult {
    pub(crate) payment_address: Address,
    pub(crate) outputs: Vec<TransactionOutput>,
}

pub(crate) fn generate_swap_payment_outputs<T>(
    coin: T,
    time_lock: u32,
    my_pub: &[u8],
    other_pub: &[u8],
    secret_hash: &[u8],
    amount: BigDecimal,
) -> Result<SwapPaymentOutputsResult, String>
where
    T: AsRef<UtxoCoinFields>,
{
    let my_public = try_s!(Public::from_slice(my_pub));
    let redeem_script = payment_script(
        time_lock,
        secret_hash,
        &my_public,
        &try_s!(Public::from_slice(other_pub)),
    );
    let redeem_script_hash = dhash160(&redeem_script);
    let amount = try_s!(sat_from_big_decimal(&amount, coin.as_ref().decimals));
    let htlc_out = TransactionOutput {
        value: amount,
        script_pubkey: Builder::build_p2sh(&redeem_script_hash.into()).into(),
    };
    // record secret hash to blockchain too making it impossible to lose
    // lock time may be easily brute forced so it is not mandatory to record it
    let mut op_return_builder = Builder::default().push_opcode(Opcode::OP_RETURN);

    // add the full redeem script to the OP_RETURN for ARRR to simplify the validation for the daemon
    op_return_builder = if coin.as_ref().conf.ticker == "ARRR" {
        op_return_builder.push_data(&redeem_script)
    } else {
        op_return_builder.push_bytes(secret_hash)
    };

    let op_return_script = op_return_builder.into_bytes();

    let op_return_out = TransactionOutput {
        value: 0,
        script_pubkey: op_return_script,
    };

    let payment_address = Address {
        checksum_type: coin.as_ref().conf.checksum_type,
        hash: redeem_script_hash.into(),
        prefix: coin.as_ref().conf.p2sh_addr_prefix,
        t_addr_prefix: coin.as_ref().conf.p2sh_t_addr_prefix,
        hrp: coin.as_ref().conf.bech32_hrp.clone(),
        addr_format: UtxoAddressFormat::Standard,
    };
    let result = SwapPaymentOutputsResult {
        payment_address,
        outputs: vec![htlc_out, op_return_out],
    };
    Ok(result)
}

pub fn payment_script(time_lock: u32, secret_hash: &[u8], pub_0: &Public, pub_1: &Public) -> Script {
    let builder = Builder::default();
    builder
        .push_opcode(Opcode::OP_IF)
        .push_bytes(&time_lock.to_le_bytes())
        .push_opcode(Opcode::OP_CHECKLOCKTIMEVERIFY)
        .push_opcode(Opcode::OP_DROP)
        .push_bytes(pub_0)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_opcode(Opcode::OP_SIZE)
        .push_bytes(&[32])
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_opcode(Opcode::OP_HASH160)
        .push_bytes(secret_hash)
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_bytes(pub_1)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ENDIF)
        .into_script()
}

pub fn dex_fee_script(uuid: [u8; 16], time_lock: u32, watcher_pub: &Public, sender_pub: &Public) -> Script {
    let builder = Builder::default();
    builder
        .push_bytes(&uuid)
        .push_opcode(Opcode::OP_DROP)
        .push_opcode(Opcode::OP_IF)
        .push_bytes(&time_lock.to_le_bytes())
        .push_opcode(Opcode::OP_CHECKLOCKTIMEVERIFY)
        .push_opcode(Opcode::OP_DROP)
        .push_bytes(sender_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_bytes(watcher_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ENDIF)
        .into_script()
}

pub async fn can_refund_htlc<T>(coin: &T, locktime: u64) -> Result<CanRefundHtlc, MmError<UtxoRpcError>>
where
    T: UtxoCommonOps,
{
    let now = now_ms() / 1000;
    if now < locktime {
        let to_wait = locktime - now + 1;
        return Ok(CanRefundHtlc::HaveToWait(to_wait.max(3600)));
    }

    let mtp = coin.get_current_mtp().await?;
    let locktime = coin.p2sh_tx_locktime(locktime as u32).await?;

    if locktime < mtp {
        Ok(CanRefundHtlc::CanRefundNow)
    } else {
        let to_wait = (locktime - mtp + 1) as u64;
        Ok(CanRefundHtlc::HaveToWait(to_wait.max(3600)))
    }
}

pub async fn p2sh_tx_locktime<T>(coin: &T, ticker: &str, htlc_locktime: u32) -> Result<u32, MmError<UtxoRpcError>>
where
    T: UtxoCommonOps,
{
    let lock_time = if ticker == "KMD" {
        (now_ms() / 1000) as u32 - 3600 + 2 * 777
    } else {
        coin.get_current_mtp().await? - 1
    };
    Ok(lock_time.max(htlc_locktime))
}

pub fn get_htlc_key_pair<T>(coin: &T) -> Option<KeyPair>
where
    T: AsRef<UtxoCoinFields>,
{
    match &coin.as_ref().priv_key_policy {
        PrivKeyPolicy::KeyPair(_) | PrivKeyPolicy::HDWallet { .. } => None,
        PrivKeyPolicy::Trezor => Some(KeyPair::random_compressed()),
    }
}

#[test]
fn test_pubkey_from_script_sig() {
    let script_sig = Script::from("473044022071edae37cf518e98db3f7637b9073a7a980b957b0c7b871415dbb4898ec3ebdc022031b402a6b98e64ffdf752266449ca979a9f70144dba77ed7a6a25bfab11648f6012103ad6f89abc2e5beaa8a3ac28e22170659b3209fe2ddf439681b4b8f31508c36fa");
    let expected_pub = H264::from("03ad6f89abc2e5beaa8a3ac28e22170659b3209fe2ddf439681b4b8f31508c36fa");
    let actual_pub = pubkey_from_script_sig(&script_sig).unwrap();
    assert_eq!(expected_pub, actual_pub);

    let script_sig_err = Script::from("473044022071edae37cf518e98db3f7637b9073a7a980b957b0c7b871415dbb4898ec3ebdc022031b402a6b98e64ffdf752266449ca979a9f70144dba77ed7a6a25bfab11648f6012103ad6f89abc2e5beaa8a3ac28e22170659b3209fe2ddf439681b4b8f31508c36fa21");
    pubkey_from_script_sig(&script_sig_err).unwrap_err();

    let script_sig_err = Script::from("493044022071edae37cf518e98db3f7637b9073a7a980b957b0c7b871415dbb4898ec3ebdc022031b402a6b98e64ffdf752266449ca979a9f70144dba77ed7a6a25bfab11648f6012103ad6f89abc2e5beaa8a3ac28e22170659b3209fe2ddf439681b4b8f31508c36fa");
    pubkey_from_script_sig(&script_sig_err).unwrap_err();
}
