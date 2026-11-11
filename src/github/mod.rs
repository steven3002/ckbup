mod client;
mod error;
mod release;

pub use client::GithubClient;
pub use error::GithubError;
pub use release::{Release, fetch_release};
