//! The record kept beside a partial download, `<asset>.part.toml`. It tells a
//! later run whether the bytes on disk belong to the same release asset.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Asset;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartRecord {
    /// The asset's `browser_download_url`, never the signed, expiring URL that
    /// GitHub redirects it to.
    pub url: String,
    pub id: u64,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// Validators of the response the bytes on disk came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

impl PartRecord {
    /// A record for a download of `asset` that has received nothing yet.
    pub fn for_asset(asset: &Asset) -> Self {
        PartRecord {
            url: asset.browser_download_url.clone(),
            id: asset.id,
            size: asset.size,
            digest: asset.digest.clone(),
            etag: None,
            last_modified: None,
        }
    }

    /// Whether the record was written for `asset`: the same URL, id, size and
    /// digest. A file replaced under the same name gets a new id.
    pub fn describes(&self, asset: &Asset) -> bool {
        self.url == asset.browser_download_url
            && self.id == asset.id
            && self.size == asset.size
            && self.digest == asset.digest
    }

    /// The record at `path`; `None` when it is missing or cannot be read.
    pub fn read(path: &Path) -> Option<Self> {
        toml::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(io::Error::other)?;
        fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> Asset {
        Asset {
            id: 753671710,
            name: "ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz".to_string(),
            size: 3013794,
            browser_download_url: "https://github.com/nervosnetwork/ckb-standalone-debugger/releases/download/v1.1.2/ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz".to_string(),
            digest: Some(
                "sha256:1b03e2b6b83be85c1fa2410140eb75aa699a08ab430ecfc6d4fad99443b5b690"
                    .to_string(),
            ),
        }
    }

    #[test]
    fn round_trips_through_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset.part.toml");
        let mut record = PartRecord::for_asset(&asset());
        record.etag = Some("\"0x8DF43A6F4907D84\"".to_string());
        record.last_modified = Some("Mon, 16 Nov 2026 06:00:45 GMT".to_string());

        record.write(&path).unwrap();

        assert_eq!(PartRecord::read(&path), Some(record));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("id = 753671710\n"), "{text}");
        assert!(text.contains("size = 3013794\n"), "{text}");
    }

    #[test]
    fn a_missing_or_damaged_record_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset.part.toml");
        assert_eq!(PartRecord::read(&path), None);
        fs::write(&path, "url = \"cut off").unwrap();
        assert_eq!(PartRecord::read(&path), None);
    }

    #[test]
    fn describes_only_the_same_asset() {
        let record = PartRecord::for_asset(&asset());
        assert!(record.describes(&asset()));

        let replaced = Asset {
            id: 753671711,
            ..asset()
        };
        let resized = Asset {
            size: 3013795,
            ..asset()
        };
        let unhashed = Asset {
            digest: None,
            ..asset()
        };
        for other in [replaced, resized, unhashed] {
            assert!(!record.describes(&other), "{other:?}");
        }
    }
}
