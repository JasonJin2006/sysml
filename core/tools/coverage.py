"""Semantic coverage: one `check` per validation model, every other model attached as a library.

The corpus's numbered directories import each other, so a file checked alone reports every
cross-file import as unresolved. That is a property of how the corpus is organised, not of the
model, so every other file is attached as a library and the file under test is the only one
audited.

`--json` goes last on purpose: the CLI parses `--x value` pairs, and a flag placed before
`--library` would take `--library` as its value.

Every file is checked with its Xpect assertion blocks blanked (see `blank_xpect`). The corpus is an
Xpect suite, and `//* … */` is test syntax: the harness strips the block before parsing, and
`pilot.py` blanks the same regions when it scores a model. Checking the raw text instead reads
declarations that exist only inside an assertion, and those phantom members make real references
ambiguous — measured, 15 of the 20 REF001 this check produced came from them.

Usage:  python tools/coverage.py [-v] [--bin PATH] [--root PATH]
"""
import argparse
import collections
import concurrent.futures
import json
import os
import subprocess
import sys
import tempfile


def find_defaults():
    here = os.path.dirname(os.path.abspath(__file__))
    core = os.path.dirname(here)
    project = os.path.dirname(core)
    return (
        os.path.join(core, "target", "release", "sysml.exe"),
        os.path.join(project, "research", "upstream", "release", "sysml", "src"),
    )


# Defects in the official validation corpus, recorded with the evidence that each is the model's
# and not the checker's. Subtracting them keeps 56/56 meaningful; anything not listed here still
# fails the gate, and an entry that stops being reported fails it too.
#
# The entry for `04-Functional Allocation/4a` was removed when this tool started blanking Xpect
# blocks: the two `unresolved: 3a-Function-based Behavior::provide power` reports were about a
# `perform` written inside an assertion block, which the harness never parses. With the block
# blanked the file reports nothing, so the allowance had nothing left to subtract.
KNOWN_CORPUS_DEFECTS = {
    "validation/10-Analysis and Trades/10d-Dynamics Analysis.sysml": [
        {
            "code": "SEM077",
            "message": "`10d-Dynamics Analysis::AnalysisModel::DynamicsAnalysis::position` is "
                       "given an initial value with `:=` but is not `var`; only a variable can be "
                       "assigned (validateFeatureValueIsInitial)",
            "count": 1,
            "why": "`private attribute position := initialPosition;` writes `:=` on a feature that "
                   "is not `var` (line 62; `speed` at 63 is the same). `:=` sets `isInitial` and "
                   "nothing else (KerML.xtext:744-749, SysML.xtext:614-615), and `isVariable` "
                   "defaults to false with no redefinition (generated.rs:2245), so the model "
                   "violates `validateFeatureValueIsInitial` as the OCL states it. The rule is "
                   "right and the corpus model states the violation; the official tool does not "
                   "check this constraint, consistent with the 30 rules it reports as unevaluated.",
        },
        {
            "code": "SEM077",
            "message": "`10d-Dynamics Analysis::AnalysisModel::DynamicsAnalysis::speed` is given an "
                       "initial value with `:=` but is not `var`; only a variable can be assigned "
                       "(validateFeatureValueIsInitial)",
            "count": 1,
            "why": "`private attribute speed := initialSpeed;` at line 63, the same shape as "
                   "`position` above.",
        },
    ],
}


def blank_xpect(text):
    """Blank every line covered by a `//* … */` region, keeping the line count.

    Xpect writes a multi-assertion block that way, and the body is test syntax rather than model
    syntax: `"message" at "snippet"` is not a declaration. The harness strips the block before
    parsing, and `pilot.py` blanks the same regions, so a raw-text check measures something neither
    the reference implementation nor our own scorer ever sees.

    The lines are blanked rather than removed so that a diagnostic's line number still points at the
    line a reader would open.
    """
    out = []
    inside = False
    for line in text.split("\n"):
        if not inside:
            at = line.find("//*")
            if at < 0:
                out.append(line)
                continue
            prefix = line[:at]
            if "*/" not in line[at + 3:]:
                inside = True
            out.append(prefix)
            continue
        if "*/" in line:
            inside = False
        out.append("")
    return "\n".join(out)


def check_one(args, target, libraries, rel):
    """One `check` of `target`, with every other corpus file attached as a library.

    `target` and `libraries` are the staged copies (assertion blocks blanked); `rel` is the path to
    report, relative to the corpus root. Returns `(rel, issues, bad_json)`. Kept free of shared state
    so the callers can run it in parallel: the allowance subtraction that needs a shared counter
    happens afterwards, in the driver, where the files are already ordered.
    """
    cmd = [args.bin, "check", target]
    for other in libraries:
        cmd += ["--library", other]
    cmd.append("--json")
    run = subprocess.run(cmd, capture_output=True, text=True)
    try:
        doc = json.loads(run.stdout.lstrip("\ufeff"))
    except json.JSONDecodeError:
        return rel, None, f"BAD-JSON {target}: {run.stdout[:200]} {run.stderr[:200]}"
    issues = [
        (i["code"], i["message"])
        for i in doc.get("issues", [])
        if i.get("severity") == "error"
    ]
    return rel, issues, None


def main():
    bin_path, root = find_defaults()
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=bin_path)
    ap.add_argument("--root", default=root)
    ap.add_argument("--verbose", "-v", action="store_true")
    # Each check is a separate process that reloads the corpus, so this is a CPU-bound fan-out,
    # not I/O. The corpus is small; the reason to cap the default is memory per process rather
    # than cores.
    ap.add_argument("--jobs", "-j", type=int, default=min(os.cpu_count() or 4, 16))
    args = ap.parse_args()

    if not os.path.exists(args.bin):
        sys.exit(f"no binary at {args.bin}; run build.bat build --release first")

    files = sorted(
        os.path.join(dp, n)
        for dp, _, names in os.walk(args.root)
        for n in names
        if n.endswith(".sysml")
    )
    if not files:
        sys.exit(f"no .sysml under {args.root}")

    # `executor.map` yields in submission order, so `failed` stays in corpus order and the
    # verbose listing does not shuffle between runs.
    results = []
    with tempfile.TemporaryDirectory() as staging:
        # One staged copy per file, assertion blocks blanked, plus the relative path to report.
        # The copies are named by index rather than by basename: two corpus directories may hold a
        # file of the same name, and a library that shadows another would change the verdict.
        staged = []
        for number, path in enumerate(files):
            text = open(path, encoding="utf-8", errors="replace").read()
            copy = os.path.join(staging, f"{number}.sysml")
            with open(copy, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(blank_xpect(text))
            rel = os.path.relpath(path, args.root).replace(os.sep, "/")
            staged.append((rel, copy))
        copies = [copy for _rel, copy in staged]

        def run(item):
            rel, copy = item
            return check_one(args, copy, [p for p in copies if p != copy], rel)

        with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
            for result in pool.map(run, staged):
                results.append(result)

    clean = 0
    codes = collections.Counter()
    failed = []
    used = collections.Counter()
    for rel, issues, bad in results:
        if bad is not None:
            print(bad)
            continue
        # A known defect in the corpus is subtracted from the count, by name and by number: an
        # allowance that stops matching is reported as stale rather than quietly widening.
        allowed = KNOWN_CORPUS_DEFECTS.get(rel, [])
        remaining = []
        for code, message in issues:
            hit = next(
                (a for a in allowed if a["code"] == code and a["message"] == message
                 and used[(rel, code, message)] < a["count"]),
                None,
            )
            if hit is not None:
                used[(rel, code, message)] += 1
                continue
            remaining.append((code, message))
        errors = len(remaining)
        if errors == 0:
            clean += 1
        else:
            failed.append((rel, errors, remaining))
        for code, _ in remaining:
            codes[code] += 1

    print(f"{clean}/{len(files)} {dict(codes)}")
    stale = []
    for rel, entries in KNOWN_CORPUS_DEFECTS.items():
        for a in entries:
            if used[(rel, a["code"], a["message"])] != a["count"]:
                stale.append((rel, a["code"], a["message"]))
    if stale:
        print("STALE ALLOWANCES (the corpus no longer reports these):")
        for rel, code, message in stale:
            print(f"   {rel}  {code}  {message}")
        return 1
    if args.verbose:
        for name, errors, issues in failed:
            print(f"\n== {name}  errors={errors}")
            for code, message in issues:
                print(f"   {code}: {message}")
    return 0 if clean == len(files) else 1


if __name__ == "__main__":
    sys.exit(main())
