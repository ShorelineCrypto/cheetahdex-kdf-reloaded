//! Pairing lifecycle: fresh symmetric material and the `wc:` URI shown to the
//! user.

use crate::metadata::SUPPORTED_RELAY_PROTOCOL;
use rand::Rng;
use relay_rpc::domain::Topic;
use wc_common::SymKey;

/// A pairing: a relay topic plus the symmetric key both peers use before a
/// session is settled.
#[derive(Clone)]
pub struct Pairing {
    pub topic: Topic,
    pub sym_key: SymKey,
    pub expiry: u64,
}

impl Pairing {
    /// Generates a brand new pairing with a random topic and symmetric key,
    /// expiring `ttl_secs` from now.
    pub fn generate(ttl_secs: u64) -> Self {
        let sym_key: SymKey = rand::thread_rng().gen();
        let expiry = unix_now().saturating_add(ttl_secs);
        Pairing {
            topic: Topic::generate(),
            sym_key,
            expiry,
        }
    }

    /// The `wc:<topic>@2?...` URI to display to the user.
    pub fn uri(&self) -> String { build_pairing_uri(&self.topic, &self.sym_key, self.expiry) }
}

/// Formats a WalletConnect v2 pairing URI.
pub fn build_pairing_uri(topic: &Topic, sym_key: &SymKey, expiry: u64) -> String {
    format!(
        "wc:{}@2?relay-protocol={}&symKey={}&expiryTimestamp={}",
        topic.as_ref(),
        SUPPORTED_RELAY_PROTOCOL,
        hex::encode(sym_key),
        expiry,
    )
}

/// Current unix timestamp in seconds.
fn unix_now() -> u64 { chrono::Utc::now().timestamp().max(0) as u64 }
