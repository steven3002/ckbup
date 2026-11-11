use crate::archive::ArchiveFormat;
use crate::platform::{Host, Os};

use super::ToolError;
use super::registry::{Packaging, ToolSpec};

/// Names of the release assets ckbup needs for one tool, tag and host.
#[derive(Debug, PartialEq, Eq)]
pub struct ReleaseAssets {
    pub archive: String,
    pub format: ArchiveFormat,
    pub sums_file: Option<String>,
}

/// Returns the target triple of the official binary for `host`.
pub fn target_triple(tool: &ToolSpec, host: Host) -> Result<&'static str, ToolError> {
    tool.targets
        .iter()
        .find(|(os, arch, _)| *os == host.os && *arch == host.arch)
        .map(|(_, _, triple)| *triple)
        .ok_or(ToolError::NoOfficialBinary {
            tool: tool.name,
            os: host.os.as_str(),
            arch: host.arch.as_str(),
        })
}

pub fn release_assets(tool: &ToolSpec, tag: &str, host: Host) -> Result<ReleaseAssets, ToolError> {
    let triple = target_triple(tool, host)?;

    let format = match (tool.packaging, host.os) {
        (Packaging::ZipOnMacAndWindows, Os::Macos | Os::Windows) => ArchiveFormat::Zip,
        _ => ArchiveFormat::TarGz,
    };
    let stem = format!("{}_{}_{}", tool.name, tag, triple);
    Ok(ReleaseAssets {
        archive: format!("{stem}.{}", format.extension()),
        format,
        sums_file: tool.sums_file.then(|| format!("{stem}-sha256.txt")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Arch;
    use crate::tools::find_tool;

    fn host(os: Os, arch: Arch) -> Host {
        Host { os, arch }
    }

    #[test]
    fn debugger_is_tar_gz_with_sums_file_on_windows() {
        let tool = find_tool("ckb-debugger").unwrap();
        let assets = release_assets(tool, "v1.1.1", host(Os::Windows, Arch::X86_64)).unwrap();
        assert_eq!(
            assets.archive,
            "ckb-debugger_v1.1.1_x86_64-pc-windows-msvc.tar.gz"
        );
        assert_eq!(assets.format, ArchiveFormat::TarGz);
        assert_eq!(
            assets.sums_file.as_deref(),
            Some("ckb-debugger_v1.1.1_x86_64-pc-windows-msvc-sha256.txt")
        );
    }

    #[test]
    fn cli_uses_zip_on_mac_and_windows_and_tar_gz_on_linux() {
        let tool = find_tool("ckb-cli").unwrap();
        let cases = [
            (
                Os::Windows,
                Arch::X86_64,
                "ckb-cli_v2.0.0_x86_64-pc-windows-msvc.zip",
            ),
            (
                Os::Macos,
                Arch::X86_64,
                "ckb-cli_v2.0.0_x86_64-apple-darwin.zip",
            ),
            (
                Os::Macos,
                Arch::Aarch64,
                "ckb-cli_v2.0.0_aarch64-apple-darwin.zip",
            ),
            (
                Os::Linux,
                Arch::X86_64,
                "ckb-cli_v2.0.0_x86_64-unknown-linux-gnu.tar.gz",
            ),
            (
                Os::Linux,
                Arch::Aarch64,
                "ckb-cli_v2.0.0_aarch64-unknown-linux-gnu.tar.gz",
            ),
        ];
        for (os, arch, expected) in cases {
            let assets = release_assets(tool, "v2.0.0", host(os, arch)).unwrap();
            assert_eq!(assets.archive, expected);
            assert_eq!(assets.sums_file, None);
        }
    }

    #[test]
    fn debugger_has_no_x86_64_macos_binary() {
        let tool = find_tool("ckb-debugger").unwrap();
        let err = release_assets(tool, "v1.1.1", host(Os::Macos, Arch::X86_64)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "no official ckb-debugger binary for macos/x86_64"
        );
    }

    #[test]
    fn windows_arm64_has_no_binary() {
        let tool = find_tool("ckb-cli").unwrap();
        assert!(release_assets(tool, "v2.0.0", host(Os::Windows, Arch::Aarch64)).is_err());
    }
}
