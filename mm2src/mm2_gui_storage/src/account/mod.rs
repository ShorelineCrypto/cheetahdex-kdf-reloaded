use crate::account::storage::AccountStorageError;
use mm2_err_handle::prelude::*;
use mm2_number::BigDecimal;
use rpc::v1::types::H160 as H160Json;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::BTreeSet;

pub(crate) mod storage;

pub const MAX_ACCOUNT_NAME_LENGTH: usize = 255;
pub const MAX_ACCOUNT_DESCRIPTION_LENGTH: usize = 600;
pub const MAX_TICKER_LENGTH: usize = 255;

pub(crate) type HwPubkey = H160Json;

#[derive(Clone, Copy, Debug, Deserialize_repr, Serialize_repr)]
#[repr(u8)]
pub(crate) enum AccountType {
    // crd:pin-begin
    Iguana = 0,
    HD = 1,
    HW = 2,
    // crd:pin-end
}

impl TryFrom<i64> for AccountType {
    type Error = MmError<AccountStorageError>;

    /// Decodes the persisted discriminant. The integer encoding is part of the
    /// on-disk contract (`Iguana = 0`, `HD = 1`, `HW = 2`) and must not drift.
    fn try_from(raw: i64) -> Result<Self, Self::Error> {
        match raw {
            // crd:pin-begin
            0 => Ok(AccountType::Iguana),
            1 => Ok(AccountType::HD),
            2 => Ok(AccountType::HW),
            // crd:pin-end
            other => MmError::err(AccountStorageError::ErrorDeserializing(format!(
                "Unexpected 'account_type' discriminant: {other}"
            ))),
        }
    }
}

/// Identity kinds that may legally become the active account.
/// Hardware-wallet identities are intentionally excluded (see chapter 24 R2).
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(u8)]
pub(crate) enum EnabledAccountType {
    // crd:pin-begin
    Iguana = 0,
    HD = 1,
    // crd:pin-end
}

impl TryFrom<i64> for EnabledAccountType {
    type Error = MmError<AccountStorageError>;

    fn try_from(raw: i64) -> Result<Self, Self::Error> {
        // Decode through the full set first, then forbid the one kind that may
        // never become the active account.
        match AccountType::try_from(raw)? {
            // crd:pin-begin
            AccountType::Iguana => Ok(EnabledAccountType::Iguana),
            AccountType::HD => Ok(EnabledAccountType::HD),
            AccountType::HW => MmError::err(AccountStorageError::ErrorDeserializing(
                "A hardware-wallet account is not eligible to be enabled".to_string(),
            )),
            // crd:pin-end
        }
    }
}

impl From<EnabledAccountType> for AccountType {
    fn from(enabled: EnabledAccountType) -> Self {
        match enabled {
            // crd:pin-begin
            EnabledAccountType::Iguana => AccountType::Iguana,
            EnabledAccountType::HD => AccountType::HD,
            // crd:pin-end
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "type")] // crd:pin
#[serde(rename_all = "lowercase")] // crd:pin
pub enum AccountId {
    // crd:pin-begin
    Iguana,
    HD { account_idx: u32 },
    HW { device_pubkey: HwPubkey },
    // crd:pin-end
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "type")] // crd:pin
#[serde(rename_all = "lowercase")] // crd:pin
pub enum EnabledAccountId {
    // crd:pin-begin
    Iguana,
    HD { account_idx: u32 },
    // crd:pin-end
}

impl From<EnabledAccountId> for AccountId {
    fn from(enabled: EnabledAccountId) -> Self {
        // Widening the enabled subset back into the full identity space is total:
        // every enabled variant has a structurally identical `AccountId` counterpart.
        match enabled {
            // crd:pin-begin
            EnabledAccountId::Iguana => AccountId::Iguana,
            EnabledAccountId::HD { account_idx } => AccountId::HD { account_idx },
            // crd:pin-end
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AccountInfo {
    // crd:pin-begin
    pub(crate) account_id: AccountId,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) balance_usd: BigDecimal,
    // crd:pin-end
}

#[derive(Debug, PartialEq, Serialize)]
pub struct AccountWithEnabledFlag {
    #[serde(flatten)] // crd:pin
    pub(crate) account_info: AccountInfo,
    /// Marks the currently active account. At most one record in a listing
    /// carries `true`.
    pub(crate) enabled: bool, // crd:pin
}

#[derive(Debug, PartialEq, Serialize)]
pub struct AccountWithCoins {
    #[serde(flatten)] // crd:pin
    pub(crate) account_info: AccountInfo,
    pub(crate) coins: BTreeSet<String>, // crd:pin
}
