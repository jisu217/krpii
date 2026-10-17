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
- **Context labels:** a number directly preceded by a label such as "주문번호" (order
  number) or "회원번호" (member number) is treated as a business identifier, not PII.

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

Measured on a synthetic corpus of 10,000 Korean sentences containing 11,990 PII values
and 7,937 decoys, 30% of which appear without a label such as "주문번호". A detection
counts as correct only when its type, start, and end all match exactly.

| Entity | Support | Precision | Recall | F1 |
|---|--:|--:|--:|--:|
| `PHONE` | 2,416 | 0.997 | 1.000 | 0.999 |
| `EMAIL` | 2,311 | 1.000 | 1.000 | 1.000 |
| `CARD` | 2,370 | 0.988 | 1.000 | 0.994 |
| `RRN` | 2,477 | 1.000 | 1.000 | 1.000 |
| `BRN` | 2,416 | 0.988 | 1.000 | 0.994 |
| **Overall (micro)** | 11,990 | 0.995 | 1.000 | 0.997 |

A recall of 1.000 means every notation in the corpus is supported; it does not imply that
every notation found in real text is.

Decoys that look numeric but are not PII:

| Decoy | False-positive rate |
|---|--:|
| Tracking numbers, amounts, timestamps, product codes, service numbers, postal codes, IPv4 addresses | 0.0% |
| 16-digit order IDs (detected as `CARD`) | 3.2% |
| 10-digit member IDs (detected as `BRN` or `PHONE`) | 4.0% |

Every remaining false positive is an unlabelled ID. When a label such as "주문번호" or
"회원번호" directly precedes a number, krpii treats it as a business identifier.

### Why validation matters

The same corpus, scored against a regex-only baseline that uses krpii's candidate
patterns and digit-boundary rule but skips every validation step:

| Entity | Precision (regex only) | Precision (krpii) | Recall (regex only) | Recall (krpii) |
|---|--:|--:|--:|--:|
| `PHONE` | 0.890 | 0.997 | 1.000 | 1.000 |
| `EMAIL` | 1.000 | 1.000 | 1.000 | 1.000 |
| `CARD` | 0.733 | 0.988 | 1.000 | 1.000 |
| `RRN` | 1.000 | 1.000 | 1.000 | 1.000 |
| `BRN` | 0.734 | 0.988 | 1.000 | 1.000 |
| **Overall (micro)** | 0.855 | 0.995 | 1.000 | 1.000 |

**Validation cuts decoys flagged as PII from 25.6% to 0.8% at no cost to recall.** The
corpus does not yet contain 13-digit decoys shaped like resident registration numbers, so
the effect of birth-date validation is not reflected here.

### Comparison with Presidio

[Presidio](https://github.com/data-privacy-stack/presidio) is the most widely used open-source
PII detector and ships Korean recognizers. It was scored on the same corpus by the same code.

| System | Precision | Recall | F1 | Decoys flagged as PII |
|---|--:|--:|--:|--:|
| Regex only | 0.855 | 1.000 | 0.922 | 25.6% |
| Presidio 2.2.364 | 0.899 | 0.826 | 0.861 | 13.8% |
| **krpii** | **0.995** | **1.000** | **0.997** | **0.8%** |

Per entity, precision / recall:

| Entity | Presidio 2.2.364 | krpii |
|---|--:|--:|
| `PHONE` | 0.716 / 0.954 | 0.997 / 1.000 |
| `EMAIL` | 1.000 / 0.668 | 1.000 / 1.000 |
| `CARD` | 0.995 / 0.549 | 0.988 / 1.000 |
| `RRN` | 1.000 / 0.945 | 1.000 / 1.000 |
| `BRN` | 0.957 / 1.000 | 0.988 / 1.000 |

Cost of one invocation, which is what an editor hook pays on every event (median of 10
fresh processes scanning one sentence on an Apple Silicon Mac):

| Tool | Per invocation | Install size |
|---|--:|--:|
| **krpii** | **6 ms** | **1.8 MiB** |
| Presidio 2.2.364 | 1,511 ms | 176.3 MiB |

**Setup.** Presidio ships its Korean recognizers disabled, so every Korean recognizer was
enabled, the card and e-mail recognizers were configured for Korean, and phone parsing was
set to the KR region, with the `ko_core_news_sm` 3.8.0 spaCy model. This is Presidio's best
available Korean configuration, not its default. Its default score threshold was kept,
overlapping results were resolved with krpii's own rule, and unbalanced edge brackets were
trimmed from every system's spans before matching.

**Where the differences come from.**

- Card numbers and e-mail addresses followed directly by a Korean particle, as in
  `…@example.com으로`, were not detected. Every one of the 814 card numbers followed by
  Hangul was missed.
- Business service numbers (`1588-xxxx`) and toll-free `080` numbers are reported as phone
  numbers. This benchmark treats them as non-personal, which accounts for most of Presidio's
  `PHONE` false positives. Under a policy that masks every phone number, its precision would
  be higher.
- Resident registration numbers separated by a space are not supported.
- The driver-licence recognizer flagged 78 twelve-digit parcel tracking numbers.

Known limitations, shown directly by these numbers:

- **Unlabelled digit runs.** About one in ten random 16-digit or 10-digit numbers passes the
  Luhn or business-number checksum by chance. Without a preceding label, nothing
  distinguishes such a number from a real card or business registration number.

Synthetic data cannot capture every real-world notation, so treat these figures as a
regression baseline rather than a guarantee. To reproduce them:

```console
$ cargo run --release -p krpii-bench
$ cargo run --release -p krpii-bench -- --seed 7 --samples 50000 --show-errors 5
```

To reproduce the Presidio comparison:

```console
$ python -m venv .venv && .venv/bin/pip install -r benchmarks/presidio/requirements.txt
$ cargo run --release -p krpii-bench -- --export-corpus target/corpus.jsonl
$ .venv/bin/python benchmarks/presidio/predict.py --corpus target/corpus.jsonl --output target/presidio.jsonl
$ cargo run --release -p krpii-bench -- --predictions "Presidio 2.2.364=target/presidio.jsonl"
$ cargo build --release -p krpii && .venv/bin/python benchmarks/presidio/startup.py --krpii target/release/krpii
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
│   └─ presidio/     # Scripts that run Presidio on the same corpus
└─ .github/workflows/
```

## Roadmap

- [x] `krpii-core`: phone, e-mail, card, resident and business registration numbers
- [x] `krpii` CLI: `scan`, `mask`
- [x] Synthetic evaluation set with per-entity precision and recall (`benchmarks/`)
- [x] Comparison with Presidio on accuracy, latency, and install size
- [x] Context labels to suppress checksum collisions on order and member IDs
- [x] Parenthesized area codes in phone numbers
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
