use std::io::{self, Write};

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

/// Writer adapter that hashes every byte passed through to the inner writer.
pub struct HashingWriter<W> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> HashingWriter<W> {
    pub fn new(inner: W) -> Self {
        HashingWriter {
            inner,
            hasher: Sha256::new(),
        }
    }

    /// Flushes the inner writer and returns it with the lowercase hex SHA-256.
    pub fn finish(mut self) -> io::Result<(W, String)> {
        self.inner.flush()?;
        let hex = self
            .hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok((self.inner, hex))
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.update(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
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
    fn hashes_while_writing() {
        let mut writer = HashingWriter::new(Vec::new());
        writer.write_all(b"abc").unwrap();
        let (bytes, hex) = writer.finish().unwrap();
        assert_eq!(bytes, b"abc");
        assert_eq!(hex, ABC_SHA256);
    }

    #[test]
    fn compares_case_insensitively() {
        assert!(ensure_match("a", "digest", &ABC_SHA256.to_ascii_uppercase(), ABC_SHA256).is_ok());
        let err = ensure_match("a", "digest", ABC_SHA256, &"0".repeat(64)).unwrap_err();
        assert!(err.to_string().contains("checksum mismatch for a"));
    }
}
