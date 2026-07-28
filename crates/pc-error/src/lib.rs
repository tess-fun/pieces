//! The shared error vocabulary for the `pieces` stack.
//!
//! One idea: every failure carries a [`Code`], and the code — not the concrete
//! error type — decides how the failure is rendered. A service turns it into an
//! HTTP status, a CLI turns it into an exit status, a retry layer asks it
//! whether to try again. Classify once at the origin; every surface downstream
//! gets it right for free.
//!
//! # Which type do I use?
//!
//! - **In a library**: define a precise `thiserror` enum and implement
//!   [`Coded`] for it. Callers keep the ability to match on specific variants.
//! - **At a boundary** (request handler, CLI command, task runner): convert to
//!   [`Report`]. It erases the concrete type, keeps the cause chain, and is
//!   what you log or render.
//!
//! ```
//! use pc_error::{Code, Coded, Report};
//!
//! #[derive(Debug, thiserror::Error)]
//! enum StoreError {
//!     #[error("no user with id {0}")]
//!     NoSuchUser(u64),
//!     #[error("connection pool exhausted")]
//!     PoolBusy,
//! }
//!
//! impl Coded for StoreError {
//!     fn code(&self) -> Code {
//!         match self {
//!             Self::NoSuchUser(_) => Code::NotFound,
//!             Self::PoolBusy => Code::Exhausted,
//!         }
//!     }
//! }
//!
//! let report = Report::wrap(StoreError::NoSuchUser(42));
//! assert_eq!(report.code(), Code::NotFound);
//! assert_eq!(report.code().exit_code(), 66);
//! assert!(!report.code().is_retryable());
//! ```
//!
//! # Features
//!
//! - `http` — `Code::status()` and `From<Code> for http::StatusCode`.
//! - `serde` — `Code` serializes as its stable `snake_case` name.

mod code;
mod report;

pub use code::Code;
pub use report::{Chain, Coded, Report};

/// `Result` with the error type fixed to [`Report`].
///
/// Use at boundaries. Inside a library, prefer a concrete error type.
pub type Result<T, E = Report> = core::result::Result<T, E>;
