use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use ureq::http::Response;
use ureq::{Agent, Body, BodyReader, Proxy};

use super::GithubError;

const USER_AGENT: &str = concat!("ckbup/", env!("CARGO_PKG_VERSION"));
const API_ROOT: &str = "https://api.github.com";

/// Minimal GitHub client: JSON API calls and release asset downloads.
pub struct GithubClient {
    agent: Agent,
    token: Option<String>,
}

/// The status and the headers of a release asset response that decide
/// whether its body continues the bytes already on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetHead {
    pub status: u16,
    pub content_range: Option<String>,
    pub content_encoding: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// A release asset response whose body has not been read yet.
pub struct AssetResponse {
    pub head: AssetHead,
    body: Body,
}

impl AssetResponse {
    pub fn into_reader(self) -> BodyReader<'static> {
        self.body.into_reader()
    }
}

impl GithubClient {
    /// Builds a client that sends `GITHUB_TOKEN` to the API when it is set,
    /// and uses the proxy named by the usual environment variables.
    pub fn from_env() -> Self {
        let token = std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|token| !token.trim().is_empty());
        Self::new(token, Proxy::try_from_env())
    }

    /// A client for the local test server: no token, and no proxy from the
    /// environment between it and `127.0.0.1`.
    #[cfg(test)]
    pub fn for_test_server() -> Self {
        Self::new(None, None)
    }

    fn new(token: Option<String>, proxy: Option<Proxy>) -> Self {
        let config = Agent::config_builder()
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(600)))
            .proxy(proxy)
            .build();
        GithubClient {
            agent: Agent::new_with_config(config),
            token,
        }
    }

    /// GETs `path` (relative to the API root) and decodes the JSON body.
    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, GithubError> {
        let url = format!("{API_ROOT}{path}");
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let response = request.call().map_err(|source| GithubError::Request {
            url: url.clone(),
            source,
        })?;
        let mut response = check_status(response, &url)?;
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|source| GithubError::Request {
                url: url.clone(),
                source,
            })?;
        serde_json::from_str(&body).map_err(|source| GithubError::InvalidJson { url, source })
    }

    /// Streams the file at `url` into `writer` and returns the number of bytes written.
    ///
    /// Release downloads go to public URLs, so the token is never attached to them.
    pub fn download(&self, url: &str, writer: &mut impl Write) -> Result<u64, GithubError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|source| GithubError::Request {
                url: url.to_string(),
                source,
            })?;
        let response = check_status(response, url)?;
        let mut reader = response.into_body().into_reader();
        io::copy(&mut reader, writer).map_err(|source| GithubError::Io {
            url: url.to_string(),
            source,
        })
    }

    /// GETs the release asset at `url` from byte `offset` on, asking for it
    /// unencoded. A 416 comes back as a response, because for a download that
    /// resumes it can mean that nothing is left; every other status that is
    /// not a success is an error. The token is never attached.
    pub fn get_asset(&self, url: &str, offset: u64) -> Result<AssetResponse, GithubError> {
        let mut request = self.agent.get(url).header("Accept-Encoding", "identity");
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request.call().map_err(|source| GithubError::Request {
            url: url.to_string(),
            source,
        })?;
        let response = if response.status().as_u16() == 416 {
            response
        } else {
            check_status(response, url)?
        };
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let head = AssetHead {
            status: response.status().as_u16(),
            content_range: header("content-range"),
            content_encoding: header("content-encoding"),
            etag: header("etag"),
            last_modified: header("last-modified"),
        };
        Ok(AssetResponse {
            head,
            body: response.into_body(),
        })
    }
}

fn check_status<B>(response: Response<B>, url: &str) -> Result<Response<B>, GithubError> {
    let status = response.status().as_u16();
    if response.status().is_success() {
        return Ok(response);
    }
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    if matches!(status, 403 | 429) && header("x-ratelimit-remaining").as_deref() == Some("0") {
        let reset = header("x-ratelimit-reset")
            .and_then(|value| value.parse::<u64>().ok())
            .map(describe_reset)
            .unwrap_or_else(|| "later".to_string());
        return Err(GithubError::RateLimited { reset });
    }
    let url = url.to_string();
    if status == 404 {
        return Err(GithubError::NotFound { url });
    }
    Err(GithubError::Status { url, status })
}

fn describe_reset(reset_epoch: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let minutes = reset_epoch.saturating_sub(now).div_ceil(60);
    format!("in about {minutes} minute(s) (epoch {reset_epoch})")
}
