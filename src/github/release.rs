use serde::Deserialize;

use super::{GithubClient, GithubError};

/// GitHub's maximum page size; larger values are cut to it silently.
const MAX_PER_PAGE: u32 = 100;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    #[serde(default)]
    pub published_at: Option<String>,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    /// GitHub's id for the uploaded file; a file replaced under the same name
    /// gets a new id.
    #[serde(default)]
    pub id: u64,
    pub name: String,
    /// Size in bytes.
    #[serde(default)]
    pub size: u64,
    pub browser_download_url: String,
    /// `sha256:<hex>`; absent or null on assets GitHub has not hashed.
    #[serde(default)]
    pub digest: Option<String>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }

    pub fn require_asset(&self, name: &str) -> Result<&Asset, GithubError> {
        self.asset(name).ok_or_else(|| GithubError::AssetMissing {
            tag: self.tag_name.clone(),
            name: name.to_string(),
        })
    }

    /// Not a draft, not flagged as a prerelease, and a tag without a `-`
    /// suffix: some projects publish release candidates without the flag.
    pub fn is_stable(&self) -> bool {
        !self.draft && !self.prerelease && !self.tag_name.contains('-')
    }

    /// The `YYYY-MM-DD` part of `published_at`.
    pub fn published_date(&self) -> Option<&str> {
        self.published_at.as_deref().and_then(|date| date.get(..10))
    }
}

/// Fetches the release with the exact tag `tag` with a single API call.
pub fn fetch_release(
    client: &GithubClient,
    owner: &str,
    repo: &str,
    tag: &str,
) -> Result<Release, GithubError> {
    client
        .get_json(&format!("/repos/{owner}/{repo}/releases/tags/{tag}"))
        .map_err(|err| match err {
            GithubError::NotFound { .. } => GithubError::ReleaseNotFound {
                repo: format!("{owner}/{repo}"),
                tag: tag.to_string(),
            },
            err => err,
        })
}

/// Fetches the first page of a repository's releases, up to 100, with a
/// single API call. GitHub documents no order for this list, so callers sort it.
pub fn list_releases(
    client: &GithubClient,
    owner: &str,
    repo: &str,
) -> Result<Vec<Release>, GithubError> {
    client.get_json(&format!(
        "/repos/{owner}/{repo}/releases?per_page={MAX_PER_PAGE}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_release_flags_and_date() {
        let json = r#"[
            {"tag_name": "v0.6.1-rc1", "draft": false, "prerelease": false,
             "published_at": "2026-01-10T03:00:00Z", "assets": []},
            {"tag_name": "v1.1.2", "draft": false, "prerelease": false,
             "published_at": "2026-11-16T05:56:01Z", "assets": []},
            {"tag_name": "v1.2.0", "draft": true, "prerelease": false,
             "published_at": null, "assets": []},
            {"tag_name": "v0.10.0-rc3", "draft": false, "prerelease": true,
             "published_at": "2026-11-12T00:00:00Z", "assets": []}
        ]"#;
        let releases: Vec<Release> = serde_json::from_str(json).unwrap();
        let stable: Vec<&str> = releases
            .iter()
            .filter(|release| release.is_stable())
            .map(|release| release.tag_name.as_str())
            .collect();
        assert_eq!(stable, ["v1.1.2"]);
        assert_eq!(releases[1].published_date(), Some("2026-11-16"));
        assert_eq!(releases[2].published_date(), None);
    }

    #[test]
    fn single_release_lookups_need_no_flags() {
        let release: Release =
            serde_json::from_str(r#"{"tag_name": "v2.0.0", "assets": []}"#).unwrap();
        assert!(release.is_stable());
        assert_eq!(release.published_date(), None);
    }

    #[test]
    fn reads_the_asset_id_and_size() {
        let release: Release = serde_json::from_str(
            r#"{"tag_name": "v1.1.2", "assets": [
                {"id": 753671710, "name": "ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz",
                 "size": 3013794, "browser_download_url": "u",
                 "digest": "sha256:1b03e2b6b83be85c1fa2410140eb75aa699a08ab430ecfc6d4fad99443b5b690"},
                {"name": "listed-without-them.txt", "browser_download_url": "u"}]}"#,
        )
        .unwrap();
        assert_eq!(release.assets[0].id, 753671710);
        assert_eq!(release.assets[0].size, 3013794);
        assert_eq!((release.assets[1].id, release.assets[1].size), (0, 0));
    }
}
