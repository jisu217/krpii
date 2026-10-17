# krpii

Credential and Korean PII detection for AI coding.

krpii checks prompts, file reads, and command output for API keys, tokens, and private keys, and
for Korean identifiers such as resident registration, passport, and card numbers, before they
reach the model.

Existing detectors such as Presidio load a language model and take over a second per run, too slow
for a hook that fires on every tool call. krpii is a 2 MiB binary that starts and scans in about
6 ms.

### Claude Code plugin

1. Install the `krpii` binary. Building from source requires Rust 1.85+.

   ```sh
   cargo install krpii
   ```

   No Rust? Download a prebuilt binary for macOS, Linux, or Windows from
   [Releases](https://github.com/jisu217/krpii/releases) and put it on `PATH`.

2. Add the marketplace.

   ```sh
   claude plugin marketplace add jisu217/krpii
   ```

3. Install the plugin.

   ```sh
   claude plugin install krpii@krpii
   ```

| Path | What the plugin does |
|:--|:--|
| Submitted prompt | Blocks it if it contains credentials or PII |
| `Read` | Denies the read and suggests a command that produces masked output |
| `Bash` and `Grep` output | Replaces each detected value with a tag such as `<RRN>` before Claude sees it |

The plugin runs `krpii hook`, so the `krpii` binary must be on `PATH`.

### Command line

The binary installed in step 1 above also works on its own.

```sh
krpii mask prompt.txt --style tag --types rrn,card,secret
krpii mask application.log                 # Partial masking
krpii scan application.log                 # JSON Lines, without matched values
```

Omit the file or use `-` to read stdin. `--types` takes any of `phone`, `email`, `card`, `rrn`,
`brn`, `passport`, `driver_license`, and `secret`, and `--min-score` sets a confidence threshold
from 0 to 1.

`scan` reports UTF-8 byte spans and exits with `1` when it finds a match, `0` otherwise. `mask`
exits with `0` on success; both use `2` for usage or I/O errors. In an interactive terminal, a
one-line summary goes to standard error; `--quiet` hides it.

### Rust library

```sh
cargo add krpii-core
```

`krpii-core` provides `detect`, `mask`, and a configurable `Detector`. An `Entity` holds only its
byte span and confidence score, never the matched value.

### Evaluation

10,000 synthetic Korean sentences · Exact type and span matching · Seed `42`

| System | PII F1 | Decoys flagged | Per invocation | Install size |
|:--|--:|--:|--:|--:|
| **krpii** | **0.986** | **0.5%** | **6.4 ms** | **2.0 MiB** |
| krpii without validation | 0.908 | 33.8% | — | — |
| Presidio 2.2.364 | 0.857 | 17.3% | 1,520 ms | 176.3 MiB |

- **PII F1** covers the seven Korean PII types. Credentials are scored separately because
  Presidio has no credential recognizers: krpii reaches F1 1.000 on 1,594 synthetic keys, tokens,
  private keys, and connection-string passwords, and flags none of the documentation placeholders.
- **Per invocation** is the median of 100 fresh processes scanning one sentence on Apple Silicon,
  alternating between the tools. It is the cost an editor hook pays on every event.
- **Comparison.** *krpii without validation* uses the same patterns but skips checksums, date
  checks, and context labels. Presidio runs with its Korean recognizers enabled and configured for
  Korean. Every system is scored by the same code, and service numbers count as decoys.

> **Note:** The corpus and detector share an author, so these results do not establish
> real-world accuracy.

```sh
cargo run --release --locked -p krpii-bench
```

### Limitations

- **Plugin.** Only submitted prompts, `Read` calls, and `Bash` and `Grep` output are checked;
  other tools, such as MCP servers, are not. Masking output changes only what Claude sees: the
  command has already run, and Claude Code telemetry records the original output. Blocked
  prompts may remain in local history or transcripts, and the `Read` check skips binary files and
  files larger than 16 MiB.
- **Detection.** Names, addresses, and unsupported formats are not detected, and a label such as
  "order number" before a value marks it as a business identifier, which can suppress a real one.
  Credentials are limited to fixed-prefix formats. Previous-format passport numbers and
  unseparated license numbers need a nearby keyword such as "passport" or "license", because
  without one they look like serials and tracking numbers; Presidio recalls more of them at the
  cost of flagging those look-alikes.
- **Masking.** Partial masking keeps part of each value; tag mode removes it entirely. Undetected
  text is left unchanged in both modes.

> **Note:** krpii guards specific input paths. It is not a complete boundary against data
> exposure.

[MIT License](LICENSE)
