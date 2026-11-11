use std::fs::{self, File};
use std::io;
use std::path::Path;

use zip::ZipArchive;

use super::ArchiveError;
use super::entry_path::safe_relative_path;

/// Extracts regular files and directories; symlink entries are skipped.
pub fn extract(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let zip_err = |source| ArchiveError::Zip {
        path: archive.to_path_buf(),
        source,
    };
    let file = File::open(archive).map_err(|source| ArchiveError::Read {
        path: archive.to_path_buf(),
        source,
    })?;
    let mut zip = ZipArchive::new(file).map_err(zip_err)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(zip_err)?;
        let name = entry.name().map_err(zip_err)?.into_owned();
        let relative =
            safe_relative_path(Path::new(&name)).ok_or_else(|| ArchiveError::UnsafeEntry {
                archive: archive.to_path_buf(),
                entry: name.clone(),
            })?;
        let target = dest.join(relative);
        let write_err = |source: io::Error| ArchiveError::Write {
            path: target.clone(),
            source,
        };
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(write_err)?;
        } else if entry.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(write_err)?;
            }
            let mut out = File::create(&target).map_err(write_err)?;
            io::copy(&mut entry, &mut out).map_err(write_err)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for (name, data) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn extracts_top_folder_layout() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        write_zip(
            &archive,
            &[("top/tool.exe", b"bin"), ("top/COPYING", b"doc")],
        );
        let dest = dir.path().join("out");
        extract(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("top").join("tool.exe")).unwrap(), b"bin");
    }

    #[test]
    fn rejects_parent_dir_entries() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        write_zip(&archive, &[("../evil", b"x")]);
        let err = extract(&archive, &dir.path().join("out")).unwrap_err();
        assert!(matches!(err, ArchiveError::UnsafeEntry { .. }));
        assert!(!dir.path().join("evil").exists());
    }
}
