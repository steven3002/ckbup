//! Real installs from GitHub into a temporary `CKBUP_HOME`.
//!
//! These tests use the network and are ignored by default:
//! `cargo test -- --ignored`

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_ckbup(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ckbup"))
        .args(args)
        .env("CKBUP_HOME", home)
        .output()
        .expect("failed to run ckbup")
}

fn ckbup(home: &Path, args: &[&str]) -> String {
    let output = run_ckbup(home, args);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "ckbup {args:?} failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

fn exe(binary: &str) -> String {
    format!("{binary}{}", std::env::consts::EXE_SUFFIX)
}

fn version_dir(home: &Path, tool: &str, version: &str) -> PathBuf {
    home.join("toolchains").join(tool).join(version)
}

/// Runs `<home>/bin/<binary> --version` and returns its standard output.
fn run_installed(home: &Path, binary: &str) -> String {
    let output = Command::new(home.join("bin").join(exe(binary)))
        .arg("--version")
        .output()
        .expect("installed binary did not start");
    assert!(output.status.success(), "{binary} --version failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_no_downloads_left(home: &Path) {
    assert!(
        std::fs::read_dir(home.join("downloads"))
            .unwrap()
            .next()
            .is_none(),
        "temporary download files were left behind"
    );
}

#[test]
#[ignore = "downloads release assets from GitHub"]
fn installs_ckb_debugger_and_ckb_cli() {
    let home = tempfile::tempdir().unwrap();

    let out = ckbup(home.path(), &["install", "ckb-debugger@v1.1.1"]);
    assert!(out.contains("Verified SHA-256 against GitHub asset digest"));
    assert!(out.contains("-sha256.txt"));
    assert!(run_installed(home.path(), "ckb-debugger").contains("1.1.1"));

    let out = ckbup(home.path(), &["install", "ckb-cli@v2.0.0"]);
    assert!(out.contains("Verified SHA-256 against GitHub asset digest"));
    assert!(run_installed(home.path(), "ckb-cli").contains("2.0.0"));

    let active = std::fs::read_to_string(home.path().join("active.toml")).unwrap();
    assert!(active.contains("ckb-debugger = \"v1.1.1\""));
    assert!(active.contains("ckb-cli = \"v2.0.0\""));
    assert_no_downloads_left(home.path());
}

#[test]
#[ignore = "downloads release assets from GitHub"]
fn installs_ckb_with_its_docs() {
    let home = tempfile::tempdir().unwrap();

    let out = ckbup(home.path(), &["install", "ckb@v0.210.0"]);
    assert!(out.contains("Verified SHA-256 against GitHub asset digest"));
    assert!(run_installed(home.path(), "ckb").contains("ckb 0.210.0"));

    let package = version_dir(home.path(), "ckb", "v0.210.0");
    assert!(package.join("docs").is_dir());
    assert!(out.contains("Docs: "));
    assert!(package.join(exe("ckb-cli")).is_file());
    assert!(!home.path().join("bin").join(exe("ckb-cli")).exists());
    assert_no_downloads_left(home.path());
}

#[test]
#[ignore = "downloads release assets from GitHub"]
fn installs_fnn_with_fnn_cli_and_config_templates() {
    let home = tempfile::tempdir().unwrap();

    let out = ckbup(home.path(), &["install", "fnn@v0.9.1"]);
    assert!(out.contains("Verified SHA-256 against GitHub asset digest"));
    assert!(run_installed(home.path(), "fnn").contains("v0.9.1"));
    assert!(run_installed(home.path(), "fnn-cli").contains("fnn-cli 0.9.1"));

    let package = version_dir(home.path(), "fnn", "v0.9.1");
    assert!(
        package
            .join("config")
            .join("testnet")
            .join("config.yml")
            .is_file()
    );
    assert!(out.contains("Config templates: "));
    assert_no_downloads_left(home.path());
}

#[test]
#[ignore = "downloads release assets from GitHub"]
fn installs_ckb_light_client_with_config_templates() {
    let home = tempfile::tempdir().unwrap();

    let out = ckbup(home.path(), &["install", "ckb-light-client@v0.5.4"]);
    assert!(out.contains("Verified SHA-256 against GitHub asset digest"));
    assert!(run_installed(home.path(), "ckb-light-client").contains("CKB Light Client 0.5.4"));

    let package = version_dir(home.path(), "ckb-light-client", "v0.5.4");
    assert!(package.join("config").join("testnet.toml").is_file());
    assert_no_downloads_left(home.path());
}

#[test]
#[ignore = "downloads release assets from GitHub"]
fn release_without_checksums_needs_allow_unverified() {
    let home = tempfile::tempdir().unwrap();

    let refused = run_ckbup(home.path(), &["install", "ckb-light-client@v0.4.1"]);
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("GitHub lists no SHA-256"), "{stderr}");
    assert!(stderr.contains("--allow-unverified"), "{stderr}");
    let toolchains = home.path().join("toolchains");
    assert!(!toolchains.exists() || std::fs::read_dir(&toolchains).unwrap().next().is_none());

    let allowed = run_ckbup(
        home.path(),
        &["install", "ckb-light-client@v0.4.1", "--allow-unverified"],
    );
    let stderr = String::from_utf8_lossy(&allowed.stderr);
    assert!(allowed.status.success(), "{stderr}");
    assert!(stderr.contains("without checksum verification"), "{stderr}");
    assert!(run_installed(home.path(), "ckb-light-client").contains("0.4.1"));
}
