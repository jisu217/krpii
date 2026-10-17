"""Measure per-invocation latency and install size of krpii and Presidio.

Editor hooks start a fresh process for every event, so the relevant latency is
a cold run that loads the tool and scans one short text, not steady-state
throughput. Each tool is invoked as a hook would invoke it. After one discarded
warm-up run per tool, the tools are run in alternation so that any change in
system load affects both equally, and the median and interquartile range of
the wall-clock times are reported.

Run with the interpreter of the virtual environment that has Presidio
installed::

    python startup.py --krpii ../../target/release/krpii
"""

from __future__ import annotations

import argparse
import statistics
import subprocess
import sys
import sysconfig
import time
from pathlib import Path

SAMPLE = "홍길동 010-1234-5678, 주민번호 900101-1234568로 가입했습니다."
PREDICT_SCRIPT = Path(__file__).with_name("predict.py")


def time_once(command: list[str]) -> float:
    """Return the wall-clock seconds of one fresh invocation."""
    started = time.perf_counter()
    # krpii exits with 1 when it finds PII, so the exit code is not checked.
    subprocess.run(command, input=SAMPLE, text=True, capture_output=True)
    return time.perf_counter() - started


def time_alternating(tools: dict[str, list[str]], runs: int) -> dict[str, list[float]]:
    """Time ``runs`` invocations of each tool, alternating between the tools."""
    for command in tools.values():
        time_once(command)  # Warm-up: loads the binaries and libraries into the page cache.
    durations: dict[str, list[float]] = {name: [] for name in tools}
    for _ in range(runs):
        for name, command in tools.items():
            durations[name].append(time_once(command))
    return durations


# Packaging tools present in every virtual environment but unused at runtime.
PACKAGING_PREFIXES = ("pip", "setuptools", "wheel", "_distutils_hack", "distutils-precedence")


def runtime_size(site_packages: Path) -> int:
    """Return the size in bytes of everything under ``site_packages`` except packaging tools."""
    total = 0
    for entry in site_packages.iterdir():
        if entry.name.startswith(PACKAGING_PREFIXES):
            continue
        files = [entry] if entry.is_file() else entry.rglob("*")
        total += sum(f.stat().st_size for f in files if f.is_file() and not f.is_symlink())
    return total


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--krpii", required=True, type=Path, help="path to a release krpii binary")
    parser.add_argument("--runs", type=int, default=100, help="invocations per tool")
    args = parser.parse_args()

    tools = {
        "krpii": [str(args.krpii), "scan"],
        "Presidio": [sys.executable, str(PREDICT_SCRIPT), "--stdin"],
    }
    sizes = {
        "krpii": args.krpii.stat().st_size,
        # Everything Presidio needs at runtime: the analyzer, spaCy, the Korean
        # model, and their dependencies. The interpreter itself is excluded.
        "Presidio": runtime_size(Path(sysconfig.get_paths()["purelib"])),
    }

    print(f"{args.runs} runs per tool, alternating, after one warm-up run each.\n")
    print("| Tool | Median per invocation | Interquartile range | Install size |")
    print("|---|--:|--:|--:|")
    for name, durations in time_alternating(tools, args.runs).items():
        q1, _, q3 = statistics.quantiles(durations, n=4)
        print(
            f"| {name} | {statistics.median(durations) * 1000:,.1f} ms "
            f"| {q1 * 1000:,.1f}-{q3 * 1000:,.1f} ms | {sizes[name] / 1024 / 1024:,.1f} MiB |"
        )


if __name__ == "__main__":
    main()
