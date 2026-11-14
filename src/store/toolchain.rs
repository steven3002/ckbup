use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{Layout, StoreError};

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
/// `executables` are marked executable on Unix before the final rename.
pub fn place_package(
    layout: &Layout,
    tool: &str,
    version: &str,
    package_root: &Path,
    executables: &[String],
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
    fs::rename(&staged, &target).map_err(StoreError::io("cannot write", &target))?;
    Ok(target)
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

    fn layout(dir: &Path) -> Layout {
        Layout::resolve(Some(dir.join("home").into()), None).unwrap()
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
        let placed = place_package(&layout, "fnn", "v0.9.1", &package, &executables).unwrap();

        assert_eq!(placed, layout.toolchain_dir("fnn", "v0.9.1"));
        assert_eq!(fs::read(placed.join("fnn-cli")).unwrap(), b"cli");
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

        let err = place_package(&layout, "ckb", "v0.210.0", &package, &[]).unwrap_err();
        assert!(matches!(err, StoreError::Occupied { .. }));
        assert_eq!(fs::read(existing.join("notes.txt")).unwrap(), b"keep");
    }
}
