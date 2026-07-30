//! {{description}}

mod cli;
mod config;

use std::process::ExitCode;

use clap::Parser as _;
use pc_error::{Report, ResultExt as _};

use crate::cli::{Args, Command};

fn main() -> ExitCode {
    let args = Args::parse();

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let code = err.code();
            // Both, deliberately: `tracing` may not be initialized yet (config
            // could be what failed), and stderr is what a human at a terminal
            // actually reads.
            tracing::error!(error.code = %code, error.chain = %err.chained(), "fatal");
            eprintln!("{}: {}", env!("CARGO_PKG_NAME"), err.chained());
            // Distinct exit codes let a caller branch on *why* without parsing
            // our output. 64 = bad usage, 66 = missing input, 70 = our bug.
            ExitCode::from(code.exit_code())
        }
    }
}

fn run(args: &Args) -> Result<(), Report> {
    let config = config::load(args)?;

    // `--print-config` must work before telemetry, since a broken telemetry
    // config is one of the things you would use it to diagnose.
    if matches!(args.command, Command::PrintConfig) {
        let rendered =
            pc_config::to_redacted_json(&config).context("could not render configuration")?;
        println!("{rendered}");
        return Ok(());
    }

    let _guard = pc_telemetry::init(&config.telemetry).classify()?;

    match args.command {
        Command::Run => execute(&config),
        Command::PrintConfig => unreachable!("handled above"),
    }
}

fn execute(config: &config::Config) -> Result<(), Report> {
    tracing::info!(workers = config.workers, "starting");

    if config.api_key.is_empty() {
        // Checked without exposing the value; `Invalid` becomes exit code 64.
        return Err(Report::invalid(
            "api_key is not set — put it in config.toml or {{env_prefix}}_API_KEY",
        ));
    }

    tracing::info!("done");
    Ok(())
}
