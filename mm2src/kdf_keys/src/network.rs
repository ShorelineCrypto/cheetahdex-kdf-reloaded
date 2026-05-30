// Network discriminator — used by activation flows that key off
// mainnet / testnet / Komodo prefix tables.

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Network {
    Mainnet,
    Testnet,
    Komodo,
}
