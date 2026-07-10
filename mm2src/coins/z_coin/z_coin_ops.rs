use super::*;

impl ZCoin {
    #[inline(always)]
    pub fn z_rpc(&self) -> &(dyn ZRpcOps + Send + Sync) { self.utxo_arc.rpc_client.as_ref() }

    #[inline(always)]
    pub fn rpc_client(&self) -> &UtxoRpcClientEnum { &self.utxo_arc.rpc_client }

    #[inline(always)]
    pub fn is_sapling_state_synced(&self) -> bool { self.z_fields.sapling_state_synced.load(AtomicOrdering::Relaxed) }

    #[inline(always)]
    pub fn my_z_address_encoded(&self) -> String { self.z_fields.my_z_addr_encoded.clone() }

    /// Returns all unspents included currently unspendable (not confirmed)
    pub(crate) async fn my_z_unspents_ordered(&self) -> UtxoRpcResult<Vec<ZUnspent>> {
        let min_conf = 0;
        let max_conf = i32::MAX as u32;
        let watch_only = true;

        let mut unspents = self
            .z_rpc()
            .z_list_unspent(min_conf, max_conf, watch_only, &[&self.z_fields.my_z_addr_encoded])
            .compat()
            .await?;

        unspents.sort_unstable_by(|a, b| a.amount.cmp(&b.amount));
        Ok(unspents)
    }

    /// shielded outputs are not spendable until confirmed
    pub(crate) async fn my_spendable_z_unspents_ordered(&self) -> UtxoRpcResult<Vec<ZUnspent>> {
        let min_conf = 1;
        let max_conf = i32::MAX as u32;
        let watch_only = true;

        let mut unspents = self
            .z_rpc()
            .z_list_unspent(min_conf, max_conf, watch_only, &[&self.z_fields.my_z_addr_encoded])
            .compat()
            .await?;

        unspents.sort_unstable_by(|a, b| a.amount.cmp(&b.amount));
        Ok(unspents)
    }

    pub(crate) async fn get_one_kbyte_tx_fee(&self) -> UtxoRpcResult<BigDecimal> {
        let fee = self.get_tx_fee().await?;
        match fee {
            ActualTxFee::Dynamic(fee) | ActualTxFee::FixedPerKb(fee) => {
                Ok(big_decimal_from_sat_unsigned(fee, self.decimals()))
            },
        }
    }

    /// Generates a tx sending outputs from our address
    pub(crate) async fn gen_tx(
        &self,
        t_outputs: Vec<TxOut>,
        z_outputs: Vec<ZOutput>,
    ) -> Result<(ZTransaction, AdditionalTxData), MmError<GenTxError>> {
        let _lock = self.z_fields.z_unspent_mutex.lock().await;
        while !self.is_sapling_state_synced() {
            Timer::sleep(0.5).await
        }
        let tx_fee = self.get_one_kbyte_tx_fee().await.mm_err(Into::into)?;
        let t_output_sat: u64 = t_outputs.iter().fold(0, |cur, out| cur + u64::from(out.value));
        let z_output_sat: u64 = z_outputs.iter().fold(0, |cur, out| cur + u64::from(out.amount));
        let total_output_sat = t_output_sat + z_output_sat;
        let total_output = big_decimal_from_sat_unsigned(total_output_sat, self.utxo_arc.decimals);
        let total_required = &total_output + &tx_fee;

        let z_unspents = self.my_spendable_z_unspents_ordered().await.mm_err(Into::into)?;
        let mut selected_unspents = Vec::new();
        let mut total_input_amount = BigDecimal::from(0u8);
        let mut change = BigDecimal::from(0u8);

        let mut received_by_me = 0u64;

        for unspent in z_unspents {
            total_input_amount += unspent.amount.to_decimal();
            selected_unspents.push(unspent);

            if total_input_amount >= total_required {
                change = &total_input_amount - &total_required;
                break;
            }
        }

        if total_input_amount < total_required {
            return MmError::err(GenTxError::InsufficientBalance {
                coin: self.ticker().into(),
                available: total_input_amount,
                required: total_required,
            });
        }

        let current_block = self
            .utxo_arc
            .rpc_client
            .get_block_count()
            .compat()
            .await
            .mm_err(Into::into)? as u32;
        let mut tx_builder = ZTxBuilder::new(self.z_fields.consensus_params.clone(), current_block.into());

        let mut ext = HashMap::new();

        let evk = ExtendedFullViewingKey::from(&self.z_fields.z_spending_key);
        ext.insert(AccountId::default(), evk);
        let mut selected_notes_with_witness: Vec<(_, IncrementalWitness<Node>)> =
            Vec::with_capacity(selected_unspents.len());

        for unspent in selected_unspents {
            let prev_tx = self
                .rpc_client()
                .get_verbose_transaction(&unspent.txid)
                .compat()
                .await
                .mm_err(Into::into)?;

            let height = prev_tx.height.or_mm_err(|| GenTxError::PrevTxNotConfirmed)?;

            let z_cash_tx = ZTransaction::read(prev_tx.hex.as_slice())
                .map_to_mm(|err| GenTxError::TxReadError { err, hex: prev_tx.hex })?;
            let decrypted = decrypt_transaction(
                &self.z_fields.consensus_params,
                BlockHeight::from_u32(height as u32),
                &z_cash_tx,
                &ext,
            );

            let decrypted_output = decrypted
                .iter()
                .find(|out| out.index as u32 == unspent.out_index)
                .or_mm_err(|| GenTxError::DecryptedOutputNotFound)?;
            let witness = self
                .get_unspent_witness(&decrypted_output.note, height as u32)
                .await
                .mm_err(Into::into)?;
            selected_notes_with_witness.push((decrypted_output.note.clone(), witness));
        }

        for (note, witness) in selected_notes_with_witness {
            tx_builder.add_sapling_spend(
                self.z_fields.z_spending_key.clone(),
                *self.z_fields.my_z_addr.diversifier(),
                note,
                witness.path().or_mm_err(|| GenTxError::FailedToGetMerklePath)?,
            )?;
        }

        for z_out in z_outputs {
            if z_out.to_addr == self.z_fields.my_z_addr {
                received_by_me += u64::from(z_out.amount);
            }

            tx_builder.add_sapling_output(z_out.viewing_key, z_out.to_addr, z_out.amount, z_out.memo)?;
        }

        if change > BigDecimal::from(0u8) {
            let change_sat = sat_from_big_decimal(&change, self.utxo_arc.decimals).mm_err(Into::into)?;
            received_by_me += change_sat;

            tx_builder.add_sapling_output(
                None,
                self.z_fields.my_z_addr.clone(),
                Amount::from_u64(change_sat).map_to_mm(|_| {
                    GenTxError::NumConversion(NumConversError(format!(
                        "Failed to get ZCash amount from {}",
                        change_sat
                    )))
                })?,
                None,
            )?;
        }

        for output in t_outputs {
            tx_builder.add_tx_out(output);
        }

        let (tx, _) =
            tokio::task::block_in_place(|| tx_builder.build(consensus::BranchId::Sapling, &self.z_fields.z_tx_prover))?;

        let additional_data = AdditionalTxData {
            received_by_me,
            spent_by_me: sat_from_big_decimal(&total_input_amount, self.decimals()).mm_err(Into::into)?,
            fee_amount: sat_from_big_decimal(&tx_fee, self.decimals()).mm_err(Into::into)?,
            unused_change: None,
            kmd_rewards: None,
        };
        Ok((tx, additional_data))
    }

    pub async fn send_outputs(
        &self,
        t_outputs: Vec<TxOut>,
        z_outputs: Vec<ZOutput>,
    ) -> Result<ZTransaction, MmError<SendOutputsErr>> {
        let (tx, _) = self.gen_tx(t_outputs, z_outputs).await.mm_err(Into::into)?;
        let mut tx_bytes = Vec::with_capacity(1024);
        tx.write(&mut tx_bytes).expect("Write should not fail");

        self.rpc_client()
            .send_raw_transaction(tx_bytes.into())
            .compat()
            .await
            .mm_err(Into::into)?;

        self.rpc_client()
            .wait_for_confirmations(
                H256Json::from(tx.txid().0).reversed(),
                tx.expiry_height.into(),
                1,
                false,
                now_ms() + 4000,
                10,
            )
            .compat()
            .await
            .map_to_mm(SendOutputsErr::TxNotMined)?;
        Ok(tx)
    }

    #[inline(always)]
    pub(crate) fn sqlite_conn(&self) -> MutexGuard<'_, Connection> { self.z_fields.sqlite.lock().unwrap() }

    pub async fn get_unspent_witness(
        &self,
        note: &Note,
        tx_height: u32,
    ) -> Result<IncrementalWitness<Node>, MmError<GetUnspentWitnessErr>> {
        let mut attempts = 0;
        let states = loop {
            let states = tokio::task::block_in_place(|| query_states_after_height(&self.sqlite_conn(), tx_height))?;
            if states.is_empty() {
                if attempts > 2 {
                    return MmError::err(GetUnspentWitnessErr::EmptyDbResult);
                }
                attempts += 1;
                Timer::sleep(10.).await;
            } else {
                break states;
            }
        };

        let mut tree = states[0].prev_tree_state.clone();
        let mut witness = None::<IncrementalWitness<Node>>;

        let note_cmu = H256::from(note.cmu().to_bytes());
        for state in states {
            for cmu in state.cmus {
                let build_witness = cmu == note_cmu;
                let node = Node::new(cmu.take());
                match witness {
                    Some(ref mut w) => w
                        .append(node)
                        .map_to_mm(|_| GetUnspentWitnessErr::TreeOrWitnessAppendFailed)?,
                    None => tree
                        .append(node)
                        .map_to_mm(|_| GetUnspentWitnessErr::TreeOrWitnessAppendFailed)?,
                };

                if build_witness {
                    witness = Some(IncrementalWitness::from_tree(&tree));
                }
            }
        }

        witness.or_mm_err(|| GetUnspentWitnessErr::OutputCmuNotFoundInCache)
    }

    #[inline(always)]
    pub(crate) fn into_weak_parts(self) -> (UtxoWeak, Weak<ZCoinFields>) {
        (self.utxo_arc.downgrade(), Arc::downgrade(&self.z_fields))
    }

    pub(crate) fn from_weak_parts(utxo: &UtxoWeak, z_fields: &Weak<ZCoinFields>) -> Option<Self> {
        let utxo_arc = utxo.upgrade()?;
        let z_fields = z_fields.upgrade()?;

        Some(ZCoin { utxo_arc, z_fields })
    }
}
