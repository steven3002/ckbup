//! The files of a release download in `<home>/downloads/`, the lock that keeps
//! two ckbup runs from writing them at the same time, and the sweep of old
//! leftovers.
//!
//! A lock file is deleted only by the process that holds its lock, while it
//! holds it. On Windows, a lock file that someone else deletes can be created
//! again and locked by a third process while the holder still holds the old one.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{Layout, StoreError};

/// Leftovers in `downloads/` older than this are swept.
const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const PART_SUFFIX: &str = ".part";
const RECORD_SUFFIX: &str = ".part.toml";
const LOCK_SUFFIX: &str = ".lock";
/// The prefix tempfile gives the work folder of each install.
const WORK_PREFIX: &str = ".tmp";

/// The files of the download of one release asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadFiles {
    pub asset: String,
    /// `<asset>.part`: the bytes received so far.
    pub part: PathBuf,
    /// `<asset>.part.toml`: which release asset the bytes belong to.
    pub record: PathBuf,
    /// `<asset>.lock`: locked while a run writes the other two.
    pub lock: PathBuf,
}

impl DownloadFiles {
    pub fn new(layout: &Layout, asset: &str) -> Self {
        Self::in_dir(&layout.downloads_dir(), asset)
    }

    fn in_dir(dir: &Path, asset: &str) -> Self {
        DownloadFiles {
            asset: asset.to_string(),
            part: dir.join(format!("{asset}{PART_SUFFIX}")),
            record: dir.join(format!("{asset}{RECORD_SUFFIX}")),
            lock: dir.join(format!("{asset}{LOCK_SUFFIX}")),
        }
    }
}

/// The held lock of one download. Dropping it deletes the lock file while the
/// lock is still held, then closes the file, which releases the lock.
#[derive(Debug)]
pub struct DownloadLock {
    path: PathBuf,
    _file: File,
}

impl DownloadLock {
    /// Takes the lock of `files`, creating `downloads/` and the lock file when
    /// needed. It never waits: while another process holds the lock, it fails.
    pub fn acquire(files: &DownloadFiles) -> Result<Self, StoreError> {
        if let Some(dir) = files.lock.parent() {
            fs::create_dir_all(dir).map_err(StoreError::io("cannot create", dir))?;
        }
        match try_lock(&files.lock) {
            Ok(Some(lock)) => Ok(lock),
            Ok(None) => Err(StoreError::DownloadBusy {
                asset: files.asset.clone(),
                lock: files.lock.clone(),
            }),
            Err(err) => Err(StoreError::io("cannot lock", &files.lock)(err)),
        }
    }
}

impl Drop for DownloadLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Opens or creates the lock file for writing, which the lock needs on every
/// OS, and tries its lock once. `None` means another handle holds it.
fn try_lock(path: &Path) -> io::Result<Option<DownloadLock>> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(DownloadLock {
            path: path.to_path_buf(),
            _file: file,
        })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(err)) => Err(err),
    }
}

/// Moves a completed download into `work_dir` as `<asset>`, deletes its
/// record, then releases `lock`.
pub fn take_download(
    files: &DownloadFiles,
    lock: DownloadLock,
    work_dir: &Path,
) -> Result<PathBuf, StoreError> {
    let archive = work_dir.join(&files.asset);
    fs::rename(&files.part, &archive)
        .map_err(StoreError::io("cannot move the download to", &archive))?;
    let _ = fs::remove_file(&files.record);
    drop(lock);
    Ok(archive)
}

/// Deletes what interrupted runs left in `downloads/` more than 7 days ago: a
/// download's `.part`, record and lock file, which go together, and the
/// `.tmp*` work folders of installs that were killed.
///
/// A download is deleted only after its lock is taken, so one that another
/// run is writing is skipped. Every failure is ignored.
pub fn sweep_downloads(layout: &Layout) {
    sweep_at(&layout.downloads_dir(), SystemTime::now());
}

fn sweep_at(dir: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut downloads: BTreeMap<String, Vec<Option<SystemTime>>> = BTreeMap::new();
    let mut work_dirs = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let modified = entry.metadata().and_then(|meta| meta.modified()).ok();
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_dir && name.starts_with(WORK_PREFIX) {
            work_dirs.push((entry.path(), modified));
        } else if let Some(asset) = asset_of(&name).filter(|_| !is_dir) {
            downloads
                .entry(asset.to_string())
                .or_default()
                .push(modified);
        }
    }

    for (asset, times) in downloads {
        let newest = times
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|times| times.into_iter().max());
        if !is_stale(newest, now) {
            continue;
        }
        let files = DownloadFiles::in_dir(dir, &asset);
        if let Ok(Some(lock)) = try_lock(&files.lock) {
            let _ = fs::remove_file(&files.part);
            let _ = fs::remove_file(&files.record);
            drop(lock);
        }
    }
    for (path, modified) in work_dirs {
        if is_stale(modified, now) {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// The asset a download file belongs to, from its name.
fn asset_of(name: &str) -> Option<&str> {
    [RECORD_SUFFIX, PART_SUFFIX, LOCK_SUFFIX]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .filter(|asset| !asset.is_empty())
}

/// Whether `modified` lies more than `STALE_AFTER` before `now`. An unknown or
/// future time is never stale.
fn is_stale(modified: Option<SystemTime>, now: SystemTime) -> bool {
    modified
        .and_then(|time| now.duration_since(time).ok())
        .is_some_and(|age| age > STALE_AFTER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake_home;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn files(dir: &Path, asset: &str) -> DownloadFiles {
        let files = DownloadFiles::in_dir(dir, asset);
        fs::create_dir_all(dir).unwrap();
        for path in [&files.part, &files.record, &files.lock] {
            fs::write(path, b"").unwrap();
        }
        files
    }

    fn set_modified(path: &Path, time: SystemTime) {
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn names_the_three_files_of_a_download() {
        let dir = tempfile::tempdir().unwrap();
        let layout = fake_home::layout(dir.path());
        let files = DownloadFiles::new(&layout, "fnn_v0.10.1-x86_64-windows.tar.gz");
        let downloads = layout.downloads_dir();
        assert_eq!(
            files.part,
            downloads.join("fnn_v0.10.1-x86_64-windows.tar.gz.part")
        );
        assert_eq!(
            files.record,
            downloads.join("fnn_v0.10.1-x86_64-windows.tar.gz.part.toml")
        );
        assert_eq!(
            files.lock,
            downloads.join("fnn_v0.10.1-x86_64-windows.tar.gz.lock")
        );
    }

    #[test]
    fn a_second_handle_in_the_same_process_is_refused_until_the_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let files = DownloadFiles::in_dir(&dir.path().join("downloads"), "ckb.zip");

        let first = DownloadLock::acquire(&files).unwrap();
        let err = DownloadLock::acquire(&files).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "another ckbup is downloading ckb.zip ({}); run the command again when it finishes",
                files.lock.display()
            )
        );

        drop(first);
        DownloadLock::acquire(&files).unwrap();
    }

    #[test]
    fn the_holder_can_delete_its_held_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let files = DownloadFiles::in_dir(dir.path(), "ckb.zip");
        let lock = DownloadLock::acquire(&files).unwrap();

        fs::remove_file(&files.lock).unwrap();

        assert!(!files.lock.exists());
        drop(lock);
        let again = DownloadLock::acquire(&files).unwrap();
        drop(again);
        assert!(!files.lock.exists());
    }

    #[test]
    fn taking_a_download_moves_it_and_leaves_no_record_or_lock() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        let files = files(&downloads, "ckb.zip");
        fs::write(&files.part, b"archive").unwrap();
        let work = downloads.join(".tmpWORK");
        fs::create_dir(&work).unwrap();
        fs::remove_file(&files.lock).unwrap();
        let lock = DownloadLock::acquire(&files).unwrap();

        let archive = take_download(&files, lock, &work).unwrap();

        assert_eq!(archive, work.join("ckb.zip"));
        assert_eq!(fs::read(&archive).unwrap(), b"archive");
        assert_eq!(fake_home::names(&downloads), [".tmpWORK"]);
    }

    #[test]
    fn the_sweep_deletes_old_downloads_and_work_folders_only() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        let now = SystemTime::now();
        files(&downloads, "old.tar.gz");
        let recent = files(&downloads, "recent.tar.gz");
        set_modified(&recent.part, now + 7 * DAY);
        fs::create_dir_all(downloads.join(".tmpOLD").join("extracted")).unwrap();
        fs::write(downloads.join("notes.txt"), b"not ckbup's").unwrap();

        sweep_at(&downloads, now + 8 * DAY);

        assert_eq!(
            fake_home::names(&downloads),
            [
                "notes.txt",
                "recent.tar.gz.lock",
                "recent.tar.gz.part",
                "recent.tar.gz.part.toml"
            ]
        );
    }

    #[test]
    fn the_sweep_skips_a_download_whose_lock_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        let held = files(&downloads, "held.tar.gz");
        let lock = DownloadLock::acquire(&held).unwrap();

        sweep_at(&downloads, SystemTime::now() + 8 * DAY);

        assert!(held.part.exists());
        assert!(held.record.exists());
        assert!(held.lock.exists());
        drop(lock);
    }

    #[test]
    fn the_sweep_of_a_missing_folder_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        sweep_at(&downloads, SystemTime::now());
        assert!(!downloads.exists());
    }

    #[test]
    fn only_the_names_of_downloads_have_an_asset() {
        assert_eq!(asset_of("ckb.zip.part"), Some("ckb.zip"));
        assert_eq!(asset_of("ckb.zip.part.toml"), Some("ckb.zip"));
        assert_eq!(asset_of("ckb.zip.lock"), Some("ckb.zip"));
        for name in ["ckb.zip", ".part", "notes.toml", ".tmpX"] {
            assert_eq!(asset_of(name), None, "{name}");
        }
    }
}
