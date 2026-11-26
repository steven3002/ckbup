//! `list`, `use` and `remove` through the real binary, each in a temporary
//! `CKBUP_HOME`.
//!
//! Most tests use a fake home with dummy executables and no network. The
//! ignored ones talk to GitHub and install real releases:
//! `cargo test -- --ignored`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_ckbup(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ckbup"))
        .args(args)
        .env("CKBUP_HOME", home)
        .output()
        .expect("failed to run ckbup")
}

/// Runs ckbup, expects success and returns its standard output.
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

/// Runs ckbup, expects exit code `code` and returns its standard error.
fn ckbup_fails(home: &Path, args: &[&str], code: i32) -> String {
    let output = run_ckbup(home, args);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(code),
        "ckbup {args:?}\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    stderr
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
    sorted(
        entries
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect(),
    )
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

/// Every folder and file under `root` with the bytes of each file.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert(relative, None);
                walk(&path, root, out);
            } else {
                out.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// A home with fnn v0.10.1 (active) and v0.9.1, a `.partial` fnn folder from an
/// interrupted install, and ckb-cli v2.0.0 (active) in a folder that holds only
/// its binary.
fn fake_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    for version in ["v0.10.1", "v0.9.1", ".v0.9.2.partial"] {
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
        let source = version_dir(root, "fnn", "v0.10.1").join(exe(binary));
        fs::copy(source, bin.join(exe(binary))).unwrap();
    }
    fs::copy(cli.join(exe("ckb-cli")), bin.join(exe("ckb-cli"))).unwrap();
    fs::write(
        root.join("active.toml"),
        "ckb-cli = \"v2.0.0\"\nfnn = \"v0.10.1\"\n",
    )
    .unwrap();
    home
}

#[test]
fn list_shows_every_tool_and_skips_partial_folders() {
    let home = fake_home();
    let before = snapshot(home.path());

    let out = ckbup(home.path(), &["list"]);

    assert_eq!(
        out,
        "\
ckb               not installed
ckb-cli           v2.0.0 (active)
ckb-debugger      not installed
fnn               v0.10.1 (active), v0.9.1
ckb-light-client  not installed
"
    );
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn list_of_one_tool_shows_each_folder() {
    let home = fake_home();

    let out = ckbup(home.path(), &["list", "fnn"]);

    let new = version_dir(home.path(), "fnn", "v0.10.1");
    let old = version_dir(home.path(), "fnn", "v0.9.1");
    assert_eq!(
        out,
        format!(
            "v0.10.1  active  {}\nv0.9.1           {}\n",
            new.display(),
            old.display()
        )
    );
    assert_eq!(
        ckbup(home.path(), &["list", "ckb-debugger"]),
        "ckb-debugger: no versions installed. Install one with: ckbup install ckb-debugger\n"
    );
}

#[test]
fn use_switches_the_files_in_bin_and_the_active_version() {
    let home = fake_home();
    let bin = home.path().join("bin");

    let out = ckbup(home.path(), &["use", "fnn@v0.9.1"]);

    assert!(out.starts_with(&format!(
        "fnn v0.9.1 is active: {}, {}\n",
        bin.join(exe("fnn")).display(),
        bin.join(exe("fnn-cli")).display()
    )));
    assert_eq!(fs::read(bin.join(exe("fnn"))).unwrap(), b"fnn v0.9.1");
    assert_eq!(
        fs::read(bin.join(exe("fnn-cli"))).unwrap(),
        b"fnn-cli v0.9.1"
    );
    let active = fs::read_to_string(home.path().join("active.toml")).unwrap();
    assert!(active.contains("fnn = \"v0.9.1\""), "{active}");
    assert!(active.contains("ckb-cli = \"v2.0.0\""), "{active}");
    assert_eq!(
        names(&bin),
        sorted(vec![exe("ckb-cli"), exe("fnn"), exe("fnn-cli")])
    );

    ckbup(home.path(), &["use", "fnn@0.10.1"]);
    assert_eq!(fs::read(bin.join(exe("fnn"))).unwrap(), b"fnn v0.10.1");
}

#[test]
fn use_of_a_version_that_is_not_installed_changes_nothing() {
    let home = fake_home();
    let before = snapshot(home.path());

    let err = ckbup_fails(home.path(), &["use", "fnn@v0.9.0"], 1);

    assert_eq!(
        err,
        "error: fnn v0.9.0 is not installed. Installed: v0.10.1, v0.9.1. Install it with: ckbup install fnn@v0.9.0\n"
    );
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn use_of_the_active_version_only_holds_it() {
    let home = fake_home();
    let mut expected = snapshot(home.path());
    expected.insert(
        PathBuf::from("active.toml"),
        Some(b"ckb-cli = \"v2.0.0\"\nfnn = \"v0.10.1\"\nheld = [\"ckb-cli\"]\n".to_vec()),
    );

    let out = ckbup(home.path(), &["use", "ckb-cli@v2.0.0"]);

    assert_eq!(
        out,
        "ckb-cli v2.0.0 is already active\nckb-cli is held at v2.0.0; ckbup update skips it until you run: ckbup update ckb-cli\n"
    );
    assert_eq!(snapshot(home.path()), expected);
}

#[test]
fn use_and_remove_need_a_version() {
    let home = fake_home();
    let err = ckbup_fails(home.path(), &["use", "fnn"], 2);
    assert!(err.contains("ckbup use fnn@v0.10.1"), "{err}");
    let err = ckbup_fails(home.path(), &["remove", "fnn"], 2);
    assert!(err.contains("ckbup remove fnn@v0.9.1"), "{err}");
}

#[test]
fn remove_refuses_the_active_version_while_another_is_installed() {
    let home = fake_home();
    let before = snapshot(home.path());

    let err = ckbup_fails(home.path(), &["remove", "fnn@v0.10.1"], 1);

    assert_eq!(
        err,
        "error: fnn v0.10.1 is the active version. Switch first, for example: ckbup use fnn@v0.9.1\n"
    );
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn remove_deletes_a_version_that_is_not_active() {
    let home = fake_home();
    let bin = snapshot(&home.path().join("bin"));

    let out = ckbup(home.path(), &["remove", "fnn@v0.9.1"]);

    assert_eq!(
        out,
        format!(
            "Removed fnn v0.9.1 ({})\n",
            version_dir(home.path(), "fnn", "v0.9.1").display()
        )
    );
    assert_eq!(
        names(&home.path().join("toolchains").join("fnn")),
        [".v0.9.2.partial", "v0.10.1"]
    );
    assert_eq!(snapshot(&home.path().join("bin")), bin);
    assert!(!ckbup(home.path(), &["list", "fnn"]).contains("v0.9.1"));
}

#[test]
fn removing_the_last_version_deactivates_the_tool() {
    let home = fake_home();
    let bin = home.path().join("bin");

    let out = ckbup(home.path(), &["remove", "ckb-cli@v2.0.0"]);

    assert!(
        out.ends_with(&format!(
            "ckb-cli has no active version now; removed {}\n",
            bin.join(exe("ckb-cli")).display()
        )),
        "{out}"
    );
    assert!(names(&home.path().join("toolchains").join("ckb-cli")).is_empty());
    assert_eq!(names(&bin), sorted(vec![exe("fnn"), exe("fnn-cli")]));
    let active = fs::read_to_string(home.path().join("active.toml")).unwrap();
    assert_eq!(active, "fnn = \"v0.10.1\"\n");
}

#[test]
#[ignore = "lists releases on GitHub"]
fn list_available_marks_the_active_release_and_missing_checksums() {
    let home = tempfile::tempdir().unwrap();
    let dir = version_dir(home.path(), "ckb-light-client", "v0.5.4");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(exe("ckb-light-client")), "dummy").unwrap();
    fs::write(
        home.path().join("active.toml"),
        "ckb-light-client = \"v0.5.4\"\n",
    )
    .unwrap();

    let out = ckbup(home.path(), &["list", "--available", "ckb-light-client"]);

    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].starts_with(
            "ckb-light-client (nervosnetwork/ckb-light-client), newest stable releases for "
        ),
        "{out}"
    );
    assert_eq!(
        lines.last(),
        Some(&"Older releases: https://github.com/nervosnetwork/ckb-light-client/releases")
    );
    let releases = &lines[1..lines.len() - 1];
    assert!(!releases.is_empty() && releases.len() <= 10, "{out}");
    assert!(releases.iter().all(|line| !line.contains("-rc")), "{out}");
    let line = |tag: &str| {
        *releases
            .iter()
            .find(|line| line.split_whitespace().next() == Some(tag))
            .unwrap_or_else(|| panic!("{tag} is missing:\n{out}"))
    };
    assert!(line("v0.5.4").ends_with("  active"), "{out}");
    assert!(
        line("v0.4.1").ends_with("  no checksum (needs --allow-unverified)"),
        "{out}"
    );
}

#[test]
#[ignore = "downloads release assets from GitHub"]
fn switches_and_removes_real_ckb_cli_releases() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    let bin_cli = root.join("bin").join(exe("ckb-cli"));
    let binary = |version: &str| {
        fs::read(version_dir(root, "ckb-cli", version).join(exe("ckb-cli"))).unwrap()
    };

    ckbup(root, &["install", "ckb-cli@v2.0.0"]);
    let out = ckbup(root, &["install", "ckb-cli@v1.16.0"]);
    assert!(
        out.contains(
            "ckb-cli v2.0.0 remains the active version; v1.16.0 is installed alongside it. Switch with: ckbup use ckb-cli@v1.16.0"
        ),
        "{out}"
    );
    assert_eq!(fs::read(&bin_cli).unwrap(), binary("v2.0.0"));
    assert_ne!(binary("v1.16.0"), binary("v2.0.0"));

    let out = ckbup(root, &["use", "ckb-cli@v1.16.0"]);
    assert!(out.contains("ckb-cli v1.16.0 is active: "), "{out}");
    assert_eq!(fs::read(&bin_cli).unwrap(), binary("v1.16.0"));
    ckbup(root, &["use", "ckb-cli@v2.0.0"]);
    assert_eq!(fs::read(&bin_cli).unwrap(), binary("v2.0.0"));

    #[cfg(windows)]
    windows::switch_and_remove_while_running(root);
    #[cfg(not(windows))]
    {
        let out = ckbup(root, &["remove", "ckb-cli@v1.16.0"]);
        assert!(out.starts_with("Removed ckb-cli v1.16.0 ("), "{out}");
        assert!(!out.contains("still in use"), "{out}");
    }

    let out = ckbup(root, &["remove", "ckb-cli@v2.0.0"]);
    assert!(out.starts_with("Removed ckb-cli v2.0.0 ("), "{out}");
    assert!(out.contains("ckb-cli has no active version now"), "{out}");
    assert!(names(&root.join("toolchains").join("ckb-cli")).is_empty());
    assert!(names(&root.join("bin")).is_empty());
    let active = fs::read_to_string(root.join("active.toml")).unwrap();
    assert!(!active.contains("ckb-cli"), "{active}");
}

/// Windows refuses to delete a running executable, or a folder with a
/// process's working directory in it; these checks run ckb-cli while ckbup
/// switches and removes versions.
#[cfg(windows)]
mod windows {
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use super::{ckbup, ckbup_fails, snapshot, version_dir};

    /// ckb-cli started with no subcommand, which keeps it in its interactive
    /// mode while its stdin pipe stays open. It is killed when dropped, so a
    /// failed assertion never leaves it running.
    struct RunningCkbCli {
        child: Child,
        log: PathBuf,
    }

    impl RunningCkbCli {
        /// Starts `exe` in `cwd`, with its `CKB_CLI_HOME` and output log in
        /// `scratch`, and returns once its start-up banner has been printed.
        fn start(exe: &Path, cwd: &Path, scratch: &Path) -> Self {
            let cli_home = scratch.join("ckb-cli-home");
            fs::create_dir_all(&cli_home).unwrap();
            let log = scratch.join("output.log");
            let output = File::create(&log).unwrap();
            let child = Command::new(exe)
                .current_dir(cwd)
                .env("CKB_CLI_HOME", &cli_home)
                .stdin(Stdio::piped())
                .stdout(output.try_clone().unwrap())
                .stderr(output)
                .spawn()
                .unwrap();
            let mut running = RunningCkbCli { child, log };
            running.wait_for_banner();
            running
        }

        fn is_running(&mut self) -> bool {
            matches!(self.child.try_wait(), Ok(None))
        }

        /// Waits until ckb-cli has printed its banner and then nothing more
        /// for a moment, which it does once it waits on stdin.
        fn wait_for_banner(&mut self) {
            let started = Instant::now();
            let mut size = 0;
            let mut quiet_since = Instant::now();
            loop {
                assert!(
                    self.is_running(),
                    "ckb-cli exited during start-up:\n{}",
                    fs::read_to_string(&self.log).unwrap_or_default()
                );
                let now = fs::metadata(&self.log).map_or(0, |meta| meta.len());
                if now != size {
                    size = now;
                    quiet_since = Instant::now();
                } else if size > 0 && quiet_since.elapsed() >= Duration::from_millis(1500) {
                    return;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(60),
                    "ckb-cli printed no banner within 60 s"
                );
                sleep(Duration::from_millis(200));
            }
        }
    }

    impl Drop for RunningCkbCli {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Expects ckb-cli v2.0.0 (active) and v1.16.0 to be installed in `home`.
    /// Leaves v2.0.0 active, v1.16.0 removed, and leftovers that only the next
    /// ckbup run for ckb-cli can delete.
    pub fn switch_and_remove_while_running(home: &Path) {
        let scratch = tempfile::tempdir().unwrap();
        let neutral = scratch.path().join("cwd");
        fs::create_dir_all(&neutral).unwrap();
        let bin_cli = home.join("bin").join("ckb-cli.exe");
        let binary = |version: &str| {
            fs::read(version_dir(home, "ckb-cli", version).join("ckb-cli.exe")).unwrap()
        };

        let mut from_bin = RunningCkbCli::start(&bin_cli, &neutral, &scratch.path().join("bin"));
        ckbup(home, &["use", "ckb-cli@v1.16.0"]);
        assert!(from_bin.is_running());
        assert_eq!(fs::read(&bin_cli).unwrap(), binary("v1.16.0"));
        ckbup(home, &["use", "ckb-cli@v2.0.0"]);
        assert!(from_bin.is_running());
        assert_eq!(fs::read(&bin_cli).unwrap(), binary("v2.0.0"));

        let old = version_dir(home, "ckb-cli", "v1.16.0");
        let mut from_folder = RunningCkbCli::start(
            &old.join("ckb-cli.exe"),
            &neutral,
            &scratch.path().join("folder"),
        );
        let out = ckbup(home, &["remove", "ckb-cli@v1.16.0"]);
        assert!(out.starts_with("Removed ckb-cli v1.16.0 ("), "{out}");
        assert!(
            out.contains(
                ".v1.16.0.removing is still in use; it will be deleted the next time ckbup installs, switches or removes ckb-cli"
            ),
            "{out}"
        );
        assert!(from_folder.is_running());
        assert!(!old.exists());
        let listed = ckbup(home, &["list", "ckb-cli"]);
        assert!(!listed.contains("v1.16.0"), "{listed}");

        let current = version_dir(home, "ckb-cli", "v2.0.0");
        let in_folder = RunningCkbCli::start(&bin_cli, &current, &scratch.path().join("cwd-user"));
        let before = snapshot(home);
        let err = ckbup_fails(home, &["remove", "ckb-cli@v2.0.0"], 1);
        assert!(
            err.contains(&format!(
                "cannot remove ckb-cli v2.0.0: a program or terminal is using {}",
                current.display()
            )),
            "{err}"
        );
        assert_eq!(snapshot(home), before);

        drop(in_folder);
        drop(from_folder);
        drop(from_bin);
        // Give Windows a moment to release the files of the stopped processes.
        sleep(Duration::from_secs(1));
    }
}
