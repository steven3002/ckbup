use clap::{Args, Parser, Subcommand};

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
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Tool to install, as <tool> or <tool>@<version>
    pub target: String,

    /// Install even when no published checksum is available
    #[arg(long)]
    pub allow_unverified: bool,
}
