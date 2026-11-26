use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{InstallRecord, Layout, StoreError, leftover};

/// Returns the version folder of `tool` at `version` if its main binary is there.
///
/// Folders that hold only the binary, as written by earlier ckbup releases,
/// count as installed and are left untouched.
pub fn installed_dir(
    layout: &Layout,
    tool: &str,
    version: &str,
    binary_file: &str,
) -> Option<PathBuf> {
    let dir = layout.toolchain_dir(tool, version);
    dir.join(binary_file).is_file().then_some(dir)
}

/// Moves an extracted release package into `<home>/toolchains/<tool>/<version>/`.
///
/// The package is staged under a hidden name in the same parent folder and then
/// renamed into place, so the version folder never holds a partial install.
/// Before the final rename, `executables` are marked executable on Unix and
/// `record` is written into the staged folder.
pub fn place_package(
    layout: &Layout,
    tool: &str,
    version: &str,
    package_root: &Path,
    executables: &[String],
    record: &InstallRecord,
) -> Result<PathBuf, StoreError> {
    let target = layout.toolchain_dir(tool, version);
    if target.exists() {
        return Err(StoreError::Occupied { path: target });
    }
    let parent = layout.tool_dir(tool);
    fs::create_dir_all(&parent).map_err(StoreError::io("cannot create", &parent))?;

    let staged = parent.join(format!(".{version}.partial"));
    match fs::remove_dir_all(&staged) {
        Err(err) if err.kind() != ErrorKind::NotFound => {
            return Err(StoreError::io("cannot remove", &staged)(err));
        }
        _ => {}
    }
    fs::rename(package_root, &staged)
        .map_err(StoreError::io("cannot move the package to", &staged))?;
    for file in executables {
        make_executable(&staged.join(file))?;
    }
    record.write_to(&staged)?;
    fs::rename(&staged, &target).map_err(StoreError::io("cannot write", &target))?;
    Ok(target)
}

/// Renames the folder of `tool` at `version` to a free hidden
/// `.<version>.removing` name and returns that name.
///
/// The folder stops counting as installed at once, and a deletion that stops
/// part-way can only leave a hidden folder behind. If the rename fails, which
/// happens when a program or a terminal is using the folder, nothing changes.
pub fn hide_version(layout: &Layout, tool: &str, version: &str) -> Result<PathBuf, StoreError> {
    let dir = layout.toolchain_dir(tool, version);
    let hidden = leftover::removing_path(&layout.tool_dir(tool), version);
    fs::rename(&dir, &hidden).map_err(|source| match source.kind() {
        ErrorKind::NotFound => StoreError::io("cannot rename", &dir)(source),
        _ => StoreError::InUse {
            tool: tool.to_string(),
            version: version.to_string(),
            path: dir.clone(),
            source,
        },
    })?;
    Ok(hidden)
}

/// Gives a folder hidden by `hide_version` its version name back.
pub fn unhide_version(
    layout: &Layout,
    tool: &str,
    version: &str,
    hidden: &Path,
) -> Result<(), StoreError> {
    let dir = layout.toolchain_dir(tool, version);
    fs::rename(hidden, &dir).map_err(StoreError::io("cannot restore", &dir))
}

/// Deletes a folder hidden by `hide_version`. Returns `false` when part of it
/// is still in use, such as a binary from it that still runs; the next sweep
/// for the tool tries again.
pub fn delete_hidden(hidden: &Path) -> bool {
    match fs::remove_dir_all(hidden) {
        Ok(()) => true,
        Err(err) => err.kind() == ErrorKind::NotFound,
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(StoreError::io("cannot set permissions on", path))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Variant;

    fn layout(dir: &Path) -> Layout {
        Layout::resolve(Some(dir.join("home").into()), None).unwrap()
    }

    fn record(archive: &str) -> InstallRecord {
        InstallRecord {
            archive: archive.to_string(),
            variant: Variant::Portable,
        }
    }

    #[test]
    fn binary_only_version_folder_counts_as_installed() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let version_dir = layout.toolchain_dir("ckb-cli", "v2.0.0");
        fs::create_dir_all(&version_dir).unwrap();
        fs::write(version_dir.join("ckb-cli.exe"), b"s1").unwrap();

        assert_eq!(
            installed_dir(&layout, "ckb-cli", "v2.0.0", "ckb-cli.exe"),
            Some(version_dir.clone())
        );
        assert_eq!(
            installed_dir(&layout, "ckb-cli", "v1.0.0", "ckb-cli.exe"),
            None
        );
        assert_eq!(fs::read(version_dir.join("ckb-cli.exe")).unwrap(), b"s1");
    }

    #[test]
    fn moves_the_whole_package_into_place() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let package = dir.path().join("extracted");
        fs::create_dir_all(package.join("config").join("testnet")).unwrap();
        fs::write(package.join("fnn"), b"fnn").unwrap();
        fs::write(package.join("fnn-cli"), b"cli").unwrap();
        fs::write(
            package.join("config").join("testnet").join("config.yml"),
            b"",
        )
        .unwrap();

        let executables = ["fnn".to_string(), "fnn-cli".to_string()];
        let archive = "fnn_v0.9.1-x86_64-linux-portable.tar.gz";
        let placed = place_package(
            &layout,
            "fnn",
            "v0.9.1",
            &package,
            &executables,
            &record(archive),
        )
        .unwrap();

        assert_eq!(placed, layout.toolchain_dir("fnn", "v0.9.1"));
        assert_eq!(fs::read(placed.join("fnn-cli")).unwrap(), b"cli");
        assert_eq!(InstallRecord::read_from(&placed), Some(record(archive)));
        assert!(
            placed
                .join("config")
                .join("testnet")
                .join("config.yml")
                .is_file()
        );
        assert!(!package.exists());
        assert!(!layout.tool_dir("fnn").join(".v0.9.1.partial").exists());
    }

    #[test]
    fn refuses_to_overwrite_an_existing_version_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let existing = layout.toolchain_dir("ckb", "v0.210.0");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("notes.txt"), b"keep").unwrap();
        let package = dir.path().join("extracted");
        fs::create_dir_all(&package).unwrap();

        let err = place_package(
            &layout,
            "ckb",
            "v0.210.0",
            &package,
            &[],
            &record("ckb.tar.gz"),
        )
        .unwrap_err();
        assert!(matches!(err, StoreError::Occupied { .. }));
        assert_eq!(fs::read(existing.join("notes.txt")).unwrap(), b"keep");
        assert_eq!(InstallRecord::read_from(&existing), None);
    }

    #[test]
    fn hides_then_deletes_a_version_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let version_dir = layout.toolchain_dir("fnn", "v0.9.1");
        fs::create_dir_all(version_dir.join("config")).unwrap();
        fs::write(version_dir.join("fnn"), b"fnn").unwrap();

        let hidden = hide_version(&layout, "fnn", "v0.9.1").unwrap();
        assert_eq!(hidden, layout.tool_dir("fnn").join(".v0.9.1.removing"));
        assert!(!version_dir.exists());
        assert_eq!(installed_dir(&layout, "fnn", "v0.9.1", "fnn"), None);

        assert!(delete_hidden(&hidden));
        assert!(!hidden.exists());
        assert!(delete_hidden(&hidden));
    }

    #[test]
    fn unhiding_restores_the_version_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let version_dir = layout.toolchain_dir("fnn", "v0.9.1");
        fs::create_dir_all(&version_dir).unwrap();
        fs::write(version_dir.join("fnn"), b"fnn").unwrap();
        fs::create_dir(layout.tool_dir("fnn").join(".v0.9.1.removing")).unwrap();

        let hidden = hide_version(&layout, "fnn", "v0.9.1").unwrap();
        assert_eq!(hidden, layout.tool_dir("fnn").join(".v0.9.1.removing.1"));
        unhide_version(&layout, "fnn", "v0.9.1", &hidden).unwrap();

        assert_eq!(fs::read(version_dir.join("fnn")).unwrap(), b"fnn");
        assert!(!hidden.exists());
    }

    #[test]
    fn hiding_a_missing_folder_is_not_reported_as_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        fs::create_dir_all(layout.tool_dir("fnn")).unwrap();

        let err = hide_version(&layout, "fnn", "v0.9.1").unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }), "{err}");
    }
}
