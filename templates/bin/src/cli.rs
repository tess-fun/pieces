//! Command-line surface.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Serialize;

/// {{description}}
#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"), version, about)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Command,

    /// Read configuration from this file instead of searching.
    #[arg(long, short, global = true, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,

    /// Increase log verbosity. Repeat for more: -v, -vv.
    #[arg(long, short, global = true, action = clap::ArgAction::Count)]
    pub(crate) verbose: u8,

    /// Log as JSON regardless of whether stderr is a terminal.
    #[arg(long, global = true)]
    pub(crate) json_logs: bool,

    /// Number of concurrent workers.
    #[arg(long, global = true, value_name = "N")]
    pub(crate) workers: Option<usize>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Do the thing.
    Run,
    /// Print the resolved configuration, with secrets redacted, and exit.
    ///
    /// The first thing to reach for when behaviour differs between machines.
    PrintConfig,
}

/// The subset of flags that override configuration.
///
/// Every field is `Option` and skipped when `None`, so an unset flag leaves the
/// lower layers alone instead of clobbering them with a null.
#[derive(Debug, Serialize)]
pub(crate) struct Overrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    workers: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry: Option<TelemetryOverrides>,
}

#[derive(Debug, Serialize)]
struct TelemetryOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<pc_telemetry::Format>,
}

impl Args {
    /// Project the flags onto the shape of `Config`.
    #[must_use]
    pub(crate) fn overrides(&self) -> Overrides {
        let filter = match self.verbose {
            0 => None,
            1 => Some("debug".to_owned()),
            _ => Some("trace".to_owned()),
        };
        let format = self.json_logs.then_some(pc_telemetry::Format::Json);

        Overrides {
            workers: self.workers,
            telemetry: (filter.is_some() || format.is_some())
                .then_some(TelemetryOverrides { filter, format }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Args, Command};
    use clap::Parser as _;

    fn parse(args: &[&str]) -> Args {
        Args::parse_from(std::iter::once("app").chain(args.iter().copied()))
    }

    #[test]
    fn no_flags_produces_no_overrides() {
        let json = serde_json::to_string(&parse(&["run"]).overrides()).unwrap();
        assert_eq!(json, "{}", "an unset flag must not clobber a config file");
    }

    #[test]
    fn verbosity_maps_to_filter_directives() {
        let one = serde_json::to_value(parse(&["-v", "run"]).overrides()).unwrap();
        assert_eq!(one["telemetry"]["filter"], "debug");

        let two = serde_json::to_value(parse(&["-vv", "run"]).overrides()).unwrap();
        assert_eq!(two["telemetry"]["filter"], "trace");
    }

    #[test]
    fn json_logs_sets_only_the_format() {
        let v = serde_json::to_value(parse(&["--json-logs", "run"]).overrides()).unwrap();
        assert_eq!(v["telemetry"]["format"], "json");
        assert!(v["telemetry"].get("filter").is_none());
    }

    #[test]
    fn workers_flag_overrides_only_workers() {
        let v = serde_json::to_value(parse(&["--workers", "9", "run"]).overrides()).unwrap();
        assert_eq!(v["workers"], 9);
        assert!(v.get("telemetry").is_none());
    }

    #[test]
    fn global_flags_work_after_the_subcommand() {
        let args = parse(&["run", "--verbose", "--workers", "2"]);
        assert!(matches!(args.command, Command::Run));
        assert_eq!(args.workers, Some(2));
    }
}
