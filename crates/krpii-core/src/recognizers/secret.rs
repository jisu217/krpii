//! Recognizer for credentials with an unambiguous format.
//!
//! Only formats identifiable by a fixed prefix or delimiter are covered, so a
//! long random-looking string is never reported merely for its shape:
//!
//! - API keys and tokens from widely used providers, such as `sk-ant-…`,
//!   `ghp_…`, and `AKIA…`.
//! - PEM private key blocks.
//! - Passwords embedded in database connection strings. Only the password is
//!   reported, so masking keeps the rest of the URL readable.
//!
//! Documentation placeholders such as `AKIAIOSFODNN7EXAMPLE`, `sk-xxxx…`, and
//! `${DB_PASSWORD}` are rejected.

use std::ops::Range;
use std::sync::LazyLock;

use super::{MASK_CHAR, Recognizer};
use crate::entity::EntityType;
use crate::pattern::{Captures, Pattern, Prefilter};

static PATTERN: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::new(
        r"(?x)
        (?P<token>
            sk-(?:ant-|proj-|svcacct-|admin-)?[A-Za-z0-9_-]{20,}   # Anthropic, OpenAI
          | gh[pousr]_[A-Za-z0-9]{36}                             # GitHub tokens
          | github_pat_[A-Za-z0-9_]{22,}                          # GitHub fine-grained tokens
          | (?:AKIA|ASIA)[0-9A-Z]{16}                             # AWS access key IDs
          | xox[abeoprs]-[A-Za-z0-9-]{10,}                        # Slack tokens
          | AIza[0-9A-Za-z_-]{35}                                 # Google API keys
          | (?:sk|rk)_live_[0-9A-Za-z]{24,}                       # Stripe live keys
        )
        |
        (?P<private_key>
            -----BEGIN\ (?:[A-Z0-9]+\ )*PRIVATE\ KEY-----
            (?s:.*?)
            -----END\ (?:[A-Z0-9]+\ )*PRIVATE\ KEY-----
        )
        |
        (?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|rediss?|amqps?)://
        [^\s:/@]+ : (?P<password>[^\s@/]+) @
        ",
        Prefilter::Enabled,
    )
    .expect("secret pattern is valid")
});

/// Substrings at least one of which every match contains, checked before the
/// pattern is compiled. Each alternative of [`PATTERN`] must contain one of
/// them; `detects_provider_tokens` fails if an alternative is left uncovered.
const ANCHORS: &[&str] = &[
    "sk-",
    "_live_",
    "gh",
    "github_pat_",
    "AKIA",
    "ASIA",
    "xox",
    "AIza",
    "-----BEGIN",
    "://",
];

/// Known token prefixes, kept visible when a token is partially masked so the
/// reader can tell which credential was removed. Longer prefixes come first.
const TOKEN_PREFIXES: &[&str] = &[
    "sk-svcacct-",
    "github_pat_",
    "sk-admin-",
    "sk-proj-",
    "sk-ant-",
    "sk_live_",
    "rk_live_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "xoxa-",
    "xoxb-",
    "xoxe-",
    "xoxo-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
    "AKIA",
    "ASIA",
    "AIza",
    "sk-",
];

/// Words that mark a value as a documentation placeholder, compared
/// case-insensitively. Each is long or delimited enough that random key
/// material is vanishingly unlikely to contain it.
const PLACEHOLDER_WORDS: &[&str] = &[
    "example",
    "your_",
    "your-",
    "placeholder",
    "dummy",
    "redacted",
];

/// Literal fillers that mark a placeholder, compared case-sensitively so that
/// random mixed-case material such as `xXXx` is not mistaken for `xxxx`.
const PLACEHOLDER_FILLERS: &[&str] = &["xxxx", "XXXX", "****", "...", "<", ">", "${"];

/// Connection-string passwords that are conventional placeholders, not secrets.
const PLACEHOLDER_PASSWORDS: &[&str] = &["password", "passwd", "pass", "secret", "pwd"];

/// A run of one repeated character at least this long marks a placeholder,
/// such as `sk-aaaaaaaaaaaaaaaaaaaa`.
const PLACEHOLDER_REPEAT: usize = 8;

/// A private key block whose body has fewer encoded characters than this is a
/// truncated example rather than a usable key.
const MIN_PRIVATE_KEY_BODY: usize = 64;

const SCORE_TOKEN: f64 = 0.95;
const SCORE_PRIVATE_KEY: f64 = 0.99;
const SCORE_PASSWORD: f64 = 0.9;

/// Detects API keys and tokens, private key blocks, and connection-string passwords.
pub(crate) struct SecretRecognizer;

impl Recognizer for SecretRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Secret
    }

    fn pattern(&self) -> &Pattern {
        &PATTERN
    }

    fn may_match(&self, text: &str) -> bool {
        ANCHORS.iter().any(|anchor| text.contains(anchor))
    }

    fn score(&self, text: &str, captures: &Captures<'_>) -> Option<f64> {
        if let Some(token) = captures.name("token") {
            let isolated = !text[..token.start()]
                .chars()
                .next_back()
                .is_some_and(is_token_char)
                && !text[token.end()..]
                    .chars()
                    .next()
                    .is_some_and(is_token_char);
            return (isolated && !is_placeholder(token.as_str())).then_some(SCORE_TOKEN);
        }
        if let Some(block) = captures.name("private_key") {
            let body_length = private_key_body(block.as_str())
                .chars()
                .filter(|c| !c.is_whitespace())
                .count();
            let usable = body_length >= MIN_PRIVATE_KEY_BODY && !is_placeholder(block.as_str());
            return usable.then_some(SCORE_PRIVATE_KEY);
        }
        let password = captures.name("password")?.as_str();
        let placeholder = PLACEHOLDER_PASSWORDS
            .iter()
            .any(|p| p.eq_ignore_ascii_case(password))
            || password.starts_with(['$', '%', '{', '<'])
            || is_placeholder(password);
        (!placeholder).then_some(SCORE_PASSWORD)
    }

    fn span(&self, captures: &Captures<'_>) -> Range<usize> {
        captures
            .name("password")
            .or_else(|| captures.get(0))
            .expect("group 0 always participates")
            .range()
    }

    fn mask_partial(&self, value: &str) -> String {
        if value.starts_with("-----BEGIN") {
            return mask_private_key(value);
        }
        let prefix = TOKEN_PREFIXES
            .iter()
            .find(|prefix| value.starts_with(*prefix))
            .map_or("", |prefix| *prefix);
        let hidden = value[prefix.len()..].chars().count();
        format!("{prefix}{}", MASK_CHAR.to_string().repeat(hidden))
    }
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    PLACEHOLDER_WORDS.iter().any(|word| lower.contains(word))
        || PLACEHOLDER_FILLERS
            .iter()
            .any(|filler| value.contains(filler))
        || longest_repeat(value) >= PLACEHOLDER_REPEAT
}

fn longest_repeat(value: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    let mut previous = None;
    for c in value.chars() {
        current = if Some(c) == previous { current + 1 } else { 1 };
        longest = longest.max(current);
        previous = Some(c);
    }
    longest
}

/// Returns the text between the BEGIN and END lines of a PEM block.
fn private_key_body(block: &str) -> &str {
    let start = block.find('\n').map_or(block.len(), |i| i + 1);
    let end = block.rfind("-----END").unwrap_or(block.len()).max(start);
    &block[start..end]
}

/// Keeps the BEGIN and END lines of a PEM block and masks every encoded
/// character between them, preserving line breaks.
fn mask_private_key(block: &str) -> String {
    let start = block.find('\n').map_or(block.len(), |i| i + 1);
    let end = block.rfind("-----END").unwrap_or(block.len()).max(start);
    let body: String = block[start..end]
        .chars()
        .map(|c| if c.is_whitespace() { c } else { MASK_CHAR })
        .collect();
    format!("{}{body}{}", &block[..start], &block[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        SecretRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    /// Builds a synthetic key at runtime so no complete credential literal
    /// lives in the source, where secret scanners would flag it.
    fn key(prefix: &str, body_char: char, length: usize) -> String {
        let body: String = (0..length)
            .map(|i| {
                let offset = u8::try_from(i % 10).expect("digit fits in u8");
                if i % 2 == 0 {
                    body_char
                } else {
                    char::from(b'0' + offset)
                }
            })
            .collect();
        format!("{prefix}{body}")
    }

    #[test]
    fn detects_provider_tokens() {
        for token in [
            key("sk-ant-api03-", 'q', 40),
            key("sk-proj-", 'r', 40),
            key("ghp_", 'm', 36),
            key("github_pat_", 'k', 40),
            key("AKIA", 'Q', 16),
            key("xoxb-", 'z', 24),
            key("AIza", 'w', 35),
        ] {
            let text = format!("export TOKEN={token}");
            assert_eq!(found(&text), [token.as_str()], "input: {text}");
        }
    }

    #[test]
    fn rejects_documentation_placeholders() {
        for text in [
            "aws_access_key_id = AKIAIOSFODNN7EXAMPLE",
            "OPENAI_API_KEY=sk-proj-xxxxxxxxxxxxxxxxxxxxxxxx",
            "ANTHROPIC_API_KEY=sk-ant-your-api-key-goes-here-now",
            "postgres://app:${DB_PASSWORD}@db:5432/app",
            "postgres://app:password@localhost/app",
            "mysql://root:<password>@db/app",
        ] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn accepts_random_material_that_only_resembles_a_filler() {
        // Mixed case `xXXx` occurs in random keys and is not the filler `xxxx`.
        let token = format!("{}xXXx{}", key("sk-proj-", 'r', 20), key("", 'N', 20));
        let text = format!("OPENAI_API_KEY={token}");
        assert_eq!(found(&text), [token.as_str()]);
    }

    #[test]
    fn rejects_tokens_embedded_in_longer_identifiers() {
        let text = format!("x{}", key("AKIA", 'Q', 16));
        assert!(found(&text).is_empty(), "input: {text}");
        let text = format!("{}9", key("AKIA", 'Q', 16));
        assert!(found(&text).is_empty(), "input: {text}");
    }

    #[test]
    fn reports_only_the_password_of_a_connection_string() {
        let text = "DATABASE_URL=postgres://app:Tr0ub4dor3@db.internal:5432/app";
        assert_eq!(found(text), ["Tr0ub4dor3"]);
        let text = "MONGO=mongodb+srv://svc:p4ss-W0rd@cluster0.example.net/db";
        assert_eq!(found(text), ["p4ss-W0rd"]);
    }

    #[test]
    fn detects_a_private_key_block_and_rejects_a_truncated_example() {
        let body = key("", 'A', 80);
        let block =
            format!("-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----");
        let text = format!("key:\n{block}\n");
        assert_eq!(found(&text), [block.as_str()]);

        let truncated = "-----BEGIN PRIVATE KEY-----\nMIIEv...\n-----END PRIVATE KEY-----";
        assert!(found(truncated).is_empty(), "truncated example");
    }

    #[test]
    fn mask_partial_keeps_the_prefix_only() {
        let token = key("ghp_", 'm', 36);
        assert_eq!(
            SecretRecognizer.mask_partial(&token),
            format!("ghp_{}", "*".repeat(36))
        );
        assert_eq!(SecretRecognizer.mask_partial("Tr0ub4dor3"), "**********");
    }

    #[test]
    fn mask_partial_keeps_the_pem_delimiters() {
        let block = "-----BEGIN PRIVATE KEY-----\nAB12\nCD34\n-----END PRIVATE KEY-----";
        assert_eq!(
            SecretRecognizer.mask_partial(block),
            "-----BEGIN PRIVATE KEY-----\n****\n****\n-----END PRIVATE KEY-----"
        );
    }
}
