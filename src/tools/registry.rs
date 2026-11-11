use crate::platform::{Arch, Os};

use super::ToolError;

/// How a tool's release archives are packaged on a given OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packaging {
    /// `.tar.gz` on every platform.
    TarGzEverywhere,
    /// `.zip` on macOS and Windows, `.tar.gz` on Linux.
    ZipOnMacAndWindows,
}

/// Static description of an installable tool and its official releases.
#[derive(Debug)]
pub struct ToolSpec {
    pub name: &'static str,
    pub owner: &'static str,
    pub repo: &'static str,
    pub binary: &'static str,
    pub packaging: Packaging,
    /// Whether releases publish a `<archive stem>-sha256.txt` next to each archive.
    pub sums_file: bool,
    /// Hosts with an official prebuilt binary, as (os, arch, target triple).
    pub targets: &'static [(Os, Arch, &'static str)],
}

const CKB_DEBUGGER: ToolSpec = ToolSpec {
    name: "ckb-debugger",
    owner: "nervosnetwork",
    repo: "ckb-standalone-debugger",
    binary: "ckb-debugger",
    packaging: Packaging::TarGzEverywhere,
    sums_file: true,
    targets: &[
        (Os::Linux, Arch::X86_64, "x86_64-unknown-linux-gnu"),
        (Os::Linux, Arch::Aarch64, "aarch64-unknown-linux-gnu"),
        (Os::Macos, Arch::Aarch64, "aarch64-apple-darwin"),
        (Os::Windows, Arch::X86_64, "x86_64-pc-windows-msvc"),
    ],
};

const CKB_CLI: ToolSpec = ToolSpec {
    name: "ckb-cli",
    owner: "nervosnetwork",
    repo: "ckb-cli",
    binary: "ckb-cli",
    packaging: Packaging::ZipOnMacAndWindows,
    sums_file: false,
    targets: &[
        (Os::Linux, Arch::X86_64, "x86_64-unknown-linux-gnu"),
        (Os::Linux, Arch::Aarch64, "aarch64-unknown-linux-gnu"),
        (Os::Macos, Arch::Aarch64, "aarch64-apple-darwin"),
        (Os::Macos, Arch::X86_64, "x86_64-apple-darwin"),
        (Os::Windows, Arch::X86_64, "x86_64-pc-windows-msvc"),
    ],
};

const TOOLS: &[ToolSpec] = &[CKB_DEBUGGER, CKB_CLI];

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
        let err = find_tool("ckb-foo").unwrap_err().to_string();
        assert!(err.contains("ckb-debugger, ckb-cli"));
    }
}
