//! Core data types shared by recognizers, detection, and masking.

use std::fmt;
use std::ops::Range;
use std::str::FromStr;

use crate::error::ParseError;

/// Categories of sensitive data that krpii can detect: Korean personally
/// identifiable information and credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntityType {
    /// Korean mobile, landline, and `VoIP` telephone numbers.
    Phone,
    /// E-mail addresses.
    Email,
    /// Payment card numbers validated with the Luhn checksum.
    Card,
    /// Resident registration numbers (주민등록번호), including foreigner numbers.
    Rrn,
    /// Business registration numbers (사업자등록번호).
    Brn,
    /// Credentials with an unambiguous format: API keys and tokens, private key
    /// blocks, and passwords embedded in database connection strings.
    Secret,
}

impl EntityType {
    /// Every supported entity type, in canonical order.
    pub const ALL: [Self; 6] = [
        Self::Phone,
        Self::Email,
        Self::Card,
        Self::Rrn,
        Self::Brn,
        Self::Secret,
    ];

    /// The Korean personally identifiable information types, excluding credentials.
    pub const PII: [Self; 5] = [Self::Phone, Self::Email, Self::Card, Self::Rrn, Self::Brn];

    const NAMES: &'static [&'static str] = &["PHONE", "EMAIL", "CARD", "RRN", "BRN", "SECRET"];

    /// The stable, upper-case label used in tags, CLI flags, and serialized output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "PHONE",
            Self::Email => "EMAIL",
            Self::Card => "CARD",
            Self::Rrn => "RRN",
            Self::Brn => "BRN",
            Self::Secret => "SECRET",
        }
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EntityType {
    type Err = ParseError;

    /// Parses a label case-insensitively, so both `"PHONE"` and `"phone"` are accepted.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|entity_type| entity_type.as_str().eq_ignore_ascii_case(value))
            .ok_or_else(|| ParseError::new("entity type", value, Self::NAMES))
    }
}

/// A single PII occurrence located within an input string.
///
/// An `Entity` deliberately stores only its span, not the matched text, so
/// that logging or serializing it can never leak the value it describes. Use
/// [`Entity::as_str`] to borrow the value from the original input when needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// The category of the detected entity.
    pub entity_type: EntityType,
    /// Inclusive start byte offset into the original text.
    pub start: usize,
    /// Exclusive end byte offset into the original text.
    pub end: usize,
    /// Confidence in `[0.0, 1.0]`. Higher means the candidate passed stronger
    /// validation (checksums, date checks, conventional formatting).
    pub score: f64,
}

impl Entity {
    /// The byte range of this entity within the original text.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.start..self.end
    }

    /// The span length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Always `false`; recognizers never produce empty spans.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Borrows the matched value from `text`, the string this entity was detected in.
    ///
    /// # Panics
    ///
    /// Panics if `text` is not the string the entity was produced from and the
    /// span is out of bounds or not on a character boundary.
    #[must_use]
    pub fn as_str<'a>(&self, text: &'a str) -> &'a str {
        &text[self.span()]
    }

    /// Returns whether this entity's span intersects `other`'s span.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_type_round_trips_through_strings() {
        for entity_type in EntityType::ALL {
            assert_eq!(entity_type.as_str().parse::<EntityType>(), Ok(entity_type));
        }
    }

    #[test]
    fn entity_type_parsing_is_case_insensitive() {
        assert_eq!("rrn".parse::<EntityType>(), Ok(EntityType::Rrn));
    }

    #[test]
    fn unknown_entity_type_lists_valid_options() {
        let error = "PASSPORT".parse::<EntityType>().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("PHONE, EMAIL, CARD, RRN, BRN, SECRET")
        );
    }
}
