//! `update`, holds, the archive cache and `remove`'s cache line through the
//! real binary, each in a temporary `CKBUP_HOME`.
//!
//! Most tests use a fake home with dummy executables and no network; where a
//! test must show that no request is made, every request goes to a proxy on a
//! closed local port and so cannot succeed. The ignored ones talk to GitHub
//! and install real releases: `cargo test -- --ignored`

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The proxy variables ureq reads, in its order.
const PROXY_VARIABLES: [&str; 6] = [
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

fn command(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ckbup"));
    command.args(args).env("CKBUP_HOME", home);
    command
}

fn run_ckbup(home: &Path, args: &[&str]) -> Output {
    command(home, args).output().expect("failed to run ckbup")
}

/// Runs ckbup with every request sent to a proxy on a closed local port, so
/// any request it makes fails.
fn run_offline(home: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let mut command = command(home, args);
    for name in PROXY_VARIABLES
        .iter()
        .chain(&["NO_PROXY", "no_proxy", "GITHUB_TOKEN"])
    {
        command.env_remove(name);
    }
    command.env("HTTPS_PROXY", proxy);
    for (name, value) in extra_env {
        command.env(name, value);
    }
    command.output().expect("failed to run ckbup")
}

/// Expects success and returns standard output.
fn succeeded(args: &[&str], output: Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "ckbup {args:?} failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// Expects exit code `code` and returns standard error.
fn failed(args: &[&str], output: Output, code: i32) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(code),
        "ckbup {args:?}\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    stderr
}

fn ckbup(home: &Path, args: &[&str]) -> String {
    succeeded(args, run_ckbup(home, args))
}

fn exe(binary: &str) -> String {
    format!("{binary}{}", std::env::consts::EXE_SUFFIX)
}

fn version_dir(home: &Path, tool: &str, version: &str) -> PathBuf {
    home.join("toolchains").join(tool).join(version)
}

fn names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// A home with fnn v0.10.1 and v0.9.1 (active), and ckb-cli v2.0.0 (active)
/// in a folder that holds only its binary. Nothing is held.
fn fake_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    for version in ["v0.10.1", "v0.9.1"] {
        let dir = version_dir(root, "fnn", version);
        fs::create_dir_all(dir.join("config")).unwrap();
        for binary in ["fnn", "fnn-cli"] {
            fs::write(dir.join(exe(binary)), format!("{binary} {version}")).unwrap();
        }
    }
    let cli = version_dir(root, "ckb-cli", "v2.0.0");
    fs::create_dir_all(&cli).unwrap();
    fs::write(cli.join(exe("ckb-cli")), "ckb-cli v2.0.0").unwrap();

    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for binary in ["fnn", "fnn-cli"] {
        let source = version_dir(root, "fnn", "v0.9.1").join(exe(binary));
        fs::copy(source, bin.join(exe(binary))).unwrap();
    }
    fs::copy(cli.join(exe("ckb-cli")), bin.join(exe("ckb-cli"))).unwrap();
    fs::write(
        root.join("active.toml"),
        "ckb-cli = \"v2.0.0\"\nfnn = \"v0.9.1\"\n",
    )
    .unwrap();
    home
}

#[test]
fn update_of_a_tool_that_is_not_installed_fails_and_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let args = ["update", "fnn"];

    let err = failed(&args, run_offline(home.path(), &args, &[]), 1);

    assert_eq!(
        err,
        "error: fnn is not installed. Install it with: ckbup install fnn\n"
    );
    assert!(!home.path().join("downloads").exists());
    assert!(!home.path().join("cache").exists());
    assert!(names(home.path()).is_empty());
}

#[test]
fn update_of_an_empty_home_says_how_to_install_a_tool() {
    let home = tempfile::tempdir().unwrap();
    let args = ["update"];

    let out = succeeded(&args, run_offline(home.path(), &args, &[]));

    assert_eq!(
        out,
        "No tools are installed. Install one with: ckbup install <tool>\n"
    );
}

#[test]
fn use_holds_the_tool_and_list_shows_it() {
    let home = fake_home();

    let out = ckbup(home.path(), &["use", "fnn@v0.9.1"]);

    assert!(
        out.ends_with(
            "fnn is held at v0.9.1; ckbup update skips it until you run: ckbup update fnn\n"
        ),
        "{out}"
    );
    let active = fs::read_to_string(home.path().join("active.toml")).unwrap();
    assert_eq!(
        active,
        "ckb-cli = \"v2.0.0\"\nfnn = \"v0.9.1\"\nheld = [\"fnn\"]\n"
    );
    let list = ckbup(home.path(), &["list"]);
    assert!(
        list.contains("fnn               v0.10.1, v0.9.1 (active, held)\n"),
        "{list}"
    );
    assert!(
        list.contains("ckb-cli           v2.0.0 (active)\n"),
        "{list}"
    );
}

#[test]
fn update_skips_every_held_tool_without_a_request() {
    let home = fake_home();
    ckbup(home.path(), &["use", "fnn@v0.9.1"]);
    ckbup(home.path(), &["use", "ckb-cli@v2.0.0"]);
    let control = ["list", "--available", "fnn"];
    let err = failed(&control, run_offline(home.path(), &control, &[]), 1);
    assert!(
        err.starts_with("error: request to https://api.github.com/"),
        "a request through the closed proxy should fail: {err}"
    );
    let before = fs::read(home.path().join("active.toml")).unwrap();

    let args = ["update"];
    let out = succeeded(&args, run_offline(home.path(), &args, &[]));

    assert!(
        out.starts_with(
            "ckb-cli v2.0.0 is held; move it to the latest stable release with: ckbup update ckb-cli\n\
             fnn v0.9.1 is held; move it to the latest stable release with: ckbup update fnn\n"
        ),
        "{out}"
    );
    assert_eq!(fs::read(home.path().join("active.toml")).unwrap(), before);
}

#[test]
fn a_bad_keep_archives_value_fails_before_any_request() {
    let home = tempfile::tempdir().unwrap();
    for args in [["install", "fnn"], ["update", "fnn"]] {
        let output = run_offline(home.path(), &args, &[("CKBUP_KEEP_ARCHIVES", "2")]);

        let err = failed(&args, output, 1);

        assert_eq!(err, "error: CKBUP_KEEP_ARCHIVES must be 1 or 0\n");
    }
    assert!(names(home.path()).is_empty());
}

#[test]
fn remove_keeps_the_cached_archive_and_says_where_it_is() {
    let home = fake_home();
    let cache = home.path().join("cache").join("fnn").join("v0.10.1");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        cache.join("fnn_v0.10.1-x86_64-windows.tar.gz"),
        vec![0u8; 1_500_000],
    )
    .unwrap();

    let out = ckbup(home.path(), &["remove", "fnn@v0.10.1"]);

    assert_eq!(
        out,
        format!(
            "Removed fnn v0.10.1 ({})\nThe archive stays in the cache: {} (1.5 MB). Delete it by hand to free the space.\n",
            version_dir(home.path(), "fnn", "v0.10.1").display(),
            cache.display()
        )
    );
    assert_eq!(names(&cache), ["fnn_v0.10.1-x86_64-windows.tar.gz"]);
}

/// The version that `<tool> <version> is active: ...` names in `out`.
fn activated(out: &str, tool: &str) -> String {
    let prefix = format!("{tool} ");
    out.lines()
        .find_map(|line| {
            let rest = line.strip_prefix(&prefix)?;
            let (version, _) = rest.split_once(" is active: ")?;
            Some(version.to_string())
        })
        .unwrap_or_else(|| panic!("no activation of {tool} in:\n{out}"))
}

fn assert_nothing_cached_or_left(home: &Path) {
    assert!(!home.join("cache").exists(), "the cache was written");
    assert!(
        names(&home.join("downloads")).is_empty(),
        "download files were left: {:?}",
        names(&home.join("downloads"))
    );
}

/// ckb-cli v1.16.0 and v2.0.0 are stable and publish archives for every host
/// ckbup supports. The binaries are compared, never run.
#[test]
#[ignore = "downloads release assets from GitHub"]
fn holds_steer_update_and_remove_old_cleans_up() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    let bin_cli = root.join("bin").join(exe("ckb-cli"));
    let binary =
        |version: &str| fs::read(version_dir(root, "ckb-cli", version).join(exe("ckb-cli")));

    let out = ckbup(root, &["install", "ckb-cli@v1.16.0"]);
    assert!(
        out.contains(
            "ckb-cli is held at v1.16.0; ckbup update skips it until you run: ckbup update ckb-cli\n"
        ),
        "{out}"
    );
    assert_nothing_cached_or_left(root);

    let args = ["update"];
    let out = succeeded(&args, run_offline(root, &args, &[]));
    assert!(
        out.starts_with(
            "ckb-cli v1.16.0 is held; move it to the latest stable release with: ckbup update ckb-cli\n"
        ),
        "{out}"
    );

    let out = ckbup(root, &["update", "ckb-cli"]);
    let latest = activated(&out, "ckb-cli");
    assert_ne!(latest, "v1.16.0");
    assert!(
        out.contains("Verified SHA-256 against GitHub asset digest"),
        "{out}"
    );
    assert!(
        out.contains("ckb-cli now follows the latest stable release\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "ckb-cli v1.16.0 stays installed. Switch back with: ckbup use ckb-cli@v1.16.0. Remove it with: ckbup remove ckb-cli@v1.16.0\n"
        ),
        "{out}"
    );
    assert_eq!(fs::read(&bin_cli).unwrap(), binary(&latest).unwrap());
    assert!(binary("v1.16.0").is_ok());
    let active = fs::read_to_string(root.join("active.toml")).unwrap();
    assert_eq!(active, format!("ckb-cli = \"{latest}\"\n"));
    assert_nothing_cached_or_left(root);

    ckbup(root, &["use", "ckb-cli@v1.16.0"]);
    assert_eq!(fs::read(&bin_cli).unwrap(), binary("v1.16.0").unwrap());

    let out = ckbup(root, &["update", "ckb-cli", "--remove-old"]);
    assert!(!out.contains("Downloading"), "{out}");
    assert_eq!(activated(&out, "ckb-cli"), latest);
    assert!(out.contains("Removed ckb-cli v1.16.0 ("), "{out}");
    assert!(!out.contains("stays installed"), "{out}");
    assert_eq!(fs::read(&bin_cli).unwrap(), binary(&latest).unwrap());
    assert_eq!(
        names(&root.join("toolchains").join("ckb-cli")),
        [latest.as_str()]
    );

    let out = ckbup(root, &["update"]);
    assert!(
        out.starts_with(&format!("ckb-cli {latest} is up to date (latest stable)\n")),
        "{out}"
    );
}

#[test]
#[cfg_attr(
    all(target_os = "macos", target_arch = "x86_64"),
    ignore = "ckb-debugger publishes no archive for macOS x86_64"
)]
#[cfg_attr(
    not(all(target_os = "macos", target_arch = "x86_64")),
    ignore = "downloads release assets from GitHub"
)]
fn a_kept_archive_survives_remove_and_installs_again_without_a_download() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    let cache = root.join("cache").join("ckb-debugger").join("v1.1.2");

    let out = ckbup(root, &["install", "ckb-debugger@v1.1.2", "--keep-archives"]);
    assert!(out.contains("Kept the archive in "), "{out}");
    let kept = names(&cache);
    assert_eq!(kept.len(), 1, "{kept:?}");
    let archive = cache.join(&kept[0]);
    let bytes = fs::metadata(&archive).unwrap().len();
    assert!(names(&root.join("downloads")).is_empty());

    let out = ckbup(root, &["remove", "ckb-debugger@v1.1.2"]);
    assert!(
        out.ends_with(&format!(
            "The archive stays in the cache: {} ({:.1} MB). Delete it by hand to free the space.\n",
            cache.display(),
            bytes as f64 / 1_000_000.0
        )),
        "{out}"
    );
    assert!(archive.is_file());

    let out = ckbup(root, &["install", "ckb-debugger@v1.1.2"]);
    assert!(
        out.contains(&format!("Using cached {}\n", archive.display())),
        "{out}"
    );
    assert!(!out.contains("Downloading"), "{out}");
    assert!(
        out.contains("Verified SHA-256 against GitHub asset digest"),
        "{out}"
    );
    assert!(names(&root.join("downloads")).is_empty());
    assert!(archive.is_file());
    let version = Command::new(root.join("bin").join(exe("ckb-debugger")))
        .arg("--version")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&version.stdout).contains("1.1.2"));
}
