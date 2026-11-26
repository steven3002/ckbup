use std::cmp::Ordering;

use crate::cli::UpdateArgs;
use crate::commands::install::{self, InstallOptions};
use crate::commands::remove;
use crate::error::Error;
use crate::github::{GithubClient, GithubError};
use crate::output::{activation_report, held_skip, hold_report, path_hint};
use crate::platform::Host;
use crate::releases;
use crate::store::{self, ActiveState, Hold, InstallRecord, Layout, StoreError};
use crate::tools::{self, ToolSpec, Variant, newest_first};

/// The flags that apply to every tool an update moves.
#[derive(Debug, Clone, Copy)]
struct UpdateOptions {
    portable: bool,
    allow_unverified: bool,
    keep_archives: bool,
    remove_old: bool,
}

/// What a tool's versions look like before its update.
#[derive(Debug, Clone, Copy)]
struct Current<'a> {
    /// The version `active.toml` records, if any.
    active: Option<&'a str>,
    /// Whether the folder of that version is installed.
    active_installed: bool,
    /// Whether `bin/` holds that version's executables.
    bin_complete: bool,
    /// Whether the latest stable release is installed.
    latest_installed: bool,
}

/// What `update` does for one tool once it knows the latest stable release.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    UpToDate,
    /// The active version is newer than the latest stable release, so the
    /// update would be a downgrade.
    ActiveNewer,
    /// Make the latest stable release active, installing it first when
    /// `install` is set. `replaced` is the older version that was active and
    /// stays installed.
    Activate {
        install: bool,
        replaced: Option<String>,
    },
}

pub fn run(args: &UpdateArgs) -> Result<(), Error> {
    let keep_archives = store::keep_archives(args.keep_archives)?;
    let host = Host::detect();
    let layout = Layout::from_env()?;
    let recorded = store::active_state(&layout)?;
    let named = args.tool.is_some();
    let selected: Vec<&ToolSpec> = match args.tool.as_deref() {
        Some(name) => {
            let tool = tools::find_tool(name)?;
            if !is_present(&layout, tool, host, &recorded)? {
                return Err(StoreError::NotInstalledToUpdate {
                    tool: tool.name.to_string(),
                }
                .into());
            }
            vec![tool]
        }
        None => {
            let mut present = Vec::new();
            for tool in tools::all_tools() {
                if is_present(&layout, tool, host, &recorded)? {
                    present.push(tool);
                }
            }
            present
        }
    };
    if selected.is_empty() {
        println!("No tools are installed. Install one with: ckbup install <tool>");
        return Ok(());
    }

    store::sweep_downloads(&layout);
    let client = GithubClient::from_env();
    let options = UpdateOptions {
        portable: args.portable,
        allow_unverified: args.allow_unverified,
        keep_archives,
        remove_old: args.remove_old,
    };
    let mut failed = false;
    for tool in selected {
        if skips(named, recorded.held.contains(tool.name)) {
            let version = recorded.versions.get(tool.name).map(String::as_str);
            print!("{}", held_skip(tool.name, version));
            continue;
        }
        match update_tool(&client, &layout, tool, host, named, options) {
            Ok(()) => {}
            Err(err @ Error::Github(GithubError::RateLimited { .. })) => return Err(err),
            Err(err) => {
                eprintln!("error: {}: {err}", tool.name);
                failed = true;
            }
        }
    }

    if let Some(hint) = path_hint(&layout.bin_dir(), host.os) {
        println!("\n{hint}");
    }
    if failed { Err(Error::Reported) } else { Ok(()) }
}

/// Whether `tool` has an installed version or an `active.toml` entry.
fn is_present(
    layout: &Layout,
    tool: &ToolSpec,
    host: Host,
    recorded: &ActiveState,
) -> Result<bool, Error> {
    let binary_file = host.executable_name(tool.binary);
    Ok(recorded.versions.contains_key(tool.name)
        || !store::installed_versions(layout, tool.name, &binary_file)?.is_empty())
}

/// Whether `update` leaves a tool alone: a held tool is skipped unless it was
/// named on the command line.
fn skips(named: bool, held: bool) -> bool {
    held && !named
}

/// Whether the new version prefers the portable build: with `--portable`, or
/// when the active version was installed from a portable build. A record never
/// forces the standard build, because some standard Linux builds need a newer
/// glibc than the portable ones.
fn prefers_portable(flag: bool, record: Option<&InstallRecord>) -> bool {
    flag || record.is_some_and(|record| record.variant == Variant::Portable)
}

fn decide(current: Current, latest: &str) -> Step {
    let install = !current.latest_installed;
    let Some(active) = current.active else {
        return Step::Activate {
            install,
            replaced: None,
        };
    };
    match newest_first(active, latest) {
        Ordering::Less => Step::ActiveNewer,
        Ordering::Equal if current.active_installed && current.bin_complete => Step::UpToDate,
        Ordering::Equal => Step::Activate {
            install,
            replaced: None,
        },
        Ordering::Greater => Step::Activate {
            install,
            replaced: current.active_installed.then(|| active.to_string()),
        },
    }
}

/// Moves one tool to its latest stable release: one API request, plus the
/// download when that release is not installed yet. A tool named on the
/// command line has its hold released.
fn update_tool(
    client: &GithubClient,
    layout: &Layout,
    tool: &ToolSpec,
    host: Host,
    named: bool,
    options: UpdateOptions,
) -> Result<(), Error> {
    let executables = tool.executables(host);
    store::sweep(layout, tool.name, &executables);
    let release = releases::latest_stable(client, tool)?;
    let latest = release.tag_name.as_str();
    let binary_file = host.executable_name(tool.binary);
    let active = store::active_version(layout, tool.name)?;
    let active_dir = active
        .as_deref()
        .and_then(|version| store::installed_dir(layout, tool.name, version, &binary_file));
    let latest_dir = store::installed_dir(layout, tool.name, latest, &binary_file);
    let current = Current {
        active: active.as_deref(),
        active_installed: active_dir.is_some(),
        bin_complete: active_dir
            .as_deref()
            .is_some_and(|dir| store::bin_holds(layout, dir, &executables)),
        latest_installed: latest_dir.is_some(),
    };
    let hold = if named { Hold::Release } else { Hold::Keep };

    let (install, replaced) = match decide(current, latest) {
        Step::UpToDate => {
            println!("{} {latest} is up to date (latest stable)", tool.name);
            let was_held = store::set_hold(layout, tool.name, hold)?;
            print!("{}", hold_report(tool.name, latest, hold, was_held));
            return Ok(());
        }
        Step::ActiveNewer => {
            println!(
                "{} {} is newer than the latest stable release {latest}; nothing to update",
                tool.name,
                active.as_deref().unwrap_or_default()
            );
            let was_held = store::set_hold(layout, tool.name, hold)?;
            print!("{}", hold_report(tool.name, latest, hold, was_held));
            return Ok(());
        }
        Step::Activate { install, replaced } => (install, replaced),
    };

    let version_dir = match latest_dir {
        Some(dir) if !install => dir,
        _ => {
            let record = active_dir.as_deref().and_then(InstallRecord::read_from);
            let install_options = InstallOptions {
                prefer_portable: prefers_portable(options.portable, record.as_ref()),
                allow_unverified: options.allow_unverified,
                keep_archives: options.keep_archives,
            };
            install::install_release(client, layout, tool, host, &release, install_options)?
        }
    };
    let activation = store::activate(layout, tool.name, latest, &version_dir, &executables, hold)?;
    print!(
        "{}",
        activation_report(tool.name, latest, &activation.on_path, &activation.removed)
    );
    print!(
        "{}",
        hold_report(tool.name, latest, hold, activation.was_held)
    );

    let Some(old) = replaced else {
        return Ok(());
    };
    if options.remove_old {
        let removed = remove::remove_version(layout, tool, &old, host)?;
        print!(
            "{}",
            remove::removal_report(layout, tool.name, &old, &removed)
        );
    } else {
        println!(
            "{0} {old} stays installed. Switch back with: ckbup use {0}@{old}. Remove it with: ckbup remove {0}@{old}",
            tool.name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current(active: Option<&str>) -> Current<'_> {
        Current {
            active,
            active_installed: true,
            bin_complete: true,
            latest_installed: false,
        }
    }

    #[test]
    fn the_latest_stable_release_that_is_active_is_up_to_date() {
        let up_to_date = Current {
            latest_installed: true,
            ..current(Some("v0.10.1"))
        };
        assert_eq!(decide(up_to_date, "v0.10.1"), Step::UpToDate);
    }

    #[test]
    fn an_active_latest_release_missing_from_bin_is_activated_again() {
        let incomplete = Current {
            bin_complete: false,
            latest_installed: true,
            ..current(Some("v0.10.1"))
        };
        assert_eq!(
            decide(incomplete, "v0.10.1"),
            Step::Activate {
                install: false,
                replaced: None
            }
        );
    }

    #[test]
    fn an_active_version_newer_than_the_latest_stable_is_left_alone() {
        assert_eq!(
            decide(current(Some("v0.11.0-rc1")), "v0.10.1"),
            Step::ActiveNewer
        );
        let not_installed = Current {
            active_installed: false,
            ..current(Some("v0.11.0"))
        };
        assert_eq!(decide(not_installed, "v0.10.1"), Step::ActiveNewer);
    }

    #[test]
    fn an_older_active_version_moves_and_stays_installed() {
        for latest_installed in [false, true] {
            let older = Current {
                latest_installed,
                ..current(Some("v0.9.1"))
            };
            assert_eq!(
                decide(older, "v0.10.1"),
                Step::Activate {
                    install: !latest_installed,
                    replaced: Some("v0.9.1".to_string())
                }
            );
        }
        assert_eq!(
            decide(current(Some("v0.10.1-rc1")), "v0.10.1"),
            Step::Activate {
                install: true,
                replaced: Some("v0.10.1-rc1".to_string())
            }
        );
    }

    #[test]
    fn a_missing_or_absent_active_version_activates_the_latest() {
        let missing = Current {
            active_installed: false,
            bin_complete: false,
            ..current(Some("v0.9.1"))
        };
        assert_eq!(
            decide(missing, "v0.10.1"),
            Step::Activate {
                install: true,
                replaced: None
            }
        );
        let missing_latest = Current {
            active_installed: false,
            bin_complete: false,
            ..current(Some("v0.10.1"))
        };
        assert_eq!(
            decide(missing_latest, "v0.10.1"),
            Step::Activate {
                install: true,
                replaced: None
            }
        );
        let nothing_active = Current {
            active_installed: false,
            bin_complete: false,
            ..current(None)
        };
        assert_eq!(
            decide(nothing_active, "v0.10.1"),
            Step::Activate {
                install: true,
                replaced: None
            }
        );
    }

    #[test]
    fn held_tools_are_skipped_unless_named() {
        assert!(skips(false, true));
        assert!(!skips(true, true));
        assert!(!skips(false, false));
        assert!(!skips(true, false));
    }

    #[test]
    fn a_portable_record_makes_the_portable_build_preferred() {
        let record = |variant| InstallRecord {
            archive: "fnn_v0.9.1-x86_64-linux-portable.tar.gz".to_string(),
            variant,
        };
        assert!(prefers_portable(false, Some(&record(Variant::Portable))));
        assert!(!prefers_portable(false, Some(&record(Variant::Standard))));
        assert!(!prefers_portable(false, None));
        assert!(prefers_portable(true, None));
        assert!(prefers_portable(true, Some(&record(Variant::Standard))));
    }
}
