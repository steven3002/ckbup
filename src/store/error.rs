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
    #[error("cannot activate {path}: the file does not exist")]
    MissingBinary { path: PathBuf },
    #[error(
        "{tool} {version} is not installed. Installed: {}. Install it with: ckbup install {tool}@{version}",
        version_list(.installed)
    )]
    NotInstalledToUse {
        tool: String,
        version: String,
        installed: Vec<String>,
    },
    #[error("{tool} {version} is not installed. Installed: {}", version_list(.installed))]
    NotInstalled {
        tool: String,
        version: String,
        installed: Vec<String>,
    },
    #[error(
        "{tool} {version} is the active version. Switch first, for example: ckbup use {tool}@{other}"
    )]
    RemoveActive {
        tool: String,
        version: String,
        other: String,
    },
    #[error(
        "cannot remove {tool} {version}: a program or terminal is using {path} ({source}). Close it and try again"
    )]
    InUse {
        tool: String,
        version: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
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

fn version_list(versions: &[String]) -> String {
    if versions.is_empty() {
        "none".to_string()
    } else {
        versions.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(versions: &[&str]) -> Vec<String> {
        versions.iter().map(|version| version.to_string()).collect()
    }

    #[test]
    fn not_installed_messages_list_the_installed_versions() {
        let to_use = StoreError::NotInstalledToUse {
            tool: "fnn".into(),
            version: "v0.9.0".into(),
            installed: installed(&["v0.10.1", "v0.9.1"]),
        };
        assert_eq!(
            to_use.to_string(),
            "fnn v0.9.0 is not installed. Installed: v0.10.1, v0.9.1. Install it with: ckbup install fnn@v0.9.0"
        );
        let to_remove = StoreError::NotInstalled {
            tool: "fnn".into(),
            version: "v0.9.0".into(),
            installed: installed(&[]),
        };
        assert_eq!(
            to_remove.to_string(),
            "fnn v0.9.0 is not installed. Installed: none"
        );
    }

    #[test]
    fn remove_active_message_names_the_switch_command() {
        let err = StoreError::RemoveActive {
            tool: "fnn".into(),
            version: "v0.10.1".into(),
            other: "v0.9.1".into(),
        };
        assert_eq!(
            err.to_string(),
            "fnn v0.10.1 is the active version. Switch first, for example: ckbup use fnn@v0.9.1"
        );
    }
}
