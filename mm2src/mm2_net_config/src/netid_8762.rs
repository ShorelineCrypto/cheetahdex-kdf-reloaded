//! Network configuration for netid 8762 — original AtomicDEX network.
//!
//! Fee parameters match the original KomoDeFi codebase:
//! - Base rate:  1/777  (~0.129%)
//! - KMD rate:   9/7770 (~0.116%, 10% discount)
//! - No burn mechanism

use lazy_static::lazy_static;
use num_rational::BigRational;

use crate::NetConfig;

/// DEX fee recipient public key (compressed, hex).
const DEX_FEE_ADDR_PUBKEY: &str = "03bc2c7ba671bae4a6fc835244c9762b41647b9827d4780a89a949b984a8ddcc06";

/// Z-address for shielded DEX fee (Zcash-based coins).
const DEX_FEE_Z_ADDR: &str = "zs1rp6426e9r6jkq2nsanl66tkd34enewrmr0uvj0zelhkcwmsy0uvxz2fhm9eu9rl3ukxvgzy2v9f";

/// Seed nodes for P2P bootstrapping on netid 8762.
const SEED_NODES: &[&str] = &["seed1.defimania.live", "seed2.defimania.live", "seed3.defimania.live"];

lazy_static! {
    static ref DEX_FEE_ADDR_RAW: Vec<u8> =
        hex::decode(DEX_FEE_ADDR_PUBKEY).expect("netid_8762: invalid DEX_FEE_ADDR_PUBKEY hex");
}

pub struct Netid8762;

impl NetConfig for Netid8762 {
    fn netid(&self) -> u16 {
        8762
    }

    fn network_name(&self) -> &'static str {
        "AtomicDEX"
    }

    fn dex_fee_addr_pubkey(&self) -> &'static str {
        DEX_FEE_ADDR_PUBKEY
    }

    fn dex_fee_addr_raw_pubkey(&self) -> &'static [u8] {
        &DEX_FEE_ADDR_RAW
    }

    fn dex_fee_z_addr(&self) -> &'static str {
        DEX_FEE_Z_ADDR
    }

    fn dex_fee_rate(&self) -> BigRational {
        // 1/777 ≈ 0.00129%
        BigRational::new(1.into(), 777.into())
    }

    fn fee_discount_tickers(&self) -> &'static [&'static str] {
        &["KMD"]
    }

    fn dex_fee_rate_discounted(&self) -> BigRational {
        // 9/7770 ≈ 0.00116% (1/777 minus 10%)
        BigRational::new(9.into(), 7770.into())
    }

    fn dex_fee_min_threshold(&self) -> BigRational {
        // 0.0001
        BigRational::new(1.into(), 10000.into())
    }

    fn seed_nodes(&self) -> &'static [&'static str] {
        SEED_NODES
    }
}
