//! Layered configuration with a fixed precedence and non-leaking secrets.
//!
//! Two things, both of which you would otherwise rewrite in every project:
//!
//! - [`Loader`] merges defaults, config files, environment variables, and CLI
//!   overrides in one unvarying order, and can tell you which layer a value
//!   came from.
//! - [`Secret`] is a string that has no `Display`, redacts itself in `Debug`
//!   and `Serialize`, and zeroes its buffer on drop — so `--print-config` and
//!   a panic backtrace are both safe by construction.
//!
//! ```
//! use pc_config::{Loader, Secret};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize)]
//! struct Config {
//!     port: u16,
//!     api_key: Secret,
//! }
//!
//! #[derive(Serialize)]
//! struct Defaults {
//!     port: u16,
//!     api_key: &'static str,
//! }
//!
//! # fn main() -> Result<(), pc_config::Error> {
//! let config: Config = Loader::new("my-app")
//!     .defaults(&Defaults { port: 8080, api_key: "" })?
//!     .without_file_search() // doctests should not read the host's config
//!     .without_env()
//!     .load()?;
//!
//! assert_eq!(config.port, 8080);
//! assert_eq!(format!("{:?}", config.api_key), "***REDACTED***");
//! # Ok(())
//! # }
//! ```
//!
//! # Precedence
//!
//! Lowest to highest: defaults, `/etc/<app>/config.toml`, the per-user config
//! dir, `./<app>.toml`, `./config.toml`, an explicit `--config` file, `<APP>_*`
//! environment variables, then CLI overrides. See [`Loader`] for the details.

mod loader;
mod secret;

use std::path::PathBuf;

use pc_error::{Code, Coded};

pub use loader::{Loader, Origin, load, to_redacted_json, user_config_path};
pub use secret::{REDACTED, Secret};

/// Something went wrong resolving configuration.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A file named explicitly by the user does not exist.
    ///
    /// Distinct from a missing *searched* path, which is normal and ignored.
    #[error("configuration file not found: {0}")]
    MissingFile(PathBuf),

    /// A layer was malformed, or the merged value did not match the target
    /// type — a missing required field, a bad enum variant, a string where a
    /// number belongs.
    ///
    /// Boxed: `figment::Error` is over 200 bytes, and every `Result` in this
    /// crate would otherwise carry that on its success path too.
    #[error("invalid configuration")]
    Extract(#[source] Box<figment::Error>),

    /// Defaults or overrides could not be serialized. Always a programmer
    /// error in the caller's own types.
    #[error("could not serialize configuration values")]
    Serialize(#[from] serde_json::Error),
}

impl From<figment::Error> for Error {
    fn from(err: figment::Error) -> Self {
        Self::Extract(Box::new(err))
    }
}

impl Coded for Error {
    fn code(&self) -> Code {
        match self {
            Self::MissingFile(_) => Code::NotFound,
            Self::Extract(_) => Code::Invalid,
            Self::Serialize(_) => Code::Internal,
        }
    }
}
