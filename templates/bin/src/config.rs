//! Application configuration.
//!
//! Everything the program needs to run, in one place, loaded from layered
//! sources. Add fields here rather than threading new CLI flags through call
//! sites — a flag is one more way to configure something, a config field is
//! the only way.

use pc_config::{Loader, Secret};
use pc_error::{Report, ResultExt as _};
use serde::{Deserialize, Serialize};

use crate::cli::Args;

/// The resolved configuration.
#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    /// How much work to do at once.
    pub(crate) workers: usize,
    /// Credential for the upstream service. Redacts itself everywhere.
    pub(crate) api_key: Secret,
    /// Logging and, later, tracing export.
    pub(crate) telemetry: pc_telemetry::Config,
}

/// Values used when nothing else supplies them.
///
/// A separate `Serialize` struct rather than `impl Default for Config`: it
/// keeps `Config`'s fields non-`Option` (so the rest of the program never
/// unwraps a maybe-configured value) while still allowing a partial file.
#[derive(Serialize)]
struct Defaults {
    workers: usize,
    api_key: &'static str,
    telemetry: pc_telemetry::Config,
}

impl Defaults {
    fn new() -> Self {
        Self {
            workers: 4,
            api_key: "",
            telemetry: pc_telemetry::Config::new(env!("CARGO_PKG_NAME")),
        }
    }
}

/// Resolve configuration from defaults, files, `{{env_prefix}}_*`, then CLI flags.
///
/// # Errors
/// If a named config file is missing, a file is malformed, or the merged
/// result does not satisfy [`Config`].
pub(crate) fn load(args: &Args) -> Result<Config, Report> {
    let mut loader = Loader::new(env!("CARGO_PKG_NAME"))
        .env_prefix("{{env_prefix}}_")
        .defaults(&Defaults::new())
        .context("invalid built-in defaults")?
        .overrides(&args.overrides())
        .context("could not apply command-line overrides")?;

    if let Some(path) = &args.config {
        loader = loader.file(path);
    }

    loader.load().context("could not load configuration")
}
