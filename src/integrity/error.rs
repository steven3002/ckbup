use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("invalid digest `{0}`; expected sha256:<64 hex characters>")]
    InvalidDigest(String),
    #[error(
        "GitHub lists no SHA-256 for {asset}, and {tool} publishes no checksum file for {tag}; older releases often have none. Re-run with --allow-unverified to install it without verification"
    )]
    MissingChecksum {
        asset: String,
        tool: String,
        tag: String,
    },
    #[error("{file} does not contain a SHA-256 hash for {asset}")]
    NoHashInSumsFile { file: String, asset: String },
    #[error("cannot read {path} to verify it: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_checksum_error_names_the_override_flag() {
        let message = IntegrityError::MissingChecksum {
            asset: "ckb-light-client_v0.4.1-x86_64-windows.tar.gz".into(),
            tool: "ckb-light-client".into(),
            tag: "v0.4.1".into(),
        }
        .to_string();
        assert!(message.starts_with(
            "GitHub lists no SHA-256 for ckb-light-client_v0.4.1-x86_64-windows.tar.gz"
        ));
        assert!(message.contains("ckb-light-client publishes no checksum file for v0.4.1"));
        assert!(message.contains("older releases often have none"));
        assert!(message.contains("Re-run with --allow-unverified"));
    }
}
