#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Macos,
    Windows,
    Other(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    Other(&'static str),
}

impl Os {
    pub fn as_str(self) -> &'static str {
        match self {
            Os::Linux => "linux",
            Os::Macos => "macos",
            Os::Windows => "windows",
            Os::Other(name) => name,
        }
    }
}

impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
            Arch::Other(name) => name,
        }
    }
}

/// Operating system and CPU architecture that ckbup runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    pub os: Os,
    pub arch: Arch,
}

impl Host {
    pub fn detect() -> Self {
        let os = match std::env::consts::OS {
            "linux" => Os::Linux,
            "macos" => Os::Macos,
            "windows" => Os::Windows,
            other => Os::Other(other),
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => Arch::X86_64,
            "aarch64" => Arch::Aarch64,
            other => Arch::Other(other),
        };
        Host { os, arch }
    }

    pub fn executable_name(self, binary: &str) -> String {
        match self.os {
            Os::Windows => format!("{binary}.exe"),
            _ => binary.to_string(),
        }
    }

    /// Target triple that release asset names use for this host.
    pub fn target_triple(self) -> Option<&'static str> {
        match (self.os, self.arch) {
            (Os::Linux, Arch::X86_64) => Some("x86_64-unknown-linux-gnu"),
            (Os::Linux, Arch::Aarch64) => Some("aarch64-unknown-linux-gnu"),
            (Os::Macos, Arch::X86_64) => Some("x86_64-apple-darwin"),
            (Os::Macos, Arch::Aarch64) => Some("aarch64-apple-darwin"),
            (Os::Windows, Arch::X86_64) => Some("x86_64-pc-windows-msvc"),
            _ => None,
        }
    }

    /// `<arch>-<os>` pair that release asset names use for this host; macOS is `darwin`.
    pub fn arch_os(self) -> Option<String> {
        let arch = match self.arch {
            Arch::X86_64 | Arch::Aarch64 => self.arch.as_str(),
            Arch::Other(_) => return None,
        };
        let os = match self.os {
            Os::Linux => "linux",
            Os::Macos => "darwin",
            Os::Windows => "windows",
            Os::Other(_) => return None,
        };
        Some(format!("{arch}-{os}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_hosts_to_asset_name_tokens() {
        let mac_arm = Host {
            os: Os::Macos,
            arch: Arch::Aarch64,
        };
        assert_eq!(mac_arm.target_triple(), Some("aarch64-apple-darwin"));
        assert_eq!(mac_arm.arch_os().as_deref(), Some("aarch64-darwin"));

        let windows_arm = Host {
            os: Os::Windows,
            arch: Arch::Aarch64,
        };
        assert_eq!(windows_arm.target_triple(), None);

        let other = Host {
            os: Os::Other("freebsd"),
            arch: Arch::X86_64,
        };
        assert_eq!(other.target_triple(), None);
        assert_eq!(other.arch_os(), None);
    }
}
