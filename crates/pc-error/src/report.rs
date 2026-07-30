use core::fmt;
use std::borrow::Cow;
use std::error::Error as StdError;

use crate::Code;

type Source = Box<dyn StdError + Send + Sync + 'static>;

/// Anything that classifies itself into the shared [`Code`] vocabulary.
///
/// Implement this on every error enum a `pieces` crate exposes. It is the
/// contract that lets a service map an arbitrary error onto a status code
/// without knowing what the error actually is.
pub trait Coded {
    /// This value's classification.
    fn code(&self) -> Code;
}

impl Coded for Code {
    fn code(&self) -> Code {
        *self
    }
}

/// A classified failure: a [`Code`], a human-readable message, and an
/// optional cause chain.
///
/// `Report` is the *boundary* type. Libraries should expose precise
/// `thiserror` enums that implement [`Coded`]; convert to `Report` only where
/// the error crosses into a request handler, a CLI command, or a log line.
#[derive(Debug)]
pub struct Report {
    code: Code,
    /// `None` means "defer to the source's own `Display`" — see
    /// [`Report::wrap`], which adds classification without restating the
    /// message and producing `"x: x"` in the chain.
    message: Option<Cow<'static, str>>,
    source: Option<Source>,
}

impl Report {
    /// Construct a report with an explicit code and message.
    pub fn new(code: Code, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            code,
            message: Some(message.into()),
            source: None,
        }
    }

    /// Lift a [`Coded`] error into a `Report`, adopting its code and keeping
    /// it as the cause.
    ///
    /// Adds no message of its own — the error already has one.
    pub fn wrap<E>(err: E) -> Self
    where
        E: Coded + StdError + Send + Sync + 'static,
    {
        Self {
            code: err.code(),
            message: None,
            source: Some(Box::new(err)),
        }
    }

    /// Attach (or replace) the underlying cause.
    #[must_use]
    pub fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Override this report's classification.
    #[must_use]
    pub const fn with_code(mut self, code: Code) -> Self {
        self.code = code;
        self
    }

    /// This failure's classification.
    #[must_use]
    pub const fn code(&self) -> Code {
        self.code
    }

    /// The top-level message, without the cause chain.
    ///
    /// Falls back to the source's own message, then to the code name, so this
    /// never returns an empty string.
    #[must_use]
    pub fn message(&self) -> Cow<'_, str> {
        match (&self.message, &self.source) {
            (Some(m), _) => Cow::Borrowed(m.as_ref()),
            (None, Some(src)) => Cow::Owned(src.to_string()),
            (None, None) => Cow::Borrowed(self.code.as_str()),
        }
    }

    /// Iterate this report and every cause beneath it, outermost first.
    #[must_use]
    pub fn chain(&self) -> Chain<'_> {
        Chain {
            next: Some(self as &(dyn StdError + 'static)),
        }
    }

    /// Render the full chain on one line: `"outer: middle: root"`.
    ///
    /// This is what a CLI should print and what a service should log. The
    /// plain [`Display`](fmt::Display) impl shows only the top message, so it
    /// stays safe to put in an API response body.
    #[must_use]
    pub fn chained(&self) -> String {
        let mut out = String::new();
        let mut last: Option<String> = None;
        for err in self.chain() {
            let segment = err.to_string();
            // `wrap` leaves the top message empty and defers to the source, so
            // the first two links render identically. Collapse the repeat.
            if last.as_ref() == Some(&segment) {
                continue;
            }
            if last.is_some() {
                out.push_str(": ");
            }
            out.push_str(&segment);
            last = Some(segment);
        }
        out
    }
}

macro_rules! constructors {
    ($($(#[$m:meta])* $name:ident => $code:ident),* $(,)?) => {
        impl Report {
            $(
                $(#[$m])*
                pub fn $name(message: impl Into<Cow<'static, str>>) -> Self {
                    Self::new(Code::$code, message)
                }
            )*
        }
    };
}

constructors! {
    /// Input was malformed, missing, or failed validation.
    invalid => Invalid,
    /// No credentials were supplied, or they could not be verified.
    unauthenticated => Unauthenticated,
    /// Credentials were valid, but the caller may not do this.
    forbidden => Forbidden,
    /// The addressed resource does not exist.
    not_found => NotFound,
    /// The operation conflicts with current state.
    conflict => Conflict,
    /// A quota, rate limit, or resource ceiling was reached.
    exhausted => Exhausted,
    /// A dependency is down or refusing work.
    unavailable => Unavailable,
    /// The operation did not finish within its deadline.
    timeout => Timeout,
    /// A bug, a broken invariant, or an otherwise unexpected failure.
    internal => Internal,
}

impl Coded for Report {
    fn code(&self) -> Code {
        self.code
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl StdError for Report {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|s| s as &(dyn StdError + 'static))
    }
}

/// Iterator over an error and its transitive causes. See [`Report::chain`].
#[derive(Debug, Clone)]
pub struct Chain<'a> {
    next: Option<&'a (dyn StdError + 'static)>,
}

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn StdError + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = current.source();
        Some(current)
    }
}

impl std::iter::FusedIterator for Chain<'_> {}

/// Turn any [`Coded`] error into a [`Report`] at a `?` boundary.
///
/// A blanket `impl<E: Coded> From<E> for Report` would be nicer, but it
/// collides with the reflexive `From<Report> for Report` in core. An extension
/// trait gets the same ergonomics without the coherence fight.
pub trait ResultExt<T> {
    /// Adopt the error's own classification and message.
    ///
    /// ```
    /// # use pc_error::{Code, Coded, Report, ResultExt as _};
    /// # #[derive(Debug, thiserror::Error)] #[error("nope")] struct E;
    /// # impl Coded for E { fn code(&self) -> Code { Code::Forbidden } }
    /// let r: Result<(), Report> = Err(E).classify();
    /// assert_eq!(r.unwrap_err().code(), Code::Forbidden);
    /// ```
    ///
    /// # Errors
    /// Propagates the receiver's error, reclassified.
    fn classify(self) -> Result<T, Report>;

    /// Adopt the classification but prepend your own message, keeping the
    /// original as the cause.
    ///
    /// Use when the error alone would not tell you *what you were doing* —
    /// "no such file" is far less useful than "could not load config: no such
    /// file".
    ///
    /// # Errors
    /// Propagates the receiver's error, reclassified and annotated.
    fn context(self, message: impl Into<Cow<'static, str>>) -> Result<T, Report>;
}

impl<T, E> ResultExt<T> for core::result::Result<T, E>
where
    E: Coded + StdError + Send + Sync + 'static,
{
    fn classify(self) -> Result<T, Report> {
        self.map_err(Report::wrap)
    }

    fn context(self, message: impl Into<Cow<'static, str>>) -> Result<T, Report> {
        self.map_err(|err| Report::new(err.code(), message).with_source(err))
    }
}

#[cfg(test)]
mod tests {
    use super::{Coded, Report, ResultExt as _};
    use crate::Code;

    #[derive(Debug, thiserror::Error)]
    #[error("row {0} is missing")]
    struct RowMissing(u32);

    impl Coded for RowMissing {
        fn code(&self) -> Code {
            Code::NotFound
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("disk on fire")]
    struct DiskOnFire;

    #[test]
    fn display_shows_only_the_top_message() {
        let report = Report::internal("could not load user").with_source(RowMissing(7));
        assert_eq!(report.to_string(), "could not load user");
    }

    #[test]
    fn chained_joins_the_whole_cause_chain() {
        let report = Report::internal("could not load user").with_source(RowMissing(7));
        assert_eq!(report.chained(), "could not load user: row 7 is missing");
    }

    #[test]
    fn wrap_adopts_the_inner_code_without_duplicating_the_message() {
        let report = Report::wrap(RowMissing(3));
        assert_eq!(report.code(), Code::NotFound);
        assert_eq!(report.to_string(), "row 3 is missing");
        assert_eq!(report.chained(), "row 3 is missing");
    }

    #[test]
    fn message_never_returns_empty() {
        assert_eq!(Report::wrap(RowMissing(1)).message(), "row 1 is missing");
        assert_eq!(Report::internal("boom").message(), "boom");
        assert_eq!(
            Report::new(Code::Timeout, String::new())
                .with_source(DiskOnFire)
                .chained(),
            ": disk on fire",
            "an explicitly empty message is the caller's choice, not a fallback"
        );
    }

    #[test]
    fn chain_length_matches_nesting_depth() {
        let bare = Report::invalid("nope");
        assert_eq!(bare.chain().count(), 1);
        assert_eq!(bare.with_source(RowMissing(1)).chain().count(), 2);
    }

    #[test]
    fn constructors_set_the_matching_code() {
        assert_eq!(Report::not_found("x").code(), Code::NotFound);
        assert_eq!(Report::forbidden("x").code(), Code::Forbidden);
        assert_eq!(Report::timeout("x").code(), Code::Timeout);
    }

    #[test]
    fn classify_adopts_code_and_message() {
        let r: Result<(), Report> = Err(RowMissing(4)).classify();
        let err = r.unwrap_err();
        assert_eq!(err.code(), Code::NotFound);
        assert_eq!(err.chained(), "row 4 is missing");
    }

    #[test]
    fn context_prepends_while_keeping_the_code_and_cause() {
        let r: Result<(), Report> = Err(RowMissing(4)).context("could not load user");
        let err = r.unwrap_err();
        assert_eq!(err.code(), Code::NotFound, "context must not reclassify");
        assert_eq!(err.chained(), "could not load user: row 4 is missing");
    }

    #[test]
    fn ok_passes_through_both_adapters() {
        let a: Result<u8, Report> = Ok::<u8, RowMissing>(1).classify();
        let b: Result<u8, Report> = Ok::<u8, RowMissing>(2).context("x");
        assert_eq!(a.unwrap(), 1);
        assert_eq!(b.unwrap(), 2);
    }

    #[test]
    fn with_code_overrides_a_wrapped_classification() {
        let report = Report::wrap(RowMissing(9)).with_code(Code::Internal);
        assert_eq!(report.code(), Code::Internal);
    }
}
