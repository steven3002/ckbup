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
}
