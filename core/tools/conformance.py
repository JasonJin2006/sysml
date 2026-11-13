"""Run the official validation models through `check`, one project per directory.

Each numbered directory in the corpus is one multi-file project: the models in it import each
other, and loading them one at a time makes every cross-file import unresolved. That is a
property of how the corpus is organised, not of the model, so the runner loads a directory's
files together and reports the project's result.

Usage:  python tools/conformance.py [--bin PATH] [--root PATH] [-v]
"""
import argparse
import collections
import json
import os
import subprocess
import sys


def find_defaults():
    here = os.path.dirname(os.path.abspath(__file__))
    core = os.path.dirname(here)
    project = os.path.dirname(core)
    return (
        os.path.join(core, "target", "release", "sysml.exe"),
        os.path.join(project, "research", "upstream", "release", "sysml", "src"),
    )


def projects(root):
    """One project per leaf directory: every .sysml file in it, loaded together."""
    out = []
    for dirpath, dirnames, names in os.walk(root):
        dirnames.sort()
        files = sorted(
            os.path.join(dirpath, n) for n in names if n.endswith(".sysml")
        )
        if files:
            out.append((os.path.relpath(dirpath, root), files))
    out.sort()
    return out


def main():
    bin_path, root = find_defaults()
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=bin_path)
    ap.add_argument("--root", default=root)
    ap.add_argument("--verbose", "-v", action="store_true")
    args = ap.parse_args()

    if not os.path.exists(args.bin):
        sys.exit(f"no binary at {args.bin}; run build.bat build --release first")

    totals = collections.Counter()
    markers = collections.Counter()
    codes = collections.Counter()
    rows = []

    for name, files in projects(args.root):
        r = subprocess.run(
            [args.bin, "check", *files, "--json"], capture_output=True, text=True
        )
        if r.stderr.strip().startswith("error:"):
            totals["parse-failed"] += 1
            rows.append((name, "PARSE-FAILED: " + r.stderr.strip()[:80]))
            continue
        try:
            doc = json.loads(r.stdout.lstrip("\ufeff"))
        except json.JSONDecodeError:
            totals["bad-json"] += 1
            rows.append((name, "BAD-JSON"))
            continue
        totals["projects"] += 1
        totals["files"] += len(files)
        semantic_errors = doc.get("semanticErrors", 0)
        not_modelled = len(doc.get("notModelled", []))
        if semantic_errors == 0:
            totals["semantic-clean"] += 1
        if not_modelled == 0:
            totals["no-unmodelled"] += 1
        for entry in doc.get("notModelled", []):
            markers[entry["construct"]] += 1
        for issue in doc.get("issues", []):
            codes[issue["code"]] += 1
        verdicts = ",".join(sorted({v["verdict"] for v in doc.get("verdicts", [])})) or "-"
        rows.append(
            (name, f"files={len(files)} semantic-errors={semantic_errors} "
                   f"not-modelled={not_modelled} verdicts={verdicts}")
        )

    print(f"projects {totals['projects']}  files {totals['files']}  "
          f"parse-failed {totals['parse-failed']}  bad-json {totals['bad-json']}")
    print(f"semantic-clean {totals['semantic-clean']}  "
          f"no-unmodelled {totals['no-unmodelled']}")
    print(f"not-modelled total {sum(markers.values())}  {dict(markers.most_common())}")
    print(f"issue codes {dict(codes.most_common())}")
    if args.verbose:
        print()
        for name, line in rows:
            print(f"{name[:52]:54} {line}")


if __name__ == "__main__":
    main()
