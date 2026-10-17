//! Synthetic value generators for PII entities and decoys.
//!
//! Every value is random and checksum-consistent where the real format
//! requires it; none is derived from real people. The format mix
//! intentionally includes notations krpii does not support yet (for example
//! parenthesized area codes) so that recall reflects real-world text rather
//! than only the formats the engine was written for.

use krpii_core::EntityType;
use krpii_core::checksum::{brn_is_valid, luhn_is_valid, rrn_checksum_is_valid};

use crate::rng::Rng;

/// Text that resembles PII or credentials but is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecoyKind {
    /// 16-digit order ID; about one in ten passes Luhn by chance.
    OrderId,
    /// 12-digit parcel tracking number.
    TrackingNumber,
    /// 10-digit membership number; about one in ten passes the BRN checksum.
    MemberId,
    /// Currency amount with thousands separators.
    Amount,
    /// ISO-like timestamp.
    Timestamp,
    /// Alphanumeric product code.
    ProductCode,
    /// Business service or toll-free number.
    ServiceNumber,
    /// Five-digit postal code.
    PostalCode,
    /// IPv4 address.
    IpAddress,
    /// 40-character hexadecimal commit hash.
    CommitHash,
    /// Random UUID, as found in request IDs.
    Uuid,
    /// Credential placeholder copied from documentation, such as
    /// `AKIAIOSFODNN7EXAMPLE` or `postgres://app:password@localhost/app`.
    PlaceholderKey,
}

impl DecoyKind {
    pub const ALL: [Self; 12] = [
        Self::OrderId,
        Self::TrackingNumber,
        Self::MemberId,
        Self::Amount,
        Self::Timestamp,
        Self::ProductCode,
        Self::ServiceNumber,
        Self::PostalCode,
        Self::IpAddress,
        Self::CommitHash,
        Self::Uuid,
        Self::PlaceholderKey,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::OrderId => "Order ID (16 digits)",
            Self::TrackingNumber => "Tracking number (12 digits)",
            Self::MemberId => "Member ID (10 digits)",
            Self::Amount => "Currency amount",
            Self::Timestamp => "Timestamp",
            Self::ProductCode => "Product code",
            Self::ServiceNumber => "Service number (1588, 080)",
            Self::PostalCode => "Postal code",
            Self::IpAddress => "IPv4 address",
            Self::CommitHash => "Commit hash",
            Self::Uuid => "UUID",
            Self::PlaceholderKey => "Documentation placeholder key",
        }
    }
}

/// Generates a synthetic value of the given entity type.
pub fn entity(rng: &mut Rng, entity_type: EntityType) -> String {
    match entity_type {
        EntityType::Phone => phone(rng),
        EntityType::Email => email(rng),
        EntityType::Card => card(rng),
        EntityType::Rrn => rrn(rng),
        EntityType::Brn => brn(rng),
        EntityType::Secret => secret(rng),
    }
}

/// Generates a synthetic decoy value of the given kind.
pub fn decoy(rng: &mut Rng, kind: DecoyKind) -> String {
    match kind {
        DecoyKind::OrderId => format!("2026{}", rng.digits(12)),
        DecoyKind::TrackingNumber => {
            let digits = rng.digits(12);
            if rng.chance(0.5) {
                digits
            } else {
                format!("{}-{}-{}", &digits[..4], &digits[4..8], &digits[8..])
            }
        }
        DecoyKind::MemberId => rng.digits(10),
        DecoyKind::Amount => with_thousands_separators(rng.range(1_000, 99_999_999)),
        DecoyKind::Timestamp => format!(
            "2026-{:02}-{:02} {:02}:{:02}:{:02}",
            rng.range(1, 12),
            rng.range(1, 28),
            rng.below(24),
            rng.below(60),
            rng.below(60)
        ),
        DecoyKind::ProductCode => format!("SKU-2026{}-{}", rng.digits(4), rng.digits(3)),
        DecoyKind::ServiceNumber => {
            let prefix = rng.pick(&["1588", "1544", "1577", "1600", "1899"]);
            if rng.chance(0.7) {
                format!("{prefix}-{}", rng.digits(4))
            } else {
                format!("080-{}-{}", rng.digits(3), rng.digits(4))
            }
        }
        DecoyKind::PostalCode => format!("{:05}", rng.range(1_000, 63_999)),
        DecoyKind::IpAddress => format!(
            "{}.{}.{}.{}",
            rng.range(1, 223),
            rng.below(256),
            rng.below(256),
            rng.range(1, 254)
        ),
        DecoyKind::CommitHash => random_chars(rng, HEX, 40),
        DecoyKind::Uuid => format!(
            "{}-{}-4{}-{}-{}",
            random_chars(rng, HEX, 8),
            random_chars(rng, HEX, 4),
            random_chars(rng, HEX, 3),
            random_chars(rng, HEX, 4),
            random_chars(rng, HEX, 12)
        ),
        DecoyKind::PlaceholderKey => (*rng.pick(&[
            "AKIAIOSFODNN7EXAMPLE",
            "sk-proj-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "sk-ant-your-api-key-goes-here",
            "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "postgres://app:password@localhost:5432/app",
            "mysql://root:${DB_PASSWORD}@db:3306/app",
        ]))
        .to_owned(),
    }
}

const HEX: &[u8] = b"0123456789abcdef";
const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const UPPER_ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Returns `length` characters drawn uniformly from `charset`.
fn random_chars(rng: &mut Rng, charset: &[u8], length: usize) -> String {
    (0..length)
        .map(|_| char::from(*rng.pick(charset)))
        .collect()
}

/// Generates a synthetic API key, token, or private key block. Bodies are
/// random, so no value matches a real credential.
fn secret(rng: &mut Rng) -> String {
    match rng.below(8) {
        0 => format!("sk-ant-api03-{}", random_chars(rng, ALNUM, 48)),
        1 => format!("sk-proj-{}", random_chars(rng, ALNUM, 48)),
        2 => format!("ghp_{}", random_chars(rng, ALNUM, 36)),
        3 => format!("github_pat_{}", random_chars(rng, ALNUM, 40)),
        4 => format!("AKIA{}", random_chars(rng, UPPER_ALNUM, 16)),
        5 => format!("xoxb-{}-{}", rng.digits(12), random_chars(rng, ALNUM, 24)),
        6 => format!("AIza{}", random_chars(rng, ALNUM, 35)),
        _ => {
            let lines: Vec<String> = (0..3).map(|_| random_chars(rng, BASE64, 64)).collect();
            format!(
                "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
                lines.join("\n")
            )
        }
    }
}

/// Generates a synthetic password for a database connection string.
pub fn db_password(rng: &mut Rng) -> String {
    let length = rng.range(12, 20);
    format!("{}!{}", random_chars(rng, ALNUM, length), rng.digits(2))
}

fn phone(rng: &mut Rng) -> String {
    let separator = *rng.pick(&["-", "-", "-", " ", ".", ""]);
    match rng.below(100) {
        // Mobile numbers dominate real-world text.
        0..=54 => format!(
            "010{separator}{}{separator}{}",
            rng.digits(4),
            rng.digits(4)
        ),
        55..=69 => {
            let middle_len = rng.range(3, 4);
            let middle = rng.digits(middle_len);
            format!("02{separator}{middle}{separator}{}", rng.digits(4))
        }
        70..=84 => {
            let area = rng.pick(&[
                "031", "032", "033", "041", "042", "051", "053", "062", "064",
            ]);
            let middle_len = rng.range(3, 4);
            let middle = rng.digits(middle_len);
            format!("{area}{separator}{middle}{separator}{}", rng.digits(4))
        }
        85..=89 => format!("070-{}-{}", rng.digits(4), rng.digits(4)),
        90..=94 => {
            let prefix = rng.pick(&["+82 10-", "+82-10-", "+82 10 "]);
            let inner = if prefix.ends_with(' ') { " " } else { "-" };
            format!("{prefix}{}{inner}{}", rng.digits(4), rng.digits(4))
        }
        // Parenthesized area codes are common in documents but not yet supported.
        _ => {
            let area = rng.pick(&["02", "031", "051"]);
            format!("({area}) {}-{}", rng.digits(3), rng.digits(4))
        }
    }
}

fn email(rng: &mut Rng) -> String {
    const NAMES: &[&str] = &["hong", "kim", "lee", "park", "choi", "jung", "kang", "yoon"];
    const WORDS: &[&str] = &["dev", "work", "mail", "biz", "contact", "official"];
    // Reserved example domains (RFC 2606) guarantee no address is real.
    const DOMAINS: &[&str] = &[
        "example.com",
        "example.net",
        "example.org",
        "mail.example.com",
        "corp.example.org",
    ];
    let name = rng.pick(NAMES);
    let local = match rng.below(3) {
        0 => {
            let suffix_len = rng.range(2, 4);
            format!("{name}{}", rng.digits(suffix_len))
        }
        1 => format!("{name}.{}", rng.pick(WORDS)),
        _ => format!("{name}_{}{}", rng.pick(WORDS), rng.digits(2)),
    };
    format!("{local}@{}", rng.pick(DOMAINS))
}

fn card(rng: &mut Rng) -> String {
    let separator = *rng.pick(&["-", "-", " ", ""]);
    if rng.chance(0.1) {
        // American Express: 15 digits grouped 4-6-5.
        let prefix = rng.pick(&["34", "37"]);
        let digits = with_check_digit(&format!("{prefix}{}", rng.digits(12)), luhn_is_valid);
        return format!(
            "{}{separator}{}{separator}{}",
            &digits[..4],
            &digits[4..10],
            &digits[10..]
        );
    }
    let prefix = rng.pick(&["4", "51", "53", "55", "35", "94"]);
    let body = rng.digits(15 - prefix.len());
    let digits = with_check_digit(&format!("{prefix}{body}"), luhn_is_valid);
    format!(
        "{}{separator}{}{separator}{}{separator}{}",
        &digits[..4],
        &digits[4..8],
        &digits[8..12],
        &digits[12..]
    )
}

fn rrn(rng: &mut Rng) -> String {
    let year = rng.range(1940, 2019);
    let month = rng.range(1, 12);
    let day = rng.range(1, days_in_month(year, month));
    let foreigner = rng.chance(0.05);
    let gender = match (year >= 2000, foreigner) {
        (false, false) => rng.range(1, 2),
        (true, false) => rng.range(3, 4),
        (false, true) => rng.range(5, 6),
        (true, true) => rng.range(7, 8),
    };
    let front = format!("{:02}{month:02}{day:02}", year % 100);
    let partial = format!("{front}{gender}{}", rng.digits(5));
    // Half follow the legacy checksum; the rest mimic numbers issued since
    // October 2020, whose last digit is unconstrained.
    let full = if rng.chance(0.5) {
        with_check_digit(&partial, rrn_checksum_is_valid)
    } else {
        format!("{partial}{}", rng.digits(1))
    };
    let separator = match rng.below(20) {
        0..=16 => "-",
        17..=18 => "",
        _ => " ",
    };
    format!("{}{separator}{}", &full[..6], &full[6..])
}

fn brn(rng: &mut Rng) -> String {
    // The leading three digits are a tax office code, which never starts with 0.
    let tax_office = rng.range(101, 999);
    let digits = with_check_digit(&format!("{tax_office}{}", rng.digits(6)), brn_is_valid);
    if rng.chance(0.85) {
        format!("{}-{}-{}", &digits[..3], &digits[3..5], &digits[5..])
    } else {
        digits
    }
}

/// Appends the unique check digit that makes `prefix` satisfy `is_valid`.
fn with_check_digit(prefix: &str, is_valid: fn(&str) -> bool) -> String {
    (0..=9)
        .map(|digit| format!("{prefix}{digit}"))
        .find(|candidate| is_valid(candidate))
        .expect("every supported checksum admits exactly one check digit")
}

fn days_in_month(year: usize, month: usize) -> usize {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn with_thousands_separators(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_digits(value: &str) -> String {
        value.chars().filter(char::is_ascii_digit).collect()
    }

    #[test]
    fn generated_cards_pass_luhn() {
        let mut rng = Rng::new(1);
        for _ in 0..500 {
            assert!(luhn_is_valid(&only_digits(&card(&mut rng))));
        }
    }

    #[test]
    fn generated_business_numbers_pass_checksum() {
        let mut rng = Rng::new(2);
        for _ in 0..500 {
            assert!(brn_is_valid(&only_digits(&brn(&mut rng))));
        }
    }

    #[test]
    fn generated_resident_numbers_have_thirteen_digits() {
        let mut rng = Rng::new(3);
        for _ in 0..500 {
            assert_eq!(only_digits(&rrn(&mut rng)).len(), 13);
        }
    }

    #[test]
    fn generated_emails_use_reserved_domains() {
        let mut rng = Rng::new(4);
        for _ in 0..100 {
            let address = email(&mut rng);
            assert!(address.split('@').nth(1).unwrap().contains("example."));
        }
    }

    #[test]
    fn thousands_separators_are_inserted_correctly() {
        assert_eq!(with_thousands_separators(1_000), "1,000");
        assert_eq!(with_thousands_separators(12_345_678), "12,345,678");
        assert_eq!(with_thousands_separators(999), "999");
    }
}
