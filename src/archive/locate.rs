use std::fs;
use std::path::{Path, PathBuf};

use super::ArchiveError;

/// Returns the root of the package extracted into `extracted`.
///
/// When the archive holds exactly one entry and it is a folder, the package is
/// that folder; otherwise the archive is flat and the package is `extracted`
/// itself. Every name in `required` must be a regular file directly in the root.
pub fn package_root(extracted: &Path, required: &[String]) -> Result<PathBuf, ArchiveError> {
    let read_err = |source| ArchiveError::Read {
        path: extracted.to_path_buf(),
        source,
    };
    let entries = fs::read_dir(extracted)
        .map_err(read_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_err)?;
    let root = match entries.as_slice() {
        [only] if only.file_type().map_err(read_err)?.is_dir() => only.path(),
        _ => extracted.to_path_buf(),
    };
    for name in required {
        let is_file = fs::symlink_metadata(root.join(name)).is_ok_and(|meta| meta.is_file());
        if !is_file {
            return Err(ArchiveError::BinaryNotFound { name: name.clone() });
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn strips_a_single_top_folder() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().join("ckb_v0.210.0_x86_64-pc-windows-msvc");
        fs::create_dir_all(top.join("docs")).unwrap();
        fs::write(top.join("ckb.exe"), b"").unwrap();
        fs::write(top.join("ckb-cli.exe"), b"").unwrap();

        assert_eq!(
            package_root(dir.path(), &required(&["ckb.exe"])).unwrap(),
            top
        );

        let cli = tempfile::tempdir().unwrap();
        let cli_top = cli.path().join("ckb-cli_v2.0.0_x86_64-unknown-linux-gnu");
        fs::create_dir(&cli_top).unwrap();
        fs::write(cli_top.join("ckb-cli"), b"").unwrap();
        fs::write(cli_top.join("COPYING"), b"").unwrap();
        assert_eq!(
            package_root(cli.path(), &required(&["ckb-cli"])).unwrap(),
            cli_top
        );
    }

    #[test]
    fn keeps_flat_archives_as_they_are() {
        let fnn = tempfile::tempdir().unwrap();
        fs::write(fnn.path().join("fnn.exe"), b"").unwrap();
        fs::write(fnn.path().join("fnn-cli.exe"), b"").unwrap();
        fs::create_dir_all(fnn.path().join("config").join("mainnet")).unwrap();
        assert_eq!(
            package_root(fnn.path(), &required(&["fnn.exe", "fnn-cli.exe"])).unwrap(),
            fnn.path()
        );

        let light_client = tempfile::tempdir().unwrap();
        fs::write(light_client.path().join("ckb-light-client"), b"").unwrap();
        fs::create_dir(light_client.path().join("config")).unwrap();
        assert_eq!(
            package_root(light_client.path(), &required(&["ckb-light-client"])).unwrap(),
            light_client.path()
        );

        let debugger = tempfile::tempdir().unwrap();
        fs::write(debugger.path().join("ckb-debugger.exe"), b"").unwrap();
        fs::write(debugger.path().join("LICENSE"), b"").unwrap();
        assert_eq!(
            package_root(debugger.path(), &required(&["ckb-debugger.exe"])).unwrap(),
            debugger.path()
        );
    }

    #[test]
    fn reports_a_missing_binary_or_companion() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("fnn"), b"").unwrap();
        fs::create_dir(dir.path().join("config")).unwrap();
        let err = package_root(dir.path(), &required(&["fnn", "fnn-cli"])).unwrap_err();
        assert_eq!(
            err.to_string(),
            "the release package has no `fnn-cli` at its top level"
        );
    }
}
