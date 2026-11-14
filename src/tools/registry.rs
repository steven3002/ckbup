use crate::platform::{Arch, Host, Os};

use super::ToolError;

/// How a tool's release archives are packaged on a given OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packaging {
    /// `.tar.gz` on every platform.
    TarGzEverywhere,
    /// `.zip` on macOS and Windows, `.tar.gz` on Linux.
    ZipOnMacAndWindows,
}

/// How a release asset name describes the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Naming {
    /// `<tool>_<tag>_<triple><variant>.<ext>`
    Triple,
    /// `<tool>_<tag>-<arch>-<os><variant>.<ext>`
    ArchOs,
}

/// Build variant of a release archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Standard,
    /// Built with RocksDB's `portable` feature and published with a `-portable` suffix.
    Portable,
}

impl Variant {
    pub fn suffix(self) -> &'static str {
        match self {
            Variant::Standard => "",
            Variant::Portable => "-portable",
        }
    }
}

/// The variant a tool tries first when the user states no preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantOrder {
    StandardFirst,
    PortableFirst,
}

impl VariantOrder {
    /// Variants in the order they are tried; `prefer_portable` puts portable first.
    pub fn variants(self, prefer_portable: bool) -> [Variant; 2] {
        match (self, prefer_portable) {
            (VariantOrder::StandardFirst, false) => [Variant::Standard, Variant::Portable],
            _ => [Variant::Portable, Variant::Standard],
        }
    }
}

/// Static description of an installable tool and its official releases.
#[derive(Debug)]
pub struct ToolSpec {
    /// Tool name, also the prefix of its release asset names.
    pub name: &'static str,
    pub owner: &'static str,
    pub repo: &'static str,
    pub binary: &'static str,
    /// Further binaries from the package that are put on PATH with `binary`.
    pub companions: &'static [&'static str],
    pub naming: Naming,
    pub packaging: Packaging,
    pub variant_order: VariantOrder,
    /// Whether releases publish a `<archive stem>-sha256.txt` next to each archive.
    pub sums_file: bool,
    /// Hosts for which at least one release has published an official binary.
    pub platforms: &'static [(Os, Arch)],
}

impl ToolSpec {
    pub fn supports(&self, host: Host) -> bool {
        self.platforms.contains(&(host.os, host.arch))
    }

    /// File names placed on PATH for `host`: the main binary, then its companions.
    pub fn executables(&self, host: Host) -> Vec<String> {
        std::iter::once(self.binary)
            .chain(self.companions.iter().copied())
            .map(|binary| host.executable_name(binary))
            .collect()
    }
}

const ALL_HOSTS: &[(Os, Arch)] = &[
    (Os::Linux, Arch::X86_64),
    (Os::Linux, Arch::Aarch64),
    (Os::Macos, Arch::X86_64),
    (Os::Macos, Arch::Aarch64),
    (Os::Windows, Arch::X86_64),
];

const CKB: ToolSpec = ToolSpec {
    name: "ckb",
    owner: "nervosnetwork",
    repo: "ckb",
    binary: "ckb",
    companions: &[],
    naming: Naming::Triple,
    packaging: Packaging::ZipOnMacAndWindows,
    variant_order: VariantOrder::StandardFirst,
    sums_file: false,
    platforms: ALL_HOSTS,
};

const CKB_CLI: ToolSpec = ToolSpec {
    name: "ckb-cli",
    owner: "nervosnetwork",
    repo: "ckb-cli",
    binary: "ckb-cli",
    companions: &[],
    naming: Naming::Triple,
    packaging: Packaging::ZipOnMacAndWindows,
    variant_order: VariantOrder::StandardFirst,
    sums_file: false,
    platforms: ALL_HOSTS,
};

const CKB_DEBUGGER: ToolSpec = ToolSpec {
    name: "ckb-debugger",
    owner: "nervosnetwork",
    repo: "ckb-standalone-debugger",
    binary: "ckb-debugger",
    companions: &[],
    naming: Naming::Triple,
    packaging: Packaging::TarGzEverywhere,
    variant_order: VariantOrder::StandardFirst,
    sums_file: true,
    platforms: &[
        (Os::Linux, Arch::X86_64),
        (Os::Linux, Arch::Aarch64),
        (Os::Macos, Arch::Aarch64),
        (Os::Windows, Arch::X86_64),
    ],
};

const FNN: ToolSpec = ToolSpec {
    name: "fnn",
    owner: "nervosnetwork",
    repo: "fiber",
    binary: "fnn",
    companions: &["fnn-cli"],
    naming: Naming::ArchOs,
    packaging: Packaging::TarGzEverywhere,
    variant_order: VariantOrder::PortableFirst,
    sums_file: false,
    platforms: ALL_HOSTS,
};

const CKB_LIGHT_CLIENT: ToolSpec = ToolSpec {
    name: "ckb-light-client",
    owner: "nervosnetwork",
    repo: "ckb-light-client",
    binary: "ckb-light-client",
    companions: &[],
    naming: Naming::ArchOs,
    packaging: Packaging::TarGzEverywhere,
    variant_order: VariantOrder::PortableFirst,
    sums_file: false,
    platforms: ALL_HOSTS,
};

const TOOLS: &[ToolSpec] = &[CKB, CKB_CLI, CKB_DEBUGGER, FNN, CKB_LIGHT_CLIENT];

pub fn find_tool(name: &str) -> Result<&'static ToolSpec, ToolError> {
    TOOLS
        .iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| ToolError::Unknown {
            name: name.to_string(),
            supported: TOOLS
                .iter()
                .map(|tool| tool.name)
                .collect::<Vec<_>>()
                .join(", "),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_tools_and_rejects_others() {
        assert_eq!(find_tool("ckb-cli").unwrap().repo, "ckb-cli");
        assert_eq!(
            find_tool("ckb-debugger").unwrap().repo,
            "ckb-standalone-debugger"
        );
        assert_eq!(find_tool("fnn").unwrap().repo, "fiber");
        let err = find_tool("ckb-foo").unwrap_err().to_string();
        assert!(err.contains("ckb, ckb-cli, ckb-debugger, fnn, ckb-light-client"));
    }

    #[test]
    fn variant_order_and_portable_preference() {
        use Variant::{Portable, Standard};
        assert_eq!(
            VariantOrder::StandardFirst.variants(false),
            [Standard, Portable]
        );
        assert_eq!(
            VariantOrder::StandardFirst.variants(true),
            [Portable, Standard]
        );
        assert_eq!(
            VariantOrder::PortableFirst.variants(false),
            [Portable, Standard]
        );
        assert_eq!(
            VariantOrder::PortableFirst.variants(true),
            [Portable, Standard]
        );
    }

    #[test]
    fn fnn_puts_fnn_cli_on_path_and_ckb_only_itself() {
        let windows = Host {
            os: Os::Windows,
            arch: Arch::X86_64,
        };
        assert_eq!(
            find_tool("fnn").unwrap().executables(windows),
            ["fnn.exe", "fnn-cli.exe"]
        );
        assert_eq!(find_tool("ckb").unwrap().executables(windows), ["ckb.exe"]);
    }
}
