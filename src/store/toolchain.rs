use std::fs;
use std::path::{Path, PathBuf};

use super::{Layout, StoreError};

/// Returns the installed binary for `tool` at `version`, if present.
pub fn installed_binary(
    layout: &Layout,
    tool: &str,
    version: &str,
    file_name: &str,
) -> Option<PathBuf> {
    let path = layout.toolchain_dir(tool, version).join(file_name);
    path.is_file().then_some(path)
}

/// Copies an extracted binary into `<home>/toolchains/<tool>/<version>/`.
///
/// The file is written under a temporary name and renamed into place, so an
/// interrupted copy never leaves a partial binary at the final path.
pub fn place_binary(
    layout: &Layout,
    tool: &str,
    version: &str,
    source: &Path,
    file_name: &str,
) -> Result<PathBuf, StoreError> {
    let dir = layout.toolchain_dir(tool, version);
    fs::create_dir_all(&dir).map_err(StoreError::io("cannot create", &dir))?;
    let target = dir.join(file_name);
    let partial = dir.join(format!("{file_name}.partial"));
    fs::copy(source, &partial).map_err(StoreError::io("cannot write", &partial))?;
    make_executable(&partial)?;
    fs::rename(&partial, &target).map_err(StoreError::io("cannot write", &target))?;
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
