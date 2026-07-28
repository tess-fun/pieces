use core::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

/// What a [`Secret`] renders as anywhere other than [`Secret::expose`].
pub const REDACTED: &str = "***REDACTED***";

/// A configuration value that must never reach a log, a crash dump, or a
/// `--print-config` listing.
///
/// The protection is structural, not conventional: there is no `Display`, the
/// `Debug` and `Serialize` impls emit [`REDACTED`], and the inner buffer is
/// zeroed on drop. Reading the real value requires typing
/// [`expose`](Secret::expose), which greps well in review.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Wrap a sensitive string.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// Read the underlying value.
    ///
    /// Every call site is a place a secret could escape. Keep them few and
    /// keep them close to the API that needs the value.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty — checkable without exposing it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl Serialize for Secret {
    /// Always emits [`REDACTED`].
    ///
    /// This makes `--print-config` safe by construction. It also means a
    /// round-trip through `serde` destroys the value — which is the intent:
    /// config is loaded, never re-serialized back to disk.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(REDACTED)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, Secret};

    #[test]
    fn debug_does_not_leak() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), REDACTED);
        assert!(!format!("{s:?}").contains("hunter2"));
    }

    #[test]
    fn debug_of_a_containing_struct_does_not_leak() {
        #[derive(Debug)]
        #[expect(dead_code, reason = "the derived Debug output is what's under test")]
        struct Config {
            user: String,
            password: Secret,
        }
        let c = Config {
            user: "root".into(),
            password: Secret::new("hunter2"),
        };
        let rendered = format!("{c:?}");
        assert!(rendered.contains("root"));
        assert!(!rendered.contains("hunter2"), "{rendered}");
    }

    #[test]
    fn serialize_redacts() {
        let json = serde_json::to_string(&Secret::new("hunter2")).unwrap();
        assert_eq!(json, format!("\"{REDACTED}\""));
    }

    #[test]
    fn deserialize_then_expose_round_trips() {
        let s: Secret = serde_json::from_str("\"hunter2\"").unwrap();
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn empty_is_checkable_without_exposing() {
        assert!(Secret::default().is_empty());
        assert!(!Secret::new("x").is_empty());
    }
}
