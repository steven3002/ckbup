use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GithubError {
    #[error("request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: ureq::Error,
    },
    #[error(
        "GitHub API rate limit reached; it resets {reset}. Set GITHUB_TOKEN to raise the limit"
    )]
    RateLimited { reset: String },
    #[error("release {tag} not found in {repo}")]
    ReleaseNotFound { repo: String, tag: String },
    #[error(
        "{source_name} has no stable release on its first page; name a version, for example ckbup install {tool}@{example}"
    )]
    NoStableRelease {
        source_name: String,
        tool: String,
        example: String,
    },
    #[error("not found: {url}")]
    NotFound { url: String },
    #[error("{url} returned HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("unexpected response from {url}: {source}")]
    InvalidJson {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to read the response from {url}: {source}")]
    Io {
        url: String,
        #[source]
        source: std::io::Error,
    },
    #[error("release {tag} has no asset named {name}")]
    AssetMissing { tag: String, name: String },
    #[error("{action} {path}: {source}")]
    File {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "download of {name} stopped at {received} of {size} bytes ({cause}); run the same command again to resume"
    )]
    DownloadStopped {
        name: String,
        received: u64,
        size: u64,
        cause: String,
    },
    #[error(
        "download of {name} is longer than its listed size of {size} bytes, so it was deleted; run the same command again"
    )]
    DownloadTooLong { name: String, size: u64 },
    #[error(
        "download of {name} does not match the release asset GitHub lists ({reason}), so it was deleted; run the same command again"
    )]
    AssetChanged { name: String, reason: String },
}

impl GithubError {
    pub fn file(action: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.to_path_buf();
        move |source| GithubError::File {
            action,
            path,
            source,
        }
    }
}
