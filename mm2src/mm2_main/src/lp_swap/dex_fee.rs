//! DEX fee computation helpers.
//!
//! All fee parameters (rates, discounts, thresholds, burn split) are sourced
//! from [`mm2_net_config::NetConfig`] keyed on the active netid. None of the
//! values are hard-coded here — adding a new network is a matter of adding
//! a new module under `mm2_net_config/src/`.
//!
//! This module is purely arithmetic: it produces a `DexFee` value but does
//! not resolve a destination address. Address resolution happens at the
//! coin-side (see for example `siacoin::SiaCoinBuilder`).

use coins::{DexFee, MmCoinEnum};
use common::mm_number::MmNumber;
use common::var;
use mm2_net_config::NetConfig;

/// Returns the effective DEX-fee floor for a swap.
///
/// Whichever is larger of the network's configured minimum DEX fee
/// (`NetConfig::dex_fee_min_threshold`) and the taker coin's `min_tx_amount`.
pub(crate) fn dex_fee_threshold(net_cfg: &dyn NetConfig, min_tx_amount: MmNumber) -> MmNumber {
    let min_fee: MmNumber = net_cfg.dex_fee_min_threshold().into();
    if min_fee < min_tx_amount {
        min_tx_amount
    } else {
        min_fee
    }
}

/// Returns the DEX fee rate (base or discounted) for a `(base, rel)` pair.
///
/// The discount applies if either ticker is in
/// `NetConfig::fee_discount_tickers()`.
pub(crate) fn dex_fee_rate(net_cfg: &dyn NetConfig, base: &str, rel: &str) -> MmNumber {
    let discount_tickers: &[&str] = if cfg!(test) && var("MYCOIN_FEE_DISCOUNT").is_ok() {
        // In tests, also give discount to MYCOIN (alongside the netid-configured tickers)
        // This is a test-only workaround; the NetConfig discount tickers are authoritative.
        let configured = net_cfg.fee_discount_tickers();
        if configured.contains(&"MYCOIN") {
            configured
        } else {
            // Fall back to checking both configured tickers and MYCOIN
            if configured.contains(&base) || configured.contains(&rel) || base == "MYCOIN" || rel == "MYCOIN" {
                return net_cfg.dex_fee_rate_discounted().into();
            } else {
                return net_cfg.dex_fee_rate().into();
            }
        }
    } else {
        net_cfg.fee_discount_tickers()
    };
    if discount_tickers.contains(&base) || discount_tickers.contains(&rel) {
        net_cfg.dex_fee_rate_discounted().into()
    } else {
        net_cfg.dex_fee_rate().into()
    }
}

/// Returns the DEX fee amount for a trade, with the threshold floor applied.
pub fn dex_fee_amount(
    net_cfg: &dyn NetConfig,
    base: &str,
    rel: &str,
    trade_amount: &MmNumber,
    dex_fee_threshold: &MmNumber,
) -> MmNumber {
    let rate = dex_fee_rate(net_cfg, base, rel);
    let fee_amount = trade_amount * &rate;
    if &fee_amount < dex_fee_threshold {
        dex_fee_threshold.clone()
    } else {
        fee_amount
    }
}

/// Convenience: compute `dex_fee_amount` deriving the threshold from
/// `taker_coin.min_tx_amount()`.
pub fn dex_fee_amount_from_taker_coin(
    net_cfg: &dyn NetConfig,
    taker_coin: &MmCoinEnum,
    maker_coin: &str,
    trade_amount: &MmNumber,
) -> MmNumber {
    let min_tx_amount = MmNumber::from(taker_coin.min_tx_amount());
    let threshold = dex_fee_threshold(net_cfg, min_tx_amount);
    dex_fee_amount(net_cfg, taker_coin.ticker(), maker_coin, trade_amount, &threshold)
}

/// Computes the full [`DexFee`] for a taker swap, applying the burn split
/// from the network configuration.
///
/// If `NetConfig::burn_enabled()` is false, returns `DexFee::Standard`.
/// Otherwise, splits the total fee according to `NetConfig::dex_fee_share()`:
///   - `fee_amount = total * share` (goes to DEX fee address)
///   - `burn_amount = total - fee_amount` (goes to OP_RETURN / burn address)
///
/// The burn destination is `KmdOpReturn` for KMD, `PreBurnAccount` for others.
pub fn compute_dex_fee(
    net_cfg: &dyn NetConfig,
    taker_coin: &MmCoinEnum,
    maker_coin: &str,
    trade_amount: &MmNumber,
) -> DexFee {
    let total = dex_fee_amount_from_taker_coin(net_cfg, taker_coin, maker_coin, trade_amount);
    DexFee::new_from_taker_coin(&**taker_coin, net_cfg, total)
}
