//! Text for the three `ckbup list` views, built from plain rows so that it
//! needs neither the filesystem nor the network.

use std::path::PathBuf;

/// How a listed version relates to `active.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Installed,
    Active,
    /// Recorded as active, but no longer installed.
    ActiveMissing,
}

impl State {
    fn label(self) -> &'static str {
        match self {
            State::Installed => "installed",
            State::Active => "active",
            State::ActiveMissing => "active, not installed",
        }
    }
}

/// An installed version, or the recorded active version when it is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    pub version: String,
    pub state: State,
    /// The version folder; `None` when the version is not installed.
    pub folder: Option<PathBuf>,
}

/// One supported tool and its versions, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRow {
    pub tool: String,
    pub versions: Vec<VersionRow>,
}

/// One stable release on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRow {
    pub tag: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Whether the release is installed or active here.
    pub state: Option<State>,
    /// What this host lacks for the release: an archive, or a checksum.
    pub note: Option<String>,
}

/// The tool, repository (`<owner>/<repo>`) and host (`<os>/<arch>`) of an
/// available-releases view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSource {
    pub tool: String,
    pub repo: String,
    pub host: String,
}

/// `ckbup list`: one line per tool with its versions, or `not installed`.
pub fn tools_view(tools: &[ToolRow]) -> String {
    let rows: Vec<Vec<String>> = tools
        .iter()
        .map(|tool| {
            let versions = if tool.versions.is_empty() {
                "not installed".to_string()
            } else {
                tool.versions
                    .iter()
                    .map(|row| match row.state {
                        State::Installed => row.version.clone(),
                        state => format!("{} ({})", row.version, state.label()),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            vec![tool.tool.clone(), versions]
        })
        .collect();
    let mut text = table(&rows);
    let hints: Vec<String> = tools
        .iter()
        .filter_map(|tool| missing_active_hint(&tool.tool, &tool.versions))
        .collect();
    if !hints.is_empty() {
        text.push('\n');
        for hint in hints {
            text.push_str(&hint);
            text.push('\n');
        }
    }
    text
}

/// `ckbup list <tool>`: one line per version with its state and folder.
pub fn tool_view(tool: &str, versions: &[VersionRow]) -> String {
    if versions.is_empty() {
        return format!("{tool}: no versions installed. Install one with: ckbup install {tool}\n");
    }
    let rows: Vec<Vec<String>> = versions
        .iter()
        .map(|row| {
            let state = match row.state {
                State::Installed => String::new(),
                state => state.label().to_string(),
            };
            let folder = row
                .folder
                .as_ref()
                .map(|folder| folder.display().to_string())
                .unwrap_or_default();
            vec![row.version.clone(), state, folder]
        })
        .collect();
    let mut text = table(&rows);
    if let Some(hint) = missing_active_hint(tool, versions) {
        text.push('\n');
        text.push_str(&hint);
        text.push('\n');
    }
    text
}

/// `ckbup list --available`: the newest stable releases of one tool, with
/// their state and what this host lacks for them.
pub fn available_view(source: &ReleaseSource, releases: &[ReleaseRow]) -> String {
    let mut text = format!(
        "{} ({}), newest stable releases for {}:\n",
        source.tool, source.repo, source.host
    );
    if releases.is_empty() {
        text.push_str("  none found\n");
    }
    let rows: Vec<Vec<String>> = releases
        .iter()
        .map(|release| {
            let notes: Vec<&str> = release
                .state
                .map(State::label)
                .into_iter()
                .chain(release.note.as_deref())
                .collect();
            vec![release.tag.clone(), release.date.clone(), notes.join(", ")]
        })
        .collect();
    for line in table(&rows).lines() {
        text.push_str("  ");
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(&format!(
        "Older releases: https://github.com/{}/releases\n",
        source.repo
    ));
    text
}

/// Says how to recover when the active version of `tool` is not installed.
fn missing_active_hint(tool: &str, versions: &[VersionRow]) -> Option<String> {
    let missing = versions
        .iter()
        .find(|row| row.state == State::ActiveMissing)?;
    let version = &missing.version;
    let reinstall = format!("ckbup install {tool}@{version}");
    let fix = match versions.iter().find(|row| row.state == State::Installed) {
        Some(other) => format!(
            "Switch with: ckbup use {tool}@{}, or install it again with: {reinstall}",
            other.version
        ),
        None => format!("Install it again with: {reinstall}"),
    };
    Some(format!(
        "{tool} {version} is recorded as active but is not installed. {fix}"
    ))
}

/// Left-aligned columns separated by two spaces. Columns that are empty in
/// every row are left out, and trailing spaces are trimmed.
fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut text = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .filter(|(_, width)| **width > 0)
            .map(|(cell, width)| format!("{cell:width$}"))
            .collect();
        text.push_str(cells.join("  ").trim_end());
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(version: &str, state: State, folder: Option<&str>) -> VersionRow {
        VersionRow {
            version: version.to_string(),
            state,
            folder: folder.map(PathBuf::from),
        }
    }

    fn tool(name: &str, versions: Vec<VersionRow>) -> ToolRow {
        ToolRow {
            tool: name.to_string(),
            versions,
        }
    }

    #[test]
    fn tools_view_shows_every_tool_on_one_line() {
        let tools = [
            tool("ckb", vec![row("v0.210.0", State::Active, None)]),
            tool("ckb-cli", vec![row("v2.0.0", State::Active, None)]),
            tool("ckb-debugger", vec![row("v1.1.1", State::Active, None)]),
            tool(
                "fnn",
                vec![
                    row("v0.10.1", State::Active, None),
                    row("v0.9.1", State::Installed, None),
                ],
            ),
            tool("ckb-light-client", vec![]),
        ];
        assert_eq!(
            tools_view(&tools),
            "\
ckb               v0.210.0 (active)
ckb-cli           v2.0.0 (active)
ckb-debugger      v1.1.1 (active)
fnn               v0.10.1 (active), v0.9.1
ckb-light-client  not installed
"
        );
    }

    #[test]
    fn tools_view_flags_an_active_version_that_is_gone() {
        let tools = [
            tool(
                "fnn",
                vec![
                    row("v0.10.1", State::Installed, None),
                    row("v0.9.1", State::ActiveMissing, None),
                ],
            ),
            tool("ckb", vec![row("v0.210.0", State::ActiveMissing, None)]),
        ];
        assert_eq!(
            tools_view(&tools),
            "\
fnn  v0.10.1, v0.9.1 (active, not installed)
ckb  v0.210.0 (active, not installed)

fnn v0.9.1 is recorded as active but is not installed. Switch with: ckbup use fnn@v0.10.1, or install it again with: ckbup install fnn@v0.9.1
ckb v0.210.0 is recorded as active but is not installed. Install it again with: ckbup install ckb@v0.210.0
"
        );
    }

    #[test]
    fn tool_view_shows_each_version_with_its_folder() {
        let versions = [
            row(
                "v0.10.1",
                State::Active,
                Some("/home/u/.ckbup/toolchains/fnn/v0.10.1"),
            ),
            row(
                "v0.9.1",
                State::Installed,
                Some("/home/u/.ckbup/toolchains/fnn/v0.9.1"),
            ),
        ];
        assert_eq!(
            tool_view("fnn", &versions),
            "\
v0.10.1  active  /home/u/.ckbup/toolchains/fnn/v0.10.1
v0.9.1           /home/u/.ckbup/toolchains/fnn/v0.9.1
"
        );
    }

    #[test]
    fn tool_view_without_versions_says_how_to_install() {
        assert_eq!(
            tool_view("fnn", &[]),
            "fnn: no versions installed. Install one with: ckbup install fnn\n"
        );
    }

    #[test]
    fn tool_view_flags_an_active_version_that_is_gone() {
        let versions = [row("v0.9.1", State::ActiveMissing, None)];
        assert_eq!(
            tool_view("fnn", &versions),
            "\
v0.9.1  active, not installed

fnn v0.9.1 is recorded as active but is not installed. Install it again with: ckbup install fnn@v0.9.1
"
        );
    }

    #[test]
    fn tool_view_without_an_active_version_has_no_state_column() {
        let versions = [row("v0.9.1", State::Installed, Some("/f/v0.9.1"))];
        assert_eq!(tool_view("fnn", &versions), "v0.9.1  /f/v0.9.1\n");
    }

    fn release(tag: &str, date: &str, state: Option<State>, note: Option<&str>) -> ReleaseRow {
        ReleaseRow {
            tag: tag.to_string(),
            date: date.to_string(),
            state,
            note: note.map(str::to_string),
        }
    }

    #[test]
    fn available_view_shows_state_notes_and_the_releases_page() {
        let source = ReleaseSource {
            tool: "ckb-light-client".to_string(),
            repo: "nervosnetwork/ckb-light-client".to_string(),
            host: "windows/x86_64".to_string(),
        };
        let releases = [
            release("v0.5.4", "2026-01-02", Some(State::Active), None),
            release("v0.5.3", "2025-09-09", None, None),
            release(
                "v0.4.1",
                "2024-11-13",
                Some(State::Installed),
                Some("no checksum (needs --allow-unverified)"),
            ),
        ];
        assert_eq!(
            available_view(&source, &releases),
            "\
ckb-light-client (nervosnetwork/ckb-light-client), newest stable releases for windows/x86_64:
  v0.5.4  2026-01-02  active
  v0.5.3  2025-09-09
  v0.4.1  2024-11-13  installed, no checksum (needs --allow-unverified)
Older releases: https://github.com/nervosnetwork/ckb-light-client/releases
"
        );
    }

    #[test]
    fn available_view_without_stable_releases() {
        let source = ReleaseSource {
            tool: "fnn".to_string(),
            repo: "nervosnetwork/fiber".to_string(),
            host: "linux/x86_64".to_string(),
        };
        assert_eq!(
            available_view(&source, &[]),
            "\
fnn (nervosnetwork/fiber), newest stable releases for linux/x86_64:
  none found
Older releases: https://github.com/nervosnetwork/fiber/releases
"
        );
    }
}
