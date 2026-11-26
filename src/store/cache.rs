//! The archive cache, `<home>/cache/<tool>/<version>/<asset>`.
//!
//! An archive is kept only while the switch is on, and only after it matched
//! a published checksum. Every install checks a cached archive against its
//! own checksums before using it, whatever the switch says. Nothing deletes
//! from the cache except a cached archive that fails that check; clearing it
//! is left to the user.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Layout, StoreError};

const SWITCH: &str = "CKBUP_KEEP_ARCHIVES";

/// Whether new verified archives are kept: always with `--keep-archives`
/// (`flag`), otherwise when `CKBUP_KEEP_ARCHIVES` is `1`. Unset, empty and `0`
/// leave it off, and any other value is an error.
pub fn keep_archives(flag: bool) -> Result<bool, StoreError> {
    if flag {
        return Ok(true);
    }
    parse_switch(std::env::var_os(SWITCH).as_deref())
}

fn parse_switch(value: Option<&OsStr>) -> Result<bool, StoreError> {
    match value.map(OsStr::to_str) {
        None | Some(Some("" | "0")) => Ok(false),
        Some(Some("1")) => Ok(true),
        Some(_) => Err(StoreError::KeepArchivesValue),
    }
}

/// The cached archive `name` of `tool` at `version`, if there is one.
pub fn cached_archive(layout: &Layout, tool: &str, version: &str, name: &str) -> Option<PathBuf> {
    let path = layout.cache_dir(tool, version).join(name);
    path.is_file().then_some(path)
}

/// Moves the verified `archive` into the cache of `tool` at `version`,
/// replacing a cached file of the same name, and returns its new path.
pub fn keep_archive(
    layout: &Layout,
    tool: &str,
    version: &str,
    archive: &Path,
) -> Result<PathBuf, StoreError> {
    let dir = layout.cache_dir(tool, version);
    fs::create_dir_all(&dir).map_err(StoreError::io("cannot create", &dir))?;
    let target = dir.join(archive.file_name().unwrap_or(archive.as_os_str()));
    fs::rename(archive, &target).map_err(StoreError::io("cannot keep the archive in", &target))?;
    Ok(target)
}

/// Deletes a cached archive that no longer matches its checksums. A copy that
/// cannot be deleted is harmless, because it is checked again before any use.
pub fn discard_cached(path: &Path) {
    let _ = fs::remove_file(path);
}

/// The cache folder of `tool` at `version` with the total size of its files,
/// when it holds any.
pub fn cached_size(layout: &Layout, tool: &str, version: &str) -> Option<(PathBuf, u64)> {
    let dir = layout.cache_dir(tool, version);
    let sizes: Vec<u64> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(fs::Metadata::is_file)
        .map(|meta| meta.len())
        .collect();
    (!sizes.is_empty()).then(|| (dir, sizes.iter().sum()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake_home;

    #[test]
    fn the_switch_accepts_1_0_and_empty_and_rejects_anything_else() {
        assert!(!parse_switch(None).unwrap());
        assert!(!parse_switch(Some(OsStr::new(""))).unwrap());
        assert!(!parse_switch(Some(OsStr::new("0"))).unwrap());
        assert!(parse_switch(Some(OsStr::new("1"))).unwrap());
        for value in ["2", "yes", "true", " 1"] {
            let err = parse_switch(Some(OsStr::new(value))).unwrap_err();
            assert_eq!(err.to_string(), "CKBUP_KEEP_ARCHIVES must be 1 or 0");
        }
    }

    #[test]
    fn the_flag_turns_the_switch_on() {
        assert!(keep_archives(true).unwrap());
    }

    #[test]
    fn keeps_finds_and_sizes_an_archive() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let work = dir.path().join("work");
        fs::create_dir_all(&work).unwrap();
        fs::write(work.join("tool.tar.gz"), vec![7u8; 1500]).unwrap();
        assert_eq!(
            cached_archive(&layout, "tool", "v1.0.0", "tool.tar.gz"),
            None
        );
        assert_eq!(cached_size(&layout, "tool", "v1.0.0"), None);

        let kept = keep_archive(&layout, "tool", "v1.0.0", &work.join("tool.tar.gz")).unwrap();

        let folder = layout.cache_dir("tool", "v1.0.0");
        assert_eq!(kept, folder.join("tool.tar.gz"));
        assert!(!work.join("tool.tar.gz").exists());
        assert_eq!(
            cached_archive(&layout, "tool", "v1.0.0", "tool.tar.gz"),
            Some(kept)
        );
        assert_eq!(cached_size(&layout, "tool", "v1.0.0"), Some((folder, 1500)));
    }

    #[test]
    fn keeping_replaces_a_cached_file_of_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let folder = layout.cache_dir("tool", "v1.0.0");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("tool.tar.gz"), b"old").unwrap();
        fs::write(dir.path().join("tool.tar.gz"), b"new").unwrap();

        keep_archive(&layout, "tool", "v1.0.0", &dir.path().join("tool.tar.gz")).unwrap();

        assert_eq!(fs::read(folder.join("tool.tar.gz")).unwrap(), b"new");
    }

    #[test]
    fn an_empty_cache_folder_has_no_size() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fs::create_dir_all(layout.cache_dir("tool", "v1.0.0")).unwrap();
        assert_eq!(cached_size(&layout, "tool", "v1.0.0"), None);
    }
}
