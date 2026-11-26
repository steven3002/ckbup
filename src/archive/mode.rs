//! File modes for extracted files on Unix.
//!
//! An archive's own modes are mapped, never copied: a file the archive marks
//! executable becomes `0o755` and every other file `0o644`, so nothing is
//! setuid, setgid or writable by others. Folders keep the user's umask.

use std::io;
use std::path::Path;

/// The mode written for a file whose archive entry has `mode`. Any execute bit
/// makes it executable; no mode, or one that could not be read, does not.
#[cfg(any(unix, test))]
pub fn file_mode(mode: Option<u32>) -> u32 {
    match mode {
        Some(mode) if mode & 0o111 != 0 => 0o755,
        _ => 0o644,
    }
}

/// Sets the mode `file_mode` gives for `mode` on the extracted file `path`.
#[cfg(unix)]
pub fn apply(path: &Path, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(file_mode(mode)))
}

/// Windows has no Unix modes, so extracted files keep their defaults.
#[cfg(not(unix))]
pub fn apply(_path: &Path, _mode: Option<u32>) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_execute_bit_gives_0o755_and_nothing_else_does() {
        let cases = [
            (Some(0o755), 0o755),
            (Some(0o644), 0o644),
            (Some(0o4755), 0o755),
            (Some(0o777), 0o755),
            (Some(0o700), 0o755),
            (Some(0o666), 0o644),
            (Some(0o100755), 0o755),
            (None, 0o644),
        ];
        for (mode, expected) in cases {
            assert_eq!(file_mode(mode), expected, "{mode:?}");
        }
    }
}
