//! Fake ckbup homes for unit tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::Layout;

/// A layout rooted at `<dir>/home`, which does not exist yet.
pub fn layout(dir: &Path) -> Layout {
    Layout::resolve(Some(dir.join("home").into()), None).unwrap()
}

/// Creates `toolchains/<tool>/<version>/` holding `files`, each containing
/// `<tool> <version> <file>` so that copies of different versions differ.
pub fn install(layout: &Layout, tool: &str, version: &str, files: &[&str]) -> PathBuf {
    let dir = layout.toolchain_dir(tool, version);
    fs::create_dir_all(&dir).unwrap();
    for file in files {
        fs::write(dir.join(file), contents(tool, version, file)).unwrap();
    }
    dir
}

/// The bytes `install` writes for `file`.
pub fn contents(tool: &str, version: &str, file: &str) -> Vec<u8> {
    format!("{tool} {version} {file}").into_bytes()
}

/// Sorted names of the entries in `dir`, or none if it does not exist.
pub fn names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// Every folder and file under `root` with the bytes of each file, so that a
/// test can check that nothing changed.
pub fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().display().to_string();
            if path.is_dir() {
                out.insert(relative, None);
                walk(&path, root, out);
            } else {
                out.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
