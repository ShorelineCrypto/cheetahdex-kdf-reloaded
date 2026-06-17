//! Session persistence.
//!
//! Sessions survive process restarts. A single trait abstracts the backend; the
//! native build persists to SQLite and the browser build to IndexedDB. Both
//! store one row per session, keyed by topic, carrying an opaque JSON payload
//! and the expiry.

use crate::error::WalletConnectError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
pub mod indexed_db;
#[cfg(not(target_arch = "wasm32"))]
pub mod sqlite;

/// The single table / object-store name shared by both backends.
pub const WC_SESSION_TABLE: &str = "wc_session";

/// One persisted session row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    /// Session topic (primary key).
    pub topic: String,
    /// Opaque JSON-encoded session payload.
    pub data: String,
    /// Unix expiry timestamp (seconds).
    pub expiry: i64,
}

/// CRUD operations every storage backend exposes.
#[async_trait]
pub trait WcStorageOps: Send + Sync {
    /// Ensures the backing table / object store exists.
    async fn init(&self) -> Result<(), WalletConnectError>;

    /// Inserts or replaces a session row.
    async fn save_session(&self, session: StoredSession) -> Result<(), WalletConnectError>;

    /// Fetches a single session by topic.
    async fn get_session(&self, topic: &str) -> Result<Option<StoredSession>, WalletConnectError>;

    /// Fetches every persisted session.
    async fn get_all_sessions(&self) -> Result<Vec<StoredSession>, WalletConnectError>;

    /// Deletes the session with the given topic.
    async fn delete_session(&self, topic: &str) -> Result<(), WalletConnectError>;
}
