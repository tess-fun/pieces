//! {{description}}
//!
//! # Error convention
//!
//! This crate exposes a precise [`Error`] enum rather than a boxed or erased
//! error type, so callers can match on specific variants. It implements
//! [`Coded`], which is what lets a service or CLI at the boundary turn it into
//! an HTTP status or an exit code without knowing anything about this crate.
//!
//! Do not convert to `pc_error::Report` in here — that is the caller's job, at
//! the point where the error stops being actionable.

use pc_error::{Code, Coded};

/// Something this crate could not do.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The supplied name was empty or malformed.
    #[error("{0:?} is not a usable name")]
    BadName(String),

    /// Nothing is registered under that name.
    #[error("no entry named {0:?}")]
    NotFound(String),
}

impl Coded for Error {
    /// Map each variant onto the shared vocabulary.
    ///
    /// Keep this exhaustive rather than using a `_` arm: a new variant should
    /// fail to compile until someone decides how it is classified, because the
    /// wrong default here silently becomes a 500 or a misleading exit code.
    fn code(&self) -> Code {
        match self {
            Self::BadName(_) => Code::Invalid,
            Self::NotFound(_) => Code::NotFound,
        }
    }
}

/// `Result` with this crate's error type.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Normalize and validate a name.
///
/// # Errors
/// [`Error::BadName`] if the name is empty once trimmed.
pub fn normalize(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::BadName(name.to_owned()));
    }
    tracing::debug!(name = trimmed, "normalized");
    Ok(trimmed.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{Error, normalize};
    use pc_error::{Code, Coded as _};

    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(normalize("  Widget ").unwrap(), "widget");
    }

    #[test]
    fn an_empty_name_is_invalid() {
        let err = normalize("   ").unwrap_err();
        assert!(matches!(err, Error::BadName(_)));
        assert_eq!(err.code(), Code::Invalid);
    }

    #[test]
    fn codes_map_to_the_right_side_of_the_client_server_split() {
        assert!(Error::BadName(String::new()).code().is_client_fault());
        assert!(Error::NotFound(String::new()).code().is_client_fault());
    }
}
