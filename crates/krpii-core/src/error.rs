//! Error types exposed by the public API.

use std::error::Error;
use std::fmt;

/// Returned when a string does not name a known variant of a public enum,
/// such as [`EntityType`](crate::EntityType) or [`MaskStyle`](crate::MaskStyle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: &'static str,
    value: String,
    expected: &'static [&'static str],
}

impl ParseError {
    pub(crate) fn new(kind: &'static str, value: &str, expected: &'static [&'static str]) -> Self {
        Self {
            kind,
            value: value.to_owned(),
            expected,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown {} {:?}; expected one of: {}",
            self.kind,
            self.value,
            self.expected.join(", ")
        )
    }
}

impl Error for ParseError {}
