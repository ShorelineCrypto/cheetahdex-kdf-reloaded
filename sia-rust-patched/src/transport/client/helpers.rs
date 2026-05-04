use super::ApiClient;
use crate::transport::endpoints::{AddressBalanceRequest, AddressBalanceResponse, AddressesEventsRequest,
                                  ConsensusIndexRequest, ConsensusTipRequest, ConsensusTipstateRequest,
                                  ConsensusTipstateResponse, ConsensusUpdatesRequest, ConsensusUpdatesResponse,
                                  DebugMineRequest, GetAddressUtxosRequest, GetEventRequest,
                                  OutputsSiacoinSpentRequest, TxpoolBroadcastRequest, TxpoolTransactionsRequest,
                                  UtxosWithBasis};
use crate::types::{Address, Currency, Event, EventDataWrapper, Hash256, PublicKey, SiacoinElement, SiacoinOutputId,
                   SpendPolicy, TransactionId, UtxoWithBasis, V2Transaction};
use crate::utils::V2TransactionBuilder;
use async_trait::async_trait;
use thiserror::Error;

/** Generic errors for the ApiClientHelpers trait
These errors are agnostic towards the ClientError generic type allowing each client implementation
to define its own error handling for transport errors.
These types are not intended for consumer use unless a custom client implementation is required.
Do not import these types directly. Use the corresponding type aliases defined in native.rs or wasm.rs. **/
pub(crate) mod generic_errors {
    use super::*;

    #[derive(Debug, Error)]
    pub enum UtxoFromTxidErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::utxo_from_txid: failed to fetch event {0}")]
        FetchEvent(#[from] ClientError),
        #[error("ApiClientHelpers::utxo_from_txid: invalid event variant {0:?}")]
        EventVariant(Event),
        #[error("ApiClientHelpers::utxo_from_txid: output index out of bounds txid: {txid} index: {index}")]
        OutputIndexOutOfBounds { txid: TransactionId, index: u32 },
        #[error("ApiClientHelpers::utxo_from_txid: get_unspent_outputs helper failed {0}")]
        FetchUtxos(#[from] GetUnspentOutputsErrorGeneric<ClientError>),
        #[error("ApiClientHelpers::utxo_from_txid: output not found txid: {txid} index: {index}")]
        NotFound { txid: TransactionId, index: u32 },
        #[error("ApiClientHelpers::utxo_from_txid: found duplicate utxo txid: {txid} index: {index}")]
        DuplicateUtxoFound { txid: TransactionId, index: u32 },
    }

    #[derive(Debug, Error)]
    pub enum GetTransactionErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_transaction: failed to fetch event {0}")]
        FetchEvent(#[from] ClientError),
        #[error("ApiClientHelpers::get_transaction: unexpected variant error {0:?}")]
        EventVariant(EventDataWrapper),
    }

    #[derive(Debug, Error)]
    pub enum GetUnconfirmedTransactionErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_unconfirmed_transaction: failed to fetch mempool {0}")]
        FetchMempool(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum FundTxSingleSourceErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::fund_tx_single_source: failed to select Utxos: {0}")]
        SelectUtxos(#[from] SelectUtxosErrorGeneric<ClientError>),
    }

    #[derive(Debug, Error)]
    pub enum GetEventErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_event failed to fetch event: {0}")]
        FetchEvent(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum GetAddressEventsErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_address_events failed to fetch unspent outputs: {0}")]
        FetchAddressEvents(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum BroadcastTransactionErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::broadcast_transaction: broadcast failed: {0}")]
        BroadcastTx(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum SelectUtxosErrorGeneric<ClientError> {
        #[error(
        "ApiClientHelpers::select_unspent_outputs: insufficent funds, available: {available:?} required: {required:?}"
    )]
        Funding { available: Currency, required: Currency },
        #[error("ApiClientHelpers::select_unspent_outputs: failed to fetch UTXOs: {0}")]
        GetUnspentOutputs(#[from] GetUnspentOutputsErrorGeneric<ClientError>),
    }

    #[derive(Debug, Error)]
    pub enum GetMedianTimestampErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_median_timestamp: failed to fetch consensus tipstate: {0}")]
        FetchTipstate(#[from] ClientError),
        #[error(
            r#"ApiClientHelpers::get_median_timestamp: expected 11 timestamps in response: {0:?}.
           The walletd state is likely corrupt as it is evidently reporting a chain height of less
           than 11 blocks."#
        )]
        TimestampVecLen(ConsensusTipstateResponse),
    }

    #[derive(Debug, Error)]
    pub enum GetUnspentOutputsErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_unspent_outputs: failed to fetch UTXOs: {0}")]
        FetchUtxos(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum CurrentHeightErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::current_height: failed to fetch current height: {0}")]
        FetchConsensusTip(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum AddressBalanceErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::address_balance: failed to fetch address balance: {0}")]
        FetchAddressBalance(#[from] ClientError),
    }

    #[derive(Debug, Error)]
    pub enum FindWhereUtxoSpentErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::find_where_utxo_spent: failed to fetch transaction event: {0}")]
        FetchEvent(#[from] ClientError),
        #[error("ApiClientHelpers::find_where_utxo_spent: expected V2Transaction event, found: {0:?}")]
        WrongEventType(Event),
    }

    #[derive(Debug, Error)]
    pub enum GetConsensusUpdatesErrorGeneric<ClientError> {
        #[error("ApiClientHelpers::get_consensus_updates_since_height: failed to fetch ChainIndex {0}")]
        FetchIndex(ClientError),
        #[error("ApiClientHelpers::get_consensus_updates_since_height: failed to fetch updates {0}")]
        FetchUpdates(ClientError),
    }

    #[derive(Debug, Error)]
    pub enum DebugMineErrorGeneric<ClientError> {
        #[error("ApiClientDebugHelpers::mine_blocks: failed to mine blocks: {0}")]
        Mine(#[from] ClientError),
    }
}

use generic_errors::*;

/// ApiClientHelpers implements client agnostic helper methods catering to the Komodo Defi Framework
/// integration. These methods provide higher level functionality than the base ApiClient trait.
/// Clients can generally implement this as simply as `impl ApiClientHelpers for Client {}`.
#[async_trait]
pub trait ApiClientHelpers: ApiClient {
    async fn current_height(&self) -> Result<u64, CurrentHeightErrorGeneric<Self::Error>> {
        Ok(self.dispatcher(ConsensusTipRequest).await?.height)
    }

    async fn address_balance(
        &self,
        address: Address,
    ) -> Result<AddressBalanceResponse, AddressBalanceErrorGeneric<Self::Error>> {
        Ok(self.dispatcher(AddressBalanceRequest { address }).await?)
    }

    async fn get_unspent_outputs(
        &self,
        address: &Address,
        limit: Option<i64>,
        offset: Option<i64>,
        include_mempool: bool,
    ) -> Result<UtxosWithBasis, GetUnspentOutputsErrorGeneric<Self::Error>> {
        Ok(self
            .dispatcher(GetAddressUtxosRequest {
                address: address.clone(),
                limit,
                offset,
                include_mempool,
            })
            .await?)
    }

    /// Fetches unspent outputs for the given address and attempts to select a subset of outputs
    /// whose total value is at least `total_amount`. The outputs are sorted from largest to smallest to minimize
    /// the number of outputs selected. The function returns a vector of the selected outputs and the difference between
    /// the total value of the selected outputs and the required amount, aka the change.
    /// # Arguments
    ///
    /// * `unspent_outputs` - A vector of `SiacoinElement`s representing unspent Siacoin outputs.
    /// * `total_amount` - The total amount required for the selection. Should generally be the sum of the outputs and miner fee.
    ///
    /// # Returns
    ///
    /// This function returns `Result<(Vec<SiacoinElement>, Currency), ApiClientHelpersError>`:
    /// * `Ok((Vec<SiacoinElement>, Currency))` - A tuple containing, a vector of the selected unspent outputs and the change amount.
    /// * `Err(ApiClientHelpersError)` - An error is returned if the available outputs cannot meet the required amount or a transport error is encountered.
    async fn select_unspent_outputs(
        &self,
        address: &Address,
        total_amount: Currency,
    ) -> Result<(UtxosWithBasis, Currency), SelectUtxosErrorGeneric<Self::Error>> {
        let mut unspent_outputs = self.get_unspent_outputs(address, None, None, true).await?;

        // Sort outputs from largest to smallest
        unspent_outputs
            .outputs
            .sort_by(|a, b| b.siacoin_output.value.0.cmp(&a.siacoin_output.value.0));

        let mut selected = Vec::new();
        let mut selected_amount = Currency::ZERO;

        // Select outputs until the total amount is reached
        for output in unspent_outputs.outputs {
            selected_amount += output.siacoin_output.value;
            selected.push(output);

            if selected_amount >= total_amount {
                break;
            }
        }

        if selected_amount < total_amount {
            return Err(SelectUtxosErrorGeneric::Funding {
                available: selected_amount,
                required: total_amount,
            })?;
        }
        let change = selected_amount - total_amount;

        Ok((
            UtxosWithBasis {
                outputs: selected,
                basis: unspent_outputs.basis,
            },
            change,
        ))
    }

    /// Fund a transaction with utxos from the given address.
    /// This should generally be used only after all outputs and miner_fee have been added to the builder.
    /// Assumes no file contracts or resolutions. This is a helper designed for Komodo DeFi Framework.
    /// Adds inputs from the given address until the total amount from outputs and miner_fee is reached.
    /// See `select_unspent_outputs` for more details on UTXO selection.
    /// # Arguments
    /// * `tx_builder` - A mutable reference to a `V2TransactionBuilder.
    /// * `public_key` - The public key of the address to spend utxos from.
    /// # Returns
    /// * `Ok(())` - The transaction builder has been successfully funded
    /// * `Err(ApiClientHelpersError)` - An error is returned if the available outputs cannot meet
    ///     the required amount or a transport error is encountered.
    async fn fund_tx_single_source(
        &self,
        mut tx_builder: V2TransactionBuilder,
        public_key: &PublicKey,
    ) -> Result<V2TransactionBuilder, FundTxSingleSourceErrorGeneric<Self::Error>> {
        let address = public_key.address();
        let outputs_total: Currency = tx_builder.siacoin_outputs.iter().map(|output| output.value).sum();

        // select utxos from public key's address that total at least the sum of outputs and miner fee
        let (selected_utxos, _) = self
            .select_unspent_outputs(&address, outputs_total + tx_builder.miner_fee)
            .await?;

        // add selected utxos as inputs to the transaction
        for utxo in &selected_utxos.outputs {
            tx_builder = tx_builder.add_siacoin_input(utxo.clone(), SpendPolicy::PublicKey(public_key.clone()));
        }

        // update the transaction's basis
        tx_builder = tx_builder.update_basis(selected_utxos.basis);

        Ok(tx_builder)
    }

    /// Fetches a SiacoinElement(a UTXO) from a TransactionId and Index
    /// Walletd doesn't currently offer an easy way to fetch the SiacoinElement type needed to build
    /// SiacoinInputs.
    async fn utxo_from_txid(
        &self,
        txid: &TransactionId,
        vout_index: u32,
    ) -> Result<UtxoWithBasis, UtxoFromTxidErrorGeneric<Self::Error>> {
        let output_id = SiacoinOutputId::new(txid.clone(), vout_index);

        // fetch the Event via /api/events/{txid}
        let event = self.dispatcher(GetEventRequest { txid: txid.clone() }).await?;

        // check that the fetched event is V2Transaction
        let tx = match event.data {
            EventDataWrapper::V2Transaction(tx) => tx,
            _ => return Err(UtxoFromTxidErrorGeneric::<Self::Error>::EventVariant(event)),
        };

        // check that the output index is within bounds
        if tx.siacoin_outputs.len() <= (vout_index as usize) {
            return Err(UtxoFromTxidErrorGeneric::<Self::Error>::OutputIndexOutOfBounds {
                txid: txid.clone(),
                index: vout_index,
            });
        }

        let output_address = tx.siacoin_outputs[vout_index as usize].address.clone();

        // fetch unspent outputs of the address
        let address_utxos = self.get_unspent_outputs(&output_address, None, None, true).await?;

        // filter the utxos to find any matching the expected SiacoinOutputId
        let filtered_utxos: Vec<SiacoinElement> = address_utxos
            .outputs
            .into_iter()
            .filter(|element| element.id == output_id)
            .collect();

        // ensure only one utxo was found
        match filtered_utxos.len() {
            1 => Ok(UtxoWithBasis {
                output: filtered_utxos[0].clone(),
                basis: address_utxos.basis,
            }),
            0 => Err(UtxoFromTxidErrorGeneric::<Self::Error>::NotFound {
                txid: txid.clone(),
                index: vout_index,
            }),
            _ => Err(UtxoFromTxidErrorGeneric::<Self::Error>::DuplicateUtxoFound {
                txid: txid.clone(),
                index: vout_index,
            }),
        }
    }

    async fn get_event(&self, event_id: &Hash256) -> Result<Event, GetEventErrorGeneric<Self::Error>> {
        Ok(self.dispatcher(GetEventRequest { txid: event_id.clone() }).await?)
    }

    async fn get_address_events(
        &self,
        address: Address,
    ) -> Result<Vec<Event>, GetAddressEventsErrorGeneric<Self::Error>> {
        let request = AddressesEventsRequest {
            address,
            limit: None,
            offset: None,
        };
        Ok(self.dispatcher(request).await?)
    }

    /// Fetch a v2 transaction from the blockchain
    // FIXME Alright - this should return a Result<Option<V2Transaction>, SomeError> to allow for
    // logic to handle the case where the transaction is not found in the blockchain
    // ApiClientError must be refactored to allow this
    async fn get_transaction(
        &self,
        txid: &TransactionId,
    ) -> Result<V2Transaction, GetTransactionErrorGeneric<Self::Error>> {
        let event = self.dispatcher(GetEventRequest { txid: txid.clone() }).await?;
        match event.data {
            EventDataWrapper::V2Transaction(tx) => Ok(tx),
            wrong_variant => Err(GetTransactionErrorGeneric::<Self::Error>::EventVariant(wrong_variant)),
        }
    }

    /// Fetch a v2 transaction from the transaction pool / mempool
    /// Returns Ok(None) if the transaction is not found in the mempool
    async fn get_unconfirmed_transaction(
        &self,
        txid: &TransactionId,
    ) -> Result<Option<V2Transaction>, GetUnconfirmedTransactionErrorGeneric<Self::Error>> {
        let found_in_mempool = self
            .dispatcher(TxpoolTransactionsRequest)
            .await?
            .v2transactions
            .into_iter()
            .find(|tx| tx.txid() == *txid);
        Ok(found_in_mempool)
    }

    /// Get the median timestamp of the chain's last 11 blocks
    /// This is used in the evaluation of SpendPolicy::After
    async fn get_median_timestamp(&self) -> Result<u64, GetMedianTimestampErrorGeneric<Self::Error>> {
        let tipstate = self.dispatcher(ConsensusTipstateRequest).await?;

        // This can happen if the chain has less than 11 blocks
        // We assume the chain is at least 11 blocks long for this helper.
        if tipstate.prev_timestamps.len() != 11 {
            return Err(GetMedianTimestampErrorGeneric::<Self::Error>::TimestampVecLen(tipstate));
        }

        let median_timestamp = tipstate.prev_timestamps[5];
        Ok(median_timestamp.timestamp() as u64)
    }

    async fn broadcast_transaction(
        &self,
        tx: &V2Transaction,
    ) -> Result<(), BroadcastTransactionErrorGeneric<Self::Error>> {
        // Use the most recent ChainIndex as basis if not provided within `tx`
        let basis = match &tx.basis {
            Some(basis) => basis.clone(),
            None => self.dispatcher(ConsensusTipRequest).await?,
        };

        let request = TxpoolBroadcastRequest {
            basis,
            transactions: vec![],
            v2transactions: vec![tx.clone()],
        };

        self.dispatcher(request).await?;
        Ok(())
    }

    async fn get_consensus_updates(
        &self,
        begin_height: u64,
    ) -> Result<ConsensusUpdatesResponse, GetConsensusUpdatesErrorGeneric<Self::Error>> {
        let index_request = ConsensusIndexRequest { height: begin_height };
        let chain_index = self
            .dispatcher(index_request)
            .await
            .map_err(GetConsensusUpdatesErrorGeneric::<Self::Error>::FetchIndex)?;

        let updates_request = ConsensusUpdatesRequest {
            height: chain_index.height,
            block_hash: chain_index.id,
            limit: None,
        };

        self.dispatcher(updates_request)
            .await
            .map_err(GetConsensusUpdatesErrorGeneric::<Self::Error>::FetchUpdates)
    }

    /// Find the transaction that spent the given utxo
    /// Wrapper for the /api/outputs/siacoin/:id/spent endpoint that assumes the utxo was
    /// spent in a V2Transaction. Returns None if the utxo has not been spent.
    async fn find_where_utxo_spent(
        &self,
        output_id: &SiacoinOutputId,
    ) -> Result<Option<V2Transaction>, FindWhereUtxoSpentErrorGeneric<Self::Error>> {
        let request = OutputsSiacoinSpentRequest {
            output_id: output_id.clone(),
        };

        let response = self.dispatcher(request).await?;

        match response.event {
            Some(event) => {
                let tx = match event.data {
                    EventDataWrapper::V2Transaction(tx) => tx,
                    // utxo was spent in an unexpected way
                    _ => return Err(FindWhereUtxoSpentErrorGeneric::<Self::Error>::WrongEventType(event)),
                };
                Ok(Some(tx))
            },
            // The utxo has not been spent
            None => Ok(None),
        }
    }

    /**
    Mine `n` blocks to the given Sia Address, `addr`.
    Does not wait for the blocks to be mined. Returns immediately after receiving a response from the walletd node.
    This endpoint is only available on Walletd nodes that have been started with `-debug`.
    **/
    async fn mine_blocks(&self, n: i64, addr: &Address) -> Result<(), DebugMineErrorGeneric<Self::Error>> {
        self.dispatcher(DebugMineRequest {
            address: addr.clone(),
            blocks: n,
        })
        .await?;
        Ok(())
    }
}
