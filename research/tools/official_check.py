"""Run the official SysML v2 validation suite through this implementation.

Suite: `sysml/src/validation` from the Systems-Modeling/SysML-v2-Release repository
(56 files, 17 categories), checked out under research/upstream/release.

Per file:
  * `sysml outline`  — parse + lower. A non-zero exit is a parse failure; the message is
    grouped for the summary.
  * `sysml resolve`  — counts type references (and satisfy ends) that bind, do not bind,
    or bind ambiguously, plus imports whose target is not in the model or the bundled
    library.
  * if the file mentions `satisfy`, `sysml graph --mode extended` — how many satisfy
    edges the projection actually emits.

Usage:
    python research/tools/official_check.py [-v]
"""

from __future__ import annotations

import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / "core"
CLI = CORE / "target" / "debug" / "sysml.exe"
SUITE = ROOT / "research" / "upstream" / "release" / "sysml" / "src" / "validation"


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(CLI), *args], capture_output=True, text=True,
        encoding="utf-8", errors="replace")


def section(text: str, heading: str) -> list[str]:
    m = re.search(rf"^{heading}\n((?:  .*\n?)*)", text, re.M)
    if not m or "(none)" in m.group(1):
        return []
    return [line.strip() for line in m.group(1).splitlines() if line.strip()]


def main() -> None:
    files = sorted(SUITE.rglob("*.sysml"))
    if not files:
        sys.exit(f"no files under {SUITE}")

    verbose = "-v" in sys.argv
    parsed: list[tuple[Path, int]] = []          # (file, outline lines)
    errored: list[tuple[Path, str]] = []
    refs = Counter()                              # resolved / unresolved / ambiguous
    bad_imports = 0
    satisfy_files: list[tuple[Path, int, int]] = []  # (file, statements, edges drawn)

    for f in files:
        outline = run("outline", str(f))
        if outline.returncode != 0:
            errored.append((f, (outline.stderr or outline.stdout).strip()))
            continue
        parsed.append((f, len(outline.stdout.splitlines())))

        report = run("resolve", str(f)).stdout
        refs["resolved"] += len(section(report, "resolved"))
        refs["unresolved"] += len(section(report, "not resolved"))
        refs["ambiguous"] += sum(
            1 for line in section(report, "not resolved") if "ambiguous" in line)
        bad_imports += sum(
            1 for line in section(report, "imports") if "-> unresolved" in line)

        if "satisfy" in f.read_text(encoding="utf-8").lower():
            graph = run("graph", str(f), "--view", "gv", "--mode", "extended").stdout
            edges = len(re.findall(r"^edge \S+ --> \S+ : «satisfy»", graph, re.M))
            want = len(re.findall(r"\bsatisfy\b", f.read_text(encoding="utf-8")))
            satisfy_files.append((f, want, edges))

    # ---- report ------------------------------------------------------------
    print(f"files           : {len(files)}")
    print(f"parsed + lowered: {len(parsed)}  ({len(parsed) * 100 // len(files)}%)")
    print(f"parse errors    : {len(errored)}")

    if errored:
        groups = Counter(msg.splitlines()[-1] if msg else "(no message)" for _, msg in errored)
        print("\nparse errors, grouped by message:")
        for msg, n in groups.most_common():
            print(f"  {n:3d}  {msg[:120]}")
        if verbose:
            print("\nparse errors, by file:")
            for f, msg in errored:
                print(f"  {f.relative_to(SUITE)}: {msg.splitlines()[-1] if msg else ''}")

    total_refs = refs["resolved"] + refs["unresolved"]
    if total_refs:
        print(f"\nname resolution over the {len(parsed)} parsed files:")
        print(f"  references (type refs + satisfy ends) : {total_refs}")
        print(f"  resolved                              : {refs['resolved']}"
              f"  ({refs['resolved'] * 100 // total_refs}%)")
        print(f"  unresolved                            : {refs['unresolved']}")
        print(f"  ambiguous                             : {refs['ambiguous']}")
    print(f"  imports with unresolved target        : {bad_imports}")

    if satisfy_files:
        print("\nsatisfy statements vs edges drawn (extended general view):")
        for f, want, got in satisfy_files:
            print(f"  {f.relative_to(SUITE.parent)}: {got} edges / {want} statements")

    # per-category table
    print("\nby category:")
    cat = Counter()
    ok = Counter()
    for f, _ in parsed:
        ok[f.parent.name] += 1
    for f in files:
        cat[f.parent.name] += 1
    for name in sorted(cat):
        print(f"  {ok[name]:3d}/{cat[name]:<3d}  {name}")


if __name__ == "__main__":
    main()
