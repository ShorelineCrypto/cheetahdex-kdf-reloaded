use crate::account::storage::{AccountStorage, AccountStorageError, AccountStorageResult};
use crate::account::{
    AccountId,
    AccountInfo,
    AccountType,
    AccountWithCoins,
    AccountWithEnabledFlag,
    EnabledAccountId, // crd:pin
    EnabledAccountType,
    HwPubkey,
    MAX_ACCOUNT_DESCRIPTION_LENGTH,
    MAX_ACCOUNT_NAME_LENGTH,
    MAX_TICKER_LENGTH, // crd:pin
};
use async_trait::async_trait;
use db_common::foreign_columns;
use db_common::sql_build::*;
use db_common::sqlite::rusqlite::types::Type;
use db_common::sqlite::rusqlite::{Connection, Error as SqlError, Result as SqlResult, Row};
use db_common::sqlite::{is_constraint_error, SqliteConnShared};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_number::BigDecimal;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::{Arc, MutexGuard};

const DEVICE_PUBKEY_MAX_LENGTH: usize = 20; // crd:pin
const BALANCE_MAX_LENGTH: usize = 255; // crd:pin

mod account_table {
    // crd:pin-begin
    /// The table name.
    pub(super) const TABLE_NAME: &str = "gui_account";

    // The following constants are the column names.
    pub(super) const ACCOUNT_TYPE: &str = "account_type";
    pub(super) const ACCOUNT_IDX: &str = "account_idx";
    pub(super) const DEVICE_PUBKEY: &str = "device_pubkey";
    pub(super) const NAME: &str = "name";
    pub(super) const DESCRIPTION: &str = "description";
    pub(super) const BALANCE_USD: &str = "balance_usd";

    /// The table PRIMARY KEY name.
    pub(super) const ACCOUNT_ID_PRIMARY_KEY: &str = "account_id_primary";
    // crd:pin-end
}

mod account_coins_table {
    // crd:pin-begin
    /// The table name.
    pub(super) const TABLE_NAME: &str = "gui_account_coins";

    // The following constants are the column names.
    pub(super) const ACCOUNT_TYPE: &str = "account_type";
    pub(super) const ACCOUNT_IDX: &str = "account_idx";
    pub(super) const DEVICE_PUBKEY: &str = "device_pubkey";
    pub(super) const COIN: &str = "coin";

    /// The table UNIQUE constraint.
    pub(super) const ACCOUNT_ID_COIN_CONSTRAINT: &str = "account_id_coin_constraint";
    // crd:pin-end
}

mod enabled_account_table {
    // crd:pin-begin
    /// The table name.
    pub(super) const TABLE_NAME: &str = "gui_account_enabled";

    // The following constants are the column names.
    pub(super) const ACCOUNT_TYPE: &str = "account_type";
    pub(super) const ACCOUNT_IDX: &str = "account_idx";
    pub(super) const DEVICE_PUBKEY: &str = "device_pubkey";
    // crd:pin-end
}

impl From<SqlError> for AccountStorageError {
    fn from(e: SqlError) -> Self {
        // Render the message before the match consumes `e`.
        let message = e.to_string();
        // Sort the rusqlite failure into one of three storage buckets: value
        // decoding faults, value encoding faults, and a catch-all internal
        // bucket for anything else.
        match e {
            // crd:pin-begin
            SqlError::InvalidColumnType(..)
            | SqlError::InvalidColumnIndex(_)
            | SqlError::IntegralValueOutOfRange(..)
            | SqlError::FromSqlConversionFailure(..) => {
                // crd:pin-end
                AccountStorageError::ErrorDeserializing(message)
            },
            // crd:pin-begin
            SqlError::NulError(_) | SqlError::Utf8Error(_) | SqlError::ToSqlConversionFailure(_) => {
                // crd:pin-end
                AccountStorageError::ErrorSerializing(message)
            },
            _ => AccountStorageError::Internal(message),
        }
    }
}

impl AccountId {
    /// SQL-typed counterpart of [`AccountId::to_tuple`]. The device pubkey is
    /// rendered as lowercase hex with no `0x` prefix.
    fn to_sql_tuple(&self) -> (i64, i64, String) {
        let (account_type, account_idx, device_pubkey) = self.to_tuple();
        encode_sql_identity(account_type as i64, account_idx, &device_pubkey)
    }

    /// SQL-typed counterpart of [`AccountId::try_from_tuple`]. The hex pubkey is
    /// parsed back through [`HwPubkey::from_str`].
    // crd:pin-begin
    pub(crate) fn try_from_sql_tuple(
        account_type: i64,
        account_idx: u32,
        device_pubkey: &str,
    ) -> AccountStorageResult<AccountId> {
        // crd:pin-end
        let account_type = AccountType::try_from(account_type)?;
        let device_pubkey =
            HwPubkey::from_str(device_pubkey).map_to_mm(|e| AccountStorageError::ErrorDeserializing(e.to_string()))?;
        AccountId::try_from_tuple(account_type, account_idx, device_pubkey)
    }
}

impl EnabledAccountId {
    /// SQL-typed projection. The device-pubkey slot is always the sentinel,
    /// since no enabled variant is hardware-wallet keyed.
    fn to_sql_tuple(self) -> (i64, i64, String) {
        let (account_type, account_idx, device_pubkey) = self.to_tuple();
        encode_sql_identity(account_type as i64, account_idx, &device_pubkey)
    }

    /// SQL-typed reconstruction from the `(type, idx)` columns; any
    /// device-pubkey column is ignored.
    pub(crate) fn try_from_sql_pair(account_type: i64, account_idx: u32) -> AccountStorageResult<EnabledAccountId> {
        let account_type = EnabledAccountType::try_from(account_type)?;
        EnabledAccountId::try_from_pair(account_type, account_idx)
    }
}

pub(crate) struct SqliteAccountStorage {
    conn: SqliteConnShared,
}

impl SqliteAccountStorage {
    pub(crate) fn new(ctx: &MmArc) -> AccountStorageResult<SqliteAccountStorage> {
        // The crate shares the central context's single SQLite handle.
        let shared = ctx
            .sqlite_connection
            .as_option()
            .or_mm_err(|| AccountStorageError::Internal("'MmCtx::sqlite_connection' is not initialized".to_owned()))?;
        Ok(SqliteAccountStorage {
            conn: Arc::clone(shared),
        })
    }

    fn lock_conn_mutex(&self) -> AccountStorageResult<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_to_mm(|e| AccountStorageError::Internal(format!("Error locking sqlite connection: {e}")))
    }

    fn init_account_table(conn: &Connection) -> AccountStorageResult<()> {
        // crd:pin-begin
        let mut builder = SqlCreateTable::new(conn, account_table::TABLE_NAME);
        builder.if_not_exist();
        builder.column(SqlColumn::new(account_table::ACCOUNT_TYPE, SqlType::Integer).not_null());
        builder.column(SqlColumn::new(account_table::ACCOUNT_IDX, SqlType::Integer).not_null());
        builder.column(
            SqlColumn::new(account_table::DEVICE_PUBKEY, SqlType::Varchar(DEVICE_PUBKEY_MAX_LENGTH)).not_null(),
        );
        builder.column(SqlColumn::new(account_table::NAME, SqlType::Varchar(MAX_ACCOUNT_NAME_LENGTH)).not_null());
        // `description` is the single nullable column.
        builder.column(SqlColumn::new(
            account_table::DESCRIPTION,
            SqlType::Varchar(MAX_ACCOUNT_DESCRIPTION_LENGTH),
        ));
        builder.column(SqlColumn::new(account_table::BALANCE_USD, SqlType::Varchar(BALANCE_MAX_LENGTH)).not_null());
        // The three identity columns together are the primary key.
        builder.constraint(PrimaryKey::new(
            account_table::ACCOUNT_ID_PRIMARY_KEY,
            [
                account_table::ACCOUNT_TYPE,
                account_table::ACCOUNT_IDX,
                account_table::DEVICE_PUBKEY,
            ],
        )?);
        builder.create().map_to_mm(AccountStorageError::from)
        // crd:pin-end
    }

    fn init_account_coins_table(conn: &Connection) -> AccountStorageResult<()> {
        // crd:pin-begin
        let mut builder = SqlCreateTable::new(conn, account_coins_table::TABLE_NAME);
        builder.if_not_exist();
        builder.column(SqlColumn::new(account_coins_table::ACCOUNT_TYPE, SqlType::Integer).not_null());
        builder.column(SqlColumn::new(account_coins_table::ACCOUNT_IDX, SqlType::Integer).not_null());
        builder.column(
            SqlColumn::new(
                account_coins_table::DEVICE_PUBKEY,
                SqlType::Varchar(DEVICE_PUBKEY_MAX_LENGTH),
            )
            .not_null(),
        );
        builder.column(SqlColumn::new(account_coins_table::COIN, SqlType::Varchar(MAX_TICKER_LENGTH)).not_null());

        // The identity columns mirror the accounts-table primary key; a cascade
        // drops the activated tickers when the parent account is removed.
        let parent_fk = ForeignKey::new(
            foreign_key::ParentTable(account_table::TABLE_NAME),
            foreign_columns![
                account_coins_table::ACCOUNT_TYPE => account_table::ACCOUNT_TYPE,
                account_coins_table::ACCOUNT_IDX => account_table::ACCOUNT_IDX,
                account_coins_table::DEVICE_PUBKEY => account_table::DEVICE_PUBKEY
            ],
        )?
        .on_event(foreign_key::Event::OnDelete, foreign_key::Action::Cascade);
        builder.constraint(parent_fk);

        // No account may list the same ticker twice.
        builder.constraint(Unique::new(
            account_coins_table::ACCOUNT_ID_COIN_CONSTRAINT,
            [
                account_coins_table::ACCOUNT_TYPE,
                account_coins_table::ACCOUNT_IDX,
                account_coins_table::DEVICE_PUBKEY,
                account_coins_table::COIN,
            ],
        )?);
        builder.create().map_to_mm(AccountStorageError::from)
        // crd:pin-end
    }

    fn init_enabled_account_table(conn: &Connection) -> AccountStorageResult<()> {
        // crd:pin-begin
        let mut builder = SqlCreateTable::new(conn, enabled_account_table::TABLE_NAME);
        builder.if_not_exist();
        builder.column(SqlColumn::new(enabled_account_table::ACCOUNT_TYPE, SqlType::Integer).not_null());
        builder.column(SqlColumn::new(enabled_account_table::ACCOUNT_IDX, SqlType::Integer).not_null());
        // Carried purely so the cascading foreign key can span the full primary key.
        builder.column(
            SqlColumn::new(
                enabled_account_table::DEVICE_PUBKEY,
                SqlType::Varchar(DEVICE_PUBKEY_MAX_LENGTH),
            )
            .not_null(),
        );

        // Clearing the parent account clears its enabled marker too.
        let parent_fk = ForeignKey::new(
            foreign_key::ParentTable(account_table::TABLE_NAME),
            foreign_columns![
                enabled_account_table::ACCOUNT_TYPE => account_table::ACCOUNT_TYPE,
                enabled_account_table::ACCOUNT_IDX => account_table::ACCOUNT_IDX,
                enabled_account_table::DEVICE_PUBKEY => account_table::DEVICE_PUBKEY,
            ],
        )?
        .on_event(foreign_key::Event::OnDelete, foreign_key::Action::Cascade);
        builder.constraint(parent_fk);

        builder.create().map_to_mm(AccountStorageError::from)
        // crd:pin-end
    }

    /// Reads the single enabled-account marker, erroring with `NoEnabledAccount`
    /// when none has been set.
    fn load_enabled_account_id_or_err(conn: &Connection) -> AccountStorageResult<EnabledAccountId> {
        let mut select = SqlQuery::select_from(conn, enabled_account_table::TABLE_NAME)?;
        select.field(enabled_account_table::ACCOUNT_TYPE)?;
        select.field(enabled_account_table::ACCOUNT_IDX)?;
        select
            .query_single_row(enabled_account_id_from_row)?
            .or_mm_err(|| AccountStorageError::NoEnabledAccount)
    }

    /// Returns the activated tickers of `account_id`.
    ///
    /// Existence of the account is **not** verified here; an unknown account
    /// simply yields an empty set.
    fn load_account_coins(conn: &Connection, account_id: &AccountId) -> AccountStorageResult<BTreeSet<String>> {
        let mut select = SqlQuery::select_from(conn, account_coins_table::TABLE_NAME)?;
        select.field(account_coins_table::COIN)?;
        restrict_to_account(&mut select, COINS_TABLE_ID_COLUMNS, account_id.to_sql_tuple())?;

        let coins = select.query(|row| row.get::<_, String>(0))?.into_iter().collect();
        Ok(coins)
    }

    /// Loads an account together with its activated coins, sharing one
    /// connection so the two reads observe a consistent snapshot.
    fn load_account_with_coins(
        conn: &Connection,
        account_id: &AccountId,
    ) -> AccountStorageResult<Option<AccountWithCoins>> {
        Self::load_account(conn, account_id)?
            .map(|account_info| {
                let coins = Self::load_account_coins(conn, account_id)?;
                Ok(AccountWithCoins { account_info, coins })
            })
            .transpose()
    }

    /// Loads a single account record, or `None` when the identity is unknown.
    fn load_account(conn: &Connection, account_id: &AccountId) -> AccountStorageResult<Option<AccountInfo>> {
        let mut select = SqlQuery::select_from(conn, account_table::TABLE_NAME)?;
        // The select order must match the indices read by `account_from_row`.
        for column in ACCOUNT_COLUMNS {
            select.field(column)?;
        }
        restrict_to_account(&mut select, ACCOUNT_TABLE_ID_COLUMNS, account_id.to_sql_tuple())?;

        select
            .query_single_row(account_from_row)
            .map_to_mm(AccountStorageError::from)
    }

    fn load_accounts(conn: &Connection) -> AccountStorageResult<BTreeMap<AccountId, AccountInfo>> {
        let mut select = SqlQuery::select_from(conn, account_table::TABLE_NAME)?;
        for column in ACCOUNT_COLUMNS {
            select.field(column)?;
        }

        let accounts = select
            .query(account_from_row)?
            .into_iter()
            .map(|account| (account.account_id.clone(), account))
            .collect();
        Ok(accounts)
    }

    fn account_exists(conn: &Connection, account_id: &AccountId) -> AccountStorageResult<bool> {
        let mut select = SqlQuery::select_from(conn, account_table::TABLE_NAME)?;
        select.count(account_table::NAME)?;
        restrict_to_account(&mut select, ACCOUNT_TABLE_ID_COLUMNS, account_id.to_sql_tuple())?;

        select
            .query_single_row(count_from_row)?
            .or_mm_err(|| AccountStorageError::Internal("'COUNT' query unexpectedly returned no row".to_string()))
            .map(|count| count > 0)
    }

    fn upload_account(conn: &Connection, account: AccountInfo) -> AccountStorageResult<()> {
        let mut insert = SqlInsert::new(conn, account_table::TABLE_NAME);
        write_account_columns(&mut insert, ACCOUNT_TABLE_ID_COLUMNS, account.account_id.to_sql_tuple())?;
        insert.column_param(account_table::NAME, account.name)?; // crd:pin
        insert.column_param(account_table::DESCRIPTION, account.description)?; // crd:pin
        insert.column_param(account_table::BALANCE_USD, account.balance_usd.to_string())?; // crd:pin

        // A primary-key clash means this identity is already stored.
        handle_constraint_error(insert.insert(), || {
            AccountStorageError::AccountExistsAlready(account.account_id)
        })?;
        Ok(())
    }

    fn delete_account(conn: &Connection, account_id: AccountId) -> AccountStorageResult<()> {
        let mut delete = SqlDelete::new(conn, account_table::TABLE_NAME)?;
        restrict_to_account(&mut delete, ACCOUNT_TABLE_ID_COLUMNS, account_id.to_sql_tuple())?;

        // Cascades take care of the coins and enabled-marker rows.
        if delete.delete()? == 0 {
            return MmError::err(AccountStorageError::NoSuchAccount(account_id));
        }
        Ok(())
    }

    /// Runs an in-place metadata update. The caller's `set_columns` closure
    /// registers the columns to assign; this helper appends the identity filter
    /// and reports `NoSuchAccount` when nothing matched.
    fn update_account<F>(conn: &Connection, account_id: AccountId, set_columns: F) -> AccountStorageResult<()>
    where
        F: FnOnce(&mut SqlUpdate) -> SqlResult<()>, // crd:pin
    {
        let mut update = SqlUpdate::new(conn, account_table::TABLE_NAME)?;
        set_columns(&mut update)?;
        restrict_to_account(&mut update, ACCOUNT_TABLE_ID_COLUMNS, account_id.to_sql_tuple())?;

        if update.update()? == 0 {
            return MmError::err(AccountStorageError::NoSuchAccount(account_id));
        }
        Ok(())
    }
}

/// The accounts-table columns, in the exact order expected by `account_from_row`
/// (identity columns 0..3, then name/description/balance at 3/4/5).
const ACCOUNT_COLUMNS: [&str; 6] = [
    // crd:pin-begin
    account_table::ACCOUNT_TYPE,
    account_table::ACCOUNT_IDX,
    account_table::DEVICE_PUBKEY,
    account_table::NAME,
    account_table::DESCRIPTION,
    account_table::BALANCE_USD,
    // crd:pin-end
];

#[async_trait]
impl AccountStorage for SqliteAccountStorage {
    async fn init(&self) -> AccountStorageResult<()> {
        let mut conn = self.lock_conn_mutex()?;
        let tx = conn.transaction()?;

        // The coins and enabled tables reference the accounts table, so it must
        // be created first.
        Self::init_account_table(&tx)?;
        Self::init_account_coins_table(&tx)?;
        Self::init_enabled_account_table(&tx)?;

        tx.commit()?;
        Ok(())
    }

    async fn load_account_coins(&self, account_id: AccountId) -> AccountStorageResult<BTreeSet<String>> {
        let conn = self.lock_conn_mutex()?;
        let coins = Self::load_account_coins(&conn, &account_id)?;

        // A non-empty set already implies the account exists; return straight
        // away. Only an empty set needs an existence probe to tell "no coins"
        // apart from "no account".
        if !coins.is_empty() {
            return Ok(coins);
        }
        if Self::account_exists(&conn, &account_id)? {
            Ok(coins)
        } else {
            MmError::err(AccountStorageError::NoSuchAccount(account_id))
        }
    }

    async fn load_accounts(&self) -> AccountStorageResult<BTreeMap<AccountId, AccountInfo>> {
        let conn = self.lock_conn_mutex()?;
        Self::load_accounts(&conn)
    }

    async fn load_accounts_with_enabled_flag(
        &self,
    ) -> AccountStorageResult<BTreeMap<AccountId, AccountWithEnabledFlag>> {
        let conn = self.lock_conn_mutex()?;
        let enabled_id = AccountId::from(Self::load_enabled_account_id_or_err(&conn)?);

        let accounts: BTreeMap<AccountId, AccountWithEnabledFlag> = Self::load_accounts(&conn)?
            .into_iter()
            .map(|(account_id, account_info)| {
                let enabled = account_id == enabled_id;
                (account_id, AccountWithEnabledFlag { account_info, enabled })
            })
            .collect();

        // The marker returned by `load_enabled_account_id_or_err` must point at a
        // real account row; otherwise the storage invariant is broken.
        if accounts.contains_key(&enabled_id) {
            Ok(accounts)
        } else {
            MmError::err(AccountStorageError::unknown_account_in_enabled_table(enabled_id))
        }
    }

    async fn load_enabled_account_id(&self) -> AccountStorageResult<EnabledAccountId> {
        let conn = self.lock_conn_mutex()?;
        Self::load_enabled_account_id_or_err(&conn)
    }

    async fn load_enabled_account_with_coins(&self) -> AccountStorageResult<AccountWithCoins> {
        let conn = self.lock_conn_mutex()?;
        let enabled_id = AccountId::from(Self::load_enabled_account_id_or_err(&conn)?);

        Self::load_account_with_coins(&conn, &enabled_id)?
            .or_mm_err(|| AccountStorageError::unknown_account_in_enabled_table(enabled_id))
    }

    async fn enable_account(&self, enabled_account_id: EnabledAccountId) -> AccountStorageResult<()> {
        let mut conn = self.lock_conn_mutex()?;
        let tx = conn.transaction()?;

        // The enabled table holds at most one row; drop the previous selection
        // before recording the new one.
        SqlDelete::new(&tx, enabled_account_table::TABLE_NAME)?.delete()?;

        let mut insert = SqlInsert::new(&tx, enabled_account_table::TABLE_NAME);
        write_account_columns(&mut insert, ENABLED_TABLE_ID_COLUMNS, enabled_account_id.to_sql_tuple())?;

        // A foreign-key violation here means the referenced account does not exist.
        let inserted = handle_constraint_error(insert.insert(), || {
            AccountStorageError::NoSuchAccount(AccountId::from(enabled_account_id))
        })?;
        if inserted != 1 {
            return MmError::err(AccountStorageError::Internal(format!(
                "Enabling an account inserted {inserted} rows, expected exactly 1"
            )));
        }

        tx.commit()?;
        Ok(())
    }

    async fn upload_account(&self, account: AccountInfo) -> AccountStorageResult<()> {
        let conn = self.lock_conn_mutex()?;
        Self::upload_account(&conn, account)
    }

    async fn delete_account(&self, account_id: AccountId) -> AccountStorageResult<()> {
        let conn = self.lock_conn_mutex()?;
        Self::delete_account(&conn, account_id)
    }

    async fn set_name(&self, account_id: AccountId, name: String) -> AccountStorageResult<()> {
        let conn = self.lock_conn_mutex()?;
        Self::update_account(&conn, account_id, |update| {
            update.set_param(account_table::NAME, name)?; // crd:pin
            Ok(())
        })
    }

    async fn set_description(&self, account_id: AccountId, description: String) -> AccountStorageResult<()> {
        let conn = self.lock_conn_mutex()?;
        Self::update_account(&conn, account_id, |update| {
            update.set_param(account_table::DESCRIPTION, description)?; // crd:pin
            Ok(())
        })
    }

    async fn set_balance(&self, account_id: AccountId, balance_usd: BigDecimal) -> AccountStorageResult<()> {
        let conn = self.lock_conn_mutex()?;
        Self::update_account(&conn, account_id, |update| {
            update.set_param(account_table::BALANCE_USD, balance_usd.to_string())?; // crd:pin
            Ok(())
        })
    }

    async fn activate_coins(&self, account_id: AccountId, tickers: Vec<String>) -> AccountStorageResult<()> {
        let mut conn = self.lock_conn_mutex()?;
        let tx = conn.transaction()?;

        let sql_id = account_id.to_sql_tuple();
        tickers.into_iter().try_for_each(|ticker| -> AccountStorageResult<()> {
            let mut insert = SqlInsert::new(&tx, account_coins_table::TABLE_NAME);
            insert.or_ignore();
            write_account_columns(&mut insert, COINS_TABLE_ID_COLUMNS, sql_id.clone())?;
            insert.column_param(account_coins_table::COIN, ticker)?; // crd:pin

            // `or_ignore` silently drops an already-activated ticker; a foreign-key
            // violation instead means the account itself is unknown.
            handle_constraint_error(insert.insert(), || {
                AccountStorageError::NoSuchAccount(account_id.clone())
            })?;
            Ok(())
        })?;

        tx.commit()?;
        Ok(())
    }

    async fn deactivate_coins(&self, account_id: AccountId, tickers: Vec<String>) -> AccountStorageResult<()> {
        let conn = self.lock_conn_mutex()?;

        let mut delete = SqlDelete::new(&conn, account_coins_table::TABLE_NAME)?;
        restrict_to_account(&mut delete, COINS_TABLE_ID_COLUMNS, account_id.to_sql_tuple())?;
        delete.and_where_in_params(account_coins_table::COIN, tickers)?; // crd:pin

        // Removing at least one row already proves the account exists. When
        // nothing matched, fall back to an existence probe to tell apart an
        // unknown account from tickers that simply were not activated.
        let removed = delete.delete()?;
        if removed > 0 || Self::account_exists(&conn, &account_id)? {
            Ok(())
        } else {
            MmError::err(AccountStorageError::NoSuchAccount(account_id))
        }
    }
}

/// Wraps an identity-decode failure as a column-conversion error so it can flow
/// back through rusqlite's `Result` channel.
fn into_sql_decode_error(e: MmError<AccountStorageError>) -> SqlError {
    let cause = std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string());
    SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(cause))
}

fn account_id_from_row(row: &Row<'_>) -> Result<AccountId, SqlError> {
    // crd:pin-begin
    let account_type: i64 = row.get(0)?;
    let account_idx: u32 = row.get(1)?;
    let device_pubkey: String = row.get(2)?;
    AccountId::try_from_sql_tuple(account_type, account_idx, &device_pubkey).map_err(into_sql_decode_error)
    // crd:pin-end
}

fn enabled_account_id_from_row(row: &Row<'_>) -> Result<EnabledAccountId, SqlError> {
    // crd:pin-begin
    let account_type: i64 = row.get(0)?;
    let account_idx: u32 = row.get(1)?;
    EnabledAccountId::try_from_sql_pair(account_type, account_idx).map_err(into_sql_decode_error)
    // crd:pin-end
}

fn account_from_row(row: &Row<'_>) -> Result<AccountInfo, SqlError> {
    // crd:pin-begin
    Ok(AccountInfo {
        account_id: account_id_from_row(row)?,
        name: row.get(3)?,
        description: row.get(4)?,
        balance_usd: bigdecimal_from_row(row, 5)?,
    })
    // crd:pin-end
}

fn count_from_row(row: &Row<'_>) -> Result<i64, SqlError> {
    row.get(0)
}

fn bigdecimal_from_row(row: &Row<'_>, idx: usize) -> Result<BigDecimal, SqlError> {
    let raw: String = row.get(idx)?;
    BigDecimal::from_str(&raw).map_err(|e| SqlError::FromSqlConversionFailure(idx, Type::Text, Box::new(e)))
}

/// Routes a SQL result: a constraint violation is reinterpreted as the supplied
/// domain error, while any other failure keeps its generic mapping.
fn handle_constraint_error<T, F>(result: SqlResult<T>, on_constraint_error: F) -> AccountStorageResult<T>
where
    F: FnOnce() -> AccountStorageError,
{
    result.map_to_mm(|e| {
        if is_constraint_error(&e) {
            on_constraint_error()
        } else {
            AccountStorageError::from(e)
        }
    })
}

/// The `(account_type, account_idx, device_pubkey)` column-name triple shared by
/// every identity-keyed table.
type IdentityColumns = (&'static str, &'static str, &'static str);

const ACCOUNT_TABLE_ID_COLUMNS: IdentityColumns = (
    account_table::ACCOUNT_TYPE,
    account_table::ACCOUNT_IDX,
    account_table::DEVICE_PUBKEY,
);

const COINS_TABLE_ID_COLUMNS: IdentityColumns = (
    account_coins_table::ACCOUNT_TYPE,
    account_coins_table::ACCOUNT_IDX,
    account_coins_table::DEVICE_PUBKEY,
);

const ENABLED_TABLE_ID_COLUMNS: IdentityColumns = (
    enabled_account_table::ACCOUNT_TYPE,
    enabled_account_table::ACCOUNT_IDX,
    enabled_account_table::DEVICE_PUBKEY,
);

/// Renders the SQL representation of an identity: the discriminant and index as
/// integers and the device pubkey as prefix-less lowercase hex.
fn encode_sql_identity(account_type: i64, account_idx: u32, device_pubkey: &HwPubkey) -> (i64, i64, String) {
    (account_type, i64::from(account_idx), format!("{device_pubkey:x}"))
}

/// Adds the shared three-column identity predicate to a WHERE-clause builder.
fn restrict_to_account<B>(builder: &mut B, columns: IdentityColumns, id: (i64, i64, String)) -> SqlResult<()>
where
    B: SqlCondition, // crd:pin
{
    let (type_col, idx_col, pubkey_col) = columns;
    let (type_val, idx_val, pubkey_val) = id;
    builder.and_where_eq(type_col, type_val)?;
    builder.and_where_eq(idx_col, idx_val)?;
    builder.and_where_eq_param(pubkey_col, pubkey_val)?;
    Ok(())
}

/// Writes the shared three-column identity into an INSERT builder.
fn write_account_columns(
    insert: &mut SqlInsert<'_>,
    columns: IdentityColumns,
    id: (i64, i64, String),
) -> SqlResult<()> {
    let (type_col, idx_col, pubkey_col) = columns;
    let (type_val, idx_val, pubkey_val) = id;
    insert.column(type_col, type_val)?;
    insert.column(idx_col, idx_val)?;
    insert.column_param(pubkey_col, pubkey_val)?;
    Ok(())
}
