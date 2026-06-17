use crate::encoding::{Encodable, Encoder};
use crate::transport::client::{
    error::ClientError, helpers::generic_errors::FundTxSingleSourceErrorGeneric, ApiClientHelpers, Client,
};
use crate::types::{
    Address, ArbitraryData, Attestation, ChainIndex, Currency, CurrencyVersion, FileContractRevisionV2, Hash256,
    Keypair, Preimage, PublicKey, SatisfiedPolicy, SiacoinElement, SiacoinInputV2, SiacoinOutput, SiacoinOutputVersion,
    SiafundInputV2, SiafundOutput, SiafundOutputVersion, SpendPolicy, UnlockKey, UtxoWithBasis, V2FileContract,
    V2FileContractResolution, V2Transaction, V2_REPLAY_PREFIX,
};

use thiserror::Error;

#[derive(Clone, Debug)]
pub struct V2TransactionBuilder {
    pub siacoin_inputs: Vec<SiacoinInputV2>,
    pub siacoin_outputs: Vec<SiacoinOutput>,
    pub siafund_inputs: Vec<SiafundInputV2>,
    pub siafund_outputs: Vec<SiafundOutput>,
    pub file_contracts: Vec<V2FileContract>,
    pub file_contract_revisions: Vec<FileContractRevisionV2>,
    pub file_contract_resolutions: Vec<V2FileContractResolution>,
    pub attestations: Vec<Attestation>,
    pub arbitrary_data: ArbitraryData,
    pub new_foundation_address: Option<Address>,
    pub miner_fee: Currency,
    // fee_policy is not part of Sia consensus and it is not encoded into any resulting transaction.
    // fee_policy has no effect unless a helper like `ApiClientHelpers::fund_tx_single_source` utilizes it.
    pub fee_policy: Option<FeePolicy>,
    // basis is not part of Sia consensus and it is not encoded into any resulting transaction.
    // It is the ChainIndex required to broadcast the transaction. This is provided by the
    // /api/addresses/:addr/siacoin/outputs Walletd API endpoint.
    pub basis: Option<ChainIndex>,
}

impl Encodable for V2TransactionBuilder {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.siacoin_inputs.len() as u64);
        for si in &self.siacoin_inputs {
            si.parent.id.encode(encoder);
        }

        encoder.write_u64(self.siacoin_outputs.len() as u64);
        for so in &self.siacoin_outputs {
            SiacoinOutputVersion::V2(so).encode(encoder);
        }

        encoder.write_u64(self.siafund_inputs.len() as u64);
        for si in &self.siafund_inputs {
            si.parent.id.encode(encoder);
        }

        encoder.write_u64(self.siafund_outputs.len() as u64);
        for so in &self.siafund_outputs {
            SiafundOutputVersion::V2(so).encode(encoder);
        }

        encoder.write_u64(self.file_contracts.len() as u64);
        for fc in &self.file_contracts {
            fc.with_nil_sigs().encode(encoder);
        }

        encoder.write_u64(self.file_contract_revisions.len() as u64);
        for fcr in &self.file_contract_revisions {
            fcr.parent.id.encode(encoder);
            fcr.revision.with_nil_sigs().encode(encoder);
        }

        encoder.write_u64(self.file_contract_resolutions.len() as u64);
        for fcr in &self.file_contract_resolutions {
            fcr.parent.id.encode(encoder);
            fcr.with_nil_sigs().encode(encoder);
        }

        encoder.write_u64(self.attestations.len() as u64);
        for att in &self.attestations {
            att.encode(encoder);
        }

        self.arbitrary_data.encode(encoder);

        encoder.write_bool(self.new_foundation_address.is_some());
        match &self.new_foundation_address {
            Some(addr) => addr.encode(encoder),
            None => (),
        }
        CurrencyVersion::V2(&self.miner_fee).encode(encoder);
    }
}

#[derive(Debug, Error)]
pub enum V2TransactionBuilderError {
    #[error("V2TransactionBuilder::satisfy_atomic_swap_success: provided index: {index} is out of bounds for inputs of length: {len}")]
    SatisfySuccessIndexOutOfBounds { len: usize, index: u32 },
    #[error("V2TransactionBuilder::satisfy_atomic_swap_refund: provided index: {index} is out of bounds for inputs of length: {len}")]
    SatisfyRefundIndexOutOfBounds { len: usize, index: u32 },
    #[error("V2TransactionBuilder::fund_tx_single_source: ApiClientHelpers methods failed: {0}")]
    FundTxSingleSource(#[from] FundTxSingleSourceErrorGeneric<ClientError>),
}

impl V2TransactionBuilder {
    pub fn new() -> Self {
        Self {
            siacoin_inputs: Vec::new(),
            siacoin_outputs: Vec::new(),
            siafund_inputs: Vec::new(),
            siafund_outputs: Vec::new(),
            file_contracts: Vec::new(),
            file_contract_revisions: Vec::new(),
            file_contract_resolutions: Vec::new(),
            attestations: Vec::new(),
            arbitrary_data: ArbitraryData::default(),
            new_foundation_address: None,
            miner_fee: Currency::ZERO,
            fee_policy: None,
            basis: None,
        }
    }

    pub fn siacoin_inputs(mut self, inputs: Vec<SiacoinInputV2>) -> Self {
        self.siacoin_inputs = inputs;
        self
    }

    pub fn siacoin_outputs(mut self, outputs: Vec<SiacoinOutput>) -> Self {
        self.siacoin_outputs = outputs;
        self
    }

    pub fn siafund_inputs(mut self, inputs: Vec<SiafundInputV2>) -> Self {
        self.siafund_inputs = inputs;
        self
    }

    pub fn siafund_outputs(mut self, outputs: Vec<SiafundOutput>) -> Self {
        self.siafund_outputs = outputs;
        self
    }

    pub fn file_contracts(mut self, contracts: Vec<V2FileContract>) -> Self {
        self.file_contracts = contracts;
        self
    }

    pub fn file_contract_revisions(mut self, revisions: Vec<FileContractRevisionV2>) -> Self {
        self.file_contract_revisions = revisions;
        self
    }

    pub fn file_contract_resolutions(mut self, resolutions: Vec<V2FileContractResolution>) -> Self {
        self.file_contract_resolutions = resolutions;
        self
    }

    pub fn attestations(mut self, attestations: Vec<Attestation>) -> Self {
        self.attestations = attestations;
        self
    }

    pub fn arbitrary_data(mut self, data: ArbitraryData) -> Self {
        self.arbitrary_data = data;
        self
    }

    pub fn new_foundation_address(mut self, address: Address) -> Self {
        self.new_foundation_address = Some(address);
        self
    }

    pub fn miner_fee(mut self, fee: Currency) -> Self {
        self.miner_fee = fee;
        self
    }

    /**
     * "weight" is the size of the transaction in bytes. This can be used to estimate miner fees.
     * The recommended method for calculating a suitable fee is to multiply the response of
     * `/txpool/fee` API endpoint and the weight to get the fee in hastings.
     */
    pub fn weight(&self) -> u64 {
        let mut encoder = Encoder::default();
        self.encode(&mut encoder);
        encoder.buffer.len() as u64
    }

    /* Input is a special case becuase we cannot generate signatures until after fully constructing
    the transaction. Only the parent field is utilized while encoding the transaction to
    calculate the signature hash.
    Policy is included here to give any signing function or method a schema for producing a
    signature for the input. Do not use this method if you are manually creating SatisfiedPolicys.
    Use siacoin_inputs() to add fully formed inputs instead. */
    pub fn add_siacoin_input(mut self, parent: SiacoinElement, policy: SpendPolicy) -> Self {
        self.siacoin_inputs.push(SiacoinInputV2 {
            parent,
            satisfied_policy: SatisfiedPolicy {
                policy,
                signatures: Vec::new(),
                preimages: Vec::new(),
            },
        });
        self
    }

    /// Update the basis of the transaction. The basis is the ChainIndex required to broadcast the
    /// transaction.
    pub fn update_basis(mut self, basis: ChainIndex) -> Self {
        // Only update the basis if the new basis is higher than the existing basis.
        match &self.basis {
            Some(existing_basis) if existing_basis.height >= basis.height => {},
            _ => self.basis = Some(basis),
        }
        self
    }

    pub fn add_siacoin_input_with_basis(self, parent: UtxoWithBasis, policy: SpendPolicy) -> Self {
        self.add_siacoin_input(parent.output, policy).update_basis(parent.basis)
    }

    pub fn add_siacoin_output(mut self, output: SiacoinOutput) -> Self {
        self.siacoin_outputs.push(output);
        self
    }

    pub fn input_sig_hash(&self) -> Hash256 {
        let mut encoder = Encoder::default();
        encoder.write_distinguisher("sig/input");
        encoder.write_u8(V2_REPLAY_PREFIX);
        self.encode(&mut encoder);
        encoder.hash()
    }

    // Sign all PublicKey or UnlockConditions policies with the provided keypairs
    // Incapable of handling threshold policies
    pub fn sign_simple(mut self, keypairs: Vec<&Keypair>) -> Self {
        let sig_hash = self.input_sig_hash();
        // let mut cloned = self;
        for keypair in keypairs {
            let sig = keypair.sign(&sig_hash.0);
            for si in &mut self.siacoin_inputs {
                match &si.satisfied_policy.policy {
                    SpendPolicy::PublicKey(pk) if pk == &keypair.public() => {
                        si.satisfied_policy.signatures.push(sig.clone())
                    },
                    SpendPolicy::UnlockConditions(uc) => {
                        for p in &uc.unlock_keys {
                            match p {
                                UnlockKey::Ed25519(pk) if pk == &keypair.public() => {
                                    si.satisfied_policy.signatures.push(sig.clone())
                                },
                                _ => (),
                            }
                        }
                    },
                    _ => (),
                }
            }
        }
        self
    }

    pub async fn fund_tx_single_source(
        self,
        client: &Client,
        source_public_key: &PublicKey,
    ) -> Result<Self, V2TransactionBuilderError> {
        Ok(client.fund_tx_single_source(self, &source_public_key).await?)
    }

    pub fn satisfy_atomic_swap_success(
        mut self,
        keypair: &Keypair,
        secret: Preimage,
        input_index: u32,
    ) -> Result<Self, V2TransactionBuilderError> {
        let sig_hash = self.input_sig_hash();
        let sig = keypair.sign(&sig_hash.0);

        // check input_index exists prior to indexing into the vector
        if self.siacoin_inputs.len() <= (input_index as usize) {
            return Err(V2TransactionBuilderError::SatisfySuccessIndexOutOfBounds {
                len: self.siacoin_inputs.len(),
                index: input_index,
            });
        }

        let htlc_input = &mut self.siacoin_inputs[input_index as usize];
        htlc_input.satisfied_policy.signatures.push(sig);
        htlc_input.satisfied_policy.preimages.push(secret);
        Ok(self)
    }

    pub fn satisfy_atomic_swap_refund(
        mut self,
        keypair: &Keypair,
        input_index: u32,
    ) -> Result<Self, V2TransactionBuilderError> {
        let sig_hash = self.input_sig_hash();
        let sig = keypair.sign(&sig_hash.0);

        // check input_index exists prior to indexing into the vector
        if self.siacoin_inputs.len() <= (input_index as usize) {
            return Err(V2TransactionBuilderError::SatisfyRefundIndexOutOfBounds {
                len: self.siacoin_inputs.len(),
                index: input_index,
            });
        }

        self.siacoin_inputs[input_index as usize]
            .satisfied_policy
            .signatures
            .push(sig);
        Ok(self)
    }

    /// Adds an appropriately sized change output to the given address if the change amount is greater
    /// than 0
    /// Use this only after all inputs, all outputs and miner_fee are appropriately set.
    pub fn add_change_output(self, address: &Address) -> Self {
        let mut cloned = self.clone();
        let inputs: Currency = self
            .siacoin_inputs
            .iter()
            .map(|vin| vin.parent.siacoin_output.value)
            .sum();

        let outputs: Currency = self.siacoin_outputs.iter().map(|vout| vout.value).sum();
        if outputs + self.miner_fee < inputs {
            let change_amount = inputs - outputs - self.miner_fee;
            cloned = self.add_siacoin_output((address, change_amount).into());
        };
        cloned
    }

    pub fn build(self) -> V2Transaction {
        V2Transaction {
            siacoin_inputs: self.siacoin_inputs,
            siacoin_outputs: self.siacoin_outputs,
            siafund_inputs: self.siafund_inputs,
            siafund_outputs: self.siafund_outputs,
            file_contracts: self.file_contracts,
            file_contract_revisions: self.file_contract_revisions,
            file_contract_resolutions: self.file_contract_resolutions,
            attestations: self.attestations,
            arbitrary_data: self.arbitrary_data,
            new_foundation_address: self.new_foundation_address,
            miner_fee: self.miner_fee,
            basis: self.basis,
        }
    }
}

impl Default for V2TransactionBuilder {
    fn default() -> Self {
        V2TransactionBuilder::new()
    }
}

/// FeePolicy is data optionally included in V2TransactionBuilder to allow easier fee calculation.
/// Sia fee calculation can be complex in comparison to a typical UTXO protocol because the fee paid
/// to the miner is not simply the sum of the inputs minus the sum of the outputs. Instead, the
/// miner fee is a distinct field within the transaction, `miner_fee`. This `miner_fee` field is part
/// of signature calculation. As a result, you can build a transaction, produce signatures and preimages
/// for the inputs only to find out that the miner_fee hastings/byte rate is lower than expected.
/// Therefore a precise hastings/byte calculation requires correctly estimating the size of all
/// satisfied inputs prior to producing signatures.
#[derive(Clone, Debug)]
pub enum FeePolicy {
    HastingsPerByte(Currency),
    HastingsFixed(Currency),
}
