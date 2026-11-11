use std::ffi::OsStr;
use std::path::Path;

use crate::platform::Os;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    Posix,
    Fish,
    Pwsh,
}

/// Returns instructions for adding `bin_dir` to PATH, or `None` if it is already there.
///
/// Shell profiles are never edited; the user runs the printed command.
pub fn path_hint(bin_dir: &Path, os: Os) -> Option<String> {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    if is_on_path(bin_dir, &path_var) {
        return None;
    }
    let shell = detect_shell(os, std::env::var("SHELL").ok().as_deref());
    Some(hint_text(shell, bin_dir))
}

fn is_on_path(bin_dir: &Path, path_var: &OsStr) -> bool {
    std::env::split_paths(path_var).any(|entry| same_dir(&entry, bin_dir))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn detect_shell(os: Os, shell_var: Option<&str>) -> Shell {
    if os == Os::Windows {
        return Shell::Pwsh;
    }
    match shell_var {
        Some(shell) if shell.ends_with("fish") => Shell::Fish,
        _ => Shell::Posix,
    }
}

fn hint_text(shell: Shell, bin_dir: &Path) -> String {
    let dir = bin_dir.display();
    match shell {
        Shell::Posix => format!(
            "{dir} is not on your PATH. Add this line to ~/.bashrc or ~/.zshrc, then open a new shell:\n\n    export PATH=\"{dir}:$PATH\"\n"
        ),
        Shell::Fish => format!(
            "{dir} is not on your PATH. Run this once in fish:\n\n    fish_add_path \"{dir}\"\n"
        ),
        Shell::Pwsh => format!(
            "{dir} is not on your PATH. Run this in PowerShell, then open a new terminal:\n\n    [Environment]::SetEnvironmentVariable('Path', '{dir};' + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn picks_shell_from_os_and_shell_var() {
        assert_eq!(
            detect_shell(Os::Windows, Some("/usr/bin/bash")),
            Shell::Pwsh
        );
        assert_eq!(detect_shell(Os::Linux, Some("/usr/bin/fish")), Shell::Fish);
        assert_eq!(detect_shell(Os::Macos, Some("/bin/zsh")), Shell::Posix);
        assert_eq!(detect_shell(Os::Linux, None), Shell::Posix);
    }

    #[test]
    fn hint_text_per_shell() {
        let dir = PathBuf::from("/home/u/.ckbup/bin");
        assert!(hint_text(Shell::Posix, &dir).contains("export PATH=\"/home/u/.ckbup/bin:$PATH\""));
        assert!(hint_text(Shell::Fish, &dir).contains("fish_add_path \"/home/u/.ckbup/bin\""));
        let ps = hint_text(Shell::Pwsh, Path::new(r"C:\Users\u\.ckbup\bin"));
        assert!(ps.contains(
            r"[Environment]::SetEnvironmentVariable('Path', 'C:\Users\u\.ckbup\bin;' + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')"
        ));
    }

    #[test]
    fn detects_dir_already_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let path_var =
            std::env::join_paths([PathBuf::from("/nonexistent"), dir.path().to_path_buf()])
                .unwrap();
        assert!(is_on_path(dir.path(), &path_var));
        assert!(!is_on_path(Path::new("/elsewhere/bin"), &path_var));
    }
}
