#!/usr/bin/env python
"""Parse and render at scale.

The largest model this project has run by hand is 136 elements, which says nothing about where the
kernel stops being usable. This generates models of a requested element count and times one pass of
each stage on them:

    outline      the parser, and the tree it produces
    check        the parser plus every semantic rule and the ledger gate
    graph        the projection, one run per view
    render       layout plus SVG for the general view
    studio       a running `sysml studio`: the document a load waits for, and the poll

    python tools/bench.py [--sizes 1000,10000] [--repeat 3] [--json]
    python tools/bench.py --check            # compare against gate/bench.json

Timings are best-of-N wall clock. Absolute milliseconds say whether today is usable; milliseconds
per thousand elements say whether the work is still linear — a stage that stays flat per thousand is
holding its shape, and one that climbs is not.

`--check` is a ceiling, not a target: a run that takes more than `--factor` times the recorded
baseline fails, which catches a change that makes the kernel twice as slow without catching ordinary
noise. The default factor is deliberately loose.

Exit code is 0 when every run completes, 1 when `--check` finds a stage over its ceiling.
"""

import argparse
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

CORE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_BINARY = os.path.join(CORE, "target", "release", "sysml.exe")
BASELINE = os.path.join(CORE, "gate", "bench.json")
VIEWS = ("gv", "iv", "tv", "pv")

# A poll answers in a few milliseconds, and a scheduler hiccup is larger than that, so a ratio alone
# would fail on noise. These stages get the larger of the factor and this floor as their ceiling.
FLOORS_MS = {"studio:fingerprint": 100.0}


def model_of(target):
    """A model of about `target` elements with something for every view to lay out.

    Each block is a definition with two ports and a connection between them (the interconnection
    view), a requirement it is satisfied by (the traceability view), and a usage of it (the general
    view). Blocks are grouped into packages so the package view has a tree rather than one node.
    Element counts are approximate; the run reports what `outline` actually counted.
    """
    lines = [
        "package Bench {",
        '    private import ScalarValues::*;',
    ]
    block = 0
    while block * 11 < target:
        if block % 8 == 0:
            if block:
                lines.append("    }")
            lines.append(f"    package Layer{block // 8} {{")
        lines += [
            f"        part def D{block} {{",
            "            port p;",
            "            attribute mass : Real = 1.0;",
            "            part a {",
            "                port q;",
            "            }",
            "            part b {",
            "                port q;",
            "            }",
            "            connect a.q to b.q;",
            "        }",
            f"        part u{block} : D{block};",
            f"        requirement R{block} {{",
            "            attribute mass : Real = 1.0;",
            "        }",
            f"        satisfy R{block} by u{block};",
        ]
        block += 1
    lines += ["    }", "}", ""]
    return "\n".join(lines)


def measure(binary, path, argv, repeat):
    """Best of `repeat` runs, in milliseconds; `None` when the command failed."""
    best = None
    for _ in range(repeat):
        start = time.perf_counter()
        proc = subprocess.run(
            [binary] + argv + [path],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        elapsed = (time.perf_counter() - start) * 1000.0
        if proc.returncode not in (0, 1, 3):
            return None
        best = elapsed if best is None else min(best, elapsed)
    return best


def elements_of(binary, path):
    proc = subprocess.run(
        [binary, "outline", path], capture_output=True, text=True, encoding="utf8", errors="replace"
    )
    return sum(1 for line in proc.stdout.splitlines() if line.strip())


def _wait_for(opener, url, seconds=30.0):
    """Whether `url` answers within `seconds`. A server still starting is not an error."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            with opener.open(url, timeout=5) as response:
                response.read()
            return True
        except OSError:
            time.sleep(0.2)
    return False


def studio_timings(binary, path, repeat):
    """Milliseconds for the two requests the canvas makes, against a running `sysml studio`.

    The document is what a load and an edit wait for. The fingerprint is what the one-second poll
    asks. Both are timed over HTTP, best of `repeat`, so the figure includes the server's handling of
    the request and the loopback round trip, which the command-line stages never see. A server that
    does not come up gives `None` for both, so the stage fails rather than disappearing.
    """
    # No proxy: the request is to this machine, and an environment proxy would time the proxy.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    server = subprocess.Popen(
        [binary, "studio", path, "--port", str(port)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    timings = {"studio:document": None, "studio:fingerprint": None}
    try:
        base = f"http://127.0.0.1:{port}"
        if not _wait_for(opener, base + "/api/fingerprint"):
            return timings
        for stage, target in (
            ("studio:document", "/api/document?view=gv&mode=extended"),
            ("studio:fingerprint", "/api/fingerprint"),
        ):
            best = None
            for _ in range(repeat):
                start = time.perf_counter()
                try:
                    with opener.open(base + target, timeout=60) as response:
                        response.read()
                except OSError:
                    best = None
                    break
                elapsed = (time.perf_counter() - start) * 1000.0
                best = elapsed if best is None else min(best, elapsed)
            timings[stage] = best
        return timings
    finally:
        server.kill()
        server.wait()


def run_size(binary, target, repeat, workdir):
    path = os.path.join(workdir, f"bench-{target}.sysml")
    model = model_of(target)
    with open(path, "w", encoding="utf8", newline="\n") as fh:
        fh.write(model)
    elements = elements_of(binary, path)
    result = {
        "target": target,
        "elements": elements,
        "bytes": len(model.encode("utf8")),
        "ms": {},
    }
    stages = [("outline", ["outline"]), ("check", ["check"]), ("render", ["render", "--out", os.path.join(workdir, "view.svg")])]
    for view in VIEWS:
        stages.append((f"graph:{view}", ["graph", "--view", view]))
    for name, argv in stages:
        result["ms"][name] = measure(binary, path, argv, repeat)
    result["ms"].update(studio_timings(binary, path, repeat))
    result["path"] = path
    return result


def report(results):
    # Both tables name their unit. They used to be distinguished only by a header line that a
    # `tail -2` cuts off, and a per-1k figure read as a total is off by the size of the model —
    # which is how a 2-second check was reported as a 167ms one.
    print(f"{'total ms':>9} {'kB':>6}  " + "  ".join(f"{n:>10}" for n in results[0]["ms"]))
    for row in results:
        cells = []
        for value in row["ms"].values():
            cells.append("      fail" if value is None else f"{value:10.0f}")
        print(f"{row['elements']:9} {row['bytes'] / 1024:6.0f}  " + "  ".join(cells))
    print()
    print(f"{'ms per 1k elem':>15}  " + "  ".join(f"{n:>10}" for n in results[0]["ms"]))
    for row in results:
        cells = []
        for value in row["ms"].values():
            if value is None or not row["elements"]:
                cells.append("      fail")
            else:
                cells.append(f"{value / (row['elements'] / 1000.0):10.0f}")
        print(f"{row['elements']:15}  " + "  ".join(cells))


def compare(results, factor):
    with open(BASELINE, encoding="utf8") as fh:
        baseline = json.load(fh)
    failures = []
    for row in results:
        recorded = baseline.get(str(row["target"]))
        if not recorded:
            continue
        for stage, value in row["ms"].items():
            was = recorded.get(stage)
            if value is None:
                failures.append(f"{row['target']} {stage}: did not run")
            elif was and value > max(was * factor, FLOORS_MS.get(stage, 0.0)):
                failures.append(
                    f"{row['target']} {stage}: {value:.0f}ms, baseline {was:.0f}ms"
                )
    for failure in failures:
        print(f"over ceiling: {failure}")
    return not failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", default="1000,10000")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--binary", default=DEFAULT_BINARY)
    parser.add_argument("--factor", type=float, default=2.0)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--quiet", action="store_true", help="one line, for the gate")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--save", action="store_true", help="record this run as the baseline")
    args = parser.parse_args()

    if not os.path.exists(args.binary):
        print(f"no binary at {args.binary}; build with `build.bat build --release`")
        return 2

    sizes = [int(s) for s in args.sizes.split(",") if s.strip()]
    with tempfile.TemporaryDirectory() as workdir:
        results = [run_size(args.binary, size, args.repeat, workdir) for size in sizes]

    if args.json:
        print(json.dumps(results, indent=2))
    elif args.quiet:
        row = results[0]
        cells = []
        for stage in ("check", "render", "studio:document", "studio:fingerprint"):
            value = row["ms"].get(stage)
            cells.append(f"{stage} {'fail' if value is None else f'{value:.0f}ms'}")
        print(f"{row['elements']} elements: " + ", ".join(cells))
    else:
        report(results)

    if args.save:
        with open(BASELINE, "w", encoding="utf8") as fh:
            json.dump({str(r["target"]): r["ms"] for r in results}, fh, indent=2)
        print(f"saved baseline to {os.path.basename(BASELINE)}")

    if args.check:
        return 0 if compare(results, args.factor) else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
