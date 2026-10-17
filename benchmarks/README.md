# Benchmarks

How krpii is evaluated, how it compares with a regex-only baseline and with Presidio, and
where it still falls short.

## Methodology

- **Corpus.** 10,000 Korean sentences containing 11,990 PII values and 7,937 decoys: order
  IDs, tracking numbers, member IDs, amounts, timestamps, product codes, service numbers,
  postal codes, and IPv4 addresses. 30% of decoys appear without a label such as "주문번호",
  so label rules are rewarded only where real text would provide labels.
- **Synthetic by design.** Every value is random and checksum-consistent where the format
  requires it. The corpus is regenerated from a seed on every run rather than stored, so the
  repository never ships thousands of valid-looking identifiers.
- **Strict scoring.** A detection counts only when its type, start, and end all match exactly.
  Unbalanced edge brackets are trimmed from every system's spans before matching, so
  `(010-1234-5678` still counts as the phone number it contains.
- **One scorer.** External tools receive the exported corpus and return predictions that the
  same Rust code scores, so every system is judged by identical rules.

## Results

| Entity | Support | Precision | Recall | F1 |
|---|--:|--:|--:|--:|
| `PHONE` | 2,416 | 0.997 | 1.000 | 0.999 |
| `EMAIL` | 2,311 | 1.000 | 1.000 | 1.000 |
| `CARD` | 2,370 | 0.988 | 1.000 | 0.994 |
| `RRN` | 2,477 | 1.000 | 1.000 | 1.000 |
| `BRN` | 2,416 | 0.988 | 1.000 | 0.994 |
| **Overall (micro)** | 11,990 | 0.995 | 1.000 | 0.997 |

| Decoy | Flagged as PII |
|---|--:|
| Tracking numbers, amounts, timestamps, product codes, service numbers, postal codes, IPv4 | 0.0% |
| 16-digit order IDs | 3.2% |
| 10-digit member IDs | 4.0% |

Every remaining false positive is an unlabelled ID that happens to pass a checksum.

## Ablation: why validation matters

A baseline that uses krpii's candidate patterns and boundary rules but skips every validation
step:

| Entity | Precision (regex only) | Precision (krpii) | Recall (regex only) | Recall (krpii) |
|---|--:|--:|--:|--:|
| `PHONE` | 0.890 | 0.997 | 1.000 | 1.000 |
| `EMAIL` | 1.000 | 1.000 | 1.000 | 1.000 |
| `CARD` | 0.733 | 0.988 | 1.000 | 1.000 |
| `RRN` | 1.000 | 1.000 | 1.000 | 1.000 |
| `BRN` | 0.734 | 0.988 | 1.000 | 1.000 |
| **Overall (micro)** | 0.855 | 0.995 | 1.000 | 1.000 |

**Validation cuts decoys flagged as PII from 25.6% to 0.8% at no cost to recall.**

## Comparison with Presidio

[Presidio](https://github.com/data-privacy-stack/presidio) is the most widely used open-source
PII detector and ships Korean recognizers.

| System | Precision | Recall | F1 | Decoys flagged as PII |
|---|--:|--:|--:|--:|
| Regex only | 0.855 | 1.000 | 0.922 | 25.6% |
| Presidio 2.2.364 | 0.899 | 0.826 | 0.861 | 13.8% |
| **krpii** | **0.995** | **1.000** | **0.997** | **0.8%** |

| Entity | Presidio 2.2.364 (P / R) | krpii (P / R) |
|---|--:|--:|
| `PHONE` | 0.716 / 0.954 | 0.997 / 1.000 |
| `EMAIL` | 1.000 / 0.668 | 1.000 / 1.000 |
| `CARD` | 0.995 / 0.549 | 0.988 / 1.000 |
| `RRN` | 1.000 / 0.945 | 1.000 / 1.000 |
| `BRN` | 0.957 / 1.000 | 0.988 / 1.000 |

Per-invocation cost, which an editor hook pays on every event (median of 10 fresh processes
scanning one sentence, Apple Silicon Mac):

| Tool | Per invocation | Install size |
|---|--:|--:|
| **krpii** | **6 ms** | **1.8 MiB** |
| Presidio 2.2.364 | 1,511 ms | 176.3 MiB |

**Setup.** Presidio ships its Korean recognizers disabled. All of them were enabled, the card
and e-mail recognizers were configured for Korean, phone parsing was set to the KR region, and
the `ko_core_news_sm` 3.8.0 spaCy model was used. This is Presidio's best available Korean
configuration, not its default. Its default score threshold was kept, and overlapping results
were resolved with krpii's own rule.

**Where the differences come from.**

- Card numbers and e-mail addresses followed directly by a Korean particle, as in
  `…@example.com으로`, are missed. All 814 card numbers followed by Hangul were missed.
- Business service numbers (`1588-xxxx`) and toll-free `080` numbers are reported as phone
  numbers. This benchmark treats them as non-personal; under a policy that masks every phone
  number, Presidio's precision would be higher.
- Space-separated resident registration numbers are not supported.
- The driver-licence recognizer flagged 78 twelve-digit parcel tracking numbers.

## Limitations

- **Self-authored corpus.** The formats were written by the same author as the detector, so a
  recall of 1.000 means every notation in the corpus is supported, not every notation in real
  text. Treat these figures as a regression baseline, not a guarantee.
- **Unlabelled IDs.** About one in ten random 16-digit or 10-digit numbers passes a checksum by
  chance. Without a label, nothing distinguishes it from a real card or business number.
- **Birth-date validation is not exercised**, because the corpus has no 13-digit decoys shaped
  like resident registration numbers.

## Reproducing

```console
$ cargo run --release -p krpii-bench
$ cargo run --release -p krpii-bench -- --seed 7 --samples 50000 --show-errors 5
```

Presidio comparison:

```console
$ python -m venv .venv && .venv/bin/pip install -r benchmarks/presidio/requirements.txt
$ cargo run --release -p krpii-bench -- --export-corpus target/corpus.jsonl
$ .venv/bin/python benchmarks/presidio/predict.py --corpus target/corpus.jsonl --output target/presidio.jsonl
$ cargo run --release -p krpii-bench -- --predictions "Presidio 2.2.364=target/presidio.jsonl"
$ cargo build --release -p krpii && .venv/bin/python benchmarks/presidio/startup.py --krpii target/release/krpii
```
