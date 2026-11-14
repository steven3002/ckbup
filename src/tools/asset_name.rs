use crate::archive::ArchiveFormat;
use crate::platform::{Host, Os};

use super::ToolError;
use super::registry::{Naming, Packaging, ToolSpec, Variant};

/// A release archive that would serve the host, with its checksum file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub archive: String,
    pub format: ArchiveFormat,
    pub sums_file: Option<String>,
    pub variant: Variant,
}

/// Fails for hosts that no release of the tool has ever had a binary for,
/// so the check needs no network request.
pub fn ensure_supported(tool: &ToolSpec, host: Host) -> Result<(), ToolError> {
    host_part(tool, host).map(drop)
}

/// Archive names that would serve `host` for `tag`, most preferred first.
pub fn candidates(
    tool: &ToolSpec,
    tag: &str,
    host: Host,
    prefer_portable: bool,
) -> Result<Vec<Candidate>, ToolError> {
    let host_part = host_part(tool, host)?;
    let format = match (tool.packaging, host.os) {
        (Packaging::ZipOnMacAndWindows, Os::Macos | Os::Windows) => ArchiveFormat::Zip,
        _ => ArchiveFormat::TarGz,
    };
    let candidates = tool
        .variant_order
        .variants(prefer_portable)
        .into_iter()
        .map(|variant| {
            let stem = format!("{}_{tag}{host_part}{}", tool.name, variant.suffix());
            Candidate {
                archive: format!("{stem}.{}", format.extension()),
                format,
                sums_file: tool.sums_file.then(|| format!("{stem}-sha256.txt")),
                variant,
            }
        })
        .collect();
    Ok(candidates)
}

/// The host part of an asset name, including its leading separator.
fn host_part(tool: &ToolSpec, host: Host) -> Result<String, ToolError> {
    let part = match tool.naming {
        Naming::Triple => host.target_triple().map(|triple| format!("_{triple}")),
        Naming::ArchOs => host.arch_os().map(|pair| format!("-{pair}")),
    };
    part.filter(|_| tool.supports(host))
        .ok_or(ToolError::NoOfficialBinary {
            tool: tool.name,
            os: host.os.as_str(),
            arch: host.arch.as_str(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Arch;
    use crate::tools::find_tool;

    const LINUX_X64: Host = Host {
        os: Os::Linux,
        arch: Arch::X86_64,
    };
    const LINUX_ARM: Host = Host {
        os: Os::Linux,
        arch: Arch::Aarch64,
    };
    const MAC_X64: Host = Host {
        os: Os::Macos,
        arch: Arch::X86_64,
    };
    const MAC_ARM: Host = Host {
        os: Os::Macos,
        arch: Arch::Aarch64,
    };
    const WINDOWS_X64: Host = Host {
        os: Os::Windows,
        arch: Arch::X86_64,
    };
    const WINDOWS_ARM: Host = Host {
        os: Os::Windows,
        arch: Arch::Aarch64,
    };

    fn names(tool: &str, tag: &str, host: Host, prefer_portable: bool) -> Vec<String> {
        candidates(find_tool(tool).unwrap(), tag, host, prefer_portable)
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.archive)
            .collect()
    }

    /// (tool, tag, host, default order, order with `--portable`)
    type Case = (
        &'static str,
        &'static str,
        Host,
        [&'static str; 2],
        [&'static str; 2],
    );

    #[test]
    fn candidate_names_for_every_tool_and_host() {
        let cases: &[Case] = &[
            (
                "ckb",
                "v0.210.0",
                LINUX_X64,
                [
                    "ckb_v0.210.0_x86_64-unknown-linux-gnu.tar.gz",
                    "ckb_v0.210.0_x86_64-unknown-linux-gnu-portable.tar.gz",
                ],
                [
                    "ckb_v0.210.0_x86_64-unknown-linux-gnu-portable.tar.gz",
                    "ckb_v0.210.0_x86_64-unknown-linux-gnu.tar.gz",
                ],
            ),
            (
                "ckb",
                "v0.210.0",
                LINUX_ARM,
                [
                    "ckb_v0.210.0_aarch64-unknown-linux-gnu.tar.gz",
                    "ckb_v0.210.0_aarch64-unknown-linux-gnu-portable.tar.gz",
                ],
                [
                    "ckb_v0.210.0_aarch64-unknown-linux-gnu-portable.tar.gz",
                    "ckb_v0.210.0_aarch64-unknown-linux-gnu.tar.gz",
                ],
            ),
            (
                "ckb",
                "v0.210.0",
                MAC_X64,
                [
                    "ckb_v0.210.0_x86_64-apple-darwin.zip",
                    "ckb_v0.210.0_x86_64-apple-darwin-portable.zip",
                ],
                [
                    "ckb_v0.210.0_x86_64-apple-darwin-portable.zip",
                    "ckb_v0.210.0_x86_64-apple-darwin.zip",
                ],
            ),
            (
                "ckb",
                "v0.210.0",
                MAC_ARM,
                [
                    "ckb_v0.210.0_aarch64-apple-darwin.zip",
                    "ckb_v0.210.0_aarch64-apple-darwin-portable.zip",
                ],
                [
                    "ckb_v0.210.0_aarch64-apple-darwin-portable.zip",
                    "ckb_v0.210.0_aarch64-apple-darwin.zip",
                ],
            ),
            (
                "ckb",
                "v0.210.0",
                WINDOWS_X64,
                [
                    "ckb_v0.210.0_x86_64-pc-windows-msvc.zip",
                    "ckb_v0.210.0_x86_64-pc-windows-msvc-portable.zip",
                ],
                [
                    "ckb_v0.210.0_x86_64-pc-windows-msvc-portable.zip",
                    "ckb_v0.210.0_x86_64-pc-windows-msvc.zip",
                ],
            ),
            (
                "ckb-cli",
                "v2.0.0",
                LINUX_X64,
                [
                    "ckb-cli_v2.0.0_x86_64-unknown-linux-gnu.tar.gz",
                    "ckb-cli_v2.0.0_x86_64-unknown-linux-gnu-portable.tar.gz",
                ],
                [
                    "ckb-cli_v2.0.0_x86_64-unknown-linux-gnu-portable.tar.gz",
                    "ckb-cli_v2.0.0_x86_64-unknown-linux-gnu.tar.gz",
                ],
            ),
            (
                "ckb-cli",
                "v2.0.0",
                LINUX_ARM,
                [
                    "ckb-cli_v2.0.0_aarch64-unknown-linux-gnu.tar.gz",
                    "ckb-cli_v2.0.0_aarch64-unknown-linux-gnu-portable.tar.gz",
                ],
                [
                    "ckb-cli_v2.0.0_aarch64-unknown-linux-gnu-portable.tar.gz",
                    "ckb-cli_v2.0.0_aarch64-unknown-linux-gnu.tar.gz",
                ],
            ),
            (
                "ckb-cli",
                "v2.0.0",
                MAC_X64,
                [
                    "ckb-cli_v2.0.0_x86_64-apple-darwin.zip",
                    "ckb-cli_v2.0.0_x86_64-apple-darwin-portable.zip",
                ],
                [
                    "ckb-cli_v2.0.0_x86_64-apple-darwin-portable.zip",
                    "ckb-cli_v2.0.0_x86_64-apple-darwin.zip",
                ],
            ),
            (
                "ckb-cli",
                "v2.0.0",
                MAC_ARM,
                [
                    "ckb-cli_v2.0.0_aarch64-apple-darwin.zip",
                    "ckb-cli_v2.0.0_aarch64-apple-darwin-portable.zip",
                ],
                [
                    "ckb-cli_v2.0.0_aarch64-apple-darwin-portable.zip",
                    "ckb-cli_v2.0.0_aarch64-apple-darwin.zip",
                ],
            ),
            (
                "ckb-cli",
                "v2.0.0",
                WINDOWS_X64,
                [
                    "ckb-cli_v2.0.0_x86_64-pc-windows-msvc.zip",
                    "ckb-cli_v2.0.0_x86_64-pc-windows-msvc-portable.zip",
                ],
                [
                    "ckb-cli_v2.0.0_x86_64-pc-windows-msvc-portable.zip",
                    "ckb-cli_v2.0.0_x86_64-pc-windows-msvc.zip",
                ],
            ),
            (
                "ckb-debugger",
                "v1.1.1",
                LINUX_X64,
                [
                    "ckb-debugger_v1.1.1_x86_64-unknown-linux-gnu.tar.gz",
                    "ckb-debugger_v1.1.1_x86_64-unknown-linux-gnu-portable.tar.gz",
                ],
                [
                    "ckb-debugger_v1.1.1_x86_64-unknown-linux-gnu-portable.tar.gz",
                    "ckb-debugger_v1.1.1_x86_64-unknown-linux-gnu.tar.gz",
                ],
            ),
            (
                "ckb-debugger",
                "v1.1.1",
                LINUX_ARM,
                [
                    "ckb-debugger_v1.1.1_aarch64-unknown-linux-gnu.tar.gz",
                    "ckb-debugger_v1.1.1_aarch64-unknown-linux-gnu-portable.tar.gz",
                ],
                [
                    "ckb-debugger_v1.1.1_aarch64-unknown-linux-gnu-portable.tar.gz",
                    "ckb-debugger_v1.1.1_aarch64-unknown-linux-gnu.tar.gz",
                ],
            ),
            (
                "ckb-debugger",
                "v1.1.1",
                MAC_ARM,
                [
                    "ckb-debugger_v1.1.1_aarch64-apple-darwin.tar.gz",
                    "ckb-debugger_v1.1.1_aarch64-apple-darwin-portable.tar.gz",
                ],
                [
                    "ckb-debugger_v1.1.1_aarch64-apple-darwin-portable.tar.gz",
                    "ckb-debugger_v1.1.1_aarch64-apple-darwin.tar.gz",
                ],
            ),
            (
                "ckb-debugger",
                "v1.1.1",
                WINDOWS_X64,
                [
                    "ckb-debugger_v1.1.1_x86_64-pc-windows-msvc.tar.gz",
                    "ckb-debugger_v1.1.1_x86_64-pc-windows-msvc-portable.tar.gz",
                ],
                [
                    "ckb-debugger_v1.1.1_x86_64-pc-windows-msvc-portable.tar.gz",
                    "ckb-debugger_v1.1.1_x86_64-pc-windows-msvc.tar.gz",
                ],
            ),
            (
                "fnn",
                "v0.9.1",
                LINUX_X64,
                [
                    "fnn_v0.9.1-x86_64-linux-portable.tar.gz",
                    "fnn_v0.9.1-x86_64-linux.tar.gz",
                ],
                [
                    "fnn_v0.9.1-x86_64-linux-portable.tar.gz",
                    "fnn_v0.9.1-x86_64-linux.tar.gz",
                ],
            ),
            (
                "fnn",
                "v0.9.1",
                LINUX_ARM,
                [
                    "fnn_v0.9.1-aarch64-linux-portable.tar.gz",
                    "fnn_v0.9.1-aarch64-linux.tar.gz",
                ],
                [
                    "fnn_v0.9.1-aarch64-linux-portable.tar.gz",
                    "fnn_v0.9.1-aarch64-linux.tar.gz",
                ],
            ),
            (
                "fnn",
                "v0.9.1",
                MAC_X64,
                [
                    "fnn_v0.9.1-x86_64-darwin-portable.tar.gz",
                    "fnn_v0.9.1-x86_64-darwin.tar.gz",
                ],
                [
                    "fnn_v0.9.1-x86_64-darwin-portable.tar.gz",
                    "fnn_v0.9.1-x86_64-darwin.tar.gz",
                ],
            ),
            (
                "fnn",
                "v0.9.1",
                MAC_ARM,
                [
                    "fnn_v0.9.1-aarch64-darwin-portable.tar.gz",
                    "fnn_v0.9.1-aarch64-darwin.tar.gz",
                ],
                [
                    "fnn_v0.9.1-aarch64-darwin-portable.tar.gz",
                    "fnn_v0.9.1-aarch64-darwin.tar.gz",
                ],
            ),
            (
                "fnn",
                "v0.9.1",
                WINDOWS_X64,
                [
                    "fnn_v0.9.1-x86_64-windows-portable.tar.gz",
                    "fnn_v0.9.1-x86_64-windows.tar.gz",
                ],
                [
                    "fnn_v0.9.1-x86_64-windows-portable.tar.gz",
                    "fnn_v0.9.1-x86_64-windows.tar.gz",
                ],
            ),
            (
                "ckb-light-client",
                "v0.5.4",
                LINUX_X64,
                [
                    "ckb-light-client_v0.5.4-x86_64-linux-portable.tar.gz",
                    "ckb-light-client_v0.5.4-x86_64-linux.tar.gz",
                ],
                [
                    "ckb-light-client_v0.5.4-x86_64-linux-portable.tar.gz",
                    "ckb-light-client_v0.5.4-x86_64-linux.tar.gz",
                ],
            ),
            (
                "ckb-light-client",
                "v0.5.5-rc1",
                LINUX_ARM,
                [
                    "ckb-light-client_v0.5.5-rc1-aarch64-linux-portable.tar.gz",
                    "ckb-light-client_v0.5.5-rc1-aarch64-linux.tar.gz",
                ],
                [
                    "ckb-light-client_v0.5.5-rc1-aarch64-linux-portable.tar.gz",
                    "ckb-light-client_v0.5.5-rc1-aarch64-linux.tar.gz",
                ],
            ),
            (
                "ckb-light-client",
                "v0.5.4",
                MAC_X64,
                [
                    "ckb-light-client_v0.5.4-x86_64-darwin-portable.tar.gz",
                    "ckb-light-client_v0.5.4-x86_64-darwin.tar.gz",
                ],
                [
                    "ckb-light-client_v0.5.4-x86_64-darwin-portable.tar.gz",
                    "ckb-light-client_v0.5.4-x86_64-darwin.tar.gz",
                ],
            ),
            (
                "ckb-light-client",
                "v0.5.5-rc1",
                MAC_ARM,
                [
                    "ckb-light-client_v0.5.5-rc1-aarch64-darwin-portable.tar.gz",
                    "ckb-light-client_v0.5.5-rc1-aarch64-darwin.tar.gz",
                ],
                [
                    "ckb-light-client_v0.5.5-rc1-aarch64-darwin-portable.tar.gz",
                    "ckb-light-client_v0.5.5-rc1-aarch64-darwin.tar.gz",
                ],
            ),
            (
                "ckb-light-client",
                "v0.5.4",
                WINDOWS_X64,
                [
                    "ckb-light-client_v0.5.4-x86_64-windows-portable.tar.gz",
                    "ckb-light-client_v0.5.4-x86_64-windows.tar.gz",
                ],
                [
                    "ckb-light-client_v0.5.4-x86_64-windows-portable.tar.gz",
                    "ckb-light-client_v0.5.4-x86_64-windows.tar.gz",
                ],
            ),
        ];
        for (tool, tag, host, default, portable) in cases {
            assert_eq!(names(tool, tag, *host, false), default, "{tool} {host:?}");
            assert_eq!(names(tool, tag, *host, true), portable, "{tool} {host:?}");
        }
    }

    #[test]
    fn only_ckb_debugger_has_checksum_files() {
        let debugger = candidates(
            find_tool("ckb-debugger").unwrap(),
            "v1.1.1",
            WINDOWS_X64,
            false,
        )
        .unwrap();
        assert_eq!(debugger[0].format, ArchiveFormat::TarGz);
        assert_eq!(
            debugger[0].sums_file.as_deref(),
            Some("ckb-debugger_v1.1.1_x86_64-pc-windows-msvc-sha256.txt")
        );
        for tool in ["ckb", "ckb-cli", "fnn", "ckb-light-client"] {
            let list = candidates(find_tool(tool).unwrap(), "v1.0.0", LINUX_X64, false).unwrap();
            assert!(list.iter().all(|candidate| candidate.sums_file.is_none()));
        }
    }

    #[test]
    fn never_names_centos_builds() {
        for tool in ["ckb", "ckb-cli", "ckb-debugger", "fnn", "ckb-light-client"] {
            for prefer_portable in [false, true] {
                let list = names(tool, "v1.0.0", LINUX_X64, prefer_portable);
                assert!(list.iter().all(|name| !name.contains("centos")));
            }
        }
    }

    #[test]
    fn unsupported_hosts_fail_without_names() {
        let err = ensure_supported(find_tool("ckb-debugger").unwrap(), MAC_X64).unwrap_err();
        assert_eq!(
            err.to_string(),
            "no official ckb-debugger binary for macos/x86_64"
        );
        for tool in ["ckb", "ckb-cli", "ckb-debugger", "fnn", "ckb-light-client"] {
            let spec = find_tool(tool).unwrap();
            assert!(ensure_supported(spec, WINDOWS_ARM).is_err());
            assert!(candidates(spec, "v1.0.0", WINDOWS_ARM, false).is_err());
        }
    }
}
