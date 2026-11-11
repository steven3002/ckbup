use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use super::{Layout, StoreError};

type ActiveVersions = BTreeMap<String, String>;

pub fn active_version(layout: &Layout, tool: &str) -> Result<Option<String>, StoreError> {
    Ok(read(layout)?.remove(tool))
}

/// Copies `binary` into `<home>/bin/` and records `version` as active for `tool`.
pub fn activate(
    layout: &Layout,
    tool: &str,
    version: &str,
    binary: &Path,
    file_name: &str,
) -> Result<(), StoreError> {
    let bin_dir = layout.bin_dir();
    fs::create_dir_all(&bin_dir).map_err(StoreError::io("cannot create", &bin_dir))?;
    replace_file(binary, &bin_dir.join(file_name))?;

    let mut versions = read(layout)?;
    versions.insert(tool.to_string(), version.to_string());
    write(layout, &versions)
}

/// Replaces `target` with a copy of `source`.
///
/// Windows refuses to overwrite a running executable but allows renaming it,
/// so an existing target is moved aside before the new copy is renamed in.
fn replace_file(source: &Path, target: &Path) -> Result<(), StoreError> {
    let staged = target.with_extension("new");
    let previous = target.with_extension("old");
    fs::copy(source, &staged).map_err(StoreError::io("cannot write", &staged))?;
    if target.exists() {
        let _ = fs::remove_file(&previous);
        fs::rename(target, &previous).map_err(StoreError::io("cannot replace", target))?;
    }
    fs::rename(&staged, target).map_err(StoreError::io("cannot write", target))?;
    // A still-running previous binary stays locked on Windows; it is removed on a later run.
    let _ = fs::remove_file(&previous);
    Ok(())
}

fn read(layout: &Layout) -> Result<ActiveVersions, StoreError> {
    let path = layout.active_file();
    match fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|source| StoreError::InvalidActiveFile { path, source })
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(ActiveVersions::new()),
        Err(err) => Err(StoreError::io("cannot read", &path)(err)),
    }
}

fn write(layout: &Layout, versions: &ActiveVersions) -> Result<(), StoreError> {
    let path = layout.active_file();
    let staged = path.with_extension("toml.new");
    let text = toml::to_string(versions).expect("a string map always serializes to TOML");
    fs::write(&staged, text).map_err(StoreError::io("cannot write", &staged))?;
    fs::rename(&staged, &path).map_err(StoreError::io("cannot write", &path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_copies_binary_and_records_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::resolve(Some(dir.path().join("home").into()), None).unwrap();
        let binary = dir.path().join("tool.exe");
        fs::write(&binary, b"v1").unwrap();

        assert_eq!(active_version(&layout, "tool").unwrap(), None);
        activate(&layout, "tool", "v1", &binary, "tool.exe").unwrap();
        fs::write(&binary, b"v2").unwrap();
        activate(&layout, "tool", "v2", &binary, "tool.exe").unwrap();

        assert_eq!(
            active_version(&layout, "tool").unwrap().as_deref(),
            Some("v2")
        );
        assert_eq!(fs::read(layout.bin_dir().join("tool.exe")).unwrap(), b"v2");
        assert!(!layout.bin_dir().join("tool.old").exists());
    }
}
