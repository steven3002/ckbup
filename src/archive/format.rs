use std::path::Path;

use super::{ArchiveError, tar_gz, zip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    TarGz,
    Zip,
}

impl ArchiveFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ArchiveFormat::TarGz => "tar.gz",
            ArchiveFormat::Zip => "zip",
        }
    }

    /// Extracts regular files and directories from `archive` into `dest`.
    pub fn extract(self, archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
        match self {
            ArchiveFormat::TarGz => tar_gz::extract(archive, dest),
            ArchiveFormat::Zip => zip::extract(archive, dest),
        }
    }
}
