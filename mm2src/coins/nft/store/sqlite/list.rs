//! `NftListStore` implementation for [`SqliteNftStore`].

use crate::nft::model::{Chain, Nft, NftList, NftListFilters};
use crate::nft::store::errors::RemoveOutcome;
use crate::nft::store::list::NftListStore;
use crate::nft::store::sqlite::schema::{
    create_inventory_sql, create_scan_progress_sql, inventory_table, SCAN_PROGRESS_TABLE, TABLE_EXISTS_SQL,
};
use crate::nft::store::sqlite::SqliteNftStore;
use async_trait::async_trait;
use db_common::async_sql_conn::AsyncConnError;
use db_common::sqlite::rusqlite::params;
use mm2_err_handle::prelude::{MmError, MmResult};
use mm2_number::BigUint;
use serde_json::Value as Json;
use std::collections::HashSet;
use std::num::NonZeroUsize;

/// Map [`Chain`] to its UPPERCASE serde label (the same encoding used by
/// the wire model and JSON payloads).
fn chain_label(chain: &Chain) -> String {
    format!("{}", chain)
}

/// Compute the inclusive 0-based offset and limit from optional pagination
/// inputs. Returns `(0, total)` when `take_all` is true.
pub(crate) fn paginate(take_all: bool, page_size: usize, page: Option<NonZeroUsize>, total: usize) -> (usize, usize) {
    if take_all {
        return (0, total);
    }
    let page_index = page.map(|p| p.get() - 1).unwrap_or(0);
    let offset = page_index.saturating_mul(page_size);
    (offset, page_size)
}

#[async_trait]
impl NftListStore for SqliteNftStore {
    type Error = AsyncConnError;

    async fn ensure_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let inv_sql = create_inventory_sql(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let scan_sql = create_scan_progress_sql();
        let chain_str = chain_label(chain);
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(&inv_sql, params![])?;
                tx.execute(&scan_sql, params![])?;
                tx.execute(
                    &format!(
                        "INSERT OR IGNORE INTO {}(chain, last_scanned_block) VALUES (?, 0);",
                        SCAN_PROGRESS_TABLE
                    ),
                    params![chain_str],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn chain_ready(&self, chain: &Chain) -> MmResult<bool, Self::Error> {
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let mut stmt = conn.prepare(TABLE_EXISTS_SQL)?;
                let exists = stmt.exists(params![name])?;
                Ok(exists)
            })
            .await
            .map_err(MmError::new)
    }

    async fn list_owned(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error> {
        if chains.is_empty() {
            return Ok(NftList {
                nfts: vec![],
                skipped: 0,
                total: 0,
            });
        }
        let mut where_clause = String::new();
        if let Some(f) = filters {
            if f.exclude_spam {
                where_clause.push_str(" AND possible_spam = 0");
            }
            if f.exclude_phishing {
                where_clause.push_str(" AND possible_phishing = 0");
            }
        }
        let mut tables = Vec::with_capacity(chains.len());
        for chain in &chains {
            let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
            tables.push(table.inner().to_owned());
        }
        let union_select = tables
            .iter()
            .map(|t| format!("SELECT block_number, payload FROM {} WHERE 1=1{}", t, where_clause))
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
                let filtered_count: i64 = conn
                    .prepare(&format!("SELECT COALESCE(SUM(c), 0) FROM ({})", union_count))?
                    .query_row(params![], |row| row.get(0))?;
                let unfiltered_count: i64 = conn
                    .prepare(&format!("SELECT COALESCE(SUM(c), 0) FROM ({})", union_total))?
                    .query_row(params![], |row| row.get(0))?;
                let total = filtered_count as usize;
                let skipped = (unfiltered_count - filtered_count).max(0) as usize;
                let (offset, limit) = paginate(take_all, page_size, page, total);
                let sql = format!(
                    "SELECT payload FROM ({}) ORDER BY block_number DESC LIMIT ? OFFSET ?",
                    union_select
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![limit as i64, offset as i64], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut nfts = Vec::with_capacity(rows.len());
                for raw in rows {
                    let nft: Nft = serde_json::from_str(&raw).map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    nfts.push(nft);
                }
                Ok(NftList { nfts, skipped, total })
            })
            .await
            .map_err(MmError::new)
    }

    async fn register_owned(
        &self,
        chain: Chain,
        items: Vec<Nft>,
        last_scanned_block: u64,
    ) -> MmResult<(), Self::Error> {
        let table = inventory_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let chain_str = chain_label(&chain);
        let scan_block = last_scanned_block as i64;
        // Pre-serialize payloads in the caller thread to avoid moving the
        // serde error type across the closure boundary.
        let mut prepared = Vec::with_capacity(items.len());
        for nft in items {
            let payload = serde_json::to_string(&nft).map_err(|e| {
                MmError::new(AsyncConnError::Internal(db_common::async_sql_conn::InternalError(
                    e.to_string(),
                )))
            })?;
            let token_address = format!("{:?}", nft.common.token_address);
            let token_id_str = nft.token_id.to_string();
            let block_number = nft.block_number as i64;
            let possible_spam = i64::from(nft.common.possible_spam);
            let possible_phishing = i64::from(nft.possible_phishing);
            let contract_type = nft.contract_type.to_string();
            let image_domain = nft.uri_meta.image_domain.clone();
            let animation_domain = nft.uri_meta.animation_domain.clone();
            let external_domain = nft.uri_meta.external_domain.clone();
            prepared.push((
                token_address,
                token_id_str,
                block_number,
                possible_spam,
                possible_phishing,
                contract_type,
                image_domain,
                animation_domain,
                external_domain,
                payload,
            ));
        }
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                {
                    let sql = format!(
                        "INSERT OR REPLACE INTO {} (\
                            token_address, token_id_str, block_number, possible_spam, \
                            possible_phishing, contract_type, image_domain, animation_domain, \
                            external_domain, payload\
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?);",
                        table_name
                    );
                    let mut stmt = tx.prepare(&sql)?;
                    for row in &prepared {
                        stmt.execute(params![
                            row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9
                        ])?;
                    }
                }
                tx.execute(
                    &format!(
                        "INSERT INTO {sp}(chain, last_scanned_block) VALUES (?, ?) \
                         ON CONFLICT(chain) DO UPDATE SET last_scanned_block = excluded.last_scanned_block;",
                        sp = SCAN_PROGRESS_TABLE
                    ),
                    params![chain_str, scan_block],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn fetch_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<Nft>, Self::Error> {
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let id_str = token_id.to_string();
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT payload FROM {} WHERE token_address = ? AND token_id_str = ?",
                    table_name
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![token_address, id_str])?;
                let row = rows.next()?;
                let raw: Option<String> = match row {
                    Some(r) => Some(r.get(0)?),
                    None => None,
                };
                Ok(raw
                    .map(|raw| serde_json::from_str::<Nft>(&raw))
                    .transpose()
                    .map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?)
            })
            .await
            .map_err(MmError::new)
    }

    async fn drop_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
        scanned_block: u64,
    ) -> MmResult<RemoveOutcome, Self::Error> {
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let id_str = token_id.to_string();
        let chain_str = chain_label(chain);
        let block = scanned_block as i64;
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                let removed = tx.execute(
                    &format!(
                        "DELETE FROM {} WHERE token_address = ? AND token_id_str = ?",
                        table_name
                    ),
                    params![token_address, id_str],
                )?;
                tx.execute(
                    &format!(
                        "INSERT INTO {sp}(chain, last_scanned_block) VALUES (?, ?) \
                         ON CONFLICT(chain) DO UPDATE SET last_scanned_block = excluded.last_scanned_block;",
                        sp = SCAN_PROGRESS_TABLE
                    ),
                    params![chain_str, block],
                )?;
                tx.commit()?;
                Ok(if removed > 0 {
                    RemoveOutcome::Removed
                } else {
                    RemoveOutcome::Absent
                })
            })
            .await
            .map_err(MmError::new)
    }

    async fn token_balance(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<String>, Self::Error> {
        let nft = self.fetch_token(chain, token_address, token_id).await?;
        Ok(nft.map(|n| n.common.amount.to_string()))
    }

    async fn merge_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        // Replace the existing record entirely; the providers layer is
        // responsible for merging metadata before calling us.
        self.register_owned(*chain, vec![nft.clone()], nft.block_number).await
    }

    async fn latest_block_in_cache(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let sql = format!("SELECT MAX(block_number) FROM {}", table_name);
                let value: Option<i64> = conn.query_row(&sql, params![], |row| row.get(0))?;
                Ok(value.map(|v| v as u64))
            })
            .await
            .map_err(MmError::new)
    }

    async fn latest_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let chain_str = chain_label(chain);
        self.conn()
            .call(move |conn| {
                let sql = format!("SELECT last_scanned_block FROM {} WHERE chain = ?", SCAN_PROGRESS_TABLE);
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![chain_str])?;
                let row = rows.next()?;
                Ok(match row {
                    Some(r) => Some(r.get::<_, i64>(0)? as u64),
                    None => None,
                })
            })
            .await
            .map_err(MmError::new)
    }

    async fn set_token_amount(&self, chain: &Chain, nft: Nft, scanned_block: u64) -> MmResult<(), Self::Error> {
        self.register_owned(*chain, vec![nft], scanned_block).await
    }

    async fn set_token_amount_and_block(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        let block = nft.block_number;
        self.register_owned(*chain, vec![nft], block).await
    }

    async fn tokens_for_contract(&self, chain: Chain, token_address: String) -> MmResult<Vec<Nft>, Self::Error> {
        let table = inventory_table(&chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
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
                    out.push(serde_json::from_str::<Nft>(&raw).map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?);
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
        update_inventory_flag(
            self,
            chain,
            token_address,
            "possible_spam",
            "possible_spam",
            possible_spam,
        )
        .await
    }

    async fn list_external_domains(&self, chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        self.conn()
            .call(move |conn| {
                let sql = format!(
                    "SELECT image_domain FROM {tn} WHERE image_domain IS NOT NULL \
                     UNION SELECT animation_domain FROM {tn} WHERE animation_domain IS NOT NULL \
                     UNION SELECT external_domain FROM {tn} WHERE external_domain IS NOT NULL",
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
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let flag_int = i64::from(possible_phishing);
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                let select = format!(
                    "SELECT token_address, token_id_str, payload FROM {} \
                     WHERE image_domain = ?1 OR animation_domain = ?1 OR external_domain = ?1",
                    table_name
                );
                let mut to_update: Vec<(String, String, Json)> = Vec::new();
                {
                    let mut stmt = tx.prepare(&select)?;
                    let mut rows = stmt.query(params![domain])?;
                    while let Some(row) = rows.next()? {
                        let address: String = row.get(0)?;
                        let id_str: String = row.get(1)?;
                        let raw: String = row.get(2)?;
                        let value: Json = serde_json::from_str(&raw).map_err(|e| {
                            db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                                0,
                                db_common::sqlite::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                        to_update.push((address, id_str, value));
                    }
                }
                let update_sql = format!(
                    "UPDATE {} SET possible_phishing = ?, payload = ? \
                     WHERE token_address = ? AND token_id_str = ?",
                    table_name
                );
                let mut stmt = tx.prepare(&update_sql)?;
                for (address, id_str, mut value) in to_update {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("possible_phishing".to_string(), Json::Bool(possible_phishing));
                    }
                    let serialized = serde_json::to_string(&value)
                        .map_err(|e| db_common::sqlite::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    stmt.execute(params![flag_int, serialized, address, id_str])?;
                }
                drop(stmt);
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn purge_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
        let table_name = table.inner().to_owned();
        let chain_str = chain_label(chain);
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(&format!("DROP TABLE IF EXISTS {}", table_name), params![])?;
                tx.execute(
                    &format!("DELETE FROM {} WHERE chain = ?", SCAN_PROGRESS_TABLE),
                    params![chain_str],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }

    async fn purge_all(&self) -> MmResult<(), Self::Error> {
        self.conn()
            .call(move |conn| {
                let tx = conn.transaction()?;
                let table_names: Vec<String> = {
                    let mut stmt = tx
                        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'NFT_%_inventory'")?;
                    let rows = stmt
                        .query_map(params![], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?;
                    rows
                };
                for name in table_names {
                    tx.execute(&format!("DROP TABLE IF EXISTS {}", name), params![])?;
                }
                tx.execute(&format!("DELETE FROM {}", SCAN_PROGRESS_TABLE), params![])?;
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(MmError::new)
    }
}

/// Update both the indexed flag column and the corresponding boolean field
/// inside the JSON payload for every record sharing a contract address.
async fn update_inventory_flag(
    store: &SqliteNftStore,
    chain: &Chain,
    token_address: String,
    json_path: &'static str,
    column: &'static str,
    value: bool,
) -> MmResult<(), AsyncConnError> {
    let table = inventory_table(chain).map_err(|e| MmError::new(AsyncConnError::from(e)))?;
    let table_name = table.inner().to_owned();
    let flag_int = i64::from(value);
    store
        .conn()
        .call(move |conn| {
            let tx = conn.transaction()?;
            let select = format!(
                "SELECT token_id_str, payload FROM {} WHERE token_address = ?",
                table_name
            );
            let mut updates: Vec<(String, Json)> = Vec::new();
            {
                let mut stmt = tx.prepare(&select)?;
                let mut rows = stmt.query(params![token_address])?;
                while let Some(row) = rows.next()? {
                    let id_str: String = row.get(0)?;
                    let raw: String = row.get(1)?;
                    let payload: Json = serde_json::from_str(&raw).map_err(|e| {
                        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
                            0,
                            db_common::sqlite::rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    updates.push((id_str, payload));
                }
            }
            let update_sql = format!(
                "UPDATE {} SET {} = ?, payload = ? WHERE token_address = ? AND token_id_str = ?",
                table_name, column
            );
            let mut stmt = tx.prepare(&update_sql)?;
            let path_segments: Vec<&str> = json_path.split('.').collect();
            for (id_str, mut payload) in updates {
                set_nested_bool(&mut payload, &path_segments, value);
                let serialized = serde_json::to_string(&payload)
                    .map_err(|e| db_common::sqlite::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                stmt.execute(params![flag_int, serialized, token_address, id_str])?;
            }
            drop(stmt);
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(MmError::new)
}

/// Walk into a JSON document along `path` and set the leaf to a boolean.
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
