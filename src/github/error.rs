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
}
