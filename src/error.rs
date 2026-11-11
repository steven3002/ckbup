use thiserror::Error;

use crate::archive::ArchiveError;
use crate::github::GithubError;
use crate::integrity::IntegrityError;
use crate::store::StoreError;
use crate::tools::ToolError;

/// Top-level error returned by commands and reported by `main`.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error(transparent)]
    Store(#[from] StoreError),
}
