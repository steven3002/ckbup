use std::path::PathBuf;

use crate::cli::RemoveArgs;
use crate::error::Error;
use crate::platform::Host;
use crate::store::{self, Layout, StoreError};
use crate::tools::{self, ToolSpec};

#[derive(Debug)]
struct Removed {
    version_dir: PathBuf,
    /// The files taken out of `bin/` when the last version was the active one.
    deactivated: Option<Vec<PathBuf>>,
    /// The hidden folder left for a later sweep because part of it is in use.
    left_behind: Option<PathBuf>,
}

pub fn run(args: &RemoveArgs) -> Result<(), Error> {
    let tool = tools::find_tool(&args.target.tool)?;
    let host = Host::detect();
    let layout = Layout::from_env()?;
    let version = args.target.version.as_str();

    let removed = remove(&layout, tool, version, host)?;
    println!(
        "Removed {} {version} ({})",
        tool.name,
        removed.version_dir.display()
    );
    if let Some(files) = &removed.deactivated {
        let files: Vec<String> = files
            .iter()
            .map(|file| file.display().to_string())
            .collect();
        match files.as_slice() {
            [] => println!("{} has no active version now", tool.name),
            files => println!(
                "{} has no active version now; removed {}",
                tool.name,
                files.join(", ")
            ),
        }
    }
    if let Some(hidden) = &removed.left_behind {
        println!(
            "{} is still in use; it will be deleted the next time ckbup installs, switches or removes {}",
            hidden.display(),
            tool.name
        );
    }
    Ok(())
}

/// Deletes the folder of one version of `tool`.
///
/// The active version is removed only when no other version is installed, and
/// the tool is then deactivated. The folder is renamed to a hidden name before
/// anything else changes, so a folder that a program or terminal is using
/// stops the removal with nothing changed.
fn remove(layout: &Layout, tool: &ToolSpec, version: &str, host: Host) -> Result<Removed, Error> {
    let executables = tool.executables(host);
    store::sweep(layout, tool.name, &executables);
    let binary_file = host.executable_name(tool.binary);
    let installed = store::installed_versions(layout, tool.name, &binary_file)?;

    let version_dir = layout.toolchain_dir(tool.name, version);
    if !version_dir.is_dir() {
        return Err(StoreError::NotInstalled {
            tool: tool.name.to_string(),
            version: version.to_string(),
            installed,
        }
        .into());
    }
    let is_active = store::active_version(layout, tool.name)?.as_deref() == Some(version);
    let other = installed.iter().find(|other| *other != version);
    if let Some(other) = other.filter(|_| is_active) {
        return Err(StoreError::RemoveActive {
            tool: tool.name.to_string(),
            version: version.to_string(),
            other: other.clone(),
        }
        .into());
    }

    let hidden = store::hide_version(layout, tool.name, version)?;
    let deactivated = if is_active {
        match store::deactivate(layout, tool.name, &executables) {
            Ok(files) => Some(files),
            Err(err) => {
                let _ = store::unhide_version(layout, tool.name, version, &hidden);
                return Err(err.into());
            }
        }
    } else {
        None
    };
    let left_behind = (!store::delete_hidden(&hidden)).then_some(hidden);
    Ok(Removed {
        version_dir,
        deactivated,
        left_behind,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::platform::{Arch, Os};
    use crate::store::fake_home;

    const LINUX: Host = Host {
        os: Os::Linux,
        arch: Arch::X86_64,
    };

    fn tool(name: &str) -> &'static ToolSpec {
        tools::find_tool(name).unwrap()
    }

    /// Installs fnn at `versions` and activates the first one.
    fn fnn_home(layout: &Layout, versions: &[&str]) {
        for version in versions {
            fake_home::install(layout, "fnn", version, &["fnn", "fnn-cli"]);
        }
        let active = layout.toolchain_dir("fnn", versions[0]);
        let executables = tool("fnn").executables(LINUX);
        store::activate(layout, "fnn", versions[0], &active, &executables).unwrap();
    }

    #[test]
    fn removes_a_version_that_is_not_active() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fnn_home(&layout, &["v0.10.1", "v0.9.1"]);
        let bin = fake_home::snapshot(&layout.bin_dir());

        let removed = remove(&layout, tool("fnn"), "v0.9.1", LINUX).unwrap();

        assert_eq!(removed.version_dir, layout.toolchain_dir("fnn", "v0.9.1"));
        assert!(removed.deactivated.is_none());
        assert!(removed.left_behind.is_none());
        assert_eq!(fake_home::names(&layout.tool_dir("fnn")), ["v0.10.1"]);
        assert_eq!(fake_home::snapshot(&layout.bin_dir()), bin);
    }

    #[test]
    fn refuses_the_active_version_while_others_are_installed() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fnn_home(&layout, &["v0.10.1", "v0.9.1", "v0.9.0"]);
        let before = fake_home::snapshot(layout.home());

        let err = remove(&layout, tool("fnn"), "v0.10.1", LINUX).unwrap_err();

        assert_eq!(
            err.to_string(),
            "fnn v0.10.1 is the active version. Switch first, for example: ckbup use fnn@v0.9.1"
        );
        assert_eq!(fake_home::snapshot(layout.home()), before);
    }

    #[test]
    fn removing_the_only_version_deactivates_the_tool() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fnn_home(&layout, &["v0.10.1"]);
        let ckb = fake_home::install(&layout, "ckb", "v0.210.0", &["ckb", "ckb-cli"]);
        store::activate(
            &layout,
            "ckb",
            "v0.210.0",
            &ckb,
            &tool("ckb").executables(LINUX),
        )
        .unwrap();

        let removed = remove(&layout, tool("fnn"), "v0.10.1", LINUX).unwrap();

        assert_eq!(
            removed.deactivated,
            Some(vec![
                layout.bin_dir().join("fnn"),
                layout.bin_dir().join("fnn-cli")
            ])
        );
        assert!(fake_home::names(&layout.tool_dir("fnn")).is_empty());
        assert_eq!(fake_home::names(&layout.bin_dir()), ["ckb"]);
        let active = store::active_versions(&layout).unwrap();
        assert_eq!(active.get("fnn"), None);
        assert_eq!(active.get("ckb").map(String::as_str), Some("v0.210.0"));
        assert_eq!(fake_home::names(&ckb), ["ckb", "ckb-cli"]);
    }

    #[test]
    fn a_version_that_is_not_installed_fails_and_lists_the_installed_ones() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fnn_home(&layout, &["v0.10.1", "v0.9.1"]);
        let before = fake_home::snapshot(layout.home());

        let err = remove(&layout, tool("fnn"), "v0.9.0", LINUX).unwrap_err();

        assert_eq!(
            err.to_string(),
            "fnn v0.9.0 is not installed. Installed: v0.10.1, v0.9.1"
        );
        assert_eq!(fake_home::snapshot(layout.home()), before);
    }

    #[test]
    fn removes_a_folder_without_its_main_binary() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fnn_home(&layout, &["v0.10.1"]);
        fake_home::install(&layout, "fnn", "v0.7.0", &["README.md"]);

        remove(&layout, tool("fnn"), "v0.7.0", LINUX).unwrap();

        assert_eq!(fake_home::names(&layout.tool_dir("fnn")), ["v0.10.1"]);
    }

    #[test]
    fn a_leftover_removing_folder_is_swept() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        fnn_home(&layout, &["v0.10.1", "v0.9.1", "v0.8.0"]);
        let leftover = layout.tool_dir("fnn").join(".v0.9.1.removing");
        fs::create_dir_all(leftover.join("config")).unwrap();
        fs::write(leftover.join("fnn"), b"left by an earlier run").unwrap();

        remove(&layout, tool("fnn"), "v0.9.1", LINUX).unwrap();

        assert_eq!(
            fake_home::names(&layout.tool_dir("fnn")),
            ["v0.10.1", "v0.8.0"]
        );
    }

    #[test]
    fn removes_the_only_binary_only_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let cli = tool("ckb-cli");
        let folder = fake_home::install(&layout, "ckb-cli", "v2.0.0", &["ckb-cli"]);
        store::activate(
            &layout,
            "ckb-cli",
            "v2.0.0",
            &folder,
            &cli.executables(LINUX),
        )
        .unwrap();

        remove(&layout, cli, "v2.0.0", LINUX).unwrap();

        assert!(fake_home::names(&layout.tool_dir("ckb-cli")).is_empty());
        assert!(fake_home::names(&layout.bin_dir()).is_empty());
        assert!(store::active_versions(&layout).unwrap().is_empty());
    }
}
