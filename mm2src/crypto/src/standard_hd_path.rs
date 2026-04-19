/// Type-safe HD derivation paths supporting BIP-44/49/84 standards.
///
/// Replaces the old bip44-only path types with generalized `StandardHDPath` that
/// supports multiple BIP43 purposes (BIP32, BIP44, BIP49, BIP84).
///
/// Path structure: `m/purpose'/coin_type'/account'/change/address_index`
///
/// # Type Aliases
/// - `StandardHDPath` — Full 5-level path (purpose through address_index)
/// - `HDPathToCoin` — First 2 levels (purpose + coin_type)
/// - `HDPathToAccount` — First 3 levels (purpose through account_id)
use crate::bip32_child::{
    Bip32Child, Bip32ChildValue, Bip32DerPathError, Bip44Tail, HardenedValue, NonHardenedValue,
};
use crate::bip44::{Bip44Chain, Bip44ChainValue};
use bip32::ChildNumber;
use derive_more::Display;
use enum_primitive_derive::Primitive;
use hw_common::primitives::Bip32Error;
use num_traits::FromPrimitive;

/// Standard HD Path for [BIP-44](https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki),
/// [BIP-49](https://github.com/bitcoin/bips/blob/master/bip-0049.mediawiki),
/// [BIP-84](https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki)
/// and similar.
/// For path as `m/purpose'/coin_type'/account'/change/address_index`.
#[rustfmt::skip]
pub type StandardHDPath =
    Bip32Child<Bip32PurposeValue, // `purpose`
    Bip32Child<HardenedValue, // `coin_type`
    Bip32Child<HardenedValue, // `account_id`
    Bip32Child<Bip44ChainValue, // `chain`
    Bip32Child<NonHardenedValue, // `address_id`
    Bip44Tail>>>>>;

/// Path down to the coin_type level: `m/purpose'/coin_type'`
#[rustfmt::skip]
pub type HDPathToCoin =
    Bip32Child<Bip32PurposeValue, // `purpose`
    Bip32Child<HardenedValue, // `coin_type`
    Bip44Tail>>;

/// Path down to the account level: `m/purpose'/coin_type'/account_id'`
#[rustfmt::skip]
pub type HDPathToAccount =
    Bip32Child<Bip32PurposeValue, // `purpose`
    Bip32Child<HardenedValue, // `coin_type`
    Bip32Child<HardenedValue, // `account_id`
    Bip44Tail>>>;

impl StandardHDPath {
    pub fn purpose(&self) -> Bip43Purpose {
        self.value()
    }

    pub fn coin_type(&self) -> u32 {
        self.child().value()
    }

    pub fn account_id(&self) -> u32 {
        self.child().child().value()
    }

    pub fn chain(&self) -> Bip44Chain {
        self.child().child().child().value()
    }

    pub fn address_id(&self) -> u32 {
        self.child().child().child().child().value()
    }

    /// Derive `HDPathToCoin` from `StandardHDPath` by taking just the first two levels.
    pub fn path_to_coin(&self) -> HDPathToCoin {
        let Bip32Child {
            value: purpose,
            child: rest,
        } = self;
        let Bip32Child { value: coin_type, .. } = rest;

        Bip32Child {
            value: purpose.clone(),
            child: Bip32Child {
                value: coin_type.clone(),
                child: Bip44Tail,
            },
        }
    }
}

impl HDPathToCoin {
    pub fn purpose(&self) -> Bip43Purpose {
        self.value()
    }

    pub fn coin_type(&self) -> u32 {
        self.child().value()
    }
}

impl HDPathToAccount {
    pub fn purpose(&self) -> Bip43Purpose {
        self.value()
    }

    pub fn coin_type(&self) -> u32 {
        self.child().value()
    }

    pub fn account_id(&self) -> u32 {
        self.child().child().value()
    }
}

/// Errors when parsing or constructing a standard HD path.
#[derive(Debug, Display, Eq, PartialEq)]
pub enum StandardHDPathError {
    #[display(fmt = "Invalid derivation path length '{}', expected '{}'", found, expected)]
    InvalidDerivationPathLength { expected: usize, found: usize },
    #[display(fmt = "Child '{}' is expected to be hardened", child)]
    ChildIsNotHardened { child: String },
    #[display(fmt = "Child '{}' is expected not to be hardened", child)]
    ChildIsHardened { child: String },
    #[display(fmt = "Unexpected '{}' child value '{}', expected: {}", child, value, expected)]
    UnexpectedChildValue {
        child: String,
        value: u32,
        expected: String,
    },
    #[display(fmt = "Unknown BIP32 error: {}", _0)]
    Bip32Error(Bip32Error),
    #[display(fmt = "Invalid coin type '{}', expected '{}'", found, expected)]
    InvalidCoinType { expected: u32, found: u32 },
    #[display(fmt = "Invalid path to coin '{}', expected '{}'", found, expected)]
    InvalidPathToCoin { expected: String, found: String },
}

impl From<Bip32DerPathError> for StandardHDPathError {
    fn from(e: Bip32DerPathError) -> Self {
        fn display_child_at(child_at: usize) -> String {
            StandardHDIndex::from_usize(child_at)
                .map(|index| format!("{index:?}"))
                .unwrap_or_else(|| "UNKNOWN".to_owned())
        }

        match e {
            Bip32DerPathError::InvalidDerivationPathLength { expected, found } => {
                StandardHDPathError::InvalidDerivationPathLength { expected, found }
            },
            Bip32DerPathError::ChildIsNotHardened { child_at } => StandardHDPathError::ChildIsNotHardened {
                child: display_child_at(child_at),
            },
            Bip32DerPathError::ChildIsHardened { child_at } => StandardHDPathError::ChildIsHardened {
                child: display_child_at(child_at),
            },
            Bip32DerPathError::UnexpectedChildValue {
                child_at,
                actual,
                expected,
            } => StandardHDPathError::UnexpectedChildValue {
                child: display_child_at(child_at),
                value: actual,
                expected,
            },
            Bip32DerPathError::Bip32Error(bip32) => StandardHDPathError::Bip32Error(bip32),
        }
    }
}

/// Index positions within a standard HD path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Primitive)]
pub enum StandardHDIndex {
    Purpose = 0,
    CoinType = 1,
    AccountId = 2,
    Chain = 3,
    AddressId = 4,
}

/// BIP43 purpose values. Determines the wallet derivation standard.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum Bip43Purpose {
    /// BIP32 generic derivation
    Bip32 = 32,
    /// BIP44 multi-account hierarchy
    Bip44 = 44,
    /// BIP49 P2WPKH-nested-in-P2SH
    Bip49 = 49,
    /// BIP84 native segwit (P2WPKH)
    Bip84 = 84,
}

/// A [`Bip32ChildValue`] for the purpose level, always hardened.
#[derive(Clone, PartialEq)]
pub struct Bip32PurposeValue {
    purpose: Bip43Purpose,
}

impl Bip32ChildValue for Bip32PurposeValue {
    type Value = Bip43Purpose;

    /// `purpose` is always a hardened child as described in the BIP44/BIP49/BIP84 standards.
    fn hardened() -> bool {
        true
    }

    fn number(&self) -> u32 {
        self.purpose as u32
    }

    fn value(&self) -> Bip43Purpose {
        self.purpose
    }

    fn from_bip32_number(child_number: ChildNumber, child_at: usize) -> Result<Self, Bip32DerPathError> {
        if !child_number.is_hardened() {
            return Err(Bip32DerPathError::ChildIsNotHardened { child_at });
        }
        let purpose = match child_number.index() {
            32 => Bip43Purpose::Bip32,
            44 => Bip43Purpose::Bip44,
            49 => Bip43Purpose::Bip49,
            84 => Bip43Purpose::Bip84,
            _ => {
                return Err(Bip32DerPathError::UnexpectedChildValue {
                    child_at,
                    actual: child_number.0,
                    expected: "one of the following: 32, 44, 49, 84".to_string(),
                })
            },
        };

        Ok(Bip32PurposeValue { purpose })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bip32_child::Bip32DerPathOps;
    use bip32::DerivationPath;
    use std::str::FromStr;

    #[test]
    fn test_from_str() {
        let der_path = StandardHDPath::from_str("m/44'/141'/1'/0/10").unwrap();
        assert_eq!(der_path.coin_type(), 141);
        assert_eq!(der_path.account_id(), 1);
        assert_eq!(der_path.chain(), Bip44Chain::External);
        assert_eq!(der_path.address_id(), 10);
    }

    #[test]
    fn test_display() {
        let der_path = HDPathToAccount::from_str("m/44'/141'/1'").unwrap();
        let actual = format!("{der_path}");
        assert_eq!(actual, "m/44'/141'/1'");
    }

    #[test]
    fn test_derive() {
        let der_path_to_coin = HDPathToCoin::from_str("m/44'/141'").unwrap();
        let der_path_to_account: HDPathToAccount =
            der_path_to_coin.derive(ChildNumber::new(10, true).unwrap()).unwrap();
        assert_eq!(
            der_path_to_account.to_derivation_path(),
            DerivationPath::from_str("m/44'/141'/10'").unwrap()
        );
    }

    #[test]
    fn test_from_invalid_length() {
        let error = StandardHDPath::from_str("m/44'/141'/0'").expect_err("derivation path is too short");
        assert_eq!(
            error,
            Bip32DerPathError::InvalidDerivationPathLength { expected: 5, found: 3 }
        );
    }

    #[test]
    fn test_purposes() {
        let path = StandardHDPath::from_str("m/32'/141'/0'/0/0").unwrap();
        assert_eq!(path.purpose(), Bip43Purpose::Bip32);

        let path = StandardHDPath::from_str("m/44'/141'/0'/0/0").unwrap();
        assert_eq!(path.purpose(), Bip43Purpose::Bip44);

        let path = StandardHDPath::from_str("m/49'/141'/0'/0/0").unwrap();
        assert_eq!(path.purpose(), Bip43Purpose::Bip49);

        let path = StandardHDPath::from_str("m/84'/141'/0'/0/0").unwrap();
        assert_eq!(path.purpose(), Bip43Purpose::Bip84);
    }

    #[test]
    fn test_path_to_coin() {
        let full = StandardHDPath::from_str("m/44'/141'/0'/0/5").unwrap();
        let coin_path = full.path_to_coin();
        assert_eq!(coin_path.purpose(), Bip43Purpose::Bip44);
        assert_eq!(coin_path.coin_type(), 141);
    }
}
