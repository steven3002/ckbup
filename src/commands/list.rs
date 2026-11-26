use crate::cli::ListArgs;
use crate::error::Error;
use crate::github::{GithubClient, Release};
use crate::output::version_list::{self, AvailableHeader, ReleaseRow, State, ToolRow, VersionRow};
use crate::platform::Host;
use crate::releases;
use crate::store::{self, ActiveState, Layout};
use crate::tools::{self, ToolSpec, newest_first};

/// How many stable releases `list --available` shows per tool.
const AVAILABLE_LIMIT: usize = 10;

/// Lists installed versions, or with `--available` the newest stable releases
/// on GitHub. It never writes to the ckbup home.
pub fn run(args: &ListArgs) -> Result<(), Error> {
    let host = Host::detect();
    let layout = Layout::from_env()?;
    let only = args.tool.as_deref().map(tools::find_tool).transpose()?;
    let selected: Vec<&ToolSpec> = match only {
        Some(tool) => vec![tool],
        None => tools::all_tools().iter().collect(),
    };
    let active = store::active_state(&layout)?;

    if args.available {
        let client = GithubClient::from_env();
        for (index, tool) in selected.iter().enumerate() {
            let stable = releases::stable_releases(&client, tool)?;
            let versions = version_rows(&layout, tool, host, &active)?;
            let header = AvailableHeader {
                tool: tool.name.to_string(),
                repo: tool.source.to_string(),
                host: format!("{}/{}", host.os.as_str(), host.arch.as_str()),
            };
            if index > 0 {
                println!();
            }
            print!(
                "{}",
                version_list::available_view(
                    &header,
                    &release_rows(tool, host, &stable, &versions)
                )
            );
        }
        return Ok(());
    }

    if let Some(tool) = only {
        let versions = version_rows(&layout, tool, host, &active)?;
        print!("{}", version_list::tool_view(tool.name, &versions));
        return Ok(());
    }
    let rows = selected
        .iter()
        .map(|tool| {
            Ok(ToolRow {
                tool: tool.name.to_string(),
                versions: version_rows(&layout, tool, host, &active)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    print!("{}", version_list::tools_view(&rows));
    Ok(())
}

/// The installed versions of `tool`, newest first, with the active one marked
/// and, when the tool is held, marked held. An active version that is no
/// longer installed is listed in its place.
fn version_rows(
    layout: &Layout,
    tool: &ToolSpec,
    host: Host,
    recorded: &ActiveState,
) -> Result<Vec<VersionRow>, Error> {
    let active = recorded.versions.get(tool.name);
    let active_state = if recorded.held.contains(tool.name) {
        State::ActiveHeld
    } else {
        State::Active
    };
    let binary_file = host.executable_name(tool.binary);
    let mut rows: Vec<VersionRow> = store::installed_versions(layout, tool.name, &binary_file)?
        .into_iter()
        .map(|version| VersionRow {
            state: if active == Some(&version) {
                active_state
            } else {
                State::Installed
            },
            folder: Some(layout.toolchain_dir(tool.name, &version)),
            version,
        })
        .collect();
    let missing = active.filter(|active| rows.iter().all(|row| &row.version != *active));
    if let Some(version) = missing {
        rows.push(VersionRow {
            version: version.clone(),
            state: State::ActiveMissing,
            folder: None,
        });
        rows.sort_by(|a, b| newest_first(&a.version, &b.version));
    }
    Ok(rows)
}

/// The first `AVAILABLE_LIMIT` of the stable releases, which come newest
/// first, with their state here and what `host` lacks for each.
fn release_rows(
    tool: &ToolSpec,
    host: Host,
    stable: &[Release],
    versions: &[VersionRow],
) -> Vec<ReleaseRow> {
    stable
        .iter()
        .take(AVAILABLE_LIMIT)
        .map(|release| ReleaseRow {
            tag: release.tag_name.clone(),
            date: release.published_date().unwrap_or_default().to_string(),
            state: versions
                .iter()
                .find(|row| row.version == release.tag_name)
                .map(|row| row.state),
            note: host_note(tool, host, release),
        })
        .collect()
}

/// What `host` lacks for `release`: an archive under the names `install`
/// looks for, or any checksum for the archive it would pick.
fn host_note(tool: &ToolSpec, host: Host, release: &Release) -> Option<String> {
    let published: Vec<&str> = release
        .assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect();
    let Ok(candidate) = tools::select_asset(tool, &release.tag_name, host, false, &published)
    else {
        return Some(format!(
            "no archive for {}/{}",
            host.os.as_str(),
            host.arch.as_str()
        ));
    };
    let digest = release
        .asset(&candidate.archive)
        .is_some_and(|asset| asset.digest.is_some());
    let sums_file = candidate
        .sums_file
        .as_deref()
        .is_some_and(|name| release.asset(name).is_some());
    (!digest && !sums_file).then(|| "no checksum (needs --allow-unverified)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};
    use crate::store::fake_home;

    const WINDOWS: Host = Host {
        os: Os::Windows,
        arch: Arch::X86_64,
    };

    /// Releases of ckb-standalone-debugger as GitHub lists them, cut down to
    /// the Windows assets, plus a draft that only push access would show.
    const DEBUGGER_RELEASES: &str = r#"[
        {"tag_name": "v0.200.0-rc1", "draft": false, "prerelease": false,
         "published_at": "2025-02-27T02:58:47Z", "assets": [
            {"name": "ckb-debugger-windows-x64.tar.gz", "browser_download_url": "u", "digest": null}]},
        {"tag_name": "v0.121.0", "draft": false, "prerelease": false,
         "published_at": "2025-02-05T08:49:24Z", "assets": [
            {"name": "ckb-debugger-windows-x64-sha256.txt", "browser_download_url": "u", "digest": null},
            {"name": "ckb-debugger-windows-x64.tar.gz", "browser_download_url": "u", "digest": null}]},
        {"tag_name": "v1.1.2", "draft": false, "prerelease": false,
         "published_at": "2026-11-16T05:56:01Z", "assets": [
            {"name": "ckb-debugger_v1.1.2_x86_64-pc-windows-msvc-sha256.txt", "browser_download_url": "u",
             "digest": "sha256:11f089e30e1db108"},
            {"name": "ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz", "browser_download_url": "u",
             "digest": "sha256:1b03e2b6b83be85c"}]},
        {"tag_name": "v1.2.0", "draft": true, "prerelease": false,
         "published_at": null, "assets": [
            {"name": "ckb-debugger_v1.2.0_x86_64-pc-windows-msvc.tar.gz", "browser_download_url": "u",
             "digest": "sha256:00"}]},
        {"tag_name": "v0.200.1", "draft": false, "prerelease": false,
         "published_at": "2025-04-24T06:15:10Z", "assets": [
            {"name": "ckb-debugger_v0.200.1_x86_64-pc-windows-msvc-sha256.txt", "browser_download_url": "u",
             "digest": null},
            {"name": "ckb-debugger_v0.200.1_x86_64-pc-windows-msvc.tar.gz", "browser_download_url": "u",
             "digest": null}]}
    ]"#;

    /// The stable releases of a page as `list --available` receives them.
    fn releases(json: &str) -> Vec<Release> {
        releases::newest_stable(serde_json::from_str(json).unwrap())
    }

    fn row(tag: &str, date: &str, state: Option<State>, note: Option<&str>) -> ReleaseRow {
        ReleaseRow {
            tag: tag.to_string(),
            date: date.to_string(),
            state,
            note: note.map(str::to_string),
        }
    }

    fn installed(version: &str, state: State) -> VersionRow {
        VersionRow {
            version: version.to_string(),
            state,
            folder: None,
        }
    }

    #[test]
    fn available_rows_keep_stable_releases_and_note_what_the_host_lacks() {
        let debugger = tools::find_tool("ckb-debugger").unwrap();
        let versions = [installed("v1.1.2", State::Active)];

        let rows = release_rows(debugger, WINDOWS, &releases(DEBUGGER_RELEASES), &versions);

        assert_eq!(
            rows,
            [
                row("v1.1.2", "2026-11-16", Some(State::Active), None),
                row("v0.200.1", "2025-04-24", None, None),
                row(
                    "v0.121.0",
                    "2025-02-05",
                    None,
                    Some("no archive for windows/x86_64")
                ),
            ]
        );
    }

    #[test]
    fn a_release_without_any_checksum_needs_allow_unverified() {
        let light_client = tools::find_tool("ckb-light-client").unwrap();
        let json = r#"[
            {"tag_name": "v0.4.1", "draft": false, "prerelease": false,
             "published_at": "2024-11-13T03:42:47Z", "assets": [
                {"name": "ckb-light-client_v0.4.1-x86_64-windows.tar.gz", "browser_download_url": "u",
                 "digest": null}]}
        ]"#;
        let versions = [installed("v0.4.1", State::Installed)];

        let rows = release_rows(light_client, WINDOWS, &releases(json), &versions);

        assert_eq!(
            rows,
            [row(
                "v0.4.1",
                "2024-11-13",
                Some(State::Installed),
                Some("no checksum (needs --allow-unverified)")
            )]
        );
    }

    #[test]
    fn only_the_newest_ten_stable_releases_show() {
        let fnn = tools::find_tool("fnn").unwrap();
        let tags = [
            "v0.6.1", "v0.10.0", "v0.5.0", "v0.8.0", "v0.9.1", "v0.10.1", "v0.7.0", "v0.5.1",
            "v0.9.0", "v0.6.0", "v0.8.1", "v0.7.1",
        ];
        let json: Vec<String> = tags
            .iter()
            .map(|tag| format!(r#"{{"tag_name": "{tag}", "assets": []}}"#))
            .collect();
        let json = format!("[{}]", json.join(","));

        let rows = release_rows(fnn, WINDOWS, &releases(&json), &[]);

        let shown: Vec<&str> = rows.iter().map(|row| row.tag.as_str()).collect();
        assert_eq!(
            shown,
            [
                "v0.10.1", "v0.10.0", "v0.9.1", "v0.9.0", "v0.8.1", "v0.8.0", "v0.7.1", "v0.7.0",
                "v0.6.1", "v0.6.0"
            ]
        );
    }

    /// `active.toml` with fnn at `version`, held when `held` is true.
    fn fnn_active(version: &str, held: bool) -> ActiveState {
        let mut state = ActiveState::default();
        state
            .versions
            .insert("fnn".to_string(), version.to_string());
        if held {
            state.held.insert("fnn".to_string());
        }
        state
    }

    #[test]
    fn version_rows_mark_the_active_version_and_one_that_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let fnn = tools::find_tool("fnn").unwrap();
        let linux = Host {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        fake_home::install(&layout, "fnn", "v0.9.1", &["fnn", "fnn-cli"]);
        fake_home::install(&layout, "fnn", "v0.10.1", &["fnn", "fnn-cli"]);

        let rows = version_rows(&layout, fnn, linux, &fnn_active("v0.9.1", false)).unwrap();
        assert_eq!(
            rows,
            [
                VersionRow {
                    version: "v0.10.1".to_string(),
                    state: State::Installed,
                    folder: Some(layout.toolchain_dir("fnn", "v0.10.1")),
                },
                VersionRow {
                    version: "v0.9.1".to_string(),
                    state: State::Active,
                    folder: Some(layout.toolchain_dir("fnn", "v0.9.1")),
                },
            ]
        );

        let rows = version_rows(&layout, fnn, linux, &fnn_active("v0.9.5", false)).unwrap();
        let states: Vec<(&str, State)> = rows
            .iter()
            .map(|row| (row.version.as_str(), row.state))
            .collect();
        assert_eq!(
            states,
            [
                ("v0.10.1", State::Installed),
                ("v0.9.5", State::ActiveMissing),
                ("v0.9.1", State::Installed),
            ]
        );

        let rows = version_rows(&layout, fnn, linux, &fnn_active("v0.9.1", true)).unwrap();
        let states: Vec<State> = rows.iter().map(|row| row.state).collect();
        assert_eq!(states, [State::Installed, State::ActiveHeld]);
    }
}
