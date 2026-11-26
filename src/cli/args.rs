use clap::{Args, Parser, Subcommand};

use super::target::{self, VersionedTarget};

/// Install official releases of CKB development tools.
#[derive(Debug, Parser)]
#[command(name = "ckbup", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install a tool, optionally at a specific version (e.g. ckb-cli@v2.0.0)
    Install(InstallArgs),
    /// List installed versions, or the stable releases on GitHub with --available
    List(ListArgs),
    /// Make an installed version active (e.g. fnn@v0.9.1)
    Use(UseArgs),
    /// Move tools to their latest stable release; held tools stay unless named
    Update(UpdateArgs),
    /// Delete an installed version (e.g. fnn@v0.9.1)
    Remove(RemoveArgs),
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Tool to install, as <tool> or <tool>@<version>
    pub target: String,

    /// Install even when no published checksum is available
    #[arg(long)]
    pub allow_unverified: bool,

    /// Prefer the portable build; try it if the standard build fails with an illegal-instruction error
    #[arg(long)]
    pub portable: bool,

    /// Keep the verified archive in the ckbup cache (also: CKBUP_KEEP_ARCHIVES=1)
    #[arg(long)]
    pub keep_archives: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Show only this tool
    pub tool: Option<String>,

    /// Show the newest stable releases on GitHub (one API request per tool)
    #[arg(long)]
    pub available: bool,
}

#[derive(Debug, Args)]
pub struct UseArgs {
    /// Installed version to activate, as <tool>@<version>
    #[arg(value_name = "TOOL@VERSION", value_parser = target::use_target)]
    pub target: VersionedTarget,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Update only this tool, even when it is held, and release its hold
    #[arg(value_name = "TOOL", value_parser = target::update_tool)]
    pub tool: Option<String>,

    /// Prefer the portable build for the new version
    #[arg(long)]
    pub portable: bool,

    /// Install a new version even when no published checksum is available
    #[arg(long)]
    pub allow_unverified: bool,

    /// Keep verified archives in the ckbup cache (also: CKBUP_KEEP_ARCHIVES=1)
    #[arg(long)]
    pub keep_archives: bool,

    /// Remove the version that the update replaced
    #[arg(long)]
    pub remove_old: bool,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Installed version to delete, as <tool>@<version>
    #[arg(value_name = "TOOL@VERSION", value_parser = target::remove_target)]
    pub target: VersionedTarget,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    #[test]
    fn command_line_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn use_and_remove_reject_a_target_without_a_version() {
        for (command, example) in [
            ("use", "ckbup use fnn@v0.10.1"),
            ("remove", "ckbup remove fnn@v0.9.1"),
        ] {
            let err = Cli::try_parse_from(["ckbup", command, "fnn"]).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::ValueValidation);
            assert!(err.to_string().contains(example), "{err}");
        }
    }

    #[test]
    fn install_keeps_an_optional_version() {
        let cli = Cli::try_parse_from(["ckbup", "install", "fnn"]).unwrap();
        assert!(matches!(cli.command, Command::Install(args) if args.target == "fnn"));
    }

    #[test]
    fn install_takes_keep_archives() {
        let cli = Cli::try_parse_from(["ckbup", "install", "fnn", "--keep-archives"]).unwrap();
        assert!(matches!(cli.command, Command::Install(args) if args.keep_archives));
    }

    #[test]
    fn update_takes_an_optional_tool_and_its_flags() {
        let cli = Cli::try_parse_from(["ckbup", "update"]).unwrap();
        let Command::Update(args) = cli.command else {
            panic!("not update");
        };
        assert_eq!(args.tool, None);
        assert!(
            !args.portable && !args.allow_unverified && !args.keep_archives && !args.remove_old
        );

        let cli = Cli::try_parse_from([
            "ckbup",
            "update",
            "fnn",
            "--portable",
            "--allow-unverified",
            "--keep-archives",
            "--remove-old",
        ])
        .unwrap();
        let Command::Update(args) = cli.command else {
            panic!("not update");
        };
        assert_eq!(args.tool.as_deref(), Some("fnn"));
        assert!(args.portable && args.allow_unverified && args.keep_archives && args.remove_old);
    }

    #[test]
    fn update_rejects_a_version() {
        let err = Cli::try_parse_from(["ckbup", "update", "fnn@v0.10.1"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string().contains("ckbup install fnn@v0.10.1"),
            "{err}"
        );
    }

    #[test]
    fn remove_takes_no_cache_flag() {
        let err =
            Cli::try_parse_from(["ckbup", "remove", "fnn@v0.9.1", "--keep-archives"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }
}
