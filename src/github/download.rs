use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::{GithubClient, GithubError};

/// Downloads the release asset at `url` to `dest`.
///
/// The body is streamed into `<dest>.part`, which is renamed to `dest` only
/// once the transfer is complete, so `dest` never holds a truncated file.
pub fn download_to_file(client: &GithubClient, url: &str, dest: &Path) -> Result<(), GithubError> {
    let partial = partial_path(dest);
    let file = File::create(&partial).map_err(GithubError::save(&partial))?;
    let mut writer = BufWriter::new(file);
    client.download(url, &mut writer)?;
    writer.flush().map_err(GithubError::save(&partial))?;
    drop(writer);
    fs::rename(&partial, dest).map_err(GithubError::save(dest))
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = OsString::from(dest.as_os_str());
    name.push(".part");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_sits_next_to_the_destination() {
        let dest = Path::new("downloads").join("fnn_v0.9.1-x86_64-windows.tar.gz");
        assert_eq!(
            partial_path(&dest),
            Path::new("downloads").join("fnn_v0.9.1-x86_64-windows.tar.gz.part")
        );
    }
}
