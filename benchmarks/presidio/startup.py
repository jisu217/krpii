"""Measure per-invocation latency and install size of krpii and Presidio.

Editor hooks start a fresh process for every event, so the relevant latency is
a cold run that loads the tool and scans one short text, not steady-state
throughput. Each tool is invoked as a hook would invoke it, several times, and
the median wall-clock time is reported.

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


def time_runs(command: list[str], runs: int) -> list[float]:
    """Return wall-clock seconds for each of ``runs`` fresh invocations."""
    durations = []
    for _ in range(runs):
        started = time.perf_counter()
        # krpii exits with 1 when it finds PII, so the exit code is not checked.
        subprocess.run(command, input=SAMPLE, text=True, capture_output=True)
        durations.append(time.perf_counter() - started)
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
    parser.add_argument("--runs", type=int, default=10, help="invocations per tool")
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

    print("| Tool | Median per invocation | Fastest | Install size |")
    print("|---|--:|--:|--:|")
    for name, command in tools.items():
        durations = time_runs(command, args.runs)
        print(
            f"| {name} | {statistics.median(durations) * 1000:,.0f} ms "
            f"| {min(durations) * 1000:,.0f} ms | {sizes[name] / 1024 / 1024:,.1f} MiB |"
        )


if __name__ == "__main__":
    main()
