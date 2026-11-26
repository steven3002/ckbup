//! The install record `.ckbup.toml` in a version folder: the release archive
//! the version came from and its build variant, so that `update` keeps a
//! portable build portable.
//!
//! It is written only into the staged folder of a new install. Version folders
//! from before records existed have none and are never rewritten.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::StoreError;
use crate::tools::Variant;

const RECORD_FILE: &str = ".ckbup.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    pub archive: String,
    pub variant: Variant,
}

#[derive(Serialize, Deserialize)]
struct RecordFile {
    archive: String,
    variant: String,
}

impl InstallRecord {
    /// Writes the record into `dir`, the staged folder of a new version.
    pub fn write_to(&self, dir: &Path) -> Result<(), StoreError> {
        let file = RecordFile {
            archive: self.archive.clone(),
            variant: variant_name(self.variant).to_string(),
        };
        let text = toml::to_string(&file).expect("two strings always serialize to TOML");
        let path = dir.join(RECORD_FILE);
        fs::write(&path, text).map_err(StoreError::io("cannot write", &path))
    }

    /// The record of the version folder `dir`. `None` when it has none, as
    /// with versions installed before records existed, or when the record
    /// cannot be read.
    pub fn read_from(dir: &Path) -> Option<Self> {
        let text = fs::read_to_string(dir.join(RECORD_FILE)).ok()?;
        let file: RecordFile = toml::from_str(&text).ok()?;
        Some(InstallRecord {
            archive: file.archive,
            variant: variant_from_name(&file.variant)?,
        })
    }
}

fn variant_name(variant: Variant) -> &'static str {
    match variant {
        Variant::Standard => "standard",
        Variant::Portable => "portable",
    }
}

fn variant_from_name(name: &str) -> Option<Variant> {
    match name {
        "standard" => Some(Variant::Standard),
        "portable" => Some(Variant::Portable),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_the_archive_and_variant() {
        let dir = tempfile::tempdir().unwrap();
        let record = InstallRecord {
            archive: "fnn_v0.10.1-x86_64-linux-portable.tar.gz".to_string(),
            variant: Variant::Portable,
        };

        record.write_to(dir.path()).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join(".ckbup.toml")).unwrap(),
            "archive = \"fnn_v0.10.1-x86_64-linux-portable.tar.gz\"\nvariant = \"portable\"\n"
        );
        assert_eq!(InstallRecord::read_from(dir.path()), Some(record));
    }

    #[test]
    fn a_folder_without_a_readable_record_has_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(InstallRecord::read_from(dir.path()), None);

        fs::write(
            dir.path().join(".ckbup.toml"),
            "archive = \"a.tar.gz\"\nvariant = \"fast\"\n",
        )
        .unwrap();
        assert_eq!(InstallRecord::read_from(dir.path()), None);
    }
}
