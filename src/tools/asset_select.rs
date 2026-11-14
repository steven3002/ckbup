use crate::archive::ArchiveFormat;
use crate::platform::Host;

use super::ToolError;
use super::asset_name::{Candidate, candidates};
use super::registry::ToolSpec;

/// Picks the first candidate archive that the release actually publishes.
///
/// `published` holds the names of every asset in the release.
pub fn select_asset(
    tool: &ToolSpec,
    tag: &str,
    host: Host,
    prefer_portable: bool,
    published: &[&str],
) -> Result<Candidate, ToolError> {
    candidates(tool, tag, host, prefer_portable)?
        .into_iter()
        .find(|candidate| published.contains(&candidate.archive.as_str()))
        .ok_or_else(|| ToolError::NoMatchingAsset {
            tool: tool.name,
            tag: tag.to_string(),
            os: host.os.as_str(),
            arch: host.arch.as_str(),
            archives: archive_list(published),
        })
}

fn archive_list(published: &[&str]) -> String {
    let mut archives: Vec<&str> = published
        .iter()
        .copied()
        .filter(|name| is_archive(name))
        .collect();
    if archives.is_empty() {
        return "  (none)".to_string();
    }
    archives.sort_unstable();
    archives
        .iter()
        .map(|name| format!("  {name}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_archive(name: &str) -> bool {
    [ArchiveFormat::TarGz, ArchiveFormat::Zip]
        .iter()
        .any(|format| name.ends_with(&format!(".{}", format.extension())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};
    use crate::tools::Variant;
    use crate::tools::find_tool;

    fn host(os: Os, arch: Arch) -> Host {
        Host { os, arch }
    }

    /// Assets of fnn v0.9.1, where arm64 and macOS builds exist only as portable.
    const FNN_V0_9_1: &[&str] = &[
        "fnn_v0.9.1-aarch64-darwin-portable.tar.gz",
        "fnn_v0.9.1-aarch64-linux-portable.tar.gz",
        "fnn_v0.9.1-x86_64-darwin-portable.tar.gz",
        "fnn_v0.9.1-x86_64-linux-portable.tar.gz",
        "fnn_v0.9.1-x86_64-linux.tar.gz",
        "fnn_v0.9.1-x86_64-windows.tar.gz",
    ];

    /// Assets of ckb-light-client v0.5.4, which has no arm64 build.
    const LIGHT_CLIENT_V0_5_4: &[&str] = &[
        "ckb-light-client_v0.5.4-x86_64-darwin-portable.tar.gz",
        "ckb-light-client_v0.5.4-x86_64-darwin.tar.gz",
        "ckb-light-client_v0.5.4-x86_64-linux-portable.tar.gz",
        "ckb-light-client_v0.5.4-x86_64-linux.tar.gz",
        "ckb-light-client_v0.5.4-x86_64-windows.tar.gz",
    ];

    /// Archive assets of ckb v0.210.0 on Windows and Linux, with a signature.
    const CKB_V0_210_0: &[&str] = &[
        "ckb_v0.210.0_aarch64-unknown-linux-gnu.tar.gz",
        "ckb_v0.210.0_x86_64-pc-windows-msvc.zip",
        "ckb_v0.210.0_x86_64-pc-windows-msvc.zip.asc",
        "ckb_v0.210.0_x86_64-unknown-centos-gnu.tar.gz",
        "ckb_v0.210.0_x86_64-unknown-linux-gnu-portable.tar.gz",
        "ckb_v0.210.0_x86_64-unknown-linux-gnu.tar.gz",
    ];

    #[test]
    fn picks_the_first_published_candidate() {
        let fnn = find_tool("fnn").unwrap();
        let linux = select_asset(
            fnn,
            "v0.9.1",
            host(Os::Linux, Arch::X86_64),
            false,
            FNN_V0_9_1,
        )
        .unwrap();
        assert_eq!(linux.archive, "fnn_v0.9.1-x86_64-linux-portable.tar.gz");
        assert_eq!(linux.variant, Variant::Portable);

        let windows = select_asset(
            fnn,
            "v0.9.1",
            host(Os::Windows, Arch::X86_64),
            false,
            FNN_V0_9_1,
        )
        .unwrap();
        assert_eq!(windows.archive, "fnn_v0.9.1-x86_64-windows.tar.gz");

        let ckb = find_tool("ckb").unwrap();
        let standard = select_asset(
            ckb,
            "v0.210.0",
            host(Os::Linux, Arch::X86_64),
            false,
            CKB_V0_210_0,
        )
        .unwrap();
        assert_eq!(
            standard.archive,
            "ckb_v0.210.0_x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn portable_flag_prefers_portable_and_falls_back_to_standard() {
        let ckb = find_tool("ckb").unwrap();
        let portable = select_asset(
            ckb,
            "v0.210.0",
            host(Os::Linux, Arch::X86_64),
            true,
            CKB_V0_210_0,
        )
        .unwrap();
        assert_eq!(
            portable.archive,
            "ckb_v0.210.0_x86_64-unknown-linux-gnu-portable.tar.gz"
        );

        let windows = select_asset(
            ckb,
            "v0.210.0",
            host(Os::Windows, Arch::X86_64),
            true,
            CKB_V0_210_0,
        )
        .unwrap();
        assert_eq!(windows.archive, "ckb_v0.210.0_x86_64-pc-windows-msvc.zip");
        assert_eq!(windows.variant, Variant::Standard);

        let arm = select_asset(
            ckb,
            "v0.210.0",
            host(Os::Linux, Arch::Aarch64),
            true,
            CKB_V0_210_0,
        )
        .unwrap();
        assert_eq!(arm.archive, "ckb_v0.210.0_aarch64-unknown-linux-gnu.tar.gz");
    }

    #[test]
    fn lists_release_archives_when_no_candidate_is_published() {
        let light_client = find_tool("ckb-light-client").unwrap();
        let err = select_asset(
            light_client,
            "v0.5.4",
            host(Os::Linux, Arch::Aarch64),
            false,
            LIGHT_CLIENT_V0_5_4,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.starts_with("ckb-light-client v0.5.4 has no release archive for linux/aarch64")
        );
        for name in LIGHT_CLIENT_V0_5_4 {
            assert!(err.contains(name), "{name} missing from: {err}");
        }

        let err = select_asset(
            find_tool("ckb").unwrap(),
            "v0.210.0",
            host(Os::Macos, Arch::Aarch64),
            false,
            &["ckb_v0.210.0_x86_64-pc-windows-msvc.zip.asc"],
        )
        .unwrap_err()
        .to_string();
        assert!(err.ends_with("  (none)"));
    }

    #[test]
    fn unsupported_host_fails_before_matching() {
        let err = select_asset(
            find_tool("fnn").unwrap(),
            "v0.9.1",
            host(Os::Windows, Arch::Aarch64),
            false,
            FNN_V0_9_1,
        )
        .unwrap_err();
        assert!(matches!(err, ToolError::NoOfficialBinary { .. }));
    }
}
