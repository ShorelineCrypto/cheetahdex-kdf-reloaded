//! App identity advertised to wallets during session proposal.

use relay_rpc::rpc::params::Metadata;

/// The default relay address the subsystem connects to.
pub const RELAY_ADDRESS: &str = "wss://relay.walletconnect.com";

/// The relay transport protocol we negotiate (IRN).
pub const SUPPORTED_RELAY_PROTOCOL: &str = "irn";

/// Human-facing application name presented to the wallet.
pub const APP_NAME: &str = "Komodo DeFi Framework";

/// Application description presented to the wallet.
pub const APP_DESCRIPTION: &str = "Komodo DeFi Framework WalletConnect client";

/// Canonical application URL.
pub const APP_URL: &str = "https://komodoplatform.com";

/// Builds the [`Metadata`] block advertised in a session proposal.
pub fn generate_metadata() -> Metadata {
    Metadata {
        name: APP_NAME.to_string(),
        description: APP_DESCRIPTION.to_string(),
        url: APP_URL.to_string(),
        icons: Vec::new(),
    }
}
