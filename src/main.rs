mod archive;
mod cli;
mod commands;
mod error;
mod github;
mod integrity;
mod output;
mod platform;
mod store;
mod tools;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Install(args) => commands::install::run(&args),
        Command::List(args) => commands::list::run(&args),
        Command::Use(args) => commands::use_version::run(&args),
        Command::Remove(args) => commands::remove::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
