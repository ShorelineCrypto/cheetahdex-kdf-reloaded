//! `NftHistoryStore` implementation for [`SqliteNftStore`].

use crate::nft::model::{Chain, NftTokenIdent, NftTransfer, NftTransferList, NftTransfersFilters, TransferMeta,
                        TransferStatus};
use crate::nft::store::history::NftHistoryStore;
use crate::nft::store::sqlite::list::paginate;
use crate::nft::store::sqlite::schema::{create_transfers_sql, transfers_table, TABLE_EXISTS_SQL};
use crate::nft::store::sqlite::SqliteNftStore;
use async_trait::async_trait;
use db_common::async_sql_conn::AsyncConnError;
use db_common::sqlite::rusqlite::params;
use ethereum_types::Address;
use mm2_err_handle::prelude::{MmError, MmResult};
use mm2_number::BigUint;
use serde_json::Value as Json;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::str::FromStr;

#[async_trait]
impl NftHistoryStore for SqliteNftStore {
    type Error = AsyncConnError;

    async fn ensure_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let sql = create_transfers_sql(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        self.conn()
            .call(move |conn| {
                conn.execute(&sql, params![])?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn chain_ready(&self, chain: &Chain) -> MmResult<bool, Self::Error> {
        let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let mut stmt = conn.prepare(TABLE_EXISTS_SQL)?;
                Ok(stmt.exists(params![name])?)
            })
            .await
            .map_err(MmError::new)
    }

    async fn list_transfers(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftTransfersFilters>,
    ) -> MmResult<NftTransferList, Self::Error> {
        if chains.is_empty() {
            return Ok(NftTransferList {
                transfer_history: vec![],
                skipped: 0,
                total: 0,
            });
        }
        let mut where_clause = String::new();
        if let Some(f) = filters {
            if f.receive {
                where_clause.push_str(" AND status = 'Receive'");
            }
            if f.send {
                where_clause.push_str(" AND status = 'Send'");
            }
            if let Some(from) = f.from_date {
                where_clause.push_str(&format!(" AND block_timestamp >= {}", from));
            }
            if let Some(to) = f.to_date {
                where_clause.push_str(&format!(" AND block_timestamp <= {}", to));
            }
            if f.exclude_spam {
                where_clause.push_str(" AND possible_spam = 0");
            }
            if f.exclude_phishing {
                where_clause.push_str(" AND possible_phishing = 0");
            }
        }
        let mut tables = Vec::with_capacity(chains.len());
        for chain in &chains {
            let t = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
            tables.push(t.inner().to_owned());
        }
        let union_select = tables
            .iter()
            .map(|t| {
                format!(
                    "SELECT block_number, block_timestamp, payload FROM {} WHERE 1=1{}",
                    t, where_clause
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        let union_count = tables
            .iter()
            .map(|t| format!("SELECT COUNT(*) AS c FROM {} WHERE 1=1{}", t, where_clause))
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        let union_total = tables
            .iter()
            .map(|t| format!("SELECT COUNT(*) AS c FROM {}", t))
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        self.conn()
            .call(move |conn| {
                let filtered: i64 = conn
                    .prepare(&format!("SELECT COALESCE(SUM(c), 0) FROM ({})", union_count))?
                    .query_row(params![], |row| row.get(0))?;
                let unfiltered: i64 = conn
                    .prepare(&format!("SELECT COALESCE(SUM(c), 0) FROM ({})", union_total))?
                    .query_row(params![], |row| row.get(0))?;
                let total = filtered as usize;
                let skipped = (unfiltered - filtered).max(0) as usize;
                let (offset, limit) = paginate(take_all, page_size, page, total);
                let sql = format!(
                    "SELECT payload FROM ({}) ORDER BY block_number DESC, block_timestamp DESC LIMIT ? OFFSET ?",
                    union_select
                );
                let mut stmt = conn.prepare(&sql)?;
                let raws = stmt
                    .query_map(params![limit as i64, offset as i64], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out = Vec::with_capacity(raws.len());
                for raw in raws {
                    out.push(parse_transfer(&raw)?);
                }
                Ok(NftTransferList {
                    transfer_history: out,
                    skipped,
                    total,
                })
            })
            .await
            .map_err(MmError::new)
    }

    async fn append_transfers(&self, chain: Chain, transfers: Vec<NftTransfer>) -> MmResult<(), Self::Error> {
        let table = transfers_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let mut prepared = Vec::with_capacity(transfers.len());
        for tr in transfers {
            let payload = serde_json::to_string(&tr).map_err(|e| {
                MmError::new(AsyncConnError::Internal(db_common::async_sql_conn::InternalError(
                    e.to_string(),
                )))
            })?;
            let token_address = format!("{:?}", tr.common.token_address);
            let token_id_str = tr.token_id.to_string();
            let block_number = tr.block_number as i64;
            let block_timestamp = tr.block_timestamp as i64;
            let possible_spam = i64::from(tr.common.possible_spam);
            let possible_phishing = i64::from(tr.possible_phishing);
            let status = tr.status.to_string();
            let token_domain = tr.token_domain.clone();
            let image_domain = tr.image_domain.clone();
            prepared.push((
                tr.common.transaction_hash.clone(),
                tr.common.log_index as i64,
                token_id_str,
                token_address,
                block_number,
                block_timestamp,
                possible_spam,
                possible_phishing,
                status,
                token_domain,
                image_domain,
                payload,
            ));
        }
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                let sql = format!(
                    "INSERT OR REPLACE INTO {} (\
                        transaction_hash, log_index, token_id_str, token_address, block_number, \
                        block_timestamp, possible_spam, possible_phishing, status, token_domain, \
                        image_domain, payload\
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);",
                    table_name
                );
                let mut stmt = tx.prepare(&sql)?;
                for row in &prepared {
                    stmt.execute(params![
                        row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, row.10, row.11
                    ])?;
                }
                drop(stmt);
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn latest_transfer_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let value: Option<i64> = conn.query_row(
                    &format!("SELECT MAX(block_number) FROM {}", table_name),
                    params![],
                    |row| row.get(0),
                )?;
                Ok(value.map(|v| v as u64))
            })
            .await
            .map_err(MmError::new)
    }

    async fn transfers_since(&self, chain: Chain, from_block: u64) -> MmResult<Vec<NftTransfer>, Self::Error> {
        let table = transfers_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let block = from_block as i64;
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT payload FROM {} WHERE block_number >= ? ORDER BY block_number ASC",
                    table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let raws = stmt
                    .query_map(params![block], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out = Vec::with_capacity(raws.len());
                for raw in raws {
                    out.push(parse_transfer(&raw)?);
                }
                Ok(out)
            })
            .await
            .map_err(MmError::new)
    }

    async fn transfers_for_token(
        &self,
        chain: Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Vec<NftTransfer>, Self::Error> {
        let table = transfers_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let id_str = token_id.to_string();
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT payload FROM {} WHERE token_address = ? AND token_id_str = ? ORDER BY block_number DESC",
                    table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let raws = stmt
                    .query_map(params![token_address, id_str], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out = Vec::with_capacity(raws.len());
                for raw in raws {
                    out.push(parse_transfer(&raw)?);
                }
                Ok(out)
            })
            .await
            .map_err(MmError::new)
    }

    async fn transfer_by_log(
        &self,
        chain: &Chain,
        transaction_hash: String,
        log_index: u32,
        token_id: BigUint,
    ) -> MmResult<Option<NftTransfer>, Self::Error> {
        let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let id_str = token_id.to_string();
        let log_idx = log_index as i64;
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT payload FROM {} WHERE transaction_hash = ? AND log_index = ? AND token_id_str = ?",
                    table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![transaction_hash, log_idx, id_str])?;
                let row = rows.next()?;
                Ok(match row {
                    Some(r) => Some(parse_transfer(&r.get::<_, String>(0)?)?),
                    None => None,
                })
            })
            .await
            .map_err(MmError::new)
    }

    async fn attach_metadata_to_transfers(
        &self,
        chain: &Chain,
        meta: TransferMeta,
        flag_spam: bool,
    ) -> MmResult<(), Self::Error> {
        let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let id_str = meta.token_id.to_string();
        let token_address = meta.token_address.clone();
        let new_token_domain = meta.token_domain.clone();
        let new_image_domain = meta.image_domain.clone();
        let new_spam = flag_spam;
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                let mut to_update: Vec<(String, i64, String, Json)> = Vec::new();
                {
                    let select = format!(
                        "SELECT transaction_hash, log_index, token_id_str, payload FROM {} \
                         WHERE token_address = ? AND token_id_str = ?",
                        table_name
                    );
                    let mut stmt = tx.prepare(&select)?;
                    let mut rows = stmt.query(params![token_address, id_str])?;
                    while let Some(row) = rows.next()? {
                        let h: String = row.get(0)?;
                        let idx: i64 = row.get(1)?;
                        let id: String = row.get(2)?;
                        let raw: String = row.get(3)?;
                        let value: Json = serde_json::from_str(&raw).map_err(|e| {
                            db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                                0,
                                db_common::sqlite::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                        to_update.push((h, idx, id, value));
                    }
                }
                let update_sql = format!(
                    "UPDATE {} SET payload = ?, token_domain = ?, image_domain = ?, possible_spam = ? \
                     WHERE transaction_hash = ? AND log_index = ? AND token_id_str = ?",
                    table_name
                );
                let mut stmt = tx.prepare(&update_sql)?;
                for (hash, idx, id, mut value) in to_update {
                    if let Some(obj) = value.as_object_mut() {
                        set_json_string(obj, "token_uri", &meta.token_uri);
                        set_json_string(obj, "token_domain", &meta.token_domain);
                        set_json_string(obj, "collection_name", &meta.collection_name);
                        set_json_string(obj, "image_url", &meta.image_url);
                        set_json_string(obj, "image_domain", &meta.image_domain);
                        set_json_string(obj, "token_name", &meta.token_name);
                        if let Some(common) = obj.get_mut("common").and_then(|c| c.as_object_mut()) {
                            common.insert("possible_spam".to_string(), Json::Bool(new_spam));
                        }
                        // The `possible_spam` field is also flattened to the
                        // root by serde when the payload was serialized, so
                        // mirror it there for round-trip safety.
                        obj.insert("possible_spam".to_string(), Json::Bool(new_spam));
                    }
                    let serialized = serde_json::to_string(&value)
                        .map_err(|e| db_common::sqlite::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    stmt.execute(params![
                        serialized,
                        new_token_domain,
                        new_image_domain,
                        i64::from(new_spam),
                        hash,
                        idx,
                        id
                    ])?;
                }
                drop(stmt);
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn transfers_missing_metadata(&self, chain: Chain) -> MmResult<Vec<NftTokenIdent>, Self::Error> {
        let table = transfers_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT DISTINCT token_address, token_id_str, payload FROM {}",
                    table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![])?;
                let mut out: Vec<NftTokenIdent> = Vec::new();
                let mut seen: HashSet<(String, String)> = HashSet::new();
                while let Some(row) = rows.next()? {
                    let addr_str: String = row.get(0)?;
                    let id_str: String = row.get(1)?;
                    if !seen.insert((addr_str.clone(), id_str.clone())) {
                        continue;
                    }
                    let raw: String = row.get(2)?;
                    let value: Json = serde_json::from_str(&raw).map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    let collection_name = value.get("collection_name").and_then(|v| v.as_str());
                    let token_name = value.get("token_name").and_then(|v| v.as_str());
                    if collection_name.is_none() || token_name.is_none() {
                        let token_id = BigUint::from_str(&id_str).map_err(|e| {
                            db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                                0,
                                db_common::sqlite::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                        out.push(NftTokenIdent {
                            token_address: addr_str,
                            token_id,
                        });
                    }
                }
                Ok(out)
            })
            .await
            .map_err(MmError::new)
    }

    async fn transfers_for_contract(
        &self,
        chain: Chain,
        token_address: String,
    ) -> MmResult<Vec<NftTransfer>, Self::Error> {
        let table = transfers_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT payload FROM {} WHERE token_address = ? ORDER BY block_number DESC",
                    table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let raws = stmt
                    .query_map(params![token_address], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out = Vec::with_capacity(raws.len());
                for raw in raws {
                    out.push(parse_transfer(&raw)?);
                }
                Ok(out)
            })
            .await
            .map_err(MmError::new)
    }

    async fn mark_contract_spam(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        update_transfer_flag(
            self,
            chain,
            FlagSelector::ContractAddress(token_address),
            "possible_spam",
            "possible_spam",
            possible_spam,
        )
        .await
    }

    async fn contract_addresses(&self, chain: Chain) -> MmResult<HashSet<Address>, Self::Error> {
        let table = transfers_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let sql = format!("SELECT DISTINCT token_address FROM {}", table_name);
                let mut stmt = conn.prepare(&sql)?;
                let raws = stmt
                    .query_map(params![], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out: HashSet<Address> = HashSet::with_capacity(raws.len());
                for raw in raws {
                    let trimmed = raw.strip_prefix("0x").unwrap_or(&raw);
                    let address = Address::from_str(trimmed).map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
                        )
                    })?;
                    out.insert(address);
                }
                Ok(out)
            })
            .await
            .map_err(MmError::new)
    }

    async fn domain_set(&self, chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
        let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT token_domain FROM {tn} WHERE token_domain IS NOT NULL \
                     UNION SELECT image_domain FROM {tn} WHERE image_domain IS NOT NULL",
                    tn = table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![], |row| row.get::<_, String>(0))?
                    .collect::<Result<HashSet<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(MmError::new)
    }

    async fn mark_domain_phishing(
        &self,
        chain: &Chain,
        domain: String,
        possible_phishing: bool,
    ) -> MmResult<(), Self::Error> {
        update_transfer_flag(
            self,
            chain,
            FlagSelector::Domain(domain),
            "possible_phishing",
            "possible_phishing",
            possible_phishing,
        )
        .await
    }

    async fn purge_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                conn.execute(&format!("DROP TABLE IF EXISTS {}", table_name), params![])?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn purge_all(&self) -> MmResult<(), Self::Error> {
        self.conn()
            .call(move |conn| {
                let names: Vec<String> = {
                    let mut stmt = conn
                        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'NFT_%_transfers'")?;
                    let rows = stmt
                        .query_map(params![], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?;
                    rows
                };
                for name in names {
                    conn.execute(&format!("DROP TABLE IF EXISTS {}", name), params![])?;
                }
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }
}

/// Selector used by [`update_transfer_flag`] to choose the WHERE clause.
enum FlagSelector {
    ContractAddress(String),
    Domain(String),
}

async fn update_transfer_flag(
    store: &SqliteNftStore,
    chain: &Chain,
    selector: FlagSelector,
    json_path: &'static str,
    column: &'static str,
    value: bool,
) -> MmResult<(), AsyncConnError> {
    let table = transfers_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
    let table_name = table.inner().to_owned();
    let flag_int = i64::from(value);
    store
        .conn()
        .call(move |conn| {
            let tx = conn.transaction()?;
            let mut to_update: Vec<(String, i64, String, Json)> = Vec::new();
            let (select_sql, bind_value): (String, String) = match &selector {
                FlagSelector::ContractAddress(addr) => (
                    format!(
                        "SELECT transaction_hash, log_index, token_id_str, payload FROM {} \
                         WHERE token_address = ?",
                        table_name
                    ),
                    addr.clone(),
                ),
                FlagSelector::Domain(d) => (
                    format!(
                        "SELECT transaction_hash, log_index, token_id_str, payload FROM {} \
                         WHERE token_domain = ?1 OR image_domain = ?1",
                        table_name
                    ),
                    d.clone(),
                ),
            };
            {
                let mut stmt = tx.prepare(&select_sql)?;
                let mut rows = stmt.query(params![bind_value])?;
                while let Some(row) = rows.next()? {
                    let h: String = row.get(0)?;
                    let idx: i64 = row.get(1)?;
                    let id: String = row.get(2)?;
                    let raw: String = row.get(3)?;
                    let v: Json = serde_json::from_str(&raw).map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    to_update.push((h, idx, id, v));
                }
            }
            let update_sql = format!(
                "UPDATE {} SET {} = ?, payload = ? WHERE transaction_hash = ? AND log_index = ? AND token_id_str = ?",
                table_name, column
            );
            let mut stmt = tx.prepare(&update_sql)?;
            let path: Vec<&str> = json_path.split('.').collect();
            for (hash, idx, id, mut payload) in to_update {
                set_nested_bool(&mut payload, &path, value);
                let serialized = serde_json::to_string(&payload)
                    .map_err(|e| db_common::sqlite::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                stmt.execute(params![flag_int, serialized, hash, idx, id])?;
            }
            drop(stmt);
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(MmError::new)
}

fn set_nested_bool(value: &mut Json, path: &[&str], target: bool) {
    if path.is_empty() {
        return;
    }
    let mut current = value;
    for (i, segment) in path.iter().enumerate() {
        let is_leaf = i + 1 == path.len();
        let map = match current.as_object_mut() {
            Some(m) => m,
            None => return,
        };
        if is_leaf {
            map.insert((*segment).to_string(), Json::Bool(target));
            return;
        }
        current = map
            .entry((*segment).to_string())
            .or_insert_with(|| Json::Object(Default::default()));
    }
}

fn set_json_string(obj: &mut serde_json::Map<String, Json>, key: &str, value: &Option<String>) {
    match value {
        Some(v) => {
            obj.insert(key.to_string(), Json::String(v.clone()));
        },
        None => {
            obj.insert(key.to_string(), Json::Null);
        },
    }
}

fn parse_transfer(raw: &str) -> db_common::sqlite::rusqlite::Result<NftTransfer> {
    serde_json::from_str(raw).map_err(|e| {
        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
            0,
            db_common::sqlite::rusqlite::types::Type::Text,
            Box::new(e),
        )
    })
}

// Quiet a warning about an unused trait import if/when callers don't need
// it directly.
#[allow(dead_code)]
const _ASSERT_TRANSFER_STATUS: TransferStatus = TransferStatus::Receive;
