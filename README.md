# krpii

Korean PII detection and masking for LLM prompts and logs. Runs locally as a single binary,
with a Rust library for embedding.

### Features

- **Korean-aware boundaries.** Detects identifiers followed directly by Korean particles.
- **Validated matches.** Checks checksums, birth dates, and separator consistency.
- **Context filtering.** Rejects numeric matches labelled as order, tracking, or customer IDs.

Supports Korean phone numbers, email addresses, payment cards, resident registration numbers,
and business registration numbers.

### Quick start

```sh
cargo install krpii

krpii mask prompt.txt --style tag --types phone,email,rrn
krpii mask application.log                 # Partial masking
krpii scan application.log                 # JSON Lines, without matched values
```

Building from source requires Rust 1.85+. Omit the file or use `-` to read stdin.
For Rust applications, install the library with `cargo add krpii-core`.

`scan` reports UTF-8 byte spans and exits with `1` when it finds a match, `0` otherwise.
`mask` exits with `0` on success; both use `2` for usage or I/O errors.

### Evaluation

10,000 synthetic Korean sentences · Exact type and span matching · Seed `42`

| System | F1 | Decoys flagged | Per invocation | Install size |
|---|--:|--:|--:|--:|
| Regex-only baseline | 0.922 | 25.6% | — | — |
| Presidio 2.2.364 | 0.861 | 13.8% | 1,511 ms | 176.3 MiB |
| krpii | 0.997 | 0.8% | 6 ms | 1.8 MiB |

Per-invocation cost is the median of 10 fresh processes scanning one sentence on Apple Silicon,
the cost an editor hook pays on every event.

The corpus and detector share an author; these results do not establish real-world accuracy.
The comparison uses a Korean-configured Presidio adapter and a separate regex baseline, with
shared scoring. Service numbers are treated as decoys under this benchmark's policy.

```sh
cargo run --release --locked -p krpii-bench
```

### Limitations

Names, addresses, and unsupported formats remain unchanged; context labels can suppress valid
matches. Partial masking retains identifying information. Tag mode removes detected values
entirely, while undetected text remains unchanged in both modes.

[MIT License](LICENSE)
