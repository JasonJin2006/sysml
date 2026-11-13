#!/usr/bin/env python3
"""Count clippy warnings for the library and binary targets, and check the count against the baseline.

The count is the number of distinct warnings: lint, file, line, and the first 60 characters of the
message. A warning that cargo reports for several targets is counted once. The gate fails when the
count is above gate/clippy.baseline. The baseline is lowered with --update once warnings are fixed,
and a person raises it by editing the file, never by running a script.

    python tools/clippy_count.py            # check against the baseline (exit 1 when above it)
    python tools/clippy_count.py --update   # record the current count as the baseline
"""

import argparse
import collections
import json
import os
import subprocess
import sys

CORE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(CORE, "gate", "clippy.baseline")


def warnings():
    proc = subprocess.run(
        ["cmd", "/c", os.path.join(CORE, "build.bat"), "clippy", "--workspace", "--message-format=json"],
        cwd=CORE,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    seen, by_lint = set(), collections.Counter()
    for line in proc.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get("reason") != "compiler-message":
            continue
        body = message["message"]
        if body["level"] != "warning":
            continue
        lint = (body.get("code") or {}).get("code") or "(none)"
        primary = next((span for span in body["spans"] if span["is_primary"]), {})
        key = (lint, primary.get("file_name", ""), primary.get("line_start", 0), body["message"][:60])
        if key not in seen:
            seen.add(key)
            by_lint[lint] += 1
    return len(seen), by_lint, proc.returncode, proc.stderr


def main():
    parser = argparse.ArgumentParser(description="Clippy warning count against gate/clippy.baseline.")
    parser.add_argument("--update", action="store_true", help="record the current count as the baseline")
    args = parser.parse_args()
    count, by_lint, code, stderr = warnings()
    if code != 0:
        print(stderr[-2000:])
        print("clippy did not run to completion")
        return 2
    print(f"clippy warnings {count} (library and binary targets)")
    for lint, n in by_lint.most_common(8):
        print(f"  {n:3d}  {lint}")
    if args.update:
        with open(BASELINE, "w", encoding="utf-8", newline="\n") as f:
            f.write(f"{count}\n")
        print(f"recorded baseline {count}")
        return 0
    if not os.path.exists(BASELINE):
        print("no baseline recorded; run with --update")
        return 2
    with open(BASELINE, encoding="utf-8") as f:
        baseline = int(f.read().split()[0])
    if count > baseline:
        print(f"above baseline {baseline} by {count - baseline}")
        return 1
    print(f"baseline {baseline}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
