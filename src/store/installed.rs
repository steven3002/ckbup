use std::fs;
use std::io::ErrorKind;

use crate::tools::newest_first;

use super::{Layout, StoreError, installed_dir};

/// The installed versions of `tool`, newest first.
///
/// A folder under `toolchains/<tool>/` counts when it holds the main binary
/// `binary_file`, the rule `installed_dir` applies, so folders that hold only
/// the binary count too. Hidden names, such as `.<version>.partial` and
/// `.<version>.removing` folders, and entries that are not folders are skipped.
pub fn installed_versions(
    layout: &Layout,
    tool: &str,
    binary_file: &str,
) -> Result<Vec<String>, StoreError> {
    let dir = layout.tool_dir(tool);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(StoreError::io("cannot read", &dir)(err)),
    };
    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry.map_err(StoreError::io("cannot read", &dir))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        if installed_dir(layout, tool, &name, binary_file).is_some() {
            versions.push(name);
        }
    }
    versions.sort_by(|a, b| newest_first(a, b));
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake_home;

    #[test]
    fn lists_installed_folders_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fake_home::install(&layout, "fnn", "v0.9.1", &["fnn", "fnn-cli"]);
        fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        fake_home::install(&layout, "fnn", ".v0.9.2.partial", &["fnn"]);
        fake_home::install(&layout, "fnn", ".v0.8.0.removing", &["fnn"]);
        fake_home::install(&layout, "fnn", "v0.7.0", &["README.md"]);
        fs::write(layout.tool_dir("fnn").join("notes.txt"), b"stray").unwrap();

        assert_eq!(
            installed_versions(&layout, "fnn", "fnn").unwrap(),
            ["v0.10.1", "v0.9.1"]
        );
    }

    #[test]
    fn a_folder_holding_only_the_binary_counts() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fake_home::install(&layout, "ckb-cli", "v2.0.0", &["ckb-cli.exe"]);

        assert_eq!(
            installed_versions(&layout, "ckb-cli", "ckb-cli.exe").unwrap(),
            ["v2.0.0"]
        );
    }

    #[test]
    fn a_tool_never_installed_has_no_versions() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        assert!(
            installed_versions(&layout, "fnn", "fnn")
                .unwrap()
                .is_empty()
        );
        assert!(!layout.home().exists());
    }
}
