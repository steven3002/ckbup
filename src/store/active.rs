use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{Layout, StoreError, bin_swap};

/// Tool name to active version, as recorded in `active.toml`.
pub type ActiveVersions = BTreeMap<String, String>;

/// What `activate` changed in `<home>/bin/`.
#[derive(Debug, PartialEq, Eq)]
pub struct Activation {
    /// The tool's executables now in `bin/`, main binary first.
    pub on_path: Vec<PathBuf>,
    /// The tool's executables that the version does not include, taken out of `bin/`.
    pub removed: Vec<PathBuf>,
}

pub fn active_version(layout: &Layout, tool: &str) -> Result<Option<String>, StoreError> {
    Ok(active_versions(layout)?.remove(tool))
}

/// Reads the whole `active.toml` table; a missing file is an empty table.
pub fn active_versions(layout: &Layout) -> Result<ActiveVersions, StoreError> {
    let path = layout.active_file();
    match fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|source| StoreError::InvalidActiveFile { path, source })
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(ActiveVersions::new()),
        Err(err) => Err(StoreError::io("cannot read", &path)(err)),
    }
}

/// Makes `version` the active version of `tool`: copies the `executables`
/// that `version_dir` holds into `<home>/bin/` and records the version in
/// `active.toml`.
///
/// `executables` lists the main binary, then its companions. The main binary
/// must be in `version_dir`; a companion it lacks is taken out of `bin/`. Every
/// copy is staged before any file in `bin/` changes, and the files in place are
/// moved aside rather than overwritten, because one of them may be running. If
/// a step fails, `bin/` and `active.toml` are left as they were. Other binaries
/// in the package, such as ckb's bundled `ckb-cli`, are never copied.
pub fn activate(
    layout: &Layout,
    tool: &str,
    version: &str,
    version_dir: &Path,
    executables: &[String],
) -> Result<Activation, StoreError> {
    if let Some(main) = executables.first() {
        let path = version_dir.join(main);
        if !path.is_file() {
            return Err(StoreError::MissingBinary { path });
        }
    }
    let mut versions = active_versions(layout)?;
    let bin_dir = layout.bin_dir();
    fs::create_dir_all(&bin_dir).map_err(StoreError::io("cannot create", &bin_dir))?;

    let (provided, lacking): (Vec<&String>, Vec<&String>) = executables
        .iter()
        .partition(|file| version_dir.join(file).is_file());
    let copies: Vec<(PathBuf, String)> = provided
        .iter()
        .map(|file| (version_dir.join(file), file.to_string()))
        .collect();
    let stale: Vec<String> = lacking
        .into_iter()
        .filter(|file| exists(&bin_dir.join(file)))
        .cloned()
        .collect();

    let staged = bin_swap::stage(&bin_dir, &copies)?;
    let swap = bin_swap::apply(&bin_dir, &staged, &stale)?;
    versions.insert(tool.to_string(), version.to_string());
    if let Err(err) = write(layout, &versions) {
        swap.undo();
        return Err(err);
    }
    swap.finish();
    Ok(Activation {
        on_path: provided.iter().map(|file| bin_dir.join(file)).collect(),
        removed: stale.iter().map(|file| bin_dir.join(file)).collect(),
    })
}

/// Takes `tool`'s `executables` out of `<home>/bin/` and drops its
/// `active.toml` entry, leaving other tools untouched. Returns the paths that
/// were taken out.
pub fn deactivate(
    layout: &Layout,
    tool: &str,
    executables: &[String],
) -> Result<Vec<PathBuf>, StoreError> {
    let mut versions = active_versions(layout)?;
    let bin_dir = layout.bin_dir();
    let present: Vec<String> = executables
        .iter()
        .filter(|file| exists(&bin_dir.join(file)))
        .cloned()
        .collect();

    let swap = bin_swap::apply(&bin_dir, &[], &present)?;
    versions.remove(tool);
    if let Err(err) = write(layout, &versions) {
        swap.undo();
        return Err(err);
    }
    swap.finish();
    Ok(present.iter().map(|file| bin_dir.join(file)).collect())
}

/// Whether `<home>/bin/` holds exactly the `executables` that `version_dir`
/// provides.
pub fn bin_holds(layout: &Layout, version_dir: &Path, executables: &[String]) -> bool {
    let bin_dir = layout.bin_dir();
    executables
        .iter()
        .all(|file| version_dir.join(file).is_file() == bin_dir.join(file).is_file())
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
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
    use crate::store::fake_home::{self, contents};

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn bin_file(layout: &Layout, file: &str) -> Vec<u8> {
        fs::read(layout.bin_dir().join(file)).unwrap()
    }

    #[test]
    fn activation_copies_binary_and_records_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let version_dir = dir.path().join("package");
        fs::create_dir(&version_dir).unwrap();
        let binary = version_dir.join("tool.exe");
        fs::write(&binary, b"v1").unwrap();

        assert_eq!(active_version(&layout, "tool").unwrap(), None);
        activate(&layout, "tool", "v1", &version_dir, &files(&["tool.exe"])).unwrap();
        fs::write(&binary, b"v2").unwrap();
        let activation =
            activate(&layout, "tool", "v2", &version_dir, &files(&["tool.exe"])).unwrap();

        assert_eq!(
            active_version(&layout, "tool").unwrap().as_deref(),
            Some("v2")
        );
        assert_eq!(bin_file(&layout, "tool.exe"), b"v2");
        assert_eq!(activation.on_path, [layout.bin_dir().join("tool.exe")]);
        assert!(activation.removed.is_empty());
        assert_eq!(fake_home::names(&layout.bin_dir()), ["tool.exe"]);
    }

    #[test]
    fn switching_fnn_back_and_forth_replaces_fnn_and_fnn_cli() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let executables = files(&["fnn", "fnn-cli"]);
        let new = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        let old = fake_home::install(&layout, "fnn", "v0.9.1", &["fnn", "fnn-cli"]);

        for (version, dir) in [("v0.10.1", &new), ("v0.9.1", &old), ("v0.10.1", &new)] {
            activate(&layout, "fnn", version, dir, &executables).unwrap();
            assert_eq!(bin_file(&layout, "fnn"), contents("fnn", version, "fnn"));
            assert_eq!(
                bin_file(&layout, "fnn-cli"),
                contents("fnn", version, "fnn-cli")
            );
            assert_eq!(
                active_version(&layout, "fnn").unwrap().as_deref(),
                Some(version)
            );
            assert_eq!(fake_home::names(&layout.bin_dir()), ["fnn", "fnn-cli"]);
        }
    }

    #[test]
    fn a_version_without_fnn_cli_takes_fnn_cli_out_of_bin() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let executables = files(&["fnn", "fnn-cli"]);
        let full = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        let damaged = fake_home::install(&layout, "fnn", "v0.9.1", &["fnn"]);
        activate(&layout, "fnn", "v0.10.1", &full, &executables).unwrap();

        let activation = activate(&layout, "fnn", "v0.9.1", &damaged, &executables).unwrap();

        assert_eq!(activation.on_path, [layout.bin_dir().join("fnn")]);
        assert_eq!(activation.removed, [layout.bin_dir().join("fnn-cli")]);
        assert_eq!(fake_home::names(&layout.bin_dir()), ["fnn"]);
        assert_eq!(bin_file(&layout, "fnn"), contents("fnn", "v0.9.1", "fnn"));
    }

    #[test]
    fn a_missing_main_binary_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let executables = files(&["fnn", "fnn-cli"]);
        let good = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        let broken = fake_home::install(&layout, "fnn", "v0.9.1", &["fnn-cli"]);
        activate(&layout, "fnn", "v0.10.1", &good, &executables).unwrap();
        let before = fake_home::snapshot(layout.home());

        let err = activate(&layout, "fnn", "v0.9.1", &broken, &executables).unwrap_err();

        assert!(matches!(err, StoreError::MissingBinary { .. }));
        assert_eq!(fake_home::snapshot(layout.home()), before);
    }

    #[test]
    fn activating_fnn_puts_fnn_and_fnn_cli_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
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

        assert_eq!(bin_file(&layout, "fnn"), b"node");
        assert_eq!(bin_file(&layout, "fnn-cli"), b"cli");
        assert!(!layout.bin_dir().join("config").exists());
    }

    #[test]
    fn activating_ckb_leaves_its_bundled_ckb_cli_out_of_bin() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let version_dir = layout.toolchain_dir("ckb", "v0.210.0");
        fs::create_dir_all(&version_dir).unwrap();
        fs::write(version_dir.join("ckb"), b"node").unwrap();
        fs::write(version_dir.join("ckb-cli"), b"bundled").unwrap();

        activate(&layout, "ckb", "v0.210.0", &version_dir, &files(&["ckb"])).unwrap();

        assert!(layout.bin_dir().join("ckb").is_file());
        assert!(!layout.bin_dir().join("ckb-cli").exists());
    }

    #[test]
    fn switching_ckb_never_touches_the_ckb_cli_tool_in_bin() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let cli = fake_home::install(&layout, "ckb-cli", "v2.0.0", &["ckb-cli"]);
        activate(&layout, "ckb-cli", "v2.0.0", &cli, &files(&["ckb-cli"])).unwrap();
        let new = fake_home::install(&layout, "ckb", "v0.210.0", &["ckb", "ckb-cli"]);
        let old = fake_home::install(&layout, "ckb", "v0.209.0", &["ckb", "ckb-cli"]);

        for (version, dir) in [("v0.210.0", &new), ("v0.209.0", &old)] {
            activate(&layout, "ckb", version, dir, &files(&["ckb"])).unwrap();
        }
        deactivate(&layout, "ckb", &files(&["ckb"])).unwrap();

        assert_eq!(
            bin_file(&layout, "ckb-cli"),
            contents("ckb-cli", "v2.0.0", "ckb-cli")
        );
        assert_eq!(fake_home::names(&layout.bin_dir()), ["ckb-cli"]);
        assert_eq!(
            active_version(&layout, "ckb-cli").unwrap().as_deref(),
            Some("v2.0.0")
        );
    }

    #[test]
    fn deactivation_takes_every_executable_out_and_drops_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let fnn = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        activate(&layout, "fnn", "v0.10.1", &fnn, &files(&["fnn", "fnn-cli"])).unwrap();
        let ckb = fake_home::install(&layout, "ckb", "v0.210.0", &["ckb"]);
        activate(&layout, "ckb", "v0.210.0", &ckb, &files(&["ckb"])).unwrap();

        let removed = deactivate(&layout, "fnn", &files(&["fnn", "fnn-cli"])).unwrap();

        assert_eq!(
            removed,
            [
                layout.bin_dir().join("fnn"),
                layout.bin_dir().join("fnn-cli")
            ]
        );
        assert_eq!(fake_home::names(&layout.bin_dir()), ["ckb"]);
        let versions = active_versions(&layout).unwrap();
        assert_eq!(versions.get("fnn"), None);
        assert_eq!(versions.get("ckb").map(String::as_str), Some("v0.210.0"));
    }

    #[test]
    fn bin_holds_compares_presence_with_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let executables = files(&["fnn", "fnn-cli"]);
        let fnn = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        assert!(!bin_holds(&layout, &fnn, &executables));

        activate(&layout, "fnn", "v0.10.1", &fnn, &executables).unwrap();
        assert!(bin_holds(&layout, &fnn, &executables));

        fs::remove_file(layout.bin_dir().join("fnn-cli")).unwrap();
        assert!(!bin_holds(&layout, &fnn, &executables));
    }
}
