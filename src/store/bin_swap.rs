//! Changes to the files in `bin/` that never overwrite or delete a file there in
//! place. Windows refuses both while the file runs, but allows renaming it, so a
//! file in place is moved aside first and deleted only once the change is done.

use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{StoreError, leftover};

/// A new copy written into `bin/` under a staging name, waiting to take the
/// place of `file`.
#[derive(Debug)]
pub(super) struct Staged {
    file: String,
    path: PathBuf,
}

/// Copies each `(source, file)` pair into `bin_dir` under a staging name.
///
/// If a copy fails, the copies already made are deleted, so `bin_dir` is left
/// as it was.
pub(super) fn stage(
    bin_dir: &Path,
    copies: &[(PathBuf, String)],
) -> Result<Vec<Staged>, StoreError> {
    let mut staged = Vec::with_capacity(copies.len());
    for (source, file) in copies {
        let path = staging_path(bin_dir, file);
        if let Err(err) = fs::copy(source, &path) {
            let _ = fs::remove_file(&path);
            discard(&staged);
            return Err(StoreError::io("cannot copy", source)(err));
        }
        staged.push(Staged {
            file: file.clone(),
            path,
        });
    }
    Ok(staged)
}

/// Puts each staged copy in place of its file and takes `removals` out of
/// `bin_dir`. A file already in place is moved aside first.
///
/// If a step fails, the steps already taken are undone and the remaining
/// staged copies are deleted.
pub(super) fn apply(
    bin_dir: &Path,
    staged: &[Staged],
    removals: &[String],
) -> Result<Swap, StoreError> {
    let mut swap = Swap {
        bin_dir: bin_dir.to_path_buf(),
        changes: Vec::new(),
    };
    for (index, next) in staged.iter().enumerate() {
        if let Err(err) = swap.put_in_place(next) {
            discard(&staged[index..]);
            swap.undo();
            return Err(err);
        }
    }
    for file in removals {
        if let Err(err) = swap.take_out(file) {
            swap.undo();
            return Err(err);
        }
    }
    Ok(swap)
}

/// The files in `bin/` that `apply` replaced or took out. The files that were
/// in place stay under moved-aside names until `finish` or `undo`.
#[derive(Debug)]
pub(super) struct Swap {
    bin_dir: PathBuf,
    changes: Vec<Change>,
}

#[derive(Debug)]
struct Change {
    file: String,
    /// Where the file that was in place now is.
    previous: Option<PathBuf>,
    /// Whether a staged copy now sits under `file`.
    replaced: bool,
}

impl Swap {
    fn put_in_place(&mut self, staged: &Staged) -> Result<(), StoreError> {
        let previous = move_aside(&self.bin_dir, &staged.file)?;
        self.changes.push(Change {
            file: staged.file.clone(),
            previous,
            replaced: false,
        });
        let target = self.bin_dir.join(&staged.file);
        fs::rename(&staged.path, &target).map_err(StoreError::io("cannot write", &target))?;
        if let Some(change) = self.changes.last_mut() {
            change.replaced = true;
        }
        Ok(())
    }

    fn take_out(&mut self, file: &str) -> Result<(), StoreError> {
        let previous = move_aside(&self.bin_dir, file)?;
        self.changes.push(Change {
            file: file.to_string(),
            previous,
            replaced: false,
        });
        Ok(())
    }

    /// Tries to delete the files that were in place. One that is still running
    /// stays under its moved-aside name until a later sweep.
    pub(super) fn finish(self) {
        for previous in self
            .changes
            .into_iter()
            .filter_map(|change| change.previous)
        {
            let _ = fs::remove_file(previous);
        }
    }

    /// Puts the files that were in place back under their names, as far as
    /// possible.
    pub(super) fn undo(self) {
        for change in self.changes.into_iter().rev() {
            if change.replaced {
                remove_new_copy(&self.bin_dir, &change.file);
            }
            if let Some(previous) = change.previous {
                let _ = fs::rename(previous, self.bin_dir.join(&change.file));
            }
        }
    }
}

/// Moves `bin_dir/file` to a free moved-aside name if it exists, and returns
/// that name.
fn move_aside(bin_dir: &Path, file: &str) -> Result<Option<PathBuf>, StoreError> {
    let current = bin_dir.join(file);
    match fs::symlink_metadata(&current) {
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(StoreError::io("cannot read", &current)(err)),
    }
    let aside = leftover::moved_aside_path(bin_dir, file);
    fs::rename(&current, &aside).map_err(StoreError::io("cannot move aside", &current))?;
    Ok(Some(aside))
}

/// Takes a copy that `apply` put in place out again, without deleting it in place.
fn remove_new_copy(bin_dir: &Path, file: &str) {
    if let Ok(Some(copy)) = move_aside(bin_dir, file) {
        let _ = fs::remove_file(copy);
    }
}

/// `<stem>.new`; a copy left there by an interrupted run is overwritten.
fn staging_path(bin_dir: &Path, file: &str) -> PathBuf {
    let stem = Path::new(file)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(file);
    bin_dir.join(format!("{stem}.new"))
}

fn discard(staged: &[Staged]) {
    for staged in staged {
        let _ = fs::remove_file(&staged.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake_home;

    fn setup(dir: &Path) -> (PathBuf, PathBuf) {
        let bin = dir.join("bin");
        let source = dir.join("source");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&source).unwrap();
        (bin, source)
    }

    fn copies(source: &Path, files: &[&str]) -> Vec<(PathBuf, String)> {
        files
            .iter()
            .map(|file| {
                fs::write(source.join(file), format!("new {file}")).unwrap();
                (source.join(file), file.to_string())
            })
            .collect()
    }

    #[test]
    fn replaces_files_and_keeps_the_previous_ones_until_finished() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, source) = setup(dir.path());
        fs::write(bin.join("fnn.exe"), b"old fnn").unwrap();

        let staged = stage(&bin, &copies(&source, &["fnn.exe", "fnn-cli.exe"])).unwrap();
        assert_eq!(
            fake_home::names(&bin),
            ["fnn-cli.new", "fnn.exe", "fnn.new"]
        );
        let swap = apply(&bin, &staged, &[]).unwrap();
        assert_eq!(fs::read(bin.join("fnn.exe")).unwrap(), b"new fnn.exe");
        assert_eq!(fs::read(bin.join("fnn.old")).unwrap(), b"old fnn");

        swap.finish();
        assert_eq!(fake_home::names(&bin), ["fnn-cli.exe", "fnn.exe"]);
        assert_eq!(
            fs::read(bin.join("fnn-cli.exe")).unwrap(),
            b"new fnn-cli.exe"
        );
    }

    #[test]
    fn moves_aside_under_a_name_no_other_file_has() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, source) = setup(dir.path());
        fs::write(bin.join("fnn"), b"current").unwrap();
        fs::write(bin.join("fnn.old"), b"still running").unwrap();

        let staged = stage(&bin, &copies(&source, &["fnn"])).unwrap();
        let swap = apply(&bin, &staged, &[]).unwrap();

        assert_eq!(fs::read(bin.join("fnn.old")).unwrap(), b"still running");
        assert_eq!(fs::read(bin.join("fnn.old.1")).unwrap(), b"current");
        swap.finish();
        assert_eq!(fake_home::names(&bin), ["fnn", "fnn.old"]);
    }

    #[test]
    fn a_failed_step_undoes_the_earlier_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, source) = setup(dir.path());
        fs::write(bin.join("fnn"), b"old fnn").unwrap();
        fs::write(bin.join("fnn-cli"), b"old fnn-cli").unwrap();
        let before = fake_home::snapshot(&bin);

        let staged = stage(&bin, &copies(&source, &["fnn", "fnn-cli"])).unwrap();
        // Losing the second staged copy makes its rename into place fail.
        fs::remove_file(bin.join("fnn-cli.new")).unwrap();
        let err = apply(&bin, &staged, &[]).unwrap_err();

        assert!(err.to_string().starts_with("cannot write"), "{err}");
        assert_eq!(fake_home::snapshot(&bin), before);
    }

    #[test]
    fn takes_files_out_and_can_put_them_back() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, _) = setup(dir.path());
        fs::write(bin.join("fnn"), b"fnn").unwrap();
        fs::write(bin.join("fnn-cli"), b"fnn-cli").unwrap();
        let before = fake_home::snapshot(&bin);
        let files = ["fnn".to_string(), "fnn-cli".to_string()];

        apply(&bin, &[], &files).unwrap().undo();
        assert_eq!(fake_home::snapshot(&bin), before);

        apply(&bin, &[], &files).unwrap().finish();
        assert!(fake_home::names(&bin).is_empty());
    }

    #[test]
    fn a_failed_copy_leaves_no_staged_file() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, source) = setup(dir.path());
        let mut list = copies(&source, &["fnn"]);
        list.push((source.join("missing"), "fnn-cli".to_string()));

        let err = stage(&bin, &list).unwrap_err();

        assert!(err.to_string().starts_with("cannot copy"), "{err}");
        assert!(fake_home::names(&bin).is_empty());
    }
}
