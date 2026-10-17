//! Check-digit algorithms used to reject candidates that merely look like PII.
//!
//! Every function expects a string of ASCII digits only. Callers strip
//! separators beforehand; any other input returns `false` rather than
//! panicking, so malformed data can never validate by accident.

const RRN_WEIGHTS: [u32; 12] = [2, 3, 4, 5, 6, 7, 8, 9, 2, 3, 4, 5];
const BRN_WEIGHTS: [u32; 9] = [1, 3, 7, 1, 3, 7, 1, 3, 5];

/// Converts `value` into its digits, or `None` if it contains anything else.
fn digits(value: &str) -> Option<Vec<u32>> {
    value.chars().map(|c| c.to_digit(10)).collect()
}

/// Validates a payment card number with the Luhn (mod 10) algorithm.
///
/// ```
/// use krpii_core::checksum::luhn_is_valid;
///
/// assert!(luhn_is_valid("4111111111111111"));
/// assert!(!luhn_is_valid("4111111111111112"));
/// ```
#[must_use]
pub fn luhn_is_valid(number: &str) -> bool {
    let Some(digits) = digits(number) else {
        return false;
    };
    if digits.is_empty() {
        return false;
    }
    // Double every second digit, counting from the rightmost (check) digit.
    let total: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(index, &digit)| {
            if index % 2 == 1 {
                let doubled = digit * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                digit
            }
        })
        .sum();
    total % 10 == 0
}

/// Validates a 10-digit Korean business registration number.
///
/// The National Tax Service algorithm weights the first nine digits by
/// `1 3 7 1 3 7 1 3 5` and additionally adds the tens digit of `d[8] * 5`
/// before taking the mod-10 complement.
#[must_use]
pub fn brn_is_valid(number: &str) -> bool {
    let Some(digits) = digits(number).filter(|d| d.len() == 10) else {
        return false;
    };
    let weighted: u32 = digits.iter().zip(BRN_WEIGHTS).map(|(d, w)| d * w).sum();
    let total = weighted + digits[8] * 5 / 10;
    (10 - total % 10) % 10 == digits[9]
}

/// Validates the legacy check digit of a 13-digit resident registration number.
///
/// Numbers issued from October 2020 onward use randomized trailing digits
/// and are not guaranteed to satisfy this formula. A failing checksum is
/// therefore evidence, not proof, that a candidate is not an RRN; the RRN
/// recognizer uses it as a scoring signal rather than a filter.
#[must_use]
pub fn rrn_checksum_is_valid(number: &str) -> bool {
    let Some(digits) = digits(number).filter(|d| d.len() == 13) else {
        return false;
    };
    let total: u32 = digits.iter().zip(RRN_WEIGHTS).map(|(d, w)| d * w).sum();
    (11 - total % 11) % 10 == digits[12]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_accepts_published_test_numbers() {
        assert!(luhn_is_valid("4111111111111111")); // Visa
        assert!(luhn_is_valid("5555555555554444")); // Mastercard
        assert!(luhn_is_valid("378282246310005")); // American Express
    }

    #[test]
    fn luhn_rejects_wrong_check_digit() {
        assert!(!luhn_is_valid("4111111111111112"));
        assert!(!luhn_is_valid("1234567812345678"));
    }

    #[test]
    fn brn_accepts_hand_computed_number() {
        // Weighted sum 165 + floor(9 * 5 / 10) = 169, so the check digit is 1.
        assert!(brn_is_valid("1234567891"));
        assert!(!brn_is_valid("1234567890"));
    }

    #[test]
    fn rrn_accepts_hand_computed_number() {
        // Weighted sum 124, 124 % 11 = 3, so the check digit is (11 - 3) % 10 = 8.
        assert!(rrn_checksum_is_valid("9001011234568"));
        assert!(!rrn_checksum_is_valid("9001011234567"));
    }

    #[test]
    fn malformed_input_never_validates() {
        assert!(!luhn_is_valid(""));
        assert!(!luhn_is_valid("4111-1111-1111-1111"));
        assert!(!luhn_is_valid("４１１１１１１１１１１１１１１１")); // Full-width digits
        assert!(!brn_is_valid("123456789"));
        assert!(!rrn_checksum_is_valid("900101123456"));
    }
}
