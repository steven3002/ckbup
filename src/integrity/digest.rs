use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::IntegrityError;

/// Parses a GitHub asset digest of the form `sha256:<hex>` into lowercase hex.
pub fn parse_api_digest(value: &str) -> Result<String, IntegrityError> {
    value
        .strip_prefix("sha256:")
        .filter(|hex| is_sha256_hex(hex))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| IntegrityError::InvalidDigest(value.to_string()))
}

pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Compares two hex hashes case-insensitively.
pub fn ensure_match(
    asset: &str,
    source_name: &str,
    expected: &str,
    actual: &str,
) -> Result<(), IntegrityError> {
    if expected.eq_ignore_ascii_case(actual) {
        return Ok(());
    }
    Err(IntegrityError::Mismatch {
        asset: asset.to_string(),
        source_name: source_name.to_string(),
        expected: expected.to_ascii_lowercase(),
        actual: actual.to_ascii_lowercase(),
    })
}

/// Returns the lowercase hex SHA-256 of the file at `path`, read in chunks.
pub fn sha256_file(path: &Path) -> Result<String, IntegrityError> {
    let read_err = |source| IntegrityError::Read {
        path: path.to_path_buf(),
        source,
    };
    let mut file = File::open(path).map_err(read_err)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err(read_err(err)),
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn parses_api_digest() {
        let upper = format!("sha256:{}", ABC_SHA256.to_ascii_uppercase());
        assert_eq!(parse_api_digest(&upper).unwrap(), ABC_SHA256);
        assert!(parse_api_digest(ABC_SHA256).is_err());
        assert!(parse_api_digest("sha256:abc").is_err());
        assert!(parse_api_digest("sha512:00").is_err());
    }

    #[test]
    fn hashes_a_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.txt");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), ABC_SHA256);
        assert!(sha256_file(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn compares_case_insensitively() {
        assert!(ensure_match("a", "digest", &ABC_SHA256.to_ascii_uppercase(), ABC_SHA256).is_ok());
        let err = ensure_match("a", "digest", ABC_SHA256, &"0".repeat(64)).unwrap_err();
        assert!(err.to_string().contains("checksum mismatch for a"));
    }
}
