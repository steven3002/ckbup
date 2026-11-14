use std::fs;
use std::path::{Path, PathBuf};

use crate::archive;
use crate::cli::InstallArgs;
use crate::error::Error;
use crate::github::{self, Asset, GithubClient, Release, fetch_release};
use crate::integrity::{self, IntegrityError};
use crate::output::path_hint;
use crate::platform::Host;
use crate::store::{self, Layout, StoreError};
use crate::tools::{self, Candidate, ToolSpec, Variant};

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
    tools::ensure_supported(tool, host)?;

    let layout = Layout::from_env()?;
    let client = GithubClient::from_env();
    let release = fetch_release(&client, tool.owner, tool.repo, requested.as_deref())?;
    let tag = release.tag_name.as_str();
    let binary_file = host.executable_name(tool.binary);
    let executables = tool.executables(host);

    let version_dir = match store::installed_dir(&layout, tool.name, tag, &binary_file) {
        Some(dir) => {
            println!(
                "{} {tag} is already installed at {}",
                tool.name,
                dir.display()
            );
            dir
        }
        None => {
            let dir = install(&client, &layout, tool, host, &release, &executables, args)?;
            println!("Installed {} {tag} to {}", tool.name, dir.display());
            dir
        }
    };
    print_package_folders(&version_dir);

    match store::active_version(&layout, tool.name)? {
        Some(active) if active != tag => println!(
            "{} {active} remains the active version; {tag} is installed alongside it",
            tool.name
        ),
        _ => {
            store::activate(&layout, tool.name, tag, &version_dir, &executables)?;
            let on_path: Vec<String> = executables
                .iter()
                .map(|file| layout.bin_dir().join(file).display().to_string())
                .collect();
            println!("{} {tag} is active: {}", tool.name, on_path.join(", "));
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

/// Selects the release archive for `host`, then downloads, verifies and places it.
fn install(
    client: &GithubClient,
    layout: &Layout,
    tool: &ToolSpec,
    host: Host,
    release: &Release,
    executables: &[String],
    args: &InstallArgs,
) -> Result<PathBuf, Error> {
    let tag = release.tag_name.as_str();
    let published: Vec<&str> = release
        .assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect();
    let candidate = tools::select_asset(tool, tag, host, args.portable, &published)?;
    if args.portable && candidate.variant == Variant::Standard {
        println!(
            "{} {tag} has no portable build for {}/{}; using {}",
            tool.name,
            host.os.as_str(),
            host.arch.as_str(),
            candidate.archive
        );
    }
    let asset = release.require_asset(&candidate.archive)?;
    let expected = expected_hashes(
        client,
        release,
        tool,
        asset,
        &candidate,
        args.allow_unverified,
    )?;

    let downloads = layout.downloads_dir();
    fs::create_dir_all(&downloads).map_err(StoreError::io("cannot create", &downloads))?;
    let work =
        tempfile::tempdir_in(&downloads).map_err(StoreError::io("cannot create", &downloads))?;
    println!("Downloading {}", asset.name);
    let archive_path = work.path().join(&asset.name);
    github::download_to_file(client, &asset.browser_download_url, &archive_path)?;
    install_verified(
        layout,
        tool.name,
        tag,
        &candidate,
        &expected,
        executables,
        work.path(),
    )
}

/// Collects every published checksum for the archive: the GitHub asset digest
/// and, where the project publishes one, its own checksum file.
fn expected_hashes(
    client: &GithubClient,
    release: &Release,
    tool: &ToolSpec,
    archive: &Asset,
    candidate: &Candidate,
    allow_unverified: bool,
) -> Result<Vec<ExpectedHash>, Error> {
    let mut expected = Vec::new();
    if let Some(digest) = &archive.digest {
        expected.push(ExpectedHash {
            source_name: "GitHub asset digest".to_string(),
            hex: integrity::parse_api_digest(digest)?,
        });
    }
    if let Some(sums) = candidate
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
            tool: tool.name.to_string(),
            tag: release.tag_name.clone(),
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

/// Verifies the downloaded archive in `work_dir` and moves its package into the store.
///
/// Nothing is written outside `work_dir` until every checksum matches; the
/// caller's temporary `work_dir` (and the download in it) is removed on return.
fn install_verified(
    layout: &Layout,
    tool: &str,
    tag: &str,
    candidate: &Candidate,
    expected: &[ExpectedHash],
    executables: &[String],
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let archive = work_dir.join(&candidate.archive);
    let actual = integrity::sha256_file(&archive)?;
    for check in expected {
        integrity::ensure_match(&candidate.archive, &check.source_name, &check.hex, &actual)?;
        println!("Verified SHA-256 against {}", check.source_name);
    }

    let extracted = work_dir.join("extracted");
    candidate.format.extract(&archive, &extracted)?;
    let package = archive::package_root(&extracted, executables)?;
    Ok(store::place_package(
        layout,
        tool,
        tag,
        &package,
        executables,
    )?)
}

/// Points to the config templates and docs that ship in the release package.
fn print_package_folders(version_dir: &Path) {
    for (label, folder) in [("Config templates", "config"), ("Docs", "docs")] {
        let path = version_dir.join(folder);
        if path.is_dir() {
            println!("{label}: {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::ArchiveFormat;
    use flate2::Compression;
    use flate2::write::GzEncoder;

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

    /// Writes a `.tar.gz` holding `entries` and returns its SHA-256.
    fn tar_gz_with(path: &Path, entries: &[(&str, &[u8])]) -> String {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        fs::write(path, &bytes).unwrap();
        integrity::sha256_file(path).unwrap()
    }

    fn candidate(archive: &str) -> Candidate {
        Candidate {
            archive: archive.to_string(),
            format: ArchiveFormat::TarGz,
            sums_file: None,
            variant: Variant::Standard,
        }
    }

    fn digest(hex: String) -> [ExpectedHash; 1] {
        [ExpectedHash {
            source_name: "GitHub asset digest".into(),
            hex,
        }]
    }

    fn home(dir: &Path) -> Layout {
        Layout::resolve(Some(dir.join("home").into()), None).unwrap()
    }

    #[test]
    fn installs_a_flat_package_when_checksums_match() {
        let dir = tempfile::tempdir().unwrap();
        let layout = home(dir.path());
        let archive = candidate("tool_v1.0.0_x.tar.gz");
        let sha = tar_gz_with(
            &dir.path().join(&archive.archive),
            &[
                ("tool", b"binary"),
                ("tool-cli", b"cli"),
                ("config/testnet.toml", b""),
            ],
        );
        let executables = ["tool".to_string(), "tool-cli".to_string()];

        let placed = install_verified(
            &layout,
            "tool",
            "v1.0.0",
            &archive,
            &digest(sha.to_ascii_uppercase()),
            &executables,
            dir.path(),
        )
        .unwrap();

        assert_eq!(placed, layout.toolchain_dir("tool", "v1.0.0"));
        assert_eq!(fs::read(placed.join("tool")).unwrap(), b"binary");
        assert_eq!(fs::read(placed.join("tool-cli")).unwrap(), b"cli");
        assert!(placed.join("config").join("testnet.toml").is_file());
    }

    #[test]
    fn installs_a_package_from_its_top_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = home(dir.path());
        let archive = candidate("node_v1.0.0_x.tar.gz");
        let sha = tar_gz_with(
            &dir.path().join(&archive.archive),
            &[
                ("node_v1.0.0_x/node", b"binary"),
                ("node_v1.0.0_x/node-cli", b"bundled"),
                ("node_v1.0.0_x/docs/rpc.md", b""),
            ],
        );

        let placed = install_verified(
            &layout,
            "node",
            "v1.0.0",
            &archive,
            &digest(sha),
            &["node".to_string()],
            dir.path(),
        )
        .unwrap();

        assert_eq!(fs::read(placed.join("node")).unwrap(), b"binary");
        assert!(placed.join("node-cli").is_file());
        assert!(placed.join("docs").join("rpc.md").is_file());
    }

    #[test]
    fn tampered_download_fails_and_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = home(dir.path());
        let archive = candidate("tool_v1.0.0_x.tar.gz");
        let path = dir.path().join(&archive.archive);
        let published_sha = tar_gz_with(&path, &[("tool", b"binary")]);
        tar_gz_with(&path, &[("tool", b"tampered")]);

        let err = install_verified(
            &layout,
            "tool",
            "v1.0.0",
            &archive,
            &digest(published_sha),
            &["tool".to_string()],
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
