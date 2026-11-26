use crate::cli::UseArgs;
use crate::error::Error;
use crate::output::{activation_report, hold_report, path_hint};
use crate::platform::Host;
use crate::store::{self, Activation, Hold, Layout, StoreError};
use crate::tools::{self, ToolSpec};

#[derive(Debug)]
enum Outcome {
    AlreadyActive { was_held: bool },
    Activated(Activation),
}

pub fn run(args: &UseArgs) -> Result<(), Error> {
    let tool = tools::find_tool(&args.target.tool)?;
    let host = Host::detect();
    let layout = Layout::from_env()?;
    let version = args.target.version.as_str();

    match switch(&layout, tool, version, host)? {
        Outcome::AlreadyActive { was_held } => {
            println!("{} {version} is already active", tool.name);
            print!("{}", hold_report(tool.name, version, Hold::Set, was_held));
        }
        Outcome::Activated(activation) => {
            print!(
                "{}",
                activation_report(tool.name, version, &activation.on_path, &activation.removed)
            );
            print!(
                "{}",
                hold_report(tool.name, version, Hold::Set, activation.was_held)
            );
            if let Some(hint) = path_hint(&layout.bin_dir(), host.os) {
                println!("\n{hint}");
            }
        }
    }
    Ok(())
}

/// Makes the installed `version` of `tool` active and holds the tool there, so
/// that `update` with no tool leaves it alone.
///
/// It works offline: the version must already be installed, and nothing
/// changes when it is not. When it is already active with its files in `bin/`,
/// only the hold is recorded.
fn switch(layout: &Layout, tool: &ToolSpec, version: &str, host: Host) -> Result<Outcome, Error> {
    let executables = tool.executables(host);
    store::sweep(layout, tool.name, &executables);
    let binary_file = host.executable_name(tool.binary);

    let Some(version_dir) = store::installed_dir(layout, tool.name, version, &binary_file) else {
        return Err(StoreError::NotInstalledToUse {
            tool: tool.name.to_string(),
            version: version.to_string(),
            installed: store::installed_versions(layout, tool.name, &binary_file)?,
        }
        .into());
    };
    let active = store::active_version(layout, tool.name)?;
    if active.as_deref() == Some(version) && store::bin_holds(layout, &version_dir, &executables) {
        let was_held = store::set_hold(layout, tool.name, Hold::Set)?;
        return Ok(Outcome::AlreadyActive { was_held });
    }
    let activation = store::activate(
        layout,
        tool.name,
        version,
        &version_dir,
        &executables,
        Hold::Set,
    )?;
    Ok(Outcome::Activated(activation))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::platform::{Arch, Os};
    use crate::store::fake_home::{self, contents};

    const LINUX: Host = Host {
        os: Os::Linux,
        arch: Arch::X86_64,
    };

    fn fnn() -> &'static ToolSpec {
        tools::find_tool("fnn").unwrap()
    }

    /// fnn v0.10.1 (active) and v0.9.1, each with `fnn` and `fnn-cli`.
    fn two_fnn_versions(layout: &Layout) {
        let active = fake_home::install(layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);
        fake_home::install(layout, "fnn", "v0.9.1", &["fnn", "fnn-cli"]);
        store::activate(
            layout,
            "fnn",
            "v0.10.1",
            &active,
            &fnn().executables(LINUX),
            Hold::Keep,
        )
        .unwrap();
    }

    #[test]
    fn switches_to_an_installed_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        two_fnn_versions(&layout);

        let outcome = switch(&layout, fnn(), "v0.9.1", LINUX).unwrap();

        assert!(matches!(outcome, Outcome::Activated(_)));
        assert_eq!(
            fs::read(layout.bin_dir().join("fnn-cli")).unwrap(),
            contents("fnn", "v0.9.1", "fnn-cli")
        );
        let state = store::active_state(&layout).unwrap();
        assert_eq!(
            state.versions.get("fnn").map(String::as_str),
            Some("v0.9.1")
        );
        assert!(state.held.contains("fnn"));
    }

    #[test]
    fn a_version_that_is_not_installed_fails_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        two_fnn_versions(&layout);
        let before = fake_home::snapshot(layout.home());

        let err = switch(&layout, fnn(), "v0.9.0", LINUX).unwrap_err();

        assert_eq!(
            err.to_string(),
            "fnn v0.9.0 is not installed. Installed: v0.10.1, v0.9.1. Install it with: ckbup install fnn@v0.9.0"
        );
        assert_eq!(fake_home::snapshot(layout.home()), before);
    }

    #[test]
    fn the_active_version_is_held_and_nothing_else_changes() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        two_fnn_versions(&layout);
        let mut expected = fake_home::snapshot(layout.home());
        expected.insert(
            "active.toml".to_string(),
            Some(b"fnn = \"v0.10.1\"\nheld = [\"fnn\"]\n".to_vec()),
        );

        let outcome = switch(&layout, fnn(), "v0.10.1", LINUX).unwrap();

        assert!(matches!(
            outcome,
            Outcome::AlreadyActive { was_held: false }
        ));
        assert_eq!(fake_home::snapshot(layout.home()), expected);

        let again = switch(&layout, fnn(), "v0.10.1", LINUX).unwrap();
        assert!(matches!(again, Outcome::AlreadyActive { was_held: true }));
        assert_eq!(fake_home::snapshot(layout.home()), expected);
    }

    #[test]
    fn the_active_version_is_activated_again_when_bin_lacks_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        two_fnn_versions(&layout);
        fs::remove_file(layout.bin_dir().join("fnn-cli")).unwrap();

        let outcome = switch(&layout, fnn(), "v0.10.1", LINUX).unwrap();

        assert!(matches!(outcome, Outcome::Activated(_)));
        assert_eq!(
            fs::read(layout.bin_dir().join("fnn-cli")).unwrap(),
            contents("fnn", "v0.10.1", "fnn-cli")
        );
    }

    #[test]
    fn leftover_moved_aside_copies_are_swept_first() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        two_fnn_versions(&layout);
        fs::write(layout.bin_dir().join("fnn.old"), b"left by an earlier run").unwrap();
        fs::write(
            layout.bin_dir().join("fnn.old.1"),
            b"left by an earlier run",
        )
        .unwrap();

        switch(&layout, fnn(), "v0.9.1", LINUX).unwrap();

        assert_eq!(fake_home::names(&layout.bin_dir()), ["fnn", "fnn-cli"]);
    }

    #[test]
    fn switches_between_binary_only_folders() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let cli = tools::find_tool("ckb-cli").unwrap();
        let first = fake_home::install(&layout, "ckb-cli", "v2.0.0", &["ckb-cli"]);
        fake_home::install(&layout, "ckb-cli", "v1.16.0", &["ckb-cli"]);
        store::activate(
            &layout,
            "ckb-cli",
            "v2.0.0",
            &first,
            &cli.executables(LINUX),
            Hold::Keep,
        )
        .unwrap();

        switch(&layout, cli, "v1.16.0", LINUX).unwrap();

        assert_eq!(
            fs::read(layout.bin_dir().join("ckb-cli")).unwrap(),
            contents("ckb-cli", "v1.16.0", "ckb-cli")
        );
        assert_eq!(
            fake_home::names(&layout.tool_dir("ckb-cli")),
            ["v1.16.0", "v2.0.0"]
        );
    }
}
