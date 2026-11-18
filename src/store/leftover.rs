//! Names for the files and folders ckbup moves out of the way, and the sweep
//! that deletes them later.
//!
//! Windows refuses to delete a running executable, or a folder that holds one,
//! but allows renaming both. So a file leaves `bin/` by being moved aside to
//! `<stem>.old` (then `<stem>.old.1`, `<stem>.old.2` and so on), and a version
//! folder is hidden as `.<version>.removing` (then `.1`, `.2` and so on) before
//! it is deleted. Whatever is still in use when ckbup tries to delete it is
//! left under that name, and the next sweep for its tool tries again.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::Layout;

const MOVED_ASIDE: &str = "old";
const REMOVING: &str = "removing";

/// A free name in `bin_dir` for moving `file` aside.
pub(super) fn moved_aside_path(bin_dir: &Path, file: &str) -> PathBuf {
    first_free(bin_dir, &moved_aside_base(file))
}

/// A free hidden name in `tool_dir` for the folder of `version` while it is removed.
pub(super) fn removing_path(tool_dir: &Path, version: &str) -> PathBuf {
    first_free(tool_dir, &format!(".{version}.{REMOVING}"))
}

/// Deletes what earlier runs left behind for one tool: its hidden `.removing`
/// version folders, and the moved-aside copies of its `executables` in `bin/`.
///
/// Every failure is ignored, because a leftover that is still in use is simply
/// tried again by a later sweep. `.partial` folders are never touched; the next
/// install of that version replaces them.
pub fn sweep(layout: &Layout, tool: &str, executables: &[String]) {
    for path in entries_named(&layout.tool_dir(tool), is_removing_name) {
        let _ = fs::remove_dir_all(path);
    }
    let bases: Vec<String> = executables
        .iter()
        .map(|file| moved_aside_base(file))
        .collect();
    for path in entries_named(&layout.bin_dir(), |name| {
        bases.iter().any(|base| is_numbered(name, base))
    }) {
        let _ = fs::remove_file(path);
    }
}

/// `<stem>.old` for `file`; the `.exe` is dropped, so a moved-aside copy is
/// never found on PATH under the tool's name.
fn moved_aside_base(file: &str) -> String {
    let stem = Path::new(file)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(file);
    format!("{stem}.{MOVED_ASIDE}")
}

/// `dir/base` if nothing is there, otherwise the first free `dir/base.<n>`.
fn first_free(dir: &Path, base: &str) -> PathBuf {
    let mut candidate = dir.join(base);
    let mut n = 0u32;
    while is_taken(&candidate) {
        n += 1;
        candidate = dir.join(format!("{base}.{n}"));
    }
    candidate
}

fn is_taken(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Whether `name` is `base` or `base.<n>`.
fn is_numbered(name: &str, base: &str) -> bool {
    match name.strip_prefix(base) {
        Some("") => true,
        Some(rest) => rest.strip_prefix('.').is_some_and(is_number),
        None => false,
    }
}

/// Whether `name` is `.<version>.removing` or `.<version>.removing.<n>`.
fn is_removing_name(name: &str) -> bool {
    let base = match name.rsplit_once('.') {
        Some((head, n)) if is_number(n) => head,
        _ => name,
    };
    name.starts_with('.') && base.ends_with(&format!(".{REMOVING}"))
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Paths of the entries in `dir` whose UTF-8 names satisfy `wanted`.
fn entries_named(dir: &Path, wanted: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(&wanted))
        .map(|entry| entry.path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake_home;

    #[test]
    fn moved_aside_names_take_the_first_free_number() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path();
        assert_eq!(moved_aside_path(bin, "fnn.exe"), bin.join("fnn.old"));
        assert_eq!(moved_aside_path(bin, "fnn"), bin.join("fnn.old"));
        fs::write(bin.join("fnn.old"), b"").unwrap();
        fs::write(bin.join("fnn.old.1"), b"").unwrap();
        assert_eq!(moved_aside_path(bin, "fnn.exe"), bin.join("fnn.old.2"));
        assert_eq!(
            moved_aside_path(bin, "fnn-cli.exe"),
            bin.join("fnn-cli.old")
        );
    }

    #[test]
    fn removing_names_are_hidden_and_take_the_first_free_number() {
        let dir = tempfile::tempdir().unwrap();
        let tool_dir = dir.path();
        assert_eq!(
            removing_path(tool_dir, "v0.9.1"),
            tool_dir.join(".v0.9.1.removing")
        );
        fs::create_dir(tool_dir.join(".v0.9.1.removing")).unwrap();
        assert_eq!(
            removing_path(tool_dir, "v0.9.1"),
            tool_dir.join(".v0.9.1.removing.1")
        );
    }

    #[test]
    fn recognises_only_its_own_names() {
        for name in ["fnn.old", "fnn.old.1", "fnn.old.12"] {
            assert!(is_numbered(name, "fnn.old"), "{name}");
        }
        for name in [
            "fnn",
            "fnn.exe",
            "fnn.olden",
            "fnn.old.",
            "fnn.old.x",
            "fnn-cli.old",
        ] {
            assert!(!is_numbered(name, "fnn.old"), "{name}");
        }
        for name in [".v0.9.1.removing", ".v0.9.1.removing.3"] {
            assert!(is_removing_name(name), "{name}");
        }
        for name in [
            "v0.9.1",
            ".v0.9.1.partial",
            "v0.9.1.removing",
            ".v0.9.1.removing.x",
        ] {
            assert!(!is_removing_name(name), "{name}");
        }
    }

    #[test]
    fn sweep_deletes_the_tools_leftovers_only() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let bin = layout.bin_dir();
        fs::create_dir_all(&bin).unwrap();
        for file in [
            "fnn",
            "fnn.old",
            "fnn.old.1",
            "fnn-cli",
            "fnn-cli.old",
            "fnn.new",
            "ckb.old",
        ] {
            fs::write(bin.join(file), file).unwrap();
        }
        let fnn = layout.tool_dir("fnn");
        for folder in [
            ".v0.8.0.removing",
            ".v0.8.0.removing.1",
            ".v0.9.2.partial",
            "v0.9.1",
        ] {
            fs::create_dir_all(fnn.join(folder).join("config")).unwrap();
            fs::write(fnn.join(folder).join("fnn"), b"fnn").unwrap();
        }
        let ckb_leftover = layout.tool_dir("ckb").join(".v0.209.0.removing");
        fs::create_dir_all(&ckb_leftover).unwrap();

        sweep(&layout, "fnn", &["fnn".to_string(), "fnn-cli".to_string()]);

        assert_eq!(
            fake_home::names(&bin),
            ["ckb.old", "fnn", "fnn-cli", "fnn.new"]
        );
        assert_eq!(fake_home::names(&fnn), [".v0.9.2.partial", "v0.9.1"]);
        assert!(ckb_leftover.is_dir());
    }

    #[test]
    fn sweep_of_ckb_never_touches_ckb_cli_files() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let bin = layout.bin_dir();
        fs::create_dir_all(&bin).unwrap();
        for file in ["ckb.exe", "ckb.old", "ckb-cli.exe", "ckb-cli.old"] {
            fs::write(bin.join(file), file).unwrap();
        }

        sweep(&layout, "ckb", &["ckb.exe".to_string()]);

        assert_eq!(
            fake_home::names(&bin),
            ["ckb-cli.exe", "ckb-cli.old", "ckb.exe"]
        );
    }

    #[test]
    fn sweep_of_a_missing_home_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        sweep(&layout, "fnn", &["fnn".to_string()]);
        assert!(!layout.bin_dir().exists());
        assert!(!layout.tool_dir("fnn").exists());
    }
}
