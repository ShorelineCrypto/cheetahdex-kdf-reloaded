use super::z_rpc::z_coin_grpc;
use super::{CheckPointBlockInfo, ZCoinBuildError, ZcoinConsensusParams};
use crate::utxo::utxo_common::big_decimal_from_sat_unsigned;
use common::mm_number::BigDecimal;
use common::{calc_total_pages, PagingOptionsEnum};
use db_common::sqlite::rusqlite::{params, Connection, OptionalExtension, NO_PARAMS};
use futures::StreamExt;
use mm2_err_handle::prelude::*;
use protobuf::Message;
use std::path::{Path, PathBuf};
use tonic::transport::Endpoint;
use zcash_client_backend::data_api::chain::scan_cached_blocks;
use zcash_client_backend::proto::compact_formats as zcash_compact;
use zcash_client_sqlite::{chain::init::init_cache_database,
                          wallet::init::{init_accounts_table, init_blocks_table, init_wallet_db},
                          BlockDb, WalletDb};
use zcash_primitives::{block::BlockHash,
                       consensus::{BlockHeight, NetworkUpgrade, Parameters},
                       zip32::ExtendedFullViewingKey};

#[derive(Clone, Debug)]
pub(crate) struct ZCoinShieldedHistory {
    compact_blocks_path: PathBuf,
    wallet_db_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ZCoinTxHistoryDetails {
    pub(crate) tx_hash: String,
    pub(crate) from: Vec<String>,
    pub(crate) to: Vec<String>,
    pub(crate) spent_by_me: BigDecimal,
    pub(crate) received_by_me: BigDecimal,
    pub(crate) my_balance_change: BigDecimal,
    pub(crate) block_height: u64,
    pub(crate) confirmations: u64,
    pub(crate) timestamp: u64,
    pub(crate) transaction_fee: BigDecimal,
    pub(crate) coin: String,
    pub(crate) internal_id: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct ZCoinTxHistoryPage {
    pub(crate) transactions: Vec<ZCoinTxHistoryDetails>,
    pub(crate) skipped: usize,
    pub(crate) total: usize,
    pub(crate) total_pages: usize,
}

#[derive(Debug)]
struct ZCoinStoredHistoryRow {
    internal_id: i64,
    tx_hash: String,
    block_height: u64,
    timestamp: u64,
    received_by_me: u64,
    spent_by_me: u64,
    received_addresses: Vec<String>,
    sent_addresses: Vec<String>,
}

impl ZCoinShieldedHistory {
    pub(crate) fn open_or_create(
        ticker: &str,
        db_dir_path: PathBuf,
        consensus_params: ZcoinConsensusParams,
        extfvk: &ExtendedFullViewingKey,
        check_point_block: Option<&CheckPointBlockInfo>,
    ) -> MmResult<Self, ZCoinBuildError> {
        let paths = ZCoinShieldedHistoryPaths::new(ticker, db_dir_path);
        if let Some(parent) = paths.wallet_db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let compact_db = BlockDb::for_path(&paths.compact_blocks_path)
            .map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;
        init_cache_database(&compact_db)
            .map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;

        let wallet_db = WalletDb::for_path(&paths.wallet_db_path, consensus_params)
            .map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;
        init_wallet_db(&wallet_db).map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;

        if !table_has_rows(&paths.wallet_db_path, "accounts")? {
            init_accounts_table(&wallet_db, &[extfvk.clone()])
                .map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;
        }

        if !table_has_rows(&paths.wallet_db_path, "blocks")? {
            if let Some(check_point) = check_point_block {
                init_blocks_table(
                    &wallet_db,
                    BlockHeight::from_u32(check_point.height),
                    BlockHash(check_point.hash.0),
                    check_point.time,
                    check_point.sapling_tree.as_slice(),
                )
                .map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;
            }
        }

        Ok(ZCoinShieldedHistory {
            compact_blocks_path: paths.compact_blocks_path,
            wallet_db_path: paths.wallet_db_path,
        })
    }

    pub(crate) fn wallet_db_path(&self) -> &Path { &self.wallet_db_path }

    pub(crate) fn compact_blocks_path(&self) -> &Path { &self.compact_blocks_path }

    pub(crate) async fn fetch_compact_blocks_from_lightwalletd(
        &self,
        consensus_params: &ZcoinConsensusParams,
        servers: &[String],
        target_height: u64,
    ) -> Result<u64, String> {
        let Some(start_height) = self.next_fetch_height(consensus_params, target_height)? else {
            return Ok(self.scanned_height()?.unwrap_or(0));
        };

        let mut last_error = None;
        for server in servers {
            match self
                .fetch_compact_blocks_from_server(server, start_height, target_height)
                .await
            {
                Ok(fetched_height) => return Ok(fetched_height),
                Err(e) => last_error = Some(format!("{}: {}", server, e)),
            }
        }

        Err(last_error.unwrap_or_else(|| "No lightwalletd servers configured".to_owned()))
    }

    fn next_fetch_height(
        &self,
        consensus_params: &ZcoinConsensusParams,
        target_height: u64,
    ) -> Result<Option<u64>, String> {
        let scanned_height = self.scanned_height()?.unwrap_or_else(|| {
            consensus_params
                .activation_height(NetworkUpgrade::Sapling)
                .map(|h| u32::from(h).saturating_sub(1) as u64)
                .unwrap_or(0)
        });
        let start_height = scanned_height + 1;
        Ok((start_height <= target_height).then_some(start_height))
    }

    async fn fetch_compact_blocks_from_server(
        &self,
        server: &str,
        start_height: u64,
        target_height: u64,
    ) -> Result<u64, String> {
        let endpoint = Endpoint::from_shared(lightwalletd_endpoint(server))
            .map_err(|e| e.to_string())?
            .connect()
            .await
            .map_err(|e| e.to_string())?;
        let mut client = z_coin_grpc::compact_tx_streamer_client::CompactTxStreamerClient::new(endpoint);
        let request = z_coin_grpc::BlockRange {
            start: Some(z_coin_grpc::BlockId {
                height: start_height,
                hash: Vec::new(),
            }),
            end: Some(z_coin_grpc::BlockId {
                height: target_height,
                hash: Vec::new(),
            }),
        };
        let mut stream = client
            .get_block_range(request)
            .await
            .map_err(|e| e.to_string())?
            .into_inner();

        let mut last_height = start_height.saturating_sub(1);
        while let Some(block) = stream.next().await {
            let block = block.map_err(|e| e.to_string())?;
            last_height = block.height;
            self.insert_compact_block(convert_compact_block(block)?)?;
        }

        if last_height < target_height {
            Err(format!(
                "lightwalletd returned compact blocks through {}, below requested {}",
                last_height, target_height
            ))
        } else {
            Ok(last_height)
        }
    }

    fn insert_compact_block(&self, block: zcash_compact::CompactBlock) -> Result<(), String> {
        let block_bytes = block.write_to_bytes().map_err(|e| e.to_string())?;
        let conn = Connection::open(&self.compact_blocks_path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO compactblocks (height, data) VALUES (?1, ?2)",
            params![u32::from(block.height()), block_bytes],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    pub(crate) fn scanned_height(&self) -> Result<Option<u64>, String> {
        let conn = Connection::open(&self.wallet_db_path).map_err(|e| e.to_string())?;
        conn.query_row("SELECT MAX(height) FROM blocks", NO_PARAMS, |row| {
            let height: Option<u32> = row.get(0)?;
            Ok(height.map(u64::from))
        })
        .map_err(|e| e.to_string())
    }

    pub(crate) fn scan_cached_blocks_to_height(
        &self,
        consensus_params: ZcoinConsensusParams,
        target_height: u64,
    ) -> Result<u64, String> {
        let initial_scanned_height = self.scanned_height()?.unwrap_or(0);
        if initial_scanned_height >= target_height {
            return Ok(initial_scanned_height);
        }

        let block_db = BlockDb::for_path(&self.compact_blocks_path).map_err(|e| e.to_string())?;
        let wallet_db =
            WalletDb::for_path(&self.wallet_db_path, consensus_params.clone()).map_err(|e| e.to_string())?;
        let mut update_ops = wallet_db.get_update_ops().map_err(|e| e.to_string())?;
        scan_cached_blocks(&consensus_params, &block_db, &mut update_ops, None).map_err(|e| e.to_string())?;

        let scanned_height = self.scanned_height()?.unwrap_or(0);
        if scanned_height >= target_height {
            Ok(scanned_height)
        } else {
            Err(format!(
                "Shielded wallet DB scanned only through height {}, below activation tip {}. \
                 Compact-block scanner/source is unavailable or has not cached enough blocks.",
                scanned_height, target_height
            ))
        }
    }

    pub(crate) fn load_page(
        &self,
        ticker: &str,
        wallet_z_address: &str,
        decimals: u8,
        current_block: u64,
        paging_options: &PagingOptionsEnum<i64>,
        limit: usize,
    ) -> Result<ZCoinTxHistoryPage, String> {
        let rows = self.load_rows()?;
        let total = rows.len();
        let skipped = skipped_by_paging(&rows, paging_options, limit)?;
        let transactions = rows
            .into_iter()
            .skip(skipped)
            .take(limit)
            .map(|row| row.into_details(ticker, wallet_z_address, decimals, current_block))
            .collect();

        Ok(ZCoinTxHistoryPage {
            transactions,
            skipped,
            total,
            total_pages: calc_total_pages(total, limit),
        })
    }

    fn load_rows(&self) -> Result<Vec<ZCoinStoredHistoryRow>, String> {
        let conn = Connection::open(&self.wallet_db_path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    t.id_tx,
                    t.txid,
                    COALESCE(t.block, 0) AS block_height,
                    COALESCE(b.time, 0) AS timestamp,
                    COALESCE((SELECT SUM(value) FROM received_notes rn WHERE rn.tx = t.id_tx), 0) AS received_by_me,
                    COALESCE((SELECT SUM(value) FROM received_notes rn WHERE rn.spent = t.id_tx), 0) AS spent_by_me,
                    COALESCE((
                        SELECT GROUP_CONCAT(DISTINCT a.address)
                        FROM received_notes rn
                        INNER JOIN accounts a ON a.account = rn.account
                        WHERE rn.tx = t.id_tx
                    ), '') AS received_addresses,
                    COALESCE((
                        SELECT GROUP_CONCAT(DISTINCT sn.address)
                        FROM sent_notes sn
                        WHERE sn.tx = t.id_tx
                    ), '') AS sent_addresses
                FROM transactions t
                LEFT JOIN blocks b ON b.height = t.block
                WHERE EXISTS (SELECT 1 FROM received_notes rn WHERE rn.tx = t.id_tx)
                   OR EXISTS (SELECT 1 FROM received_notes rn WHERE rn.spent = t.id_tx)
                   OR EXISTS (SELECT 1 FROM sent_notes sn WHERE sn.tx = t.id_tx)
                ORDER BY COALESCE(t.block, -1) DESC, t.id_tx DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(NO_PARAMS, |row| {
                let txid: Vec<u8> = row.get(1)?;
                let received: i64 = row.get(4)?;
                let spent: i64 = row.get(5)?;
                Ok(ZCoinStoredHistoryRow {
                    internal_id: row.get(0)?,
                    tx_hash: hex::encode(txid),
                    block_height: row.get::<_, u32>(2)? as u64,
                    timestamp: row.get::<_, u32>(3)? as u64,
                    received_by_me: non_negative_amount(received, 4)?,
                    spent_by_me: non_negative_amount(spent, 5)?,
                    received_addresses: split_group_concat(row.get::<_, String>(6)?),
                    sent_addresses: split_group_concat(row.get::<_, String>(7)?),
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }
}

impl ZCoinStoredHistoryRow {
    fn into_details(
        self,
        ticker: &str,
        wallet_z_address: &str,
        decimals: u8,
        current_block: u64,
    ) -> ZCoinTxHistoryDetails {
        let received_by_me = big_decimal_from_sat_unsigned(self.received_by_me, decimals);
        let spent_by_me = big_decimal_from_sat_unsigned(self.spent_by_me, decimals);
        let mut from = Vec::new();
        if self.spent_by_me > 0 {
            from.push(wallet_z_address.to_owned());
        }
        from.sort();
        from.dedup();

        let mut to = self.received_addresses;
        to.extend(self.sent_addresses);
        if self.received_by_me > 0 {
            to.push(wallet_z_address.to_owned());
        }
        to.sort();
        to.dedup();

        ZCoinTxHistoryDetails {
            tx_hash: self.tx_hash,
            from,
            to,
            spent_by_me: spent_by_me.clone(),
            received_by_me: received_by_me.clone(),
            my_balance_change: received_by_me - spent_by_me,
            block_height: self.block_height,
            confirmations: confirmations(current_block, self.block_height),
            timestamp: self.timestamp,
            transaction_fee: BigDecimal::from(0),
            coin: ticker.to_owned(),
            internal_id: self.internal_id,
        }
    }
}

fn lightwalletd_endpoint(server: &str) -> String {
    if server.starts_with("http://") || server.starts_with("https://") {
        server.to_owned()
    } else {
        format!("https://{}", server)
    }
}

fn convert_compact_block(block: z_coin_grpc::CompactBlock) -> Result<zcash_compact::CompactBlock, String> {
    let mut converted = zcash_compact::CompactBlock::new();
    converted.set_protoVersion(block.proto_version);
    converted.set_height(block.height);
    converted.set_hash(block.hash);
    converted.set_prevHash(block.prev_hash);
    converted.set_time(block.time);
    converted.set_header(block.header);
    converted.set_vtx(block.vtx.into_iter().map(convert_compact_tx).collect());
    Ok(converted)
}

fn convert_compact_tx(tx: z_coin_grpc::CompactTx) -> zcash_compact::CompactTx {
    let mut converted = zcash_compact::CompactTx::new();
    converted.set_index(tx.index);
    converted.set_hash(tx.hash);
    converted.set_fee(tx.fee);
    converted.set_spends(tx.spends.into_iter().map(convert_compact_spend).collect());
    converted.set_outputs(tx.outputs.into_iter().map(convert_compact_output).collect());
    converted
}

fn convert_compact_spend(spend: z_coin_grpc::CompactSpend) -> zcash_compact::CompactSpend {
    let mut converted = zcash_compact::CompactSpend::new();
    converted.set_nf(spend.nf);
    converted
}

fn convert_compact_output(output: z_coin_grpc::CompactOutput) -> zcash_compact::CompactOutput {
    let mut converted = zcash_compact::CompactOutput::new();
    converted.set_cmu(output.cmu);
    converted.set_epk(output.epk);
    converted.set_ciphertext(output.ciphertext);
    converted
}

struct ZCoinShieldedHistoryPaths {
    compact_blocks_path: PathBuf,
    wallet_db_path: PathBuf,
}

impl ZCoinShieldedHistoryPaths {
    fn new(ticker: &str, mut db_dir_path: PathBuf) -> Self {
        let mut compact_blocks_path = db_dir_path.clone();
        compact_blocks_path.push(format!("{}_COMPACT_BLOCKS.db", ticker));
        db_dir_path.push(format!("{}_WALLET.db", ticker));
        ZCoinShieldedHistoryPaths {
            compact_blocks_path,
            wallet_db_path: db_dir_path,
        }
    }
}

fn confirmations(current_block: u64, block_height: u64) -> u64 {
    if block_height == 0 || block_height > current_block {
        0
    } else {
        current_block + 1 - block_height
    }
}

fn skipped_by_paging(
    rows: &[ZCoinStoredHistoryRow],
    paging: &PagingOptionsEnum<i64>,
    limit: usize,
) -> Result<usize, String> {
    match paging {
        PagingOptionsEnum::FromId(from_id) => rows
            .iter()
            .position(|row| row.internal_id == *from_id)
            .map(|idx| idx + 1)
            .ok_or_else(|| format!("Unknown shielded transaction history internal_id {}", from_id)),
        PagingOptionsEnum::PageNumber(page_number) => Ok((page_number.get() - 1) * limit),
    }
}

fn table_has_rows(path: &Path, table: &str) -> MmResult<bool, ZCoinBuildError> {
    let conn = Connection::open(path).map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;
    let sql = format!("SELECT 1 FROM {} LIMIT 1", table);
    let found = conn
        .query_row(&sql, NO_PARAMS, |_| Ok(()))
        .optional()
        .map_err(|e| MmError::new(ZCoinBuildError::SaplingCacheError(e.to_string())))?;
    Ok(found.is_some())
}

fn non_negative_amount(amount: i64, column: usize) -> Result<u64, db_common::sqlite::rusqlite::Error> {
    u64::try_from(amount).map_err(|e| {
        db_common::sqlite::rusqlite::Error::FromSqlConversionFailure(
            column,
            db_common::sqlite::rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}

fn split_group_concat(value: String) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db_common::sqlite::rusqlite::params;
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zcash_primitives::zip32::ExtendedSpendingKey;

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_params() -> ZcoinConsensusParams {
        serde_json::from_value(serde_json::json!({
            "overwinter_activation_height": 1,
            "sapling_activation_height": 2,
            "blossom_activation_height": null,
            "heartwood_activation_height": null,
            "canopy_activation_height": null,
            "coin_type": 133,
            "hrp_sapling_extended_spending_key": "secret-extended-key-main",
            "hrp_sapling_extended_full_viewing_key": "zxviews",
            "hrp_sapling_payment_address": "zs",
            "b58_pubkey_address_prefix": [0x1c, 0xb8],
            "b58_script_address_prefix": [0x1c, 0xbd]
        }))
        .unwrap()
    }

    fn open_test_history() -> ZCoinShieldedHistory {
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_dir = std::env::temp_dir().join(format!(
            "kdf-zcoin-wallet-history-test-{}-{}-{}",
            std::process::id(),
            common::now_ms(),
            test_id
        ));
        let extsk = ExtendedSpendingKey::master(&[7; 32]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        ZCoinShieldedHistory::open_or_create("ARRR", db_dir, test_params(), &extfvk, None).unwrap()
    }

    fn open_checkpointed_test_history(checkpoint_height: u32) -> ZCoinShieldedHistory {
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_dir = std::env::temp_dir().join(format!(
            "kdf-zcoin-wallet-history-checkpoint-test-{}-{}-{}",
            std::process::id(),
            common::now_ms(),
            test_id
        ));
        let extsk = ExtendedSpendingKey::master(&[8; 32]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        let check_point = CheckPointBlockInfo {
            height: checkpoint_height,
            hash: rpc::v1::types::H256([9u8; 32]),
            time: 1234,
            sapling_tree: empty_sapling_tree_bytes().into(),
        };
        ZCoinShieldedHistory::open_or_create("ARRR", db_dir, test_params(), &extfvk, Some(&check_point)).unwrap()
    }

    fn empty_sapling_tree_bytes() -> Vec<u8> {
        let tree = zcash_primitives::merkle_tree::CommitmentTree::<zcash_primitives::sapling::Node>::empty();
        let mut bytes = Vec::new();
        tree.write(&mut bytes).unwrap();
        bytes
    }

    fn insert_history_fixture(history: &ZCoinShieldedHistory) {
        let conn = Connection::open(history.wallet_db_path()).unwrap();
        conn.execute(
            "INSERT INTO blocks (height, hash, time, sapling_tree) VALUES (?1, ?2, ?3, ?4)",
            params![10u32, vec![10u8; 32], 1000u32, Vec::<u8>::new()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO blocks (height, hash, time, sapling_tree) VALUES (?1, ?2, ?3, ?4)",
            params![11u32, vec![11u8; 32], 1100u32, Vec::<u8>::new()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id_tx, txid, block, tx_index) VALUES (?1, ?2, ?3, ?4)",
            params![1i64, vec![1u8; 32], 10u32, 0u32],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id_tx, txid, block, tx_index) VALUES (?1, ?2, ?3, ?4)",
            params![2i64, vec![2u8; 32], 11u32, 0u32],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO received_notes (tx, output_index, account, diversifier, value, rcm, nf, is_change)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                1i64,
                0u32,
                0u32,
                vec![3u8; 11],
                125_000_000i64,
                vec![4u8; 32],
                vec![5u8; 32],
                0u32
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO received_notes (tx, output_index, account, diversifier, value, rcm, nf, is_change, spent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                1i64,
                1u32,
                0u32,
                vec![6u8; 11],
                25_000_000i64,
                vec![7u8; 32],
                vec![8u8; 32],
                0u32,
                2i64
            ],
        )
        .unwrap();
    }

    #[test]
    fn open_or_create_initializes_public_zcash_wallet_schema() {
        let history = open_test_history();
        assert!(history.compact_blocks_path().exists());
        assert!(history.wallet_db_path().exists());

        let conn = Connection::open(history.wallet_db_path()).unwrap();
        for table in [
            "accounts",
            "blocks",
            "transactions",
            "received_notes",
            "sent_notes",
            "sapling_witnesses",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "missing table {table}");
        }
    }

    #[test]
    fn load_page_returns_newest_first_wallet_scan_history() {
        let history = open_test_history();
        insert_history_fixture(&history);

        let page = history
            .load_page(
                "ARRR",
                "zs-wallet",
                8,
                12,
                &PagingOptionsEnum::PageNumber(NonZeroUsize::new(1).unwrap()),
                10,
            )
            .unwrap();

        assert_eq!(page.total, 2);
        assert_eq!(page.transactions[0].internal_id, 2);
        assert_eq!(
            page.transactions[0].spent_by_me,
            BigDecimal::from(25) / BigDecimal::from(100)
        );
        assert_eq!(page.transactions[0].received_by_me, BigDecimal::from(0));
        assert_eq!(
            page.transactions[0].my_balance_change,
            BigDecimal::from(-25) / BigDecimal::from(100)
        );
        assert_eq!(page.transactions[0].from, vec!["zs-wallet"]);
        assert_eq!(page.transactions[0].confirmations, 2);

        assert_eq!(page.transactions[1].internal_id, 1);
        assert_eq!(
            page.transactions[1].received_by_me,
            BigDecimal::from(15) / BigDecimal::from(10)
        );
        assert!(page.transactions[1].to.contains(&"zs-wallet".to_owned()));
        assert_eq!(page.transactions[1].confirmations, 3);
    }

    #[test]
    fn from_id_unknown_is_storage_error() {
        let history = open_test_history();
        insert_history_fixture(&history);

        let err = history
            .load_page("ARRR", "zs-wallet", 8, 12, &PagingOptionsEnum::FromId(999), 10)
            .unwrap_err();

        assert!(err.contains("Unknown shielded transaction history internal_id 999"));
    }

    #[test]
    fn initialized_but_unscanned_wallet_db_does_not_reach_activation_tip() {
        let history = open_checkpointed_test_history(10);
        assert_eq!(history.scanned_height().unwrap(), Some(10));

        let err = history.scan_cached_blocks_to_height(test_params(), 11).unwrap_err();
        assert!(err.contains("below activation tip 11"));
    }

    #[test]
    fn checkpoint_at_activation_tip_counts_as_scanned_through_tip() {
        let history = open_checkpointed_test_history(10);
        let scanned = history.scan_cached_blocks_to_height(test_params(), 10).unwrap();
        assert_eq!(scanned, 10);
    }
}
