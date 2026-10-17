//! Recognizer for e-mail addresses.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::{MASK_CHAR, Recognizer, group};
use crate::entity::EntityType;

/// A pragmatic subset of RFC 5322 that covers real-world addresses while
/// deliberately ignoring quoted local parts and IP-literal domains.
static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?P<local> [A-Za-z0-9] (?: [A-Za-z0-9._%+-]{0,62} [A-Za-z0-9] )? )
        @
        (?P<domain> (?: [A-Za-z0-9] (?: [A-Za-z0-9-]{0,61} [A-Za-z0-9] )? \. )+ [A-Za-z]{2,24} )
        ",
    )
    .expect("email pattern is valid")
});

const SCORE: f64 = 0.95;

/// Detects e-mail addresses such as `hong.gildong@example.co.kr`.
pub(crate) struct EmailRecognizer;

impl Recognizer for EmailRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Email
    }

    fn pattern(&self) -> &Regex {
        &PATTERN
    }

    fn score(&self, text: &str, captures: &Captures<'_>) -> Option<f64> {
        let matched = captures.get(0)?;
        let preceded_by_local_char = text[..matched.start()]
            .bytes()
            .next_back()
            .is_some_and(is_local_part_byte);
        let followed_by_domain_char = text[matched.end()..]
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'-');
        if preceded_by_local_char || followed_by_domain_char {
            return None;
        }
        if group(captures, "local").contains("..") {
            return None;
        }
        Some(SCORE)
    }

    fn mask_partial(&self, value: &str) -> String {
        let Some((local, domain)) = value.rsplit_once('@') else {
            return value.to_owned();
        };
        // Keep the first character so that masked addresses stay
        // distinguishable from one another in logs, unless it is the whole
        // local part, in which case keeping it would mask nothing.
        let length = local.chars().count();
        let visible: String = if length > 1 {
            local.chars().take(1).collect()
        } else {
            String::new()
        };
        let hidden = MASK_CHAR
            .to_string()
            .repeat(length - visible.chars().count());
        format!("{visible}{hidden}@{domain}")
    }
}

fn is_local_part_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        EmailRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_addresses() {
        for address in [
            "user@example.com",
            "hong.gildong+news@mail.example.co.kr",
            "a_b-c@sub-domain.io",
        ] {
            let text = format!("메일은 {address}으로 보내 주세요");
            assert_eq!(found(&text), [address], "input: {text}");
        }
    }

    #[test]
    fn rejects_invalid_addresses() {
        for text in [
            "user@localhost",
            "@example.com",
            "user@.com",
            "first..last@example.com",
        ] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn mask_partial_keeps_first_character_and_domain() {
        assert_eq!(
            EmailRecognizer.mask_partial("hong@example.com"),
            "h***@example.com"
        );
    }

    #[test]
    fn mask_partial_hides_a_single_character_local_part() {
        assert_eq!(
            EmailRecognizer.mask_partial("a@example.com"),
            "*@example.com"
        );
        assert_eq!(
            EmailRecognizer.mask_partial("ab@example.com"),
            "a*@example.com"
        );
    }
}
