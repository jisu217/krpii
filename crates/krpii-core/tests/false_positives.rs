//! Regression tests for false positives.
//!
//! Each case is numeric text that commonly appears next to PII but is not PII.
//! A false positive corrupts the very text the caller wanted to preserve, so
//! every new recognizer must keep this suite passing.

use krpii_core::detect;

const DECOYS: &[(&str, &str)] = &[
    ("order ID that fails Luhn", "주문번호 2026101712345670"),
    ("parcel tracking number", "송장번호 6123-4567-8901"),
    ("currency amount", "결제금액 1,234,567원"),
    ("timestamp", "2026-10-17 14:30:00"),
    ("product code", "상품코드 SKU-20261017-001"),
    (
        "dotted number without a subscriber prefix",
        "버전 10.1234.5678",
    ),
    ("business service number", "고객센터 1588-1234"),
    ("toll-free number", "수신자부담 080-123-4567"),
    ("postal code", "우편번호 06236"),
    ("IPv4 address", "IP 192.168.0.1"),
    (
        "labelled order ID that passes Luhn",
        "주문번호 2026101712345679",
    ),
    (
        "labelled member ID that passes the BRN checksum",
        "회원번호 5506073448",
    ),
    (
        "labelled member ID shaped like a landline",
        "회원번호: 0420764465",
    ),
];

#[test]
fn common_numeric_text_is_not_pii() {
    let failures: Vec<String> = DECOYS
        .iter()
        .filter_map(|(label, text)| {
            let entities = detect(text);
            (!entities.is_empty()).then(|| format!("{label}: {text:?} -> {entities:?}"))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "false positives:\n{}",
        failures.join("\n")
    );
}
