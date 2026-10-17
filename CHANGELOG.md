# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

First public release.

### Added

- Detection of phone numbers, e-mail addresses, payment cards, resident registration numbers
  (including foreigners), and business registration numbers.
- Validation through checksums, birth-date checks, Korean-aware digit boundaries, and context
  labels such as "주문번호".
- Partial masking (`010-****-5678`) and tag masking (`<PHONE>`).
- `krpii` CLI with `scan` and `mask`, and the `krpii-core` library.
- Evaluation benchmark with a regex-only ablation and a comparison against Presidio.
- Prebuilt binaries for macOS, Linux, and Windows.

[Unreleased]: https://github.com/jisu217/krpii/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jisu217/krpii/releases/tag/v0.1.0
