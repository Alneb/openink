#!/usr/bin/env python3
"""Onyx SDK public API vs impl/java/onyx-compat, by method name (docs/API-COVERAGE.md).

Names only: a covered name still needs its behaviour checked on the device. Obfuscated
single-letter internals (jadx keeps them public) are not API and are skipped.
Usage: sdk_coverage.py [repo root]   → markdown table rows on stdout (pipe through safecat.sh)
"""
import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
SDK = ROOT / "firmware/onyx/pen/knote2_src/sources/com/onyx/android/sdk"
COMPAT = ROOT / "impl/java/onyx-compat/src/main/java/org/openink/compat"
# (SDK class, its source, our class)
CLASSES = [
    ("TouchHelper", "pen/TouchHelper.java", "pen/TouchHelper.java"),
    ("TouchPointList", "geometry/data/TouchPointList.java", "pen/TouchPointList.java"),
    ("RawInputCallback", "pen/RawInputCallback.java", "pen/RawInputCallback.java"),
    ("EpdPenManager", "pen/EpdPenManager.java", "pen/EpdPenManager.java"),
    ("RawInputReader", "pen/RawInputReader.java", "pen/RawInputReader.java"),
    ("EpdController", "api/device/epd/EpdController.java", "epd/EpdController.java"),
]
PUBLIC = re.compile(r"^\s+(?:@\w+\s+)*public (?:static )?(?:final )?(?:synchronized )?(?:abstract )?[\w<>\[\], .?]+ (\w+)\(", re.M)


def names(path: Path) -> set[str]:
    if not path.exists():
        return set()
    return {n for n in PUBLIC.findall(path.read_text(errors="replace")) if len(n) > 1 and not re.fullmatch(r"m\d+\w*", n)}  # jadx renames: m35clone


def main():
    print("| class | covered | missing |\n|---|---|---|")
    total = have = 0
    for cls, sdk, ours in CLASSES:
        want, got = names(SDK / sdk), names(COMPAT / ours)
        want.discard(cls)  # constructors
        miss = sorted(want - got)
        total += len(want)
        have += len(want) - len(miss)
        print(f"| {cls} | {len(want) - len(miss)}/{len(want)} | {', '.join(f'`{m}`' for m in miss) or '—'} |")
    print(f"\n[sdk-coverage] {have}/{total} public names covered", file=sys.stderr)


if __name__ == "__main__":
    main()
