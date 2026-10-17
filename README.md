# krpii

**Catches Korean PII in 6 ms, before it reaches your LLM or logs.**

[![CI](https://github.com/jisu217/krpii/actions/workflows/ci.yml/badge.svg)](https://github.com/jisu217/krpii/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/krpii.svg)](https://crates.io/crates/krpii)
[![docs.rs](https://img.shields.io/docsrs/krpii-core)](https://docs.rs/krpii-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

```console
$ echo "홍길동 010-1234-5678, 주민번호 900101-1234567로 가입" | krpii mask
홍길동 010-****-5678, 주민번호 900101-*******로 가입

$ echo "홍길동 010-1234-5678, 주민번호 900101-1234567로 가입" | krpii mask --style tag
홍길동 <PHONE>, 주민번호 <RRN>로 가입
```

|  | krpii | Presidio 2.2.364 |
|---|--:|--:|
| F1 on Korean text | **0.997** | 0.861 |
| Decoys flagged as PII | **0.8%** | 13.8% |
| Cost per invocation | **6 ms** | 1,511 ms |
| Install size | **1.8 MiB** | 176.3 MiB |

<sub>Same synthetic corpus and scoring code for both tools. Methodology, per-entity results, and
limitations are in [benchmarks](benchmarks/README.md).</sub>

## Why krpii

Pattern matching alone flags every long number: order IDs, tracking numbers, amounts. Each
false positive destroys text you meant to keep. krpii treats a pattern match as a candidate
and accepts it only after validation:

- **Checksums** reject card and business registration numbers that are merely the right length.
- **Birth-date checks** reject resident registration numbers dated February 30th or in the future.
- **Context labels** recognize "주문번호 2026…" as an order ID, not a card number.
- **Korean-aware boundaries** find `010-1234-5678로` even when a particle is attached, where
  word-boundary regexes fail.

Shipped as a single self-contained binary with no runtime to install, it is cheap enough to
run on every prompt, every file read, every log line.

## Installation

Download a prebuilt binary for macOS, Linux, or Windows from the
[latest release](https://github.com/jisu217/krpii/releases/latest), or build from source with
Rust 1.85 or later:

```console
$ cargo install krpii
```

## Usage

### Command line

```console
$ krpii mask [FILE] [--style partial|tag] [--types phone,rrn,...] [--min-score 0.9]
$ krpii scan [FILE] [--types ...] [--min-score ...]
```

`mask` rewrites the input. `scan` reports each entity as a JSON line and **never prints the
matched value**:

```json
{"type":"PHONE","start":10,"end":23,"score":0.95}
```

Exit codes follow `grep`: `0` when nothing is found, `1` when PII is found, `2` on error. This
makes krpii a drop-in guard for shell scripts, CI jobs, and editor hooks.

### Library

```console
$ cargo add krpii-core
```

```rust
use krpii_core::{mask, Detector, EntityType, MaskStyle};

assert_eq!(mask("연락처 010-1234-5678", MaskStyle::Partial), "연락처 010-****-5678");

let detector = Detector::new()
    .with_types([EntityType::Rrn, EntityType::Card])
    .with_min_score(0.9);
let entities = detector.detect("...");
```

An `Entity` holds only its byte span and confidence score, never the value itself, so logging
one cannot leak the data it describes.

## Supported entities

| Entity | Tag | Validation | Masked as |
|---|---|---|---|
| Phone (mobile, landline, VoIP) | `PHONE` | Subscriber prefixes only; excludes 1588 and 080 | `010-****-5678` |
| E-mail address | `EMAIL` | RFC 5322 subset | `h***@example.com` |
| Payment card | `CARD` | Luhn checksum | `4111-11**-****-1111` |
| Resident registration number | `RRN` | Real, non-future birth date; checksum as a confidence signal | `900101-*******` |
| Business registration number | `BRN` | National Tax Service checksum | `123-45-*****` |

Resident registration numbers issued since October 2020 no longer follow the legacy checksum,
so a mismatch lowers confidence instead of rejecting the number.

## How it works

```
text ─▶ candidates ─▶ validation ─▶ context ─▶ overlap resolution ─▶ entities
        permissive    checksums,     "주문번호"   highest confidence
        patterns      birth dates    → not PII    wins
```

Patterns run on Rust's `regex` crate, which guarantees linear-time matching, so no input can
trigger catastrophic backtracking.

## Roadmap

- [ ] Claude Code plugin that guards prompts and file reads
- [ ] Bank account numbers, detected through surrounding context
- [ ] Python bindings: `pip install krpii`
- [ ] Address detection and reversible masking

## Contributing

```console
$ cargo test --workspace
$ cargo clippy --workspace --all-targets -- -D warnings
```

Never commit real personal data. Every identifier in this repository is synthetic.

## License

[MIT](LICENSE). **KR** (the ISO country code for Korea) + **PII**.
