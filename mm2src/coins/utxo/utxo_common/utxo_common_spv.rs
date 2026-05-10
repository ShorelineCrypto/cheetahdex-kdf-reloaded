// utxo_common_spv — SPV proof validation, block header management

use super::*;

pub async fn validate_spv_proof<T: UtxoCommonOps>(
    coin: T,
    tx: UtxoTx,
    try_spv_proof_until: u64,
) -> Result<(), MmError<SPVError>> {
    let client = match &coin.as_ref().rpc_client {
        UtxoRpcClientEnum::Native(_) => return Ok(()),
        UtxoRpcClientEnum::Electrum(electrum_client) => electrum_client,
    };
    if tx.outputs.is_empty() {
        return MmError::err(SPVError::InvalidVout);
    }

    let (merkle_branch, block_header) = spv_proof_retry_pool(&coin, client, &tx, try_spv_proof_until).await?;
    let raw_header = RawBlockHeader::new(block_header.raw().take())?;
    let intermediate_nodes: Vec<H256> = merkle_branch
        .merkle
        .into_iter()
        .map(|hash| hash.reversed().into())
        .collect();

    let proof = SPVProof {
        tx_id: tx.hash(),
        vin: serialize_list(&tx.inputs).take(),
        vout: serialize_list(&tx.outputs).take(),
        index: merkle_branch.pos as u64,
        confirming_header: block_header,
        raw_header,
        intermediate_nodes,
    };

    proof.validate().map_err(MmError::new)
}

async fn spv_proof_retry_pool<T: UtxoCommonOps>(
    coin: &T,
    client: &ElectrumClient,
    tx: &UtxoTx,
    try_spv_proof_until: u64,
) -> Result<(TxMerkleBranch, BlockHeader), MmError<SPVError>> {
    let mut height: Option<u64> = None;
    let mut merkle_branch: Option<TxMerkleBranch> = None;

    loop {
        if now_ms() / 1000 > try_spv_proof_until {
            error!(
                "Waited too long until {} for transaction {:?} to validate spv proof",
                try_spv_proof_until,
                tx.hash(),
            );
            return Err(SPVError::Timeout.into());
        }

        if height.is_none() {
            match get_tx_height(tx, client).await {
                Ok(h) => height = Some(h),
                Err(e) => {
                    debug!("`get_tx_height` returned an error {:?}", e);
                    error!("{:?} for tx {:?}", SPVError::InvalidHeight, tx);
                },
            }
        }

        if height.is_some() && merkle_branch.is_none() {
            match client
                .blockchain_transaction_get_merkle(tx.hash().reversed().into(), height.unwrap())
                .compat()
                .await
            {
                Ok(m) => merkle_branch = Some(m),
                Err(e) => {
                    debug!("`blockchain_transaction_get_merkle` returned an error {:?}", e);
                    error!(
                        "{:?} by tx: {:?}, height: {}",
                        SPVError::UnableToGetMerkle,
                        H256Json::from(tx.hash().reversed()),
                        height.unwrap()
                    );
                },
            }
        }

        if height.is_some() && merkle_branch.is_some() {
            match block_header_from_storage_or_rpc(&coin, height.unwrap(), &coin.as_ref().block_headers_storage, client)
                .await
            {
                Ok(block_header) => {
                    return Ok((merkle_branch.unwrap(), block_header));
                },
                Err(e) => {
                    debug!("`block_header_from_storage_or_rpc` returned an error {:?}", e);
                    error!(
                        "{:?}, Received header likely not compatible with header format in mm2",
                        SPVError::UnableToGetHeader
                    );
                },
            }
        }

        error!(
            "Failed spv proof validation for transaction {:?}, retrying in {} seconds.",
            tx.hash(),
            TRY_SPV_PROOF_INTERVAL,
        );

        Timer::sleep(TRY_SPV_PROOF_INTERVAL as f64).await;
    }
}

pub async fn get_tx_height(tx: &UtxoTx, client: &ElectrumClient) -> Result<u64, MmError<GetTxHeightError>> {
    for output in tx.outputs.clone() {
        let script_pubkey_str = hex::encode(electrum_script_hash(&output.script_pubkey));
        if let Ok(history) = client.scripthash_get_history(script_pubkey_str.as_str()).compat().await {
            if let Some(item) = history
                .into_iter()
                .find(|item| item.tx_hash.reversed() == H256Json(*tx.hash()) && item.height > 0)
            {
                return Ok(item.height as u64);
            }
        }
    }
    MmError::err(GetTxHeightError::HeightNotFound)
}

pub async fn valid_block_header_from_storage<T>(
    coin: &T,
    height: u64,
    storage: &BlockHeaderStorage,
    client: &ElectrumClient,
) -> Result<BlockHeader, MmError<GetBlockHeaderError>>
where
    T: AsRef<UtxoCoinFields>,
{
    match storage
        .get_block_header(coin.as_ref().conf.ticker.as_str(), height)
        .await
        .mm_err(Into::into)?
    {
        None => {
            let bytes = client.blockchain_block_header(height).compat().await?;
            let header: BlockHeader = deserialize(bytes.0.as_slice())?;
            let params = &storage.params;
            let blocks_limit = params.blocks_limit_to_check;
            let (headers_registry, headers) = client
                .retrieve_last_headers(blocks_limit, height)
                .compat()
                .await
                .mm_err(Into::into)?;
            match spv_validation::helpers_validation::validate_headers(
                headers,
                params.difficulty_check,
                params.constant_difficulty,
            ) {
                Ok(_) => {
                    storage
                        .add_block_headers_to_storage(coin.as_ref().conf.ticker.as_str(), headers_registry)
                        .await
                        .mm_err(Into::into)?;
                    Ok(header)
                },
                Err(err) => MmError::err(GetBlockHeaderError::SPVError(err)),
            }
        },
        Some(header) => Ok(header),
    }
}

#[inline]
pub async fn block_header_from_storage_or_rpc<T>(
    coin: &T,
    height: u64,
    storage: &Option<BlockHeaderStorage>,
    client: &ElectrumClient,
) -> Result<BlockHeader, MmError<GetBlockHeaderError>>
where
    T: AsRef<UtxoCoinFields>,
{
    match storage {
        Some(ref storage) => valid_block_header_from_storage(&coin, height, storage, client).await,
        None => Ok(deserialize(
            client.blockchain_block_header(height).compat().await?.as_slice(),
        )?),
    }
}

pub async fn block_header_utxo_loop<T: UtxoCommonOps>(weak: UtxoWeak, constructor: impl Fn(UtxoArc) -> T) {
    {
        let coin = match weak.upgrade() {
            Some(arc) => constructor(arc),
            None => return,
        };
        let ticker = coin.as_ref().conf.ticker.as_str();
        let storage = match &coin.as_ref().block_headers_storage {
            None => return,
            Some(storage) => storage,
        };
        match storage.is_initialized_for(ticker).await {
            Ok(true) => info!("Block Header Storage already initialized for {}", ticker),
            Ok(false) => {
                if let Err(e) = storage.init(ticker).await {
                    error!(
                        "Couldn't initiate storage - aborting the block_header_utxo_loop: {:?}",
                        e
                    );
                    return;
                }
                info!("Block Header Storage successfully initialized for {}", ticker);
            },
            Err(_e) => return,
        };
    }
    while let Some(arc) = weak.upgrade() {
        let coin = constructor(arc);
        let storage = match &coin.as_ref().block_headers_storage {
            None => break,
            Some(storage) => storage,
        };
        let params = storage.params.clone();
        let (check_every, blocks_limit_to_check, difficulty_check, constant_difficulty) = (
            params.check_every,
            params.blocks_limit_to_check,
            params.difficulty_check,
            params.constant_difficulty,
        );
        let height =
            ok_or_continue_after_sleep!(coin.as_ref().rpc_client.get_block_count().compat().await, check_every);
        let client = match &coin.as_ref().rpc_client {
            UtxoRpcClientEnum::Native(_) => break,
            UtxoRpcClientEnum::Electrum(client) => client,
        };
        let (block_registry, block_headers) = ok_or_continue_after_sleep!(
            client
                .retrieve_last_headers(blocks_limit_to_check, height)
                .compat()
                .await,
            check_every
        );
        ok_or_continue_after_sleep!(
            validate_headers(block_headers, difficulty_check, constant_difficulty),
            check_every
        );

        let ticker = coin.as_ref().conf.ticker.as_str();
        ok_or_continue_after_sleep!(
            storage.add_block_headers_to_storage(ticker, block_registry).await,
            check_every
        );
        debug!("tick block_header_utxo_loop for {}", coin.as_ref().conf.ticker);
        Timer::sleep(check_every).await;
    }
}
