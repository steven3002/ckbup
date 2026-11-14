use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("failed to read archive {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read zip archive {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: ::zip::result::ZipError,
    },
    #[error("archive {archive} contains an unsafe entry path `{entry}`")]
    UnsafeEntry { archive: PathBuf, entry: String },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the release package has no `{name}` at its top level")]
    BinaryNotFound { name: String },
}
