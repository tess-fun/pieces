use core::fmt;

/// A coarse, stable classification of a failure.
///
/// Every error in the stack maps to exactly one `Code`. The code — not the
/// concrete error type — decides the HTTP status a service returns, the exit
/// status a CLI reports, and whether a caller should retry. Classify the
/// failure once at its origin and every surface renders it correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Code {
    /// Input was malformed, missing, or failed validation.
    Invalid,
    /// No credentials were supplied, or they could not be verified.
    Unauthenticated,
    /// Credentials were valid, but the caller may not do this.
    Forbidden,
    /// The addressed resource does not exist.
    NotFound,
    /// The operation conflicts with current state.
    Conflict,
    /// A quota, rate limit, or resource ceiling was reached.
    Exhausted,
    /// A dependency is down or refusing work.
    Unavailable,
    /// The operation did not finish within its deadline.
    Timeout,
    /// A bug, a broken invariant, or an otherwise unexpected failure.
    Internal,
}

impl Code {
    /// Every variant, in declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Invalid,
        Self::Unauthenticated,
        Self::Forbidden,
        Self::NotFound,
        Self::Conflict,
        Self::Exhausted,
        Self::Unavailable,
        Self::Timeout,
        Self::Internal,
    ];

    /// The stable machine-readable name.
    ///
    /// This string is part of your public API the moment it reaches a log
    /// aggregator or an API client. Do not rename variants casually.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Exhausted => "exhausted",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Internal => "internal",
        }
    }

    /// Parse a [`Code::as_str`] value back into a `Code`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.as_str() == s)
    }

    /// Whether retrying the identical operation could plausibly succeed.
    ///
    /// Drives backoff policy. Deliberately excludes [`Code::Internal`]: an
    /// unexpected failure retried blindly is how you turn one bug into an
    /// outage.
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::Exhausted | Self::Unavailable | Self::Timeout)
    }

    /// Whether the caller caused this.
    ///
    /// Client faults are expected traffic, not incidents — log them at `INFO`
    /// or `WARN` and keep them out of your alerting.
    #[must_use]
    pub const fn is_client_fault(self) -> bool {
        matches!(
            self,
            Self::Invalid
                | Self::Unauthenticated
                | Self::Forbidden
                | Self::NotFound
                | Self::Conflict
                | Self::Exhausted
        )
    }

    /// Process exit status, following `sysexits.h` where a code applies.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Invalid => 64,                                       // EX_USAGE
            Self::Conflict => 65,                                      // EX_DATAERR
            Self::NotFound => 66,                                      // EX_NOINPUT
            Self::Unavailable | Self::Timeout | Self::Exhausted => 69, // EX_UNAVAILABLE
            Self::Internal => 70,                                      // EX_SOFTWARE
            Self::Unauthenticated | Self::Forbidden => 77,             // EX_NOPERM
        }
    }

    /// The HTTP status this failure should be rendered as.
    #[cfg(feature = "http")]
    #[must_use]
    pub const fn status(self) -> http::StatusCode {
        match self {
            Self::Invalid => http::StatusCode::BAD_REQUEST,
            Self::Unauthenticated => http::StatusCode::UNAUTHORIZED,
            Self::Forbidden => http::StatusCode::FORBIDDEN,
            Self::NotFound => http::StatusCode::NOT_FOUND,
            Self::Conflict => http::StatusCode::CONFLICT,
            Self::Exhausted => http::StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => http::StatusCode::GATEWAY_TIMEOUT,
            Self::Internal => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "http")]
impl From<Code> for http::StatusCode {
    fn from(code: Code) -> Self {
        code.status()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Code {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Code {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // `String` rather than `&str`: non-borrowing formats (readers, msgpack)
        // cannot hand out a borrowed str, and this type must work in all of them.
        let raw = <String as serde::Deserialize>::deserialize(d)?;
        Self::parse(&raw)
            .ok_or_else(|| serde::de::Error::unknown_variant(&raw, &["invalid", "..."]))
    }
}

#[cfg(test)]
mod tests {
    use super::Code;

    #[test]
    fn as_str_round_trips_for_every_variant() {
        for &code in Code::ALL {
            assert_eq!(Code::parse(code.as_str()), Some(code), "{code}");
        }
    }

    #[test]
    fn all_contains_no_duplicates() {
        let mut seen = Code::ALL.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), Code::ALL.len());
    }

    #[test]
    fn internal_is_not_retryable() {
        // Retrying an unexpected failure is how one bug becomes an outage.
        assert!(!Code::Internal.is_retryable());
        assert!(!Code::Internal.is_client_fault());
    }

    #[test]
    fn exhausted_is_both_client_fault_and_retryable() {
        assert!(Code::Exhausted.is_client_fault());
        assert!(Code::Exhausted.is_retryable());
    }

    #[test]
    fn exit_codes_stay_in_sysexits_range() {
        for &code in Code::ALL {
            let ec = code.exit_code();
            assert!((64..=78).contains(&ec), "{code} -> {ec}");
        }
    }

    #[cfg(feature = "http")]
    #[test]
    fn client_faults_map_to_4xx_and_the_rest_to_5xx() {
        for &code in Code::ALL {
            let status = code.status();
            assert_eq!(
                code.is_client_fault(),
                status.is_client_error(),
                "{code} -> {status}"
            );
        }
    }
}
