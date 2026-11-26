//! Release lookups through each tool's release source. `install`, `update` and
//! `list --available` all find releases here, so they share one meaning of
//! "latest stable".

use crate::github::{self, GithubClient, GithubError, Release};
use crate::tools::{ReleaseSource, ToolSpec, newest_first};

/// The stable releases on the first page of `tool`'s release list, newest
/// first: not drafts, not flagged as prereleases, and no `-` in the tag.
pub fn stable_releases(
    client: &GithubClient,
    tool: &ToolSpec,
) -> Result<Vec<Release>, GithubError> {
    Ok(newest_stable(first_page(client, tool)?))
}

/// The newest stable release of `tool` by version order.
///
/// GitHub's own "latest release" follows publication instead, so a fix
/// published later for an older line, or a release candidate without the
/// prerelease flag, would take its place.
pub fn latest_stable(client: &GithubClient, tool: &ToolSpec) -> Result<Release, GithubError> {
    pick_latest(tool, first_page(client, tool)?)
}

/// The release of `tool` with the exact tag `tag`.
pub fn release(client: &GithubClient, tool: &ToolSpec, tag: &str) -> Result<Release, GithubError> {
    match tool.source {
        ReleaseSource::Github { owner, repo } => github::fetch_release(client, owner, repo, tag),
    }
}

/// One request for the newest releases of `tool`, in no promised order.
fn first_page(client: &GithubClient, tool: &ToolSpec) -> Result<Vec<Release>, GithubError> {
    match tool.source {
        ReleaseSource::Github { owner, repo } => github::list_releases(client, owner, repo),
    }
}

/// Keeps the stable releases of one page and sorts them newest first.
pub fn newest_stable(releases: Vec<Release>) -> Vec<Release> {
    let mut stable: Vec<Release> = releases.into_iter().filter(Release::is_stable).collect();
    stable.sort_by(|a, b| newest_first(&a.tag_name, &b.tag_name));
    stable
}

fn pick_latest(tool: &ToolSpec, releases: Vec<Release>) -> Result<Release, GithubError> {
    let example = releases
        .iter()
        .filter(|release| !release.draft)
        .map(|release| release.tag_name.as_str())
        .min_by(|a, b| newest_first(a, b))
        .unwrap_or("<tag>")
        .to_string();
    newest_stable(releases)
        .into_iter()
        .next()
        .ok_or_else(|| GithubError::NoStableRelease {
            source_name: tool.source.to_string(),
            tool: tool.name.to_string(),
            example,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::find_tool;

    fn page(json: &str) -> Vec<Release> {
        serde_json::from_str(json).unwrap()
    }

    fn tags(releases: &[Release]) -> Vec<&str> {
        releases
            .iter()
            .map(|release| release.tag_name.as_str())
            .collect()
    }

    #[test]
    fn keeps_stable_releases_newest_first() {
        let releases = page(
            r#"[
            {"tag_name": "v0.9.1", "assets": []},
            {"tag_name": "v1.2.0-rc1", "draft": false, "prerelease": false, "assets": []},
            {"tag_name": "v1.3.0-rc1", "draft": false, "prerelease": true, "assets": []},
            {"tag_name": "v1.4.0", "draft": true, "prerelease": false, "assets": []},
            {"tag_name": "v0.31.0", "draft": false, "prerelease": true, "assets": []},
            {"tag_name": "v0.10.1", "assets": []}
        ]"#,
        );

        assert_eq!(tags(&newest_stable(releases)), ["v0.10.1", "v0.9.1"]);
    }

    #[test]
    fn a_later_fix_on_an_older_line_is_not_the_latest() {
        let ckb = find_tool("ckb").unwrap();
        let releases = page(
            r#"[
            {"tag_name": "v0.209.1", "published_at": "2026-10-20T00:00:00Z", "assets": []},
            {"tag_name": "v0.210.0", "published_at": "2026-09-17T00:00:00Z", "assets": []},
            {"tag_name": "v0.211.0-rc1", "published_at": "2026-10-25T00:00:00Z", "assets": []}
        ]"#,
        );

        assert_eq!(pick_latest(ckb, releases).unwrap().tag_name, "v0.210.0");
    }

    #[test]
    fn a_page_without_a_stable_release_names_a_version_to_try() {
        let fnn = find_tool("fnn").unwrap();
        let releases = page(
            r#"[
            {"tag_name": "v0.11.0-rc1", "prerelease": true, "assets": []},
            {"tag_name": "v0.11.0-rc2", "prerelease": true, "assets": []},
            {"tag_name": "v0.12.0", "draft": true, "assets": []}
        ]"#,
        );

        assert_eq!(
            pick_latest(fnn, releases).unwrap_err().to_string(),
            "nervosnetwork/fiber has no stable release on its first page; name a version, for example ckbup install fnn@v0.11.0-rc2"
        );
        assert_eq!(
            pick_latest(fnn, Vec::new()).unwrap_err().to_string(),
            "nervosnetwork/fiber has no stable release on its first page; name a version, for example ckbup install fnn@<tag>"
        );
    }
}
