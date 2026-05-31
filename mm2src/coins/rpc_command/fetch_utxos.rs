use bigdecimal::BigDecimal;
use common::HttpStatusCode;
use derive_more::Display;
use http::StatusCode;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

use crate::{lp_coinfind_or_err,
            utxo::{utxo_common::big_decimal_from_sat_unsigned, GetUtxoListOps},
            CoinFindError, DerivationMethod, MmCoinEnum};

#[derive(Deserialize)]
pub struct FetchUtxosRequest {
    pub coin: String,
}

#[derive(Serialize)]
pub struct AddressUtxos {
    pub address: String,
    pub count: usize,
    pub utxos: Vec<UnspentOutputs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derivation_path: Option<String>,
}

#[derive(Serialize)]
pub struct FetchUtxosResponse {
    pub total_count: usize,
    pub addresses: Vec<AddressUtxos>,
}

#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum FetchUtxosError {
    NoSuchCoin,
    CoinNotSupported,
    InvalidAddress(String),
    Internal(String),
}

impl HttpStatusCode for FetchUtxosError {
    fn status_code(&self) -> StatusCode {
        match self {
            FetchUtxosError::NoSuchCoin => StatusCode::NOT_FOUND,
            FetchUtxosError::CoinNotSupported => StatusCode::BAD_REQUEST,
            FetchUtxosError::InvalidAddress(_) => StatusCode::BAD_REQUEST,
            FetchUtxosError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<CoinFindError> for FetchUtxosError {
    fn from(e: CoinFindError) -> Self {
        match e {
            CoinFindError::NoSuchCoin { .. } => FetchUtxosError::NoSuchCoin,
        }
    }
}

#[derive(Serialize)]
pub struct UnspentOutputs {
    txid: String,
    vout: u32,
    value: BigDecimal,
}

pub async fn fetch_utxos_rpc(ctx: MmArc, req: FetchUtxosRequest) -> MmResult<FetchUtxosResponse, FetchUtxosError> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await.map_mm_err()?;

    match coin {
        MmCoinEnum::UtxoCoin(ref coin) => match &coin.as_ref().derivation_method {
            DerivationMethod::Iguana(my_address) => {
                let (unspents, _) = coin
                    .get_unspent_ordered_list(my_address)
                    .await
                    .mm_err(|e| FetchUtxosError::Internal(format!("Couldn't fetch unspent UTXOs: {e}")))?;

                let decimals = coin.as_ref().decimals;
                let addresses_utxos = if unspents.is_empty() {
                    vec![]
                } else {
                    vec![AddressUtxos {
                        address: format!("{}", my_address),
                        count: unspents.len(),
                        utxos: unspents
                            .into_iter()
                            .map(|u| UnspentOutputs {
                                txid: u.outpoint.hash.reversed().to_string(),
                                vout: u.outpoint.index,
                                value: big_decimal_from_sat_unsigned(u.value, decimals),
                            })
                            .collect(),
                        derivation_path: None,
                    }]
                };

                let total_count = addresses_utxos.iter().map(|a| a.count).sum();
                Ok(FetchUtxosResponse {
                    total_count,
                    addresses: addresses_utxos,
                })
            },
            DerivationMethod::HDWallet(_) => Err(FetchUtxosError::Internal(
                "HD wallet UTXO fetching requires derived address cache (not yet supported)".to_string(),
            )
            .into()),
        },
        _ => Err(FetchUtxosError::CoinNotSupported.into()),
    }
}
