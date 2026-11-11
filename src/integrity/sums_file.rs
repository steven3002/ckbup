//! Parsing of published SHA-256 checksum files.
//!
//! Two layouts are accepted:
//! - `sha256sum` output: `<hex>  <file name>` per line (an optional `*` marks binary mode);
//! - a PowerShell `Get-FileHash` table: an `SHA256 <HEX> <path>` row under a header,
//!   where the path may be truncated and is therefore not matched.

use super::IntegrityError;
use super::digest::is_sha256_hex;

/// Returns the lowercase hash recorded for `asset` in `contents`.
///
/// A `sha256sum` line must name the asset. A `Get-FileHash` table carries one
/// hash per file and its path column may be truncated, so its single hash is used.
pub fn hash_for_asset(contents: &str, file: &str, asset: &str) -> Result<String, IntegrityError> {
    let mut table_hashes = Vec::new();
    for line in contents.lines() {
        let mut fields = line.split_whitespace();
        let (Some(first), Some(second)) = (fields.next(), fields.next()) else {
            continue;
        };
        if is_sha256_hex(first) && second.trim_start_matches('*') == asset {
            return Ok(first.to_ascii_lowercase());
        }
        if first.eq_ignore_ascii_case("SHA256") && is_sha256_hex(second) {
            table_hashes.push(second.to_ascii_lowercase());
        }
    }
    match table_hashes.as_slice() {
        [hash] => Ok(hash.clone()),
        _ => Err(IntegrityError::NoHashInSumsFile {
            file: file.to_string(),
            asset: asset.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0dee02804af46386527c8bb94c2ee63592594ca1ec952ea42ffa9df07a9f005e";

    #[test]
    fn reads_sha256sum_format() {
        let contents = format!("{HASH}  ckb-debugger_v1.1.1_x86_64-unknown-linux-gnu.tar.gz\n");
        let found = hash_for_asset(
            &contents,
            "sums.txt",
            "ckb-debugger_v1.1.1_x86_64-unknown-linux-gnu.tar.gz",
        )
        .unwrap();
        assert_eq!(found, HASH);
    }

    #[test]
    fn sha256sum_line_must_name_the_asset() {
        let contents = format!("{HASH}  other.tar.gz\n");
        assert!(hash_for_asset(&contents, "sums.txt", "wanted.tar.gz").is_err());
    }

    #[test]
    fn reads_powershell_table_with_uppercase_hex() {
        let contents = "\r\nAlgorithm       Hash                                                                   Path\r\n\
---------       ----                                                                   ----\r\n\
SHA256          12CE1BA46A7ACBDDEF3EF7A9DCE80C8B7D2F0315B57D6860596EE9F05D5FD003       D:\\a\\ckb-standalone-debugger\\ckb\u{2026}\r\n\r\n";
        let found = hash_for_asset(contents, "sums.txt", "ckb-debugger.tar.gz").unwrap();
        assert_eq!(
            found,
            "12ce1ba46a7acbddef3ef7a9dce80c8b7d2f0315b57d6860596ee9f05d5fd003"
        );
    }

    #[test]
    fn rejects_files_without_a_hash() {
        assert!(hash_for_asset("nothing here\n", "sums.txt", "a.tar.gz").is_err());
    }
}
