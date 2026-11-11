use std::fs::{self, File};
use std::io;
use std::path::Path;

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use super::ArchiveError;
use super::entry_path::safe_relative_path;

/// Extracts regular files and directories; links and special entries are skipped.
pub fn extract(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let read_err = |source: io::Error| ArchiveError::Read {
        path: archive.to_path_buf(),
        source,
    };
    let file = File::open(archive).map_err(read_err)?;
    let mut tar = Archive::new(GzDecoder::new(file));
    for entry in tar.entries().map_err(read_err)? {
        let mut entry = entry.map_err(read_err)?;
        let entry_path = entry.path().map_err(read_err)?.into_owned();
        let relative =
            safe_relative_path(&entry_path).ok_or_else(|| ArchiveError::UnsafeEntry {
                archive: archive.to_path_buf(),
                entry: entry_path.display().to_string(),
            })?;
        let target = dest.join(relative);
        let write_err = |source: io::Error| ArchiveError::Write {
            path: target.clone(),
            source,
        };
        match entry.header().entry_type() {
            EntryType::Directory => fs::create_dir_all(&target).map_err(write_err)?,
            EntryType::Regular | EntryType::Continuous => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(write_err)?;
                }
                let mut out = File::create(&target).map_err(write_err)?;
                io::copy(&mut entry, &mut out).map_err(write_err)?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn write_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let encoder = GzEncoder::new(File::create(path).unwrap(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn extracts_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        write_tar_gz(&archive, &[("top/tool", b"bin"), ("top/README.md", b"doc")]);
        let dest = dir.path().join("out");
        extract(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("top").join("tool")).unwrap(), b"bin");
    }

    #[test]
    fn rejects_parent_dir_entries() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.tar.gz");
        let encoder = GzEncoder::new(File::create(&archive).unwrap(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        let name = b"../evil";
        header.as_old_mut().name[..name.len()].copy_from_slice(name);
        header.set_size(1);
        header.set_cksum();
        builder.append(&header, &b"x"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        let dest = dir.path().join("out");
        let err = extract(&archive, &dest).unwrap_err();
        assert!(matches!(err, ArchiveError::UnsafeEntry { .. }));
        assert!(!dir.path().join("evil").exists());
    }
}
