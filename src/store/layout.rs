use std::ffi::OsString;
use std::path::PathBuf;

use super::StoreError;

/// Directory layout of the ckbup home.
///
/// ```text
/// <home>/bin/                         copies of the active binaries
/// <home>/toolchains/<tool>/<version>/ installed binaries
/// <home>/downloads/                   temporary download and extraction space
/// <home>/active.toml                  active version per tool
/// ```
#[derive(Debug, Clone)]
pub struct Layout {
    home: PathBuf,
}

impl Layout {
    /// Uses `CKBUP_HOME` when set and non-empty, otherwise `~/.ckbup`.
    pub fn from_env() -> Result<Self, StoreError> {
        Self::resolve(std::env::var_os("CKBUP_HOME"), std::env::home_dir())
    }

    pub fn resolve(
        ckbup_home: Option<OsString>,
        user_home: Option<PathBuf>,
    ) -> Result<Self, StoreError> {
        let home = match ckbup_home.filter(|value| !value.is_empty()) {
            Some(value) => PathBuf::from(value),
            None => user_home.ok_or(StoreError::NoHome)?.join(".ckbup"),
        };
        Ok(Layout { home })
    }

    #[cfg(test)]
    pub fn home(&self) -> &std::path::Path {
        &self.home
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.home.join("bin")
    }

    pub fn toolchain_dir(&self, tool: &str, version: &str) -> PathBuf {
        self.home.join("toolchains").join(tool).join(version)
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.home.join("downloads")
    }

    pub fn active_file(&self) -> PathBuf {
        self.home.join("active.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ckbup_home_overrides_user_home() {
        let layout =
            Layout::resolve(Some("/opt/ckbup".into()), Some(PathBuf::from("/home/u"))).unwrap();
        assert_eq!(layout.home(), Path::new("/opt/ckbup"));
        assert_eq!(
            layout.toolchain_dir("ckb-cli", "v2.0.0"),
            Path::new("/opt/ckbup/toolchains/ckb-cli/v2.0.0")
        );
    }

    #[test]
    fn defaults_to_dot_ckbup_in_user_home() {
        let layout = Layout::resolve(None, Some(PathBuf::from("/home/u"))).unwrap();
        assert_eq!(layout.bin_dir(), Path::new("/home/u/.ckbup/bin"));
        let empty = Layout::resolve(Some(OsString::new()), Some(PathBuf::from("/home/u"))).unwrap();
        assert_eq!(empty.home(), Path::new("/home/u/.ckbup"));
    }

    #[test]
    fn fails_without_any_home() {
        assert!(Layout::resolve(None, None).is_err());
    }
}
