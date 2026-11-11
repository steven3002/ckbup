use std::fs;
use std::path::{Path, PathBuf};

use super::ArchiveError;

/// Finds a regular file named `name` anywhere under `root`.
///
/// Release layouts differ between tools (flat or under a top folder), so the
/// binary is located by name. Symlinks are not followed.
pub fn find_file(root: &Path, name: &str) -> Result<PathBuf, ArchiveError> {
    search(root, name)
        .map_err(|source| ArchiveError::Read {
            path: root.to_path_buf(),
            source,
        })?
        .ok_or_else(|| ArchiveError::BinaryNotFound {
            name: name.to_string(),
        })
}

fn search(dir: &Path, name: &str) -> std::io::Result<Option<PathBuf>> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut subdirs = Vec::new();
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_file() && entry.file_name() == name {
            return Ok(Some(entry.path()));
        }
        if file_type.is_dir() {
            subdirs.push(entry.path());
        }
    }
    for subdir in subdirs {
        if let Some(found) = search(&subdir, name)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_flat_and_nested_binaries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("LICENSE"), b"").unwrap();
        fs::write(dir.path().join("ckb-debugger.exe"), b"").unwrap();
        let nested = dir.path().join("ckb-cli_v2.0.0_x86_64-pc-windows-msvc");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("ckb-cli.exe"), b"").unwrap();

        assert_eq!(
            find_file(dir.path(), "ckb-debugger.exe").unwrap(),
            dir.path().join("ckb-debugger.exe")
        );
        assert_eq!(
            find_file(dir.path(), "ckb-cli.exe").unwrap(),
            nested.join("ckb-cli.exe")
        );
    }

    #[test]
    fn reports_missing_binary() {
        let dir = tempfile::tempdir().unwrap();
        let err = find_file(dir.path(), "ckb-cli").unwrap_err();
        assert_eq!(
            err.to_string(),
            "`ckb-cli` was not found in the extracted archive"
        );
    }
}
