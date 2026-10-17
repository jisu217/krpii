# krpii

Korean PII and credential detection for AI coding workflows.

Logs, support tickets, `.env` files, and data samples can carry personal identifiers and
credentials into an agent session. krpii checks submitted prompts and file reads in Claude Code,
blocking supported values before they reach the model through those paths. Detection runs locally without a model download.

### Claude Code

```sh
cargo install krpii
claude plugin marketplace add jisu217/krpii
claude plugin install krpii@krpii
```

The plugin blocks submitted prompts containing detected PII or credentials and denies `Read`
calls on scanned files containing them. A denied read suggests a command to produce masked output; the plugin does
not create a masked copy automatically.

The `krpii` binary must be on `PATH` and include the `hook` subcommand.
Building from source requires Rust 1.85+.

### Features

- **Korean-aware boundaries.** Detects identifiers followed directly by Korean particles.
- **Validated matches.** Checks checksums, birth dates, and separator consistency.
- **Context filtering.** Rejects numeric matches labelled as order, tracking, or customer IDs.
- **Credential detection.** Recognizes provider API keys and tokens, private key blocks, and
  passwords in database URLs, and ignores documentation placeholders.

Supports Korean phone numbers, email addresses, payment cards, resident registration numbers,
business registration numbers, and credentials with fixed-prefix formats.

### CLI & library

```sh
krpii mask prompt.txt --style tag --types phone,email,rrn
krpii mask application.log                 # Partial masking
krpii scan application.log                 # JSON Lines, without matched values
```

Omit the file or use `-` to read stdin. For Rust applications, install the library with
`cargo add krpii-core`.

`scan` reports UTF-8 byte spans and exits with `1` when it finds a match, `0` otherwise.
`mask` exits with `0` on success; both use `2` for usage or I/O errors.

### Evaluation

10,000 synthetic Korean sentences · Exact type and span matching · Seed `42`

| System | PII F1 | Decoys flagged | Per invocation | Install size |
|---|--:|--:|--:|--:|
| Regex-only baseline | 0.930 | 27.3% | — | — |
| Presidio 2.2.364 | 0.866 | 11.0% | 1,093 ms | 176.3 MiB |
| krpii | 0.998 | 0.5% | 5 ms | 1.9 MiB |

PII F1 covers the five Korean PII types. Credentials are scored separately because Presidio has no
credential recognizers: krpii reaches F1 1.000 on 1,948 synthetic keys, tokens, private keys, and
connection-string passwords, and flags none of the documentation placeholder keys.

Per-invocation cost is the median of 10 fresh processes scanning one sentence on Apple Silicon,
the cost an editor hook pays on every event.

The corpus and detector share an author; these results do not establish real-world accuracy.
The comparison uses a Korean-configured Presidio adapter and a separate regex baseline, with
shared scoring. Service numbers are treated as decoys under this benchmark's policy.

```sh
cargo run --release --locked -p krpii-bench
```

### Limitations

The plugin checks submitted prompts and `Read` calls only. Shell and search-tool output is not
inspected; binary files and files larger than 16 MiB are skipped. Blocked prompts may still
appear in local history or transcripts. This is a guard for specific input paths, not a complete
boundary against data exposure.

Names, addresses, and unsupported formats remain unchanged; context labels can suppress valid
matches. Credential detection covers fixed-prefix formats only, so generic passwords and
unprefixed keys are not detected. Partial masking retains identifying information. Tag mode removes detected values
entirely, while undetected text remains unchanged in both modes.

[MIT License](LICENSE)
