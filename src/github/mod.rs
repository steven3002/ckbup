mod client;
mod download;
mod error;
mod part_record;
mod release;
#[cfg(test)]
pub mod test_server;

pub use client::GithubClient;
pub use download::download_asset;
pub use error::GithubError;
pub use release::{Asset, Release, fetch_release, list_releases};
