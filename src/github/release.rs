use serde::Deserialize;

use super::{GithubClient, GithubError};

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
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
}

/// Fetches one release with a single API call: the latest stable release when
/// `tag` is `None`, otherwise the release with that exact tag.
pub fn fetch_release(
    client: &GithubClient,
    owner: &str,
    repo: &str,
    tag: Option<&str>,
) -> Result<Release, GithubError> {
    let path = match tag {
        Some(tag) => format!("/repos/{owner}/{repo}/releases/tags/{tag}"),
        None => format!("/repos/{owner}/{repo}/releases/latest"),
    };
    client.get_json(&path).map_err(|err| match (err, tag) {
        (GithubError::NotFound { .. }, Some(tag)) => GithubError::ReleaseNotFound {
            repo: format!("{owner}/{repo}"),
            tag: tag.to_string(),
        },
        (err, _) => err,
    })
}
