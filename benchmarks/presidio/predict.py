"""Run Presidio over a krpii benchmark corpus.

Reads the JSON Lines corpus written by ``krpii-bench --export-corpus`` and
writes predictions in the format read by ``krpii-bench --predictions``.

Presidio ships its Korean recognizers disabled and has no Korean configuration
for its generic card, e-mail, and phone recognizers. This script enables all of
them for Korean, with phone parsing set to the KR region, so the comparison
reflects Presidio's best available Korean setup rather than its defaults.

Usage::

    python predict.py --corpus corpus.jsonl --output presidio.jsonl
    echo "연락처 010-1234-5678" | python predict.py --stdin
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections.abc import Iterable

from presidio_analyzer import AnalyzerEngine, RecognizerRegistry, RecognizerResult
from presidio_analyzer.nlp_engine import NlpEngineProvider
from presidio_analyzer.predefined_recognizers import (
    CreditCardRecognizer,
    EmailRecognizer,
    KrBrnRecognizer,
    KrDriverLicenseRecognizer,
    KrFrnRecognizer,
    KrPassportRecognizer,
    KrRrnRecognizer,
    PhoneRecognizer,
)

LANGUAGE = "ko"
SPACY_MODEL = "ko_core_news_sm"

# Presidio entity -> krpii entity label. krpii's RRN covers foreigners, so
# KR_FRN maps to it as well. Any unmapped type is passed through and scored as
# a type krpii does not support.
ENTITY_MAP = {
    "PHONE_NUMBER": "PHONE",
    "EMAIL_ADDRESS": "EMAIL",
    "CREDIT_CARD": "CARD",
    "KR_RRN": "RRN",
    "KR_FRN": "RRN",
    "KR_BRN": "BRN",
    "KR_PASSPORT": "PASSPORT",
    "KR_DRIVER_LICENSE": "DRIVER_LICENSE",
}


def build_analyzer() -> AnalyzerEngine:
    """Create an analyzer with every Korean-capable recognizer enabled."""
    nlp_engine = NlpEngineProvider(
        nlp_configuration={
            "nlp_engine_name": "spacy",
            "models": [{"lang_code": LANGUAGE, "model_name": SPACY_MODEL}],
        }
    ).create_engine()

    registry = RecognizerRegistry(supported_languages=[LANGUAGE])
    # The language is passed explicitly because the defaults differ: the
    # passport recognizer defaults to "kr" while the others default to "ko",
    # and a recognizer whose language does not match the analyzer never runs.
    for recognizer in (
        KrRrnRecognizer(supported_language=LANGUAGE),
        KrFrnRecognizer(supported_language=LANGUAGE),
        KrBrnRecognizer(supported_language=LANGUAGE),
        KrDriverLicenseRecognizer(supported_language=LANGUAGE),
        KrPassportRecognizer(supported_language=LANGUAGE),
        CreditCardRecognizer(supported_language=LANGUAGE),
        EmailRecognizer(supported_language=LANGUAGE),
        PhoneRecognizer(supported_language=LANGUAGE, supported_regions=("KR",)),
    ):
        registry.add_recognizer(recognizer)

    return AnalyzerEngine(nlp_engine=nlp_engine, registry=registry, supported_languages=[LANGUAGE])


def resolve_overlaps(results: Iterable[RecognizerResult]) -> list[RecognizerResult]:
    """Keep the strongest result wherever spans overlap.

    Applies the same rule krpii uses (highest score, then longest span, then
    earliest start), so neither tool is penalized merely for reporting
    overlapping candidates.
    """
    ranked = sorted(results, key=lambda r: (-r.score, -(r.end - r.start), r.start))
    accepted: list[RecognizerResult] = []
    for candidate in ranked:
        if all(candidate.end <= kept.start or kept.end <= candidate.start for kept in accepted):
            accepted.append(candidate)
    return sorted(accepted, key=lambda r: r.start)


def byte_offsets(text: str) -> list[int]:
    """Map each character index of ``text`` (and its end) to a UTF-8 byte offset."""
    offsets = [0]
    for char in text:
        offsets.append(offsets[-1] + len(char.encode("utf-8")))
    return offsets


def predict(analyzer: AnalyzerEngine, text: str) -> list[dict[str, object]]:
    """Return Presidio's detections for ``text`` in krpii's exchange format."""
    offsets = byte_offsets(text)
    return [
        {
            "type": ENTITY_MAP.get(result.entity_type, result.entity_type),
            "start": offsets[result.start],
            "end": offsets[result.end],
            "score": result.score,
        }
        for result in resolve_overlaps(analyzer.analyze(text=text, language=LANGUAGE))
    ]


def run_corpus(analyzer: AnalyzerEngine, corpus_path: str, output_path: str) -> None:
    started = time.perf_counter()
    count = 0
    with (
        open(corpus_path, encoding="utf-8") as corpus,
        open(output_path, "w", encoding="utf-8") as output,
    ):
        for line in corpus:
            sample = json.loads(line)
            entities = predict(analyzer, sample["text"])
            output.write(json.dumps({"id": sample["id"], "entities": entities}) + "\n")
            count += 1
    elapsed = time.perf_counter() - started
    print(f"Analyzed {count} samples in {elapsed:.1f} s", file=sys.stderr)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--corpus", help="corpus JSON Lines written by krpii-bench")
    parser.add_argument("--output", help="path for predictions JSON Lines")
    parser.add_argument(
        "--stdin",
        action="store_true",
        help="analyze a single text from standard input, as an editor hook would",
    )
    args = parser.parse_args()

    if args.stdin:
        print(json.dumps(predict(build_analyzer(), sys.stdin.read()), ensure_ascii=False))
    elif args.corpus and args.output:
        run_corpus(build_analyzer(), args.corpus, args.output)
    else:
        parser.error("pass either --stdin or both --corpus and --output")


if __name__ == "__main__":
    main()
