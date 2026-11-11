use std::path::{Component, Path, PathBuf};

/// Converts an archive entry name into a path relative to the extraction root.
///
/// Returns `None` for absolute paths, drive prefixes and any `..` component, so
/// an entry can never be written outside the destination directory.
pub fn safe_relative_path(entry: &Path) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for component in entry.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!relative.as_os_str().is_empty()).then_some(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_nested_relative_paths() {
        assert_eq!(
            safe_relative_path(Path::new("./dir/bin")),
            Some(PathBuf::from("dir").join("bin"))
        );
    }

    #[test]
    fn rejects_escaping_paths() {
        assert_eq!(safe_relative_path(Path::new("../evil")), None);
        assert_eq!(safe_relative_path(Path::new("dir/../../evil")), None);
        assert_eq!(safe_relative_path(Path::new("/etc/passwd")), None);
        assert_eq!(safe_relative_path(Path::new(".")), None);
    }
}
