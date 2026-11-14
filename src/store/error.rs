use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("cannot determine the home directory; set CKBUP_HOME")]
    NoHome,
    #[error("{action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} already exists but holds no installed binary; remove it and try again")]
    Occupied { path: PathBuf },
    #[error("{path} is not valid: {source}")]
    InvalidActiveFile {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl StoreError {
    pub fn io(action: &'static str, path: &std::path::Path) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.to_path_buf();
        move |source| StoreError::Io {
            action,
            path,
            source,
        }
    }
}
