//! Real installs from GitHub into a temporary `CKBUP_HOME`.
//!
//! These tests use the network and are ignored by default:
//! `cargo test -- --ignored`

use std::path::Path;
use std::process::Command;

fn ckbup(home: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ckbup"))
        .args(args)
        .env("CKBUP_HOME", home)
        .output()
        .expect("failed to run ckbup");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "ckbup {args:?} failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

fn run_installed(home: &Path, binary: &str) -> String {
    let file_name = format!("{binary}{}", std::env::consts::EXE_SUFFIX);
    let output = Command::new(home.join("bin").join(file_name))
        .arg("--version")
        .output()
        .expect("installed binary did not start");
    assert!(output.status.success(), "{binary} --version failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
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
    assert!(
        std::fs::read_dir(home.path().join("downloads"))
            .unwrap()
            .next()
            .is_none(),
        "temporary download files were left behind"
    );
}
