use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("invalid digest `{0}`; expected sha256:<64 hex characters>")]
    InvalidDigest(String),
    #[error(
        "{asset} has no published checksum; refusing to install (pass --allow-unverified to override)"
    )]
    MissingChecksum { asset: String },
    #[error("{file} does not contain a SHA-256 hash for {asset}")]
    NoHashInSumsFile { file: String, asset: String },
    #[error(
        "checksum mismatch for {asset} against {source_name}: expected {expected}, got {actual}; the download was deleted"
    )]
    Mismatch {
        asset: String,
        source_name: String,
        expected: String,
        actual: String,
    },
}
