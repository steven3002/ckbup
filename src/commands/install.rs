use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use crate::archive::{self, ArchiveFormat};
use crate::cli::InstallArgs;
use crate::error::Error;
use crate::github::{GithubClient, GithubError, Release, fetch_release};
use crate::integrity::{self, HashingWriter, IntegrityError};
use crate::output::path_hint;
use crate::platform::Host;
use crate::store::{self, Layout, StoreError};
use crate::tools::{self, ReleaseAssets, ToolSpec};

/// A SHA-256 the download must match, and where it was published.
#[derive(Debug)]
pub struct ExpectedHash {
    pub source_name: String,
    pub hex: String,
}

pub fn run(args: &InstallArgs) -> Result<(), Error> {
    let (tool_name, requested) = parse_target(&args.target);
    let tool = tools::find_tool(tool_name)?;
    let host = Host::detect();
    // Fail on unsupported hosts before spending an API request.
    tools::target_triple(tool, host)?;

    let layout = Layout::from_env()?;
    let client = GithubClient::from_env();
    let release = fetch_release(&client, tool.owner, tool.repo, requested.as_deref())?;
    let tag = release.tag_name.as_str();
    let assets = tools::release_assets(tool, tag, host)?;
    let file_name = host.executable_name(tool.binary);

    let binary = match store::installed_binary(&layout, tool.name, tag, &file_name) {
        Some(path) => {
            println!(
                "{} {tag} is already installed at {}",
                tool.name,
                path.display()
            );
            path
        }
        None => {
            let expected = expected_hashes(&client, &release, &assets, args.allow_unverified)?;
            let path = download_and_install(
                &client, &layout, tool, &release, &assets, &expected, &file_name,
            )?;
            println!("Installed {} {tag} to {}", tool.name, path.display());
            path
        }
    };

    match store::active_version(&layout, tool.name)? {
        Some(active) if active != tag => println!(
            "{} {active} remains the active version; {tag} is installed alongside it",
            tool.name
        ),
        _ => {
            store::activate(&layout, tool.name, tag, &binary, &file_name)?;
            println!(
                "{} {tag} is active: {}",
                tool.name,
                layout.bin_dir().join(&file_name).display()
            );
        }
    }

    if let Some(hint) = path_hint(&layout.bin_dir(), host.os) {
        println!("\n{hint}");
    }
    Ok(())
}

/// Splits `<tool>[@<version>]`; a version given without the `v` prefix gets one,
/// matching the tag format of every supported repository.
fn parse_target(target: &str) -> (&str, Option<String>) {
    match target.split_once('@') {
        Some((tool, version)) if version.starts_with(|c: char| c.is_ascii_digit()) => {
            (tool, Some(format!("v{version}")))
        }
        Some((tool, version)) => (tool, Some(version.to_string())),
        None => (target, None),
    }
}

/// Collects every published checksum for the archive: the GitHub asset digest
/// and, where the project publishes one, its own checksum file.
fn expected_hashes(
    client: &GithubClient,
    release: &Release,
    assets: &ReleaseAssets,
    allow_unverified: bool,
) -> Result<Vec<ExpectedHash>, Error> {
    let archive = release.require_asset(&assets.archive)?;
    let mut expected = Vec::new();
    if let Some(digest) = &archive.digest {
        expected.push(ExpectedHash {
            source_name: "GitHub asset digest".to_string(),
            hex: integrity::parse_api_digest(digest)?,
        });
    }
    if let Some(sums) = assets
        .sums_file
        .as_deref()
        .and_then(|name| release.asset(name))
    {
        let mut contents = Vec::new();
        client.download(&sums.browser_download_url, &mut contents)?;
        let text = String::from_utf8_lossy(&contents);
        expected.push(ExpectedHash {
            hex: integrity::hash_for_asset(&text, &sums.name, &archive.name)?,
            source_name: sums.name.clone(),
        });
    }
    if expected.is_empty() && !allow_unverified {
        return Err(IntegrityError::MissingChecksum {
            asset: archive.name.clone(),
        }
        .into());
    }
    if expected.is_empty() {
        eprintln!(
            "warning: installing {} without checksum verification",
            archive.name
        );
    }
    Ok(expected)
}

fn download_and_install(
    client: &GithubClient,
    layout: &Layout,
    tool: &ToolSpec,
    release: &Release,
    assets: &ReleaseAssets,
    expected: &[ExpectedHash],
    file_name: &str,
) -> Result<PathBuf, Error> {
    let archive_asset = release.require_asset(&assets.archive)?;
    let downloads = layout.downloads_dir();
    fs::create_dir_all(&downloads).map_err(StoreError::io("cannot create", &downloads))?;
    let work =
        tempfile::tempdir_in(&downloads).map_err(StoreError::io("cannot create", &downloads))?;

    let archive_path = work.path().join(&archive_asset.name);
    println!("Downloading {}", archive_asset.name);
    let file =
        File::create(&archive_path).map_err(StoreError::io("cannot write", &archive_path))?;
    let mut writer = HashingWriter::new(BufWriter::new(file));
    client.download(&archive_asset.browser_download_url, &mut writer)?;
    let (_, actual) = writer.finish().map_err(|source| GithubError::Io {
        url: archive_asset.browser_download_url.clone(),
        source,
    })?;

    install_verified(
        layout,
        tool.name,
        &release.tag_name,
        &archive_path,
        assets.format,
        &actual,
        expected,
        file_name,
        work.path(),
    )
}

/// Verifies a downloaded archive and places its binary in the toolchain store.
///
/// Nothing is written outside `work_dir` until every checksum matches; the
/// caller's temporary `work_dir` (and the download in it) is removed on return.
#[allow(clippy::too_many_arguments)]
fn install_verified(
    layout: &Layout,
    tool: &str,
    tag: &str,
    archive: &Path,
    format: ArchiveFormat,
    actual_sha256: &str,
    expected: &[ExpectedHash],
    file_name: &str,
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let archive_name = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for check in expected {
        integrity::ensure_match(&archive_name, &check.source_name, &check.hex, actual_sha256)?;
        println!("Verified SHA-256 against {}", check.source_name);
    }

    let extracted = work_dir.join("extracted");
    format.extract(archive, &extracted)?;
    let binary = archive::find_file(&extracted, file_name)?;
    Ok(store::place_binary(layout, tool, tag, &binary, file_name)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    #[test]
    fn parses_tool_and_optional_version() {
        assert_eq!(parse_target("ckb-cli"), ("ckb-cli", None));
        assert_eq!(
            parse_target("ckb-cli@v2.0.0"),
            ("ckb-cli", Some("v2.0.0".to_string()))
        );
        assert_eq!(
            parse_target("ckb-cli@2.0.0"),
            ("ckb-cli", Some("v2.0.0".to_string()))
        );
    }

    fn tar_gz_with(path: &Path, name: &str, data: &[u8]) -> String {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, name, data).unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        fs::write(path, &bytes).unwrap();
        let mut hasher = HashingWriter::new(std::io::sink());
        hasher.write_all(&bytes).unwrap();
        hasher.finish().unwrap().1
    }

    fn setup() -> (tempfile::TempDir, Layout, PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::resolve(Some(dir.path().join("home").into()), None).unwrap();
        let archive = dir.path().join("tool_v1.0.0_x.tar.gz");
        let sha = tar_gz_with(&archive, "tool", b"binary");
        (dir, layout, archive, sha)
    }

    #[test]
    fn installs_when_checksums_match() {
        let (dir, layout, archive, sha) = setup();
        let expected = [ExpectedHash {
            source_name: "GitHub asset digest".into(),
            hex: sha.to_ascii_uppercase(),
        }];
        let placed = install_verified(
            &layout,
            "tool",
            "v1.0.0",
            &archive,
            ArchiveFormat::TarGz,
            &sha,
            &expected,
            "tool",
            dir.path(),
        )
        .unwrap();
        assert_eq!(placed, layout.toolchain_dir("tool", "v1.0.0").join("tool"));
        assert_eq!(fs::read(placed).unwrap(), b"binary");
    }

    #[test]
    fn tampered_download_fails_and_installs_nothing() {
        let (dir, layout, archive, published_sha) = setup();
        let tampered_sha = tar_gz_with(&archive, "tool", b"tampered");
        let expected = [ExpectedHash {
            source_name: "GitHub asset digest".into(),
            hex: published_sha,
        }];
        let err = install_verified(
            &layout,
            "tool",
            "v1.0.0",
            &archive,
            ArchiveFormat::TarGz,
            &tampered_sha,
            &expected,
            "tool",
            dir.path(),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("checksum mismatch for tool_v1.0.0_x.tar.gz")
        );
        assert!(!layout.home().join("toolchains").exists());
        assert!(!layout.bin_dir().exists());
        assert!(!dir.path().join("extracted").exists());
    }
}
