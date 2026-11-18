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
}
