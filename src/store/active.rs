use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use super::{Layout, StoreError};

type ActiveVersions = BTreeMap<String, String>;

pub fn active_version(layout: &Layout, tool: &str) -> Result<Option<String>, StoreError> {
    Ok(read(layout)?.remove(tool))
}

/// Copies each of `executables` from `version_dir` into `<home>/bin/` and
/// records `version` as active for `tool`.
///
/// Only the named files are copied; other binaries in the package stay in it.
pub fn activate(
    layout: &Layout,
    tool: &str,
    version: &str,
    version_dir: &Path,
    executables: &[String],
) -> Result<(), StoreError> {
    let bin_dir = layout.bin_dir();
    fs::create_dir_all(&bin_dir).map_err(StoreError::io("cannot create", &bin_dir))?;
    for file in executables {
        replace_file(&version_dir.join(file), &bin_dir.join(file))?;
    }

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
    fs::copy(source, &staged).map_err(StoreError::io("cannot copy", source))?;
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

    fn layout(dir: &Path) -> Layout {
        Layout::resolve(Some(dir.join("home").into()), None).unwrap()
    }

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn activation_copies_binary_and_records_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let version_dir = dir.path().join("package");
        fs::create_dir(&version_dir).unwrap();
        let binary = version_dir.join("tool.exe");
        fs::write(&binary, b"v1").unwrap();

        assert_eq!(active_version(&layout, "tool").unwrap(), None);
        activate(&layout, "tool", "v1", &version_dir, &files(&["tool.exe"])).unwrap();
        fs::write(&binary, b"v2").unwrap();
        activate(&layout, "tool", "v2", &version_dir, &files(&["tool.exe"])).unwrap();

        assert_eq!(
            active_version(&layout, "tool").unwrap().as_deref(),
            Some("v2")
        );
        assert_eq!(fs::read(layout.bin_dir().join("tool.exe")).unwrap(), b"v2");
        assert!(!layout.bin_dir().join("tool.old").exists());
    }

    #[test]
    fn activating_fnn_puts_fnn_and_fnn_cli_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let version_dir = layout.toolchain_dir("fnn", "v0.9.1");
        fs::create_dir_all(version_dir.join("config")).unwrap();
        fs::write(version_dir.join("fnn"), b"node").unwrap();
        fs::write(version_dir.join("fnn-cli"), b"cli").unwrap();

        activate(
            &layout,
            "fnn",
            "v0.9.1",
            &version_dir,
            &files(&["fnn", "fnn-cli"]),
        )
        .unwrap();

        assert_eq!(fs::read(layout.bin_dir().join("fnn")).unwrap(), b"node");
        assert_eq!(fs::read(layout.bin_dir().join("fnn-cli")).unwrap(), b"cli");
        assert!(!layout.bin_dir().join("config").exists());
    }

    #[test]
    fn activating_ckb_leaves_its_bundled_ckb_cli_out_of_bin() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let version_dir = layout.toolchain_dir("ckb", "v0.210.0");
        fs::create_dir_all(&version_dir).unwrap();
        fs::write(version_dir.join("ckb"), b"node").unwrap();
        fs::write(version_dir.join("ckb-cli"), b"bundled").unwrap();

        activate(&layout, "ckb", "v0.210.0", &version_dir, &files(&["ckb"])).unwrap();

        assert!(layout.bin_dir().join("ckb").is_file());
        assert!(!layout.bin_dir().join("ckb-cli").exists());
    }
}
