# krpii

**Accurate detection and masking of Korean personally identifiable information (PII).**
Run it before sending prompts to an LLM or writing logs.

```console
$ echo "홍길동 010-1234-5678, 주민번호 900101-1234567로 가입" | krpii mask
홍길동 010-****-5678, 주민번호 900101-*******로 가입

$ echo "홍길동 010-1234-5678, 주민번호 900101-1234567로 가입" | krpii mask --style tag
홍길동 <PHONE>, 주민번호 <RRN>로 가입
```

## Why krpii

Regex-only scanners flag every long number, including order IDs, tracking numbers, and
amounts. Each false positive corrupts text the caller meant to keep. krpii generates
candidates permissively and reports only those that pass validation:

- **Checksums:** Luhn for card numbers and the National Tax Service algorithm for
  business registration numbers.
- **Date validation:** the birth date in a resident registration number must exist and
  must not lie in the future.
- **Format confidence:** unseparated digit runs are more likely to be other identifiers,
  so they receive lower scores.

## Supported entities

| Entity | Tag | Validation | `partial` mask |
|---|---|---|---|
| Mobile, landline, and VoIP numbers | `PHONE` | Subscriber prefixes only (excludes 1588, 080) | `010-****-5678` |
| E-mail addresses | `EMAIL` | Pragmatic subset of RFC 5322 | `h***@example.com` |
| Payment card numbers | `CARD` | Luhn checksum | `4111-11**-****-1111` |
| Resident registration numbers, including foreigners | `RRN` | Birth date check + checksum (affects score) | `900101-*******` |
| Business registration numbers | `BRN` | NTS checksum | `123-45-*****` |

> Resident registration numbers issued since October 2020 may not satisfy the legacy
> checksum. A mismatch therefore **lowers the confidence score** instead of rejecting
> the candidate.

## Evaluation

Measured on a synthetic corpus of 10,000 Korean sentences containing 11,921 PII values
and 8,031 decoys. A detection counts as correct only when its type, start, and end all
match exactly.

| Entity | Support | Precision | Recall | F1 |
|---|--:|--:|--:|--:|
| `PHONE` | 2,332 | 0.988 | 0.948 | 0.968 |
| `EMAIL` | 2,420 | 1.000 | 1.000 | 1.000 |
| `CARD` | 2,347 | 0.967 | 1.000 | 0.983 |
| `RRN` | 2,421 | 1.000 | 1.000 | 1.000 |
| `BRN` | 2,401 | 0.960 | 1.000 | 0.980 |
| **Overall (micro)** | 11,921 | 0.983 | 0.990 | 0.986 |

Decoys that look numeric but are not PII:

| Decoy | False-positive rate |
|---|--:|
| Tracking numbers, amounts, timestamps, product codes, service numbers, postal codes, IPv4 addresses | 0.0% |
| 16-digit order IDs (detected as `CARD`) | 9.1% |
| 10-digit member IDs (detected as `BRN` or `PHONE`) | 12.9% |

### Why validation matters

The same corpus, scored against a regex-only baseline that uses krpii's candidate
patterns and digit-boundary rule but skips every validation step:

| Entity | Precision (regex only) | Precision (krpii) | Recall (regex only) | Recall (krpii) |
|---|--:|--:|--:|--:|
| `PHONE` | 0.880 | 0.988 | 0.948 | 0.948 |
| `EMAIL` | 1.000 | 1.000 | 1.000 | 1.000 |
| `CARD` | 0.724 | 0.967 | 1.000 | 1.000 |
| `RRN` | 1.000 | 1.000 | 1.000 | 1.000 |
| `BRN` | 0.719 | 0.960 | 1.000 | 1.000 |
| **Overall (micro)** | 0.847 | 0.983 | 0.990 | 0.990 |

**Validation cuts decoys flagged as PII from 26.6% to 2.6%, a tenfold reduction, at
no cost to recall.** The corpus does not yet contain 13-digit decoys shaped like resident
registration numbers, so the effect of birth-date validation is not reflected here.

Known limitations, shown directly by these numbers:

- **Unseparated digit runs.** About one in ten random 16-digit or 10-digit numbers passes
  the Luhn or business-number checksum by chance. Context keywords will address this.
- **Parenthesized area codes** such as `(02) 123-4567` are not detected yet, which
  accounts for most of the `PHONE` recall gap.

Synthetic data cannot capture every real-world notation, so treat these figures as a
regression baseline rather than a guarantee. To reproduce them:

```console
$ cargo run --release -p krpii-bench
$ cargo run --release -p krpii-bench -- --seed 7 --samples 50000 --show-errors 5
```

## Name

**KR** (the ISO country code for Korea) + **PII**.

## CLI

```console
$ cargo install krpii
```

```console
$ krpii scan [FILE] [--types PHONE,RRN] [--min-score 0.9]
$ krpii mask [FILE] [--types ...] [--min-score ...] [--style partial|tag]
```

- Reads standard input when `FILE` is omitted or set to `-`.
- `scan` prints one JSON object per entity (JSON Lines). **Matched values are never printed.**

  ```json
  {"type":"PHONE","start":10,"end":23,"score":0.95}
  ```

- Exit codes follow the `grep` convention, so the binary composes with scripts, CI
  jobs, and editor hooks.

  | Code | Meaning |
  |---|---|
  | 0 | Success (`scan`: no PII found) |
  | 1 | `scan`: at least one entity found |
  | 2 | Usage or I/O error |

## Rust library

```rust
use krpii_core::{mask, Detector, EntityType, MaskStyle};

let masked = mask("연락처 010-1234-5678", MaskStyle::Partial);
assert_eq!(masked, "연락처 010-****-5678");

let detector = Detector::new()
    .with_types([EntityType::Rrn, EntityType::Card])
    .with_min_score(0.9);
let entities = detector.detect("...");
```

An `Entity` stores only its span (UTF-8 byte offsets) and score, **never the matched
value**, so logging one cannot leak the PII it describes.

## Repository layout

```
krpii/
├─ crates/
│   ├─ krpii-core/   # Detection engine: recognizers, checksums, masking
│   └─ krpii/        # CLI: the `krpii` binary
├─ benchmarks/       # Synthetic corpus generator and evaluation benchmark
└─ .github/workflows/
```

## Roadmap

- [x] `krpii-core`: phone, e-mail, card, resident and business registration numbers
- [x] `krpii` CLI: `scan`, `mask`
- [x] Synthetic evaluation set with per-entity precision and recall (`benchmarks/`)
- [ ] Context keywords to suppress checksum collisions on order and member IDs
- [ ] Parenthesized area codes in phone numbers
- [ ] Python bindings (PyO3 + maturin): `pip install krpii`
- [ ] Claude Code plugin that runs `krpii` from hooks
- [ ] Bank account numbers via surrounding context keywords
- [ ] Address detection and reversible masking

## Development

```console
$ cargo test --workspace
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo fmt --all
```

**Never use real personal data in tests.** Every number in this repository is synthetic,
constructed only to match the expected format and checksum.

## License

[MIT](LICENSE)
