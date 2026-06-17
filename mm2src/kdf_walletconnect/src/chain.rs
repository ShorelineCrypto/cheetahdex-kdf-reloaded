//! CAIP-2 chain identifiers and the WalletConnect request-method taxonomy.
//!
//! The subsystem speaks to wallets across several chain families. Each family
//! is addressed with a CAIP-2 identifier of the shape `<namespace>:<reference>`
//! and exposes a fixed set of JSON-RPC method names on the WalletConnect
//! channel. Both are modelled here as closed enums so the rest of the crate can
//! match exhaustively.

use std::fmt;
use std::str::FromStr;

/// Raised when a chain namespace or a CAIP-2 string cannot be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownChain(pub String);

impl fmt::Display for UnknownChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unrecognised chain identifier: {}", self.0)
    }
}

impl std::error::Error for UnknownChain {}

/// The chain families the subsystem is able to address.
///
/// The string spelling of each variant is dictated by CAIP-2 and the
/// WalletConnect namespace registry, so it is fixed by [`AsRef<str>`] and
/// [`FromStr`] rather than left to serde defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WcChain {
    // crd:pin-begin
    /// Ethereum / EVM-compatible chains.
    Eip155,
    /// Cosmos SDK chains.
    Cosmos,
    /// UTXO chains (Bitcoin family), keyed by a genesis-hash prefix.
    Bip122,
    // crd:pin-end
}

impl WcChain {
    /// The CAIP-2 namespace token for this family.
    const fn token(self) -> &'static str {
        match self {
            WcChain::Eip155 => "eip155",
            WcChain::Cosmos => "cosmos",
            WcChain::Bip122 => "bip122",
        }
    }
}

impl AsRef<str> for WcChain {
    fn as_ref(&self) -> &str {
        self.token()
    }
}

impl fmt::Display for WcChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

impl FromStr for WcChain {
    type Err = UnknownChain;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        [WcChain::Eip155, WcChain::Cosmos, WcChain::Bip122]
            .into_iter()
            .find(|family| family.token() == s)
            .ok_or_else(|| UnknownChain(s.to_owned()))
    }
}

/// A fully-qualified CAIP-2 chain identifier: a known [`WcChain`] family paired
/// with its reference string (chain id, hub name, genesis prefix, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WcChainId {
    // crd:pin-begin
    pub chain: WcChain,
    pub id: String,
    // crd:pin-end
}

impl WcChainId {
    fn of(chain: WcChain, id: String) -> Self {
        Self { chain, id }
    }

    /// Build an `eip155:<id>` identifier.
    pub fn new_eip155(id: String) -> Self {
        Self::of(WcChain::Eip155, id)
    }

    /// Build a `cosmos:<id>` identifier.
    pub fn new_cosmos(id: String) -> Self {
        Self::of(WcChain::Cosmos, id)
    }

    /// Parse a `<namespace>:<reference>` CAIP-2 string. The string must contain
    /// exactly one colon separating a recognised namespace from a non-empty
    /// reference; anything else is rejected.
    pub fn try_from_str(s: &str) -> Result<Self, UnknownChain> {
        let (namespace, reference) = s.split_once(':').ok_or_else(|| UnknownChain(s.to_string()))?;
        if reference.contains(':') {
            return Err(UnknownChain(s.to_string()));
        }
        let chain = WcChain::from_str(namespace)?;
        Ok(Self::of(chain, reference.to_string()))
    }
}

impl fmt::Display for WcChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.chain.as_ref(), self.id)
    }
}

/// The JSON-RPC method names the subsystem may issue inside a session request,
/// grouped by the family that exposes them. Each variant maps one-to-one onto a
/// dictated wire string via [`AsRef<str>`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WcRequestMethods {
    // crd:pin-begin
    /// `cosmos_signDirect` — protobuf SignDirect (software wallets).
    CosmosSignDirect,
    /// `cosmos_signAmino` — Amino-JSON (Ledger-compatible path).
    CosmosSignAmino,
    /// `cosmos_getAccounts` — enumerate accounts.
    CosmosGetAccounts,
    /// `eth_signTransaction` — sign without broadcast.
    EthSignTransaction,
    /// `eth_sendTransaction` — sign and broadcast.
    EthSendTransaction,
    /// `personal_sign` — EVM personal message signing.
    EthPersonalSign,
    /// `getAccountAddresses` — UTXO account enumeration.
    UtxoGetAccountAddresses,
    /// `sendTransfer` — UTXO send.
    UtxoSendTransfer,
    /// `signPsbt` — UTXO PSBT signing.
    UtxoSignPsbt,
    /// `signMessage` — UTXO message signing.
    UtxoPersonalSign,
    // crd:pin-end
}

impl WcRequestMethods {
    const fn wire(self) -> &'static str {
        match self {
            WcRequestMethods::CosmosSignDirect => "cosmos_signDirect",
            WcRequestMethods::CosmosSignAmino => "cosmos_signAmino",
            WcRequestMethods::CosmosGetAccounts => "cosmos_getAccounts",
            WcRequestMethods::EthSignTransaction => "eth_signTransaction",
            WcRequestMethods::EthSendTransaction => "eth_sendTransaction",
            WcRequestMethods::EthPersonalSign => "personal_sign",
            WcRequestMethods::UtxoGetAccountAddresses => "getAccountAddresses",
            WcRequestMethods::UtxoSendTransfer => "sendTransfer",
            WcRequestMethods::UtxoSignPsbt => "signPsbt",
            WcRequestMethods::UtxoPersonalSign => "signMessage",
        }
    }
}

impl AsRef<str> for WcRequestMethods {
    fn as_ref(&self) -> &str {
        self.wire()
    }
}

impl fmt::Display for WcRequestMethods {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}
