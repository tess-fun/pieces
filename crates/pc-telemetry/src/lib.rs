//! One-line observability setup for every binary in the stack.
//!
//! ```no_run
//! # fn main() -> Result<(), pc_telemetry::Error> {
//! let _guard = pc_telemetry::init(&pc_telemetry::Config::new("my-service"))?;
//! tracing::info!(port = 8080, "listening");
//! # Ok(())
//! # }
//! ```
//!
//! What that one line buys you:
//!
//! - **Format chosen by destination.** A terminal gets human-readable output;
//!   a pipe or a container log gets JSON. No `--log-format` flag to forget in
//!   your deployment manifest.
//! - **`RUST_LOG` always wins**, so you can raise the level on a running
//!   deployment without a code change.
//! - **Logs go to stderr.** stdout stays clean for a CLI's actual output, so
//!   `my-tool | jq` keeps working with logging on.
//! - **Panics become log events** before the process dies, with location and
//!   payload, in the same format as everything else. An unhooked panic writes
//!   raw text to stderr that your log aggregator will not parse.
//!
//! # The guard
//!
//! Hold [`Guard`] for the lifetime of the process — binding it to `_` instead
//! of `_guard` drops it immediately and, once OTLP export lands, silently
//! discards your last spans.

use core::fmt;
use std::io::IsTerminal as _;

use pc_error::{Code, Coded};
use serde::{Deserialize, Serialize};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer as _, fmt as tsfmt, registry};

/// How log events are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Human-readable on a terminal, JSON otherwise. Almost always right.
    #[default]
    Auto,
    /// Multi-line, colored, one field per line. Best for local debugging.
    Pretty,
    /// Single-line and dense. Best for a terminal you are watching.
    Compact,
    /// Newline-delimited JSON. Required for structured log ingestion.
    Json,
}

impl Format {
    /// Resolve [`Format::Auto`] against the actual destination.
    fn resolve(self) -> Self {
        match self {
            Self::Auto if std::io::stderr().is_terminal() => Self::Pretty,
            Self::Auto => Self::Json,
            other => other,
        }
    }
}

/// Telemetry settings.
///
/// Derives `Deserialize`, so it can be a field in your application's own
/// config struct and be set from a file or environment variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Identifies this process in logs and, later, in traces.
    pub service_name: String,
    /// `tracing` filter directives, e.g. `"info,my_crate=debug"`.
    ///
    /// `RUST_LOG` overrides this when set — see [`Config::filter`].
    pub filter: String,
    /// Output rendering.
    pub format: Format,
    /// Include the emitting module path on each event.
    pub with_target: bool,
    /// Force ANSI color on or off. `None` follows the terminal.
    pub ansi: Option<bool>,
    /// Log panics through `tracing` before the process unwinds.
    pub capture_panics: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            service_name: env!("CARGO_PKG_NAME").to_owned(),
            filter: "info".to_owned(),
            format: Format::Auto,
            with_target: true,
            ansi: None,
            capture_panics: true,
        }
    }
}

impl Config {
    /// Settings for a named service, everything else defaulted.
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Self::default()
        }
    }

    /// Set the fallback filter directives, used when `RUST_LOG` is unset.
    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self
    }

    /// Force a specific output format.
    #[must_use]
    pub const fn with_format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Build the filter, letting `RUST_LOG` take precedence.
    ///
    /// Precedence is deliberate and one-directional: an operator must always
    /// be able to turn logging up on a running deployment without a release.
    fn env_filter(&self) -> Result<EnvFilter, Error> {
        match std::env::var(EnvFilter::DEFAULT_ENV) {
            Ok(raw) if !raw.trim().is_empty() => Ok(EnvFilter::try_new(raw)?),
            _ => Ok(EnvFilter::try_new(&self.filter)?),
        }
    }
}

/// Keeps telemetry alive. Drop it and exporters shut down.
///
/// Today the base build has nothing to flush, so dropping early is harmless.
/// That will stop being true the moment OTLP export is enabled, so treat the
/// guard as load-bearing from the start: bind it in `main` and let it live to
/// the end of the process.
pub struct Guard {
    shutdown: Option<Box<dyn FnOnce() + Send>>,
}

impl fmt::Debug for Guard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Guard")
            .field("has_shutdown_hook", &self.shutdown.is_some())
            .finish()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown();
        }
    }
}

/// Something went wrong installing telemetry.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The filter directives in `RUST_LOG` or [`Config::filter`] are malformed.
    #[error("invalid log filter directives")]
    Filter(#[from] tracing_subscriber::filter::ParseError),

    /// A global subscriber is already installed.
    ///
    /// Usually means `init` was called twice — in a test harness, or in a
    /// library that should not be initializing telemetry at all.
    #[error("a global tracing subscriber is already installed")]
    AlreadyInitialized,
}

impl Coded for Error {
    fn code(&self) -> Code {
        match self {
            Self::Filter(_) => Code::Invalid,
            Self::AlreadyInitialized => Code::Conflict,
        }
    }
}

/// Install the global tracing subscriber.
///
/// Call exactly once, as early in `main` as possible — events emitted before
/// this are dropped.
///
/// # Errors
/// If the filter directives are malformed, or a subscriber is already
/// installed.
pub fn init(config: &Config) -> Result<Guard, Error> {
    let filter = config.env_filter()?;
    let ansi = config
        .ansi
        .unwrap_or_else(|| std::io::stderr().is_terminal());

    // Logs go to stderr so that a CLI's stdout stays machine-readable.
    let layer = match config.format.resolve() {
        Format::Json => tsfmt::layer()
            .json()
            .with_current_span(true)
            .with_target(config.with_target)
            .with_writer(std::io::stderr)
            .boxed(),
        Format::Compact => tsfmt::layer()
            .compact()
            .with_ansi(ansi)
            .with_target(config.with_target)
            .with_writer(std::io::stderr)
            .boxed(),
        // `Auto` is resolved above; `Pretty` is the remaining case.
        Format::Pretty | Format::Auto => tsfmt::layer()
            .pretty()
            .with_ansi(ansi)
            .with_target(config.with_target)
            .with_writer(std::io::stderr)
            .boxed(),
    };

    registry()
        .with(filter)
        .with(layer)
        .try_init()
        .map_err(|_| Error::AlreadyInitialized)?;

    if config.capture_panics {
        install_panic_hook();
    }

    tracing::debug!(
        service.name = %config.service_name,
        "telemetry initialized"
    );

    Ok(Guard { shutdown: None })
}

/// Route panics through `tracing` before deferring to the previous hook.
///
/// Chaining rather than replacing matters: the default hook is what prints the
/// backtrace, and a test harness installs its own.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map_or_else(|| "unknown".to_owned(), ToString::to_string);

        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");

        tracing::error!(
            panic.location = %location,
            panic.message = %message,
            "process panicked"
        );

        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::{Config, Format};

    #[test]
    fn auto_never_survives_resolution() {
        assert_ne!(Format::Auto.resolve(), Format::Auto);
    }

    #[test]
    fn auto_picks_json_when_stderr_is_not_a_terminal() {
        // `cargo test` captures stderr through a pipe, so this exercises the
        // container/CI path — exactly the one that must produce JSON.
        assert_eq!(Format::Auto.resolve(), Format::Json);
    }

    #[test]
    fn explicit_formats_are_left_alone() {
        for f in [Format::Pretty, Format::Compact, Format::Json] {
            assert_eq!(f.resolve(), f);
        }
    }

    #[test]
    fn config_default_filter_parses() {
        assert!(Config::default().env_filter().is_ok());
    }

    #[test]
    fn a_malformed_filter_is_rejected() {
        let config = Config::new("t").with_filter("this is not=a=filter=at=all");
        assert!(config.env_filter().is_err());
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = Config::new("svc").with_format(Format::Json);
        let json = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn config_deserializes_from_a_partial_document() {
        // `#[serde(default)]` means an app can set one field and inherit the
        // rest — important when this is nested in a larger config struct.
        let config: Config = serde_json::from_str(r#"{"format":"compact"}"#).unwrap();
        assert_eq!(config.format, Format::Compact);
        assert_eq!(config.filter, "info");
    }
}
