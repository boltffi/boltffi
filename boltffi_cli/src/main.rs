mod build;
mod cargo;
mod check;
mod cli;
mod commands;
mod config;
mod pack;
#[cfg(test)]
mod process_environment;
mod reporter;
mod target;
mod toolchain;

use clap::Parser;

use crate::cli::{Cli, ConfigPaths, execute_command};

fn main() {
    let cli = Cli::parse();
    let config_paths = ConfigPaths::from(&cli);

    let verbosity = if cli.quiet {
        reporter::Verbosity::Quiet
    } else if cli.verbose > 0 {
        reporter::Verbosity::Verbose
    } else {
        reporter::Verbosity::Normal
    };

    let reporter = reporter::Reporter::new(verbosity);
    let result = execute_command(cli.command, &reporter, cli.cargo_args, &config_paths);

    if let Err(err) = result {
        eprintln!("\n{} {}", console::style("error:").red().bold(), err);
        std::process::exit(1);
    }
}
