use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{Layout, StoreError, bin_swap};

/// Tool name to active version, as recorded in `active.toml`.
pub type ActiveVersions = BTreeMap<String, String>;

/// The key of the held tools in `active.toml`. No tool has this name.
const HELD_KEY: &str = "held";

/// Everything `active.toml` records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveState {
    pub versions: ActiveVersions,
    /// Tools whose version was chosen by name; `update` with no tool skips them.
    pub held: BTreeSet<String>,
}

/// What an activation does to the hold of its tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Keep,
    Set,
    Release,
}

/// What `activate` changed in `<home>/bin/`.
#[derive(Debug, PartialEq, Eq)]
pub struct Activation {
    /// The tool's executables now in `bin/`, main binary first.
    pub on_path: Vec<PathBuf>,
    /// The tool's executables that the version does not include, taken out of `bin/`.
    pub removed: Vec<PathBuf>,
    /// Whether the tool was held before the activation.
    pub was_held: bool,
}

pub fn active_version(layout: &Layout, tool: &str) -> Result<Option<String>, StoreError> {
    Ok(active_versions(layout)?.remove(tool))
}

/// The active version of every tool.
pub fn active_versions(layout: &Layout) -> Result<ActiveVersions, StoreError> {
    Ok(active_state(layout)?.versions)
}

/// Reads `active.toml`; a missing file records nothing.
///
/// It is a flat table: each string value is a tool's active version, and the
/// optional `held` list names the held tools. A file written before holds
/// existed has no `held` key, so it holds nothing.
pub fn active_state(layout: &Layout) -> Result<ActiveState, StoreError> {
    let path = layout.active_file();
    match fs::read_to_string(&path) {
        Ok(text) => parse(&text).map_err(|source| StoreError::InvalidActiveFile { path, source }),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(ActiveState::default()),
        Err(err) => Err(StoreError::io("cannot read", &path)(err)),
    }
}

/// Makes `version` the active version of `tool`: copies the `executables`
/// that `version_dir` holds into `<home>/bin/` and records the version, with
/// the tool's hold changed as `hold` says, in `active.toml`.
///
/// `executables` lists the main binary, then its companions. The main binary
/// must be in `version_dir`; a companion it lacks is taken out of `bin/`. Every
/// copy is staged before any file in `bin/` changes, and the files in place are
/// moved aside rather than overwritten, because one of them may be running. If
/// a step fails, `bin/` and `active.toml` are left as they were. Other files in
/// the package, such as ckb's bundled `ckb-cli` or the install record, are
/// never copied.
pub fn activate(
    layout: &Layout,
    tool: &str,
    version: &str,
    version_dir: &Path,
    executables: &[String],
    hold: Hold,
) -> Result<Activation, StoreError> {
    if let Some(main) = executables.first() {
        let path = version_dir.join(main);
        if !path.is_file() {
            return Err(StoreError::MissingBinary { path });
        }
    }
    let mut state = active_state(layout)?;
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
    state.versions.insert(tool.to_string(), version.to_string());
    let was_held = change_hold(&mut state.held, tool, hold);
    if let Err(err) = write(layout, &state) {
        swap.undo();
        return Err(err);
    }
    swap.finish();
    Ok(Activation {
        on_path: provided.iter().map(|file| bin_dir.join(file)).collect(),
        removed: stale.iter().map(|file| bin_dir.join(file)).collect(),
        was_held,
    })
}

/// Changes the hold of `tool` without activating anything, and returns whether
/// it was held before. `active.toml` is written only when the hold changes.
pub fn set_hold(layout: &Layout, tool: &str, hold: Hold) -> Result<bool, StoreError> {
    let mut state = active_state(layout)?;
    let before = state.held.clone();
    let was_held = change_hold(&mut state.held, tool, hold);
    if state.held != before {
        write(layout, &state)?;
    }
    Ok(was_held)
}

fn change_hold(held: &mut BTreeSet<String>, tool: &str, hold: Hold) -> bool {
    let was_held = held.contains(tool);
    match hold {
        Hold::Keep => {}
        Hold::Set => {
            held.insert(tool.to_string());
        }
        Hold::Release => {
            held.remove(tool);
        }
    }
    was_held
}

/// Takes `tool`'s `executables` out of `<home>/bin/` and drops its
/// `active.toml` entry and its hold, leaving other tools untouched. Returns the
/// paths that were taken out.
pub fn deactivate(
    layout: &Layout,
    tool: &str,
    executables: &[String],
) -> Result<Vec<PathBuf>, StoreError> {
    let mut state = active_state(layout)?;
    let bin_dir = layout.bin_dir();
    let present: Vec<String> = executables
        .iter()
        .filter(|file| exists(&bin_dir.join(file)))
        .cloned()
        .collect();

    let swap = bin_swap::apply(&bin_dir, &[], &present)?;
    state.versions.remove(tool);
    state.held.remove(tool);
    if let Err(err) = write(layout, &state) {
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

fn parse(text: &str) -> Result<ActiveState, toml::de::Error> {
    let table: toml::Table = toml::from_str(text)?;
    let mut state = ActiveState::default();
    for (key, value) in table {
        if key == HELD_KEY {
            let tools = value
                .as_array()
                .ok_or_else(|| invalid("`held` must be a list of tool names"))?;
            for tool in tools {
                let tool = tool
                    .as_str()
                    .ok_or_else(|| invalid("`held` must be a list of tool names"))?;
                state.held.insert(tool.to_string());
            }
        } else {
            let version = value
                .as_str()
                .ok_or_else(|| invalid(format!("the version of `{key}` must be a string")))?;
            state.versions.insert(key, version.to_string());
        }
    }
    Ok(state)
}

fn invalid(message: impl Display) -> toml::de::Error {
    serde::de::Error::custom(message)
}

/// The text of `active.toml` for `state`: the versions, then `held` when it
/// names any tool.
fn render(state: &ActiveState) -> String {
    let mut table: toml::Table = state
        .versions
        .iter()
        .map(|(tool, version)| (tool.clone(), toml::Value::String(version.clone())))
        .collect();
    if !state.held.is_empty() {
        let held = state
            .held
            .iter()
            .cloned()
            .map(toml::Value::String)
            .collect();
        table.insert(HELD_KEY.to_string(), toml::Value::Array(held));
    }
    toml::to_string(&table).expect("strings and a list of strings always serialize to TOML")
}

fn write(layout: &Layout, state: &ActiveState) -> Result<(), StoreError> {
    let path = layout.active_file();
    let staged = path.with_extension("toml.new");
    fs::write(&staged, render(state)).map_err(StoreError::io("cannot write", &staged))?;
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
        activate(
            &layout,
            "tool",
            "v1",
            &version_dir,
            &files(&["tool.exe"]),
            Hold::Keep,
        )
        .unwrap();
        fs::write(&binary, b"v2").unwrap();
        let activation = activate(
            &layout,
            "tool",
            "v2",
            &version_dir,
            &files(&["tool.exe"]),
            Hold::Keep,
        )
        .unwrap();

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
            activate(&layout, "fnn", version, dir, &executables, Hold::Keep).unwrap();
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
        activate(&layout, "fnn", "v0.10.1", &full, &executables, Hold::Keep).unwrap();

        let activation =
            activate(&layout, "fnn", "v0.9.1", &damaged, &executables, Hold::Keep).unwrap();

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
        activate(&layout, "fnn", "v0.10.1", &good, &executables, Hold::Keep).unwrap();
        let before = fake_home::snapshot(layout.home());

        let err =
            activate(&layout, "fnn", "v0.9.1", &broken, &executables, Hold::Keep).unwrap_err();

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
            Hold::Keep,
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

        activate(
            &layout,
            "ckb",
            "v0.210.0",
            &version_dir,
            &files(&["ckb"]),
            Hold::Keep,
        )
        .unwrap();

        assert!(layout.bin_dir().join("ckb").is_file());
        assert!(!layout.bin_dir().join("ckb-cli").exists());
    }

    #[test]
    fn switching_ckb_never_touches_the_ckb_cli_tool_in_bin() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let cli = fake_home::install(&layout, "ckb-cli", "v2.0.0", &["ckb-cli"]);
        activate(
            &layout,
            "ckb-cli",
            "v2.0.0",
            &cli,
            &files(&["ckb-cli"]),
            Hold::Keep,
        )
        .unwrap();
        let new = fake_home::install(&layout, "ckb", "v0.210.0", &["ckb", "ckb-cli"]);
        let old = fake_home::install(&layout, "ckb", "v0.209.0", &["ckb", "ckb-cli"]);

        for (version, dir) in [("v0.210.0", &new), ("v0.209.0", &old)] {
            activate(&layout, "ckb", version, dir, &files(&["ckb"]), Hold::Keep).unwrap();
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
        activate(
            &layout,
            "fnn",
            "v0.10.1",
            &fnn,
            &files(&["fnn", "fnn-cli"]),
            Hold::Keep,
        )
        .unwrap();
        let ckb = fake_home::install(&layout, "ckb", "v0.210.0", &["ckb"]);
        activate(
            &layout,
            "ckb",
            "v0.210.0",
            &ckb,
            &files(&["ckb"]),
            Hold::Keep,
        )
        .unwrap();

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

        activate(&layout, "fnn", "v0.10.1", &fnn, &executables, Hold::Keep).unwrap();
        assert!(bin_holds(&layout, &fnn, &executables));

        fs::remove_file(layout.bin_dir().join("fnn-cli")).unwrap();
        assert!(!bin_holds(&layout, &fnn, &executables));
    }

    fn state(versions: &[(&str, &str)], held: &[&str]) -> ActiveState {
        ActiveState {
            versions: versions
                .iter()
                .map(|(tool, version)| (tool.to_string(), version.to_string()))
                .collect(),
            held: held.iter().map(|tool| tool.to_string()).collect(),
        }
    }

    #[test]
    fn the_file_round_trips_with_and_without_holds() {
        let plain = state(&[("ckb-cli", "v2.0.0"), ("fnn", "v0.10.1")], &[]);
        assert_eq!(render(&plain), "ckb-cli = \"v2.0.0\"\nfnn = \"v0.10.1\"\n");
        assert_eq!(parse(&render(&plain)).unwrap(), plain);

        let held = state(
            &[
                ("ckb", "v0.210.0"),
                ("ckb-cli", "v2.0.0"),
                ("fnn", "v0.9.1"),
            ],
            &["fnn", "ckb-cli"],
        );
        assert_eq!(
            render(&held),
            "ckb = \"v0.210.0\"\nckb-cli = \"v2.0.0\"\nfnn = \"v0.9.1\"\nheld = [\"ckb-cli\", \"fnn\"]\n"
        );
        assert_eq!(parse(&render(&held)).unwrap(), held);
    }

    #[test]
    fn a_file_written_before_holds_existed_holds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fs::create_dir_all(layout.home()).unwrap();
        fs::write(
            layout.active_file(),
            "ckb = \"v0.210.0\"\nckb-cli = \"v2.0.0\"\nckb-debugger = \"v1.1.1\"\nckb-light-client = \"v0.5.4\"\nfnn = \"v0.10.1\"\n",
        )
        .unwrap();

        let recorded = active_state(&layout).unwrap();

        assert_eq!(recorded.versions.len(), 5);
        assert!(recorded.held.is_empty());
    }

    #[test]
    fn values_of_another_kind_are_not_valid() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fs::create_dir_all(layout.home()).unwrap();
        for text in [
            "fnn = 5\n",
            "held = \"fnn\"\n",
            "held = [1]\n",
            "fnn = \"v0.10.1",
        ] {
            fs::write(layout.active_file(), text).unwrap();
            let err = active_state(&layout).unwrap_err();
            assert!(
                matches!(err, StoreError::InvalidActiveFile { .. }),
                "{text}"
            );
            assert!(
                err.to_string().contains("active.toml is not valid: "),
                "{err}"
            );
        }
    }

    #[test]
    fn activation_sets_keeps_or_releases_the_hold() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let executables = files(&["fnn", "fnn-cli"]);
        let new = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        let old = fake_home::install(&layout, "fnn", "v0.9.1", &["fnn", "fnn-cli"]);
        let held = |layout: &Layout| active_state(layout).unwrap().held.contains("fnn");

        let set = activate(&layout, "fnn", "v0.9.1", &old, &executables, Hold::Set).unwrap();
        assert!(!set.was_held);
        assert!(held(&layout));

        let kept = activate(&layout, "fnn", "v0.9.1", &old, &executables, Hold::Keep).unwrap();
        assert!(kept.was_held);
        assert!(held(&layout));

        let released =
            activate(&layout, "fnn", "v0.10.1", &new, &executables, Hold::Release).unwrap();
        assert!(released.was_held);
        assert!(!held(&layout));
        assert_eq!(
            active_version(&layout, "fnn").unwrap().as_deref(),
            Some("v0.10.1")
        );
    }

    #[test]
    fn setting_a_hold_writes_only_when_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let fnn = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        activate(
            &layout,
            "fnn",
            "v0.10.1",
            &fnn,
            &files(&["fnn", "fnn-cli"]),
            Hold::Keep,
        )
        .unwrap();
        let before = fake_home::snapshot(layout.home());

        assert!(!set_hold(&layout, "fnn", Hold::Release).unwrap());
        assert!(!set_hold(&layout, "fnn", Hold::Keep).unwrap());
        assert_eq!(fake_home::snapshot(layout.home()), before);

        assert!(!set_hold(&layout, "fnn", Hold::Set).unwrap());
        assert_eq!(
            fs::read_to_string(layout.active_file()).unwrap(),
            "fnn = \"v0.10.1\"\nheld = [\"fnn\"]\n"
        );
        assert!(set_hold(&layout, "fnn", Hold::Release).unwrap());
        assert_eq!(
            fs::read_to_string(layout.active_file()).unwrap(),
            "fnn = \"v0.10.1\"\n"
        );
    }

    #[test]
    fn deactivation_drops_the_hold() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let executables = files(&["fnn", "fnn-cli"]);
        let fnn = fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        activate(&layout, "fnn", "v0.10.1", &fnn, &executables, Hold::Set).unwrap();

        deactivate(&layout, "fnn", &executables).unwrap();

        assert_eq!(active_state(&layout).unwrap(), ActiveState::default());
    }

    #[test]
    fn activation_never_copies_the_install_record() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let fnn = fake_home::install(
            &layout,
            "fnn",
            "v0.10.1",
            &["fnn", "fnn-cli", ".ckbup.toml"],
        );

        activate(
            &layout,
            "fnn",
            "v0.10.1",
            &fnn,
            &files(&["fnn", "fnn-cli"]),
            Hold::Keep,
        )
        .unwrap();

        assert_eq!(fake_home::names(&layout.bin_dir()), ["fnn", "fnn-cli"]);
    }
}
