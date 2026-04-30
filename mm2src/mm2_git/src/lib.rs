//! Abstraction layer for Git-hosted repository operations.
//!
//! Provides a generic `GitController` with pluggable backends (GitHub, etc.)
//! for fetching file metadata and deserializing JSON content from repositories.

use async_trait::async_trait;
use mm2_err_handle::prelude::MmError;
use serde::{de::DeserializeOwned, Deserialize};

pub mod github_client;
pub use github_client::*;

pub const GITHUB_API_URI: &str = "https://api.github.com";

/// Metadata for a single file in a repository directory listing.
#[derive(Clone, Debug, Deserialize)]
pub struct FileMetadata {
    pub name: String,
    pub download_url: String,
    pub size: usize,
}

/// Factory trait for creating repository client instances.
pub trait GitCommons {
    fn new(api_address: String) -> Self;
}

/// Async operations for fetching and deserializing repository content.
#[async_trait]
pub trait RepositoryOperations {
    /// Download and deserialize a JSON file from its metadata.
    async fn deserialize_json_source<T>(&self, file_metadata: FileMetadata) -> Result<T, MmError<GitControllerError>>
    where
        T: DeserializeOwned;

    /// List file metadata for a directory in a repository.
    async fn get_file_metadata_list(
        &self,
        owner: &str,
        repository_name: &str,
        branch: &str,
        dir: &str,
    ) -> Result<Vec<FileMetadata>, MmError<GitControllerError>>;
}

/// Generic controller wrapping any `RepositoryOperations` backend.
pub struct GitController<T: RepositoryOperations> {
    pub client: T,
}

impl<T: GitCommons + RepositoryOperations> GitController<T> {
    pub fn new(api_address: &str) -> Self {
        Self {
            client: T::new(api_address.to_owned()),
        }
    }
}

/// Errors that can occur during Git repository operations.
#[derive(Debug)]
pub enum GitControllerError {
    DeserializationError(String),
    HttpError(String),
}
