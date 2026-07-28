use core::fmt;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Error;

/// A configuration layer that contributed to the final value.
///
/// Returned by [`Loader::load_with_origins`]. Wire it into `--print-config`;
/// "which file did that setting actually come from" is the question you will
/// ask most often when something is misconfigured in production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Values supplied by [`Loader::defaults`].
    Defaults,
    /// A TOML file that existed and was read.
    File(PathBuf),
    /// Environment variables under the given prefix.
    Env(String),
    /// Values supplied by [`Loader::overrides`], typically from CLI flags.
    Overrides,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Defaults => f.write_str("defaults"),
            Self::File(p) => write!(f, "file {}", p.display()),
            Self::Env(prefix) => write!(f, "env {prefix}*"),
            Self::Overrides => f.write_str("overrides"),
        }
    }
}

/// Builds a configuration value from layered sources.
///
/// Precedence, lowest to highest:
///
/// 1. [`defaults`](Loader::defaults)
/// 2. `/etc/<app>/config.toml` (unix)
/// 3. the per-user config dir, e.g. `~/.config/<app>/config.toml`
/// 4. `./<app>.toml`, then `./config.toml`
/// 5. an explicit file from [`file`](Loader::file) or `<PREFIX>CONFIG`
/// 6. environment variables under `<PREFIX>`
/// 7. [`overrides`](Loader::overrides)
///
/// The order is fixed on purpose. Per-project precedence rules are a class of
/// bug that only shows up in the environment you cannot debug.
#[derive(Clone)]
pub struct Loader {
    app: String,
    env_prefix: String,
    defaults: Option<serde_json::Value>,
    overrides: Option<serde_json::Value>,
    explicit_file: Option<PathBuf>,
    search_files: bool,
    read_env: bool,
}

impl fmt::Debug for Loader {
    /// Deliberately omits `defaults` and `overrides` — they can hold secrets.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Loader")
            .field("app", &self.app)
            .field("env_prefix", &self.env_prefix)
            .field("explicit_file", &self.explicit_file)
            .field("search_files", &self.search_files)
            .field("read_env", &self.read_env)
            .finish_non_exhaustive()
    }
}

impl Loader {
    /// Start a loader for an application name, e.g. `"my-app"`.
    ///
    /// The environment prefix is derived by uppercasing and replacing
    /// non-alphanumeric characters: `"my-app"` reads `MY_APP_*`.
    #[must_use]
    pub fn new(app: impl Into<String>) -> Self {
        let app = app.into();
        let env_prefix = derive_env_prefix(&app);
        Self {
            app,
            env_prefix,
            defaults: None,
            overrides: None,
            explicit_file: None,
            search_files: true,
            read_env: true,
        }
    }

    /// Override the derived environment prefix. Include the trailing
    /// underscore: `"MYAPP_"`.
    #[must_use]
    pub fn env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = prefix.into();
        self
    }

    /// Base values, overridden by every other layer.
    ///
    /// # Errors
    /// If `values` cannot be serialized.
    pub fn defaults(mut self, values: &impl Serialize) -> Result<Self, Error> {
        self.defaults = Some(serde_json::to_value(values)?);
        Ok(self)
    }

    /// Top-priority values, typically parsed CLI flags.
    ///
    /// Serialize a struct whose unset fields are `None` and skipped, or these
    /// will clobber lower layers with nulls.
    ///
    /// # Errors
    /// If `values` cannot be serialized.
    pub fn overrides(mut self, values: &impl Serialize) -> Result<Self, Error> {
        self.overrides = Some(serde_json::to_value(values)?);
        Ok(self)
    }

    /// Read this specific file, above all other files.
    ///
    /// Unlike the searched paths, a missing explicit file is an error — the
    /// user asked for it by name.
    #[must_use]
    pub fn file(mut self, path: impl Into<PathBuf>) -> Self {
        self.explicit_file = Some(path.into());
        self
    }

    /// Skip the system, user, and working-directory file search.
    #[must_use]
    pub const fn without_file_search(mut self) -> Self {
        self.search_files = false;
        self
    }

    /// Skip the environment layer. Useful in tests, which share a process
    /// environment and cannot safely mutate it in parallel.
    #[must_use]
    pub const fn without_env(mut self) -> Self {
        self.read_env = false;
        self
    }

    /// Resolve the configuration.
    ///
    /// # Errors
    /// If an explicit file is missing, a file is malformed, or the merged
    /// value does not match `T`.
    pub fn load<T: DeserializeOwned>(self) -> Result<T, Error> {
        self.load_with_origins().map(|(value, _)| value)
    }

    /// Resolve the configuration and report which layers contributed.
    ///
    /// # Errors
    /// As [`load`](Loader::load).
    pub fn load_with_origins<T: DeserializeOwned>(self) -> Result<(T, Vec<Origin>), Error> {
        let mut figment = Figment::new();
        let mut origins = Vec::new();

        if let Some(defaults) = &self.defaults {
            figment = figment.merge(Serialized::defaults(defaults));
            origins.push(Origin::Defaults);
        }

        if self.search_files {
            for path in self.search_paths() {
                if path.is_file() {
                    figment = figment.merge(Toml::file(&path));
                    origins.push(Origin::File(path));
                }
            }
        }

        if let Some(path) = self.explicit_path()? {
            figment = figment.merge(Toml::file(&path));
            origins.push(Origin::File(path));
        }

        if self.read_env {
            // `__` nests: `MY_APP_DB__PORT` sets `db.port`.
            figment = figment.merge(Env::prefixed(&self.env_prefix).split("__"));
            origins.push(Origin::Env(self.env_prefix.clone()));
        }

        if let Some(overrides) = &self.overrides {
            figment = figment.merge(Serialized::defaults(overrides));
            origins.push(Origin::Overrides);
        }

        Ok((figment.extract()?, origins))
    }

    /// The optional file locations, lowest priority first.
    fn search_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if cfg!(unix) {
            paths.push(PathBuf::from("/etc").join(&self.app).join("config.toml"));
        }

        if let Some(dirs) = directories::ProjectDirs::from("", "", &self.app) {
            paths.push(dirs.config_dir().join("config.toml"));
        }

        paths.push(PathBuf::from(format!("{}.toml", self.app)));
        paths.push(PathBuf::from("config.toml"));
        paths
    }

    /// The explicit file, from the builder or `<PREFIX>CONFIG`.
    fn explicit_path(&self) -> Result<Option<PathBuf>, Error> {
        let from_env = if self.read_env {
            std::env::var(format!("{}CONFIG", self.env_prefix))
                .ok()
                .map(PathBuf::from)
        } else {
            None
        };

        let Some(path) = self.explicit_file.clone().or(from_env) else {
            return Ok(None);
        };

        if path.is_file() {
            Ok(Some(path))
        } else {
            Err(Error::MissingFile(path))
        }
    }
}

/// `"my-app"` -> `"MY_APP_"`.
fn derive_env_prefix(app: &str) -> String {
    let mut out: String = app
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    out.push('_');
    out
}

/// Load configuration for `app` using the default layering.
///
/// Shorthand for `Loader::new(app).load()`.
///
/// # Errors
/// As [`Loader::load`].
pub fn load<T: DeserializeOwned>(app: &str) -> Result<T, Error> {
    Loader::new(app).load()
}

/// Serialize a configuration value for display, with [`Secret`] fields
/// already redacted by their own `Serialize` impl.
///
/// This is the whole implementation of `--print-config`.
///
/// [`Secret`]: crate::Secret
///
/// # Errors
/// If `config` cannot be serialized.
pub fn to_redacted_json(config: &impl Serialize) -> Result<String, Error> {
    Ok(serde_json::to_string_pretty(config)?)
}

/// Path helper for tests and tooling: the per-user config file for `app`.
#[must_use]
pub fn user_config_path(app: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", app).map(|d| d.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::{Loader, Origin, derive_env_prefix};
    use crate::{Error, Secret};
    use serde::{Deserialize, Serialize};
    use std::io::Write as _;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Db {
        host: String,
        port: u16,
        password: Secret,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Config {
        name: String,
        db: Db,
    }

    #[derive(Serialize)]
    struct Defaults {
        name: &'static str,
        db: DbDefaults,
    }

    #[derive(Serialize)]
    struct DbDefaults {
        host: &'static str,
        port: u16,
        password: &'static str,
    }

    fn defaults() -> Defaults {
        Defaults {
            name: "unnamed",
            db: DbDefaults {
                host: "localhost",
                port: 5432,
                password: "",
            },
        }
    }

    /// A loader that touches neither the real filesystem search paths nor the
    /// process environment, so tests stay parallel-safe.
    fn isolated() -> Loader {
        Loader::new("pc-config-test")
            .without_file_search()
            .without_env()
    }

    fn write_toml(body: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn env_prefix_is_derived_from_the_app_name() {
        assert_eq!(derive_env_prefix("my-app"), "MY_APP_");
        assert_eq!(derive_env_prefix("pieces"), "PIECES_");
        assert_eq!(derive_env_prefix("a.b c"), "A_B_C_");
    }

    #[test]
    fn defaults_alone_produce_a_complete_value() {
        let cfg: Config = isolated().defaults(&defaults()).unwrap().load().unwrap();
        assert_eq!(cfg.name, "unnamed");
        assert_eq!(cfg.db.port, 5432);
    }

    #[test]
    fn a_file_overrides_defaults_field_by_field() {
        let file = write_toml("name = \"prod\"\n[db]\nport = 6000\n");
        let cfg: Config = isolated()
            .defaults(&defaults())
            .unwrap()
            .file(file.path())
            .load()
            .unwrap();

        assert_eq!(cfg.name, "prod", "file wins over defaults");
        assert_eq!(cfg.db.port, 6000, "nested file value wins");
        assert_eq!(cfg.db.host, "localhost", "unset nested field keeps default");
    }

    #[test]
    fn overrides_beat_files() {
        #[derive(Serialize)]
        struct Cli {
            name: &'static str,
        }
        let file = write_toml("name = \"from-file\"\n");
        let cfg: Config = isolated()
            .defaults(&defaults())
            .unwrap()
            .file(file.path())
            .overrides(&Cli { name: "from-cli" })
            .unwrap()
            .load()
            .unwrap();
        assert_eq!(cfg.name, "from-cli");
    }

    #[test]
    fn a_named_but_missing_file_is_an_error() {
        let err = isolated()
            .defaults(&defaults())
            .unwrap()
            .file("/definitely/not/here.toml")
            .load::<Config>()
            .unwrap_err();
        assert!(matches!(err, Error::MissingFile(_)), "{err:?}");
    }

    #[test]
    fn a_missing_required_field_is_an_error() {
        let err = isolated().load::<Config>().unwrap_err();
        assert!(matches!(err, Error::Extract(_)), "{err:?}");
    }

    #[test]
    fn origins_list_only_the_layers_that_contributed() {
        let file = write_toml("name = \"x\"\n");
        let (_, origins) = isolated()
            .defaults(&defaults())
            .unwrap()
            .file(file.path())
            .load_with_origins::<Config>()
            .unwrap();

        assert_eq!(origins.len(), 2, "{origins:?}");
        assert_eq!(origins[0], Origin::Defaults);
        assert!(matches!(origins[1], Origin::File(_)));
    }

    #[test]
    fn secrets_load_from_file_and_redact_on_print() {
        let file = write_toml("name = \"x\"\n[db]\npassword = \"hunter2\"\n");
        let cfg: Config = isolated()
            .defaults(&defaults())
            .unwrap()
            .file(file.path())
            .load()
            .unwrap();

        assert_eq!(cfg.db.password.expose(), "hunter2");

        let printed = super::to_redacted_json(&cfg).unwrap();
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(printed.contains(crate::REDACTED));
    }
}
