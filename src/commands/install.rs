use std::fs;
use std::path::{Path, PathBuf};

use crate::archive;
use crate::cli::{InstallArgs, parse_target};
use crate::error::Error;
use crate::github::{self, Asset, GithubClient, Release};
use crate::integrity::{self, IntegrityError};
use crate::output::{activation_report, hold_report, path_hint};
use crate::platform::Host;
use crate::releases;
use crate::store::{self, DownloadFiles, DownloadLock, Hold, InstallRecord, Layout, StoreError};
use crate::tools::{self, Candidate, ToolSpec, Variant};

/// A SHA-256 the download must match, and where it was published.
#[derive(Debug)]
pub struct ExpectedHash {
    pub source_name: String,
    pub hex: String,
}

/// The release archive an install needs, and the checksums it must match.
struct Wanted<'a> {
    tool: &'a str,
    tag: &'a str,
    asset: &'a Asset,
    expected: &'a [ExpectedHash],
}

/// How `install_release` picks, checks and keeps a release archive.
#[derive(Debug, Clone, Copy)]
pub struct InstallOptions {
    /// Try the portable build first.
    pub prefer_portable: bool,
    /// Install an archive that has no published checksum.
    pub allow_unverified: bool,
    /// Keep the archive in the cache once it matched a published checksum.
    pub keep_archives: bool,
}

pub fn run(args: &InstallArgs) -> Result<(), Error> {
    let keep_archives = store::keep_archives(args.keep_archives)?;
    let (tool_name, requested) = parse_target(&args.target);
    let tool = tools::find_tool(tool_name)?;
    let host = Host::detect();
    // Fail on unsupported hosts before spending an API request.
    tools::ensure_supported(tool, host)?;

    let layout = Layout::from_env()?;
    store::sweep_downloads(&layout);
    let executables = tool.executables(host);
    store::sweep(&layout, tool.name, &executables);

    let client = GithubClient::from_env();
    let release = match requested.as_deref() {
        Some(tag) => releases::release(&client, tool, tag)?,
        None => releases::latest_stable(&client, tool)?,
    };
    let tag = release.tag_name.as_str();
    let binary_file = host.executable_name(tool.binary);
    let options = InstallOptions {
        prefer_portable: args.portable,
        allow_unverified: args.allow_unverified,
        keep_archives,
    };

    let version_dir = match store::installed_dir(&layout, tool.name, tag, &binary_file) {
        Some(dir) => {
            println!(
                "{} {tag} is already installed at {}",
                tool.name,
                dir.display()
            );
            print_package_folders(&dir);
            dir
        }
        None => install_release(&client, &layout, tool, host, &release, options)?,
    };

    match store::active_version(&layout, tool.name)? {
        Some(active) if active != tag => println!(
            "{0} {active} remains the active version; {tag} is installed alongside it. Switch with: ckbup use {0}@{tag}",
            tool.name
        ),
        _ => {
            let hold = hold_on_activation(requested.as_deref());
            let activation =
                store::activate(&layout, tool.name, tag, &version_dir, &executables, hold)?;
            print!(
                "{}",
                activation_report(tool.name, tag, &activation.on_path, &activation.removed)
            );
            print!("{}", hold_report(tool.name, tag, hold, activation.was_held));
        }
    }

    if let Some(hint) = path_hint(&layout.bin_dir(), host.os) {
        println!("\n{hint}");
    }
    Ok(())
}

/// What an install that activates its version does to the tool's hold: a
/// version named on the command line holds the tool there, and installing
/// the latest stable release lets `update` move it again.
fn hold_on_activation(requested: Option<&str>) -> Hold {
    match requested {
        Some(_) => Hold::Set,
        None => Hold::Release,
    }
}

/// Installs `release` of `tool` for `host`, which must not be installed yet,
/// and returns its version folder.
///
/// It picks the release's archive for the host, takes a cached copy that
/// matches the published checksums or downloads it (resuming a download that
/// stopped), verifies it, and moves the extracted package into place with its
/// install record.
pub fn install_release(
    client: &GithubClient,
    layout: &Layout,
    tool: &ToolSpec,
    host: Host,
    release: &Release,
    options: InstallOptions,
) -> Result<PathBuf, Error> {
    let tag = release.tag_name.as_str();
    let candidate = select_archive(tool, host, release, options.prefer_portable)?;
    let asset = release.require_asset(&candidate.archive)?;
    let expected = expected_hashes(
        client,
        release,
        tool,
        asset,
        &candidate,
        options.allow_unverified,
    )?;

    let downloads = layout.downloads_dir();
    fs::create_dir_all(&downloads).map_err(StoreError::io("cannot create", &downloads))?;
    let work =
        tempfile::tempdir_in(&downloads).map_err(StoreError::io("cannot create", &downloads))?;
    let wanted = Wanted {
        tool: tool.name,
        tag,
        asset,
        expected: &expected,
    };
    let archive = obtain_archive(client, layout, &wanted, options.keep_archives, work.path())?;
    let version_dir = unpack_and_place(
        layout,
        tool.name,
        tag,
        &candidate,
        &archive,
        &tool.executables(host),
        work.path(),
    )?;
    println!("Installed {} {tag} to {}", tool.name, version_dir.display());
    print_package_folders(&version_dir);
    Ok(version_dir)
}

/// The release's archive for `host`, portable first when `prefer_portable`.
fn select_archive(
    tool: &ToolSpec,
    host: Host,
    release: &Release,
    prefer_portable: bool,
) -> Result<Candidate, Error> {
    let tag = release.tag_name.as_str();
    let published: Vec<&str> = release
        .assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect();
    let candidate = tools::select_asset(tool, tag, host, prefer_portable, &published)?;
    if prefer_portable && candidate.variant == Variant::Standard {
        println!(
            "{} {tag} has no portable build for {}/{}; using {}",
            tool.name,
            host.os.as_str(),
            host.arch.as_str(),
            candidate.archive
        );
    }
    Ok(candidate)
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

/// The verified archive: a cached copy that matches the expected checksums,
/// or a download moved into `work_dir`. With `keep` on, a download that matched
/// a published checksum moves on into the cache.
fn obtain_archive(
    client: &GithubClient,
    layout: &Layout,
    wanted: &Wanted,
    keep: bool,
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let Wanted {
        tool,
        tag,
        asset,
        expected,
    } = *wanted;
    // An archive without a published checksum is never taken from the cache.
    if !expected.is_empty()
        && let Some(cached) = store::cached_archive(layout, tool, tag, &asset.name)
    {
        let actual = integrity::sha256_file(&cached)?;
        if expected
            .iter()
            .all(|check| check.hex.eq_ignore_ascii_case(&actual))
        {
            println!("Using cached {}", cached.display());
            report_verified(expected);
            return Ok(cached);
        }
        store::discard_cached(&cached);
        println!(
            "{} does not match its checksum; downloading again",
            cached.display()
        );
    }

    let archive = fetch_archive(client, layout, asset, work_dir)?;
    verify(&archive, &asset.name, expected)?;
    if keep && !expected.is_empty() {
        let kept = store::keep_archive(layout, tool, tag, &archive)?;
        println!("Kept the archive in {}", kept.display());
        return Ok(kept);
    }
    Ok(archive)
}

/// Downloads `asset` under its lock, resuming a download that stopped, and
/// moves the complete file into `work_dir`.
fn fetch_archive(
    client: &GithubClient,
    layout: &Layout,
    asset: &Asset,
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let files = DownloadFiles::new(layout, &asset.name);
    let lock = DownloadLock::acquire(&files)?;
    github::download_asset(client, asset, &files.part, &files.record, &mut |offset| {
        if offset == 0 {
            println!("Downloading {}", asset.name);
        } else {
            println!(
                "Resuming {} at {offset} of {} bytes",
                asset.name, asset.size
            );
        }
    })?;
    Ok(store::take_download(&files, lock, work_dir)?)
}

/// Checks the SHA-256 of `archive` against every expected hash.
fn verify(archive: &Path, name: &str, expected: &[ExpectedHash]) -> Result<(), IntegrityError> {
    let actual = integrity::sha256_file(archive)?;
    for check in expected {
        integrity::ensure_match(name, &check.source_name, &check.hex, &actual)?;
    }
    report_verified(expected);
    Ok(())
}

fn report_verified(expected: &[ExpectedHash]) {
    for check in expected {
        println!("Verified SHA-256 against {}", check.source_name);
    }
}

/// Extracts the verified `archive` into `work_dir` and moves its package into
/// the store with the install record of `candidate`.
fn unpack_and_place(
    layout: &Layout,
    tool: &str,
    tag: &str,
    candidate: &Candidate,
    archive: &Path,
    executables: &[String],
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let extracted = work_dir.join("extracted");
    candidate.format.extract(archive, &extracted)?;
    let package = archive::package_root(&extracted, executables)?;
    let record = InstallRecord {
        archive: candidate.archive.clone(),
        variant: candidate.variant,
    };
    Ok(store::place_package(
        layout,
        tool,
        tag,
        &package,
        executables,
        &record,
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
    use crate::github::test_server::{Reply, TestServer};
    use crate::store::fake_home;
    use flate2::Compression;
    use flate2::write::GzEncoder;

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

    /// Checks and places the archive `archive` in `work_dir`, as an install does.
    fn verify_and_place(
        layout: &Layout,
        tool: &str,
        archive: &Candidate,
        expected: &[ExpectedHash],
        executables: &[String],
        work_dir: &Path,
    ) -> Result<PathBuf, Error> {
        let path = work_dir.join(&archive.archive);
        verify(&path, &archive.archive, expected)?;
        unpack_and_place(
            layout,
            tool,
            "v1.0.0",
            archive,
            &path,
            executables,
            work_dir,
        )
    }

    #[test]
    fn a_named_version_holds_the_tool_and_the_latest_releases_it() {
        assert_eq!(hold_on_activation(Some("v0.9.1")), Hold::Set);
        assert_eq!(hold_on_activation(None), Hold::Release);
    }

    #[test]
    fn installs_a_flat_package_when_checksums_match() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
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

        let placed = verify_and_place(
            &layout,
            "tool",
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
        assert_eq!(
            InstallRecord::read_from(&placed),
            Some(InstallRecord {
                archive: "tool_v1.0.0_x.tar.gz".to_string(),
                variant: Variant::Standard,
            })
        );
    }

    #[test]
    fn installs_a_package_from_its_top_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let archive = candidate("node_v1.0.0_x.tar.gz");
        let sha = tar_gz_with(
            &dir.path().join(&archive.archive),
            &[
                ("node_v1.0.0_x/node", b"binary"),
                ("node_v1.0.0_x/node-cli", b"bundled"),
                ("node_v1.0.0_x/docs/rpc.md", b""),
            ],
        );

        let placed = verify_and_place(
            &layout,
            "node",
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
        let layout = fake_home::layout(dir.path());
        let archive = candidate("tool_v1.0.0_x.tar.gz");
        let path = dir.path().join(&archive.archive);
        let published_sha = tar_gz_with(&path, &[("tool", b"binary")]);
        tar_gz_with(&path, &[("tool", b"tampered")]);

        let err = verify_and_place(
            &layout,
            "tool",
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

    /// The bytes of a release archive served by the test server, and their SHA-256.
    fn served_archive() -> (Vec<u8>, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");
        let sha = tar_gz_with(&path, &[("tool", b"binary")]);
        (fs::read(&path).unwrap(), sha)
    }

    fn asset_at(server: &TestServer, bytes: &[u8]) -> Asset {
        Asset {
            id: 11,
            name: "tool_v1.0.0_x.tar.gz".to_string(),
            size: bytes.len() as u64,
            browser_download_url: server.url("/tool_v1.0.0_x.tar.gz"),
            digest: None,
        }
    }

    /// Runs `obtain_archive` for `tool` v1.0.0 with a new work folder.
    fn obtain(
        layout: &Layout,
        asset: &Asset,
        expected: &[ExpectedHash],
        keep: bool,
    ) -> (Result<PathBuf, Error>, tempfile::TempDir) {
        let downloads = layout.downloads_dir();
        fs::create_dir_all(&downloads).unwrap();
        let work = tempfile::tempdir_in(&downloads).unwrap();
        let wanted = Wanted {
            tool: "tool",
            tag: "v1.0.0",
            asset,
            expected,
        };
        let client = GithubClient::for_test_server();
        let result = obtain_archive(&client, layout, &wanted, keep, work.path());
        (result, work)
    }

    fn cache_with(layout: &Layout, bytes: &[u8]) -> PathBuf {
        let folder = layout.cache_dir("tool", "v1.0.0");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("tool_v1.0.0_x.tar.gz");
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_cached_archive_that_matches_is_used_with_the_switch_off() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let (bytes, sha) = served_archive();
        let server = TestServer::start(vec![]);
        let cached = cache_with(&layout, &bytes);

        let (result, _work) = obtain(&layout, &asset_at(&server, &bytes), &digest(sha), false);

        assert_eq!(result.unwrap(), cached);
        assert!(server.requests().is_empty());
        assert_eq!(fs::read(&cached).unwrap(), bytes);
    }

    #[test]
    fn a_cached_archive_that_does_not_match_is_deleted_and_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let (bytes, sha) = served_archive();
        let server = TestServer::start(vec![Reply::full(&bytes, &[])]);
        let cached = cache_with(&layout, b"damaged copy");

        let (result, work) = obtain(&layout, &asset_at(&server, &bytes), &digest(sha), false);

        let archive = result.unwrap();
        assert_eq!(archive, work.path().join("tool_v1.0.0_x.tar.gz"));
        assert_eq!(fs::read(&archive).unwrap(), bytes);
        assert!(!cached.exists());
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn the_switch_keeps_a_verified_archive_and_never_an_unverified_one() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let (bytes, sha) = served_archive();
        let server = TestServer::start(vec![Reply::full(&bytes, &[]), Reply::full(&bytes, &[])]);
        let asset = asset_at(&server, &bytes);

        let (kept, _work) = obtain(&layout, &asset, &digest(sha), true);
        let kept = kept.unwrap();
        assert_eq!(
            kept,
            layout
                .cache_dir("tool", "v1.0.0")
                .join("tool_v1.0.0_x.tar.gz")
        );
        assert_eq!(fs::read(&kept).unwrap(), bytes);

        fs::remove_dir_all(layout.home().join("cache")).unwrap();
        let (unverified, work) = obtain(&layout, &asset, &[], true);
        assert_eq!(
            unverified.unwrap(),
            work.path().join("tool_v1.0.0_x.tar.gz")
        );
        assert!(!layout.home().join("cache").exists());
        assert_eq!(fake_home::names(&layout.downloads_dir()).len(), 2);
    }

    #[test]
    fn a_download_leaves_no_part_record_or_lock() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let (bytes, sha) = served_archive();
        let server = TestServer::start(vec![Reply::full(&bytes, &[])]);

        let (result, work) = obtain(&layout, &asset_at(&server, &bytes), &digest(sha), false);

        result.unwrap();
        let work_name = work.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(fake_home::names(&layout.downloads_dir()), [work_name]);
    }

    #[test]
    fn a_held_download_lock_stops_the_download_before_it_writes() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let (bytes, _) = served_archive();
        let server = TestServer::start(vec![Reply::full(&bytes, &[])]);
        let asset = asset_at(&server, &bytes);
        let files = DownloadFiles::new(&layout, &asset.name);
        let held = DownloadLock::acquire(&files).unwrap();
        let work = dir.path().join("work");
        fs::create_dir_all(&work).unwrap();

        let err =
            fetch_archive(&GithubClient::for_test_server(), &layout, &asset, &work).unwrap_err();

        assert_eq!(
            err.to_string(),
            format!(
                "another ckbup is downloading tool_v1.0.0_x.tar.gz ({}); run the command again when it finishes",
                files.lock.display()
            )
        );
        assert!(!files.part.exists());
        assert!(!files.record.exists());
        assert!(server.requests().is_empty());
        drop(held);
    }
}
