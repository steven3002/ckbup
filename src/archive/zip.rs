use std::fs::{self, File};
use std::io;
use std::path::Path;

use zip::ZipArchive;

use super::ArchiveError;
use super::entry_path::safe_relative_path;
use super::mode;

/// Extracts regular files and directories; symlink entries are skipped. On
/// Unix, a file the archive marks executable gets `0o755` and every other file
/// `0o644`.
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
            mode::apply(&target, entry.unix_mode()).map_err(write_err)?;
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
        let with_options: Vec<(&str, &[u8], SimpleFileOptions)> = entries
            .iter()
            .map(|(name, data)| (*name, *data, SimpleFileOptions::default()))
            .collect();
        write_zip_with_options(path, &with_options);
    }

    fn write_zip_with_options(path: &Path, entries: &[(&str, &[u8], SimpleFileOptions)]) {
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for (name, data, options) in entries {
            writer.start_file(*name, *options).unwrap();
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

    #[cfg(unix)]
    #[test]
    fn maps_entry_modes_to_0o755_or_0o644() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("modes.zip");
        let options = SimpleFileOptions::default();
        let setuid = 0o104755 << 16;
        write_zip_with_options(
            &archive,
            &[
                ("top/ckb", b"bin", options.unix_permissions(0o755)),
                ("top/README.md", b"doc", options.unix_permissions(0o644)),
                ("top/setuid", b"bin", options.external_attributes(setuid)),
            ],
        );
        let dest = dir.path().join("out");
        extract(&archive, &dest).unwrap();

        let mode = |name: &str| {
            fs::metadata(dest.join("top").join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777
        };
        assert_eq!(mode("ckb"), 0o755);
        assert_eq!(mode("README.md"), 0o644);
        assert_eq!(mode("setuid"), 0o755);
    }
}
