#!/usr/bin/env python3
"""Turn the OMG pilot implementation's Xpect models into cases the kernel can run.

Each `.xt` file is a SysML model with inline assertions:

    // XPECT errors --> "message" at "source text"

This extracts the model (dropping the XPEСT_SETUP header and the assertion
comments) and the list of expected violations, so a rule can be checked against
the authoritative implementation's own verdicts instead of against a reading of
its OCL.

Usage:
    python tools/pilot_cases.py list                 # one line per file with counts
    python tools/pilot_cases.py emit <name> <out>    # write the model, print expectations
"""
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
PILOT = HERE.parent.parent / "research" / "pilot" / "models"
PAT = re.compile(r'XPECT\s+(errors?|warnings?)\s*-+>\s*(.*?)\s*at\s*"(.*?)"', re.S)
# The grouped spelling, `//* XPECT warnings ---` … `--- */`, carries one severity for a block and
# one assertion per line. Without it this tool emitted models whose expectations were missing every
# warning the suite writes that way.
BLOCK = re.compile(
    r'XPECT\s+(errors?|warnings?)\s*-+\s*\n(.*?)\n[ \t]*-+[ \t]*(?:\*/|\n[ \t]*\*/)', re.S
)
BLOCK_ENTRY = re.compile(r'"([^"]*)"\s*at\s*"([^"]*)"')


def load(name: str):
    for f in PILOT.glob(f"*{name}*"):
        if f.name.endswith(".xt"):
            return f
    raise SystemExit(f"no .xt file matching {name!r}")


def split(text: str):
    """Return (model text, [(severity, message, at)])."""
    end = text.find("*/")
    body = text[end + 2:] if end >= 0 else text
    expects = []
    for sev, msg, at in PAT.findall(body):
        expects.append((sev.rstrip("s"), " ".join(msg.split()), at))
    for match in BLOCK.finditer(body):
        sev = match.group(1).rstrip("s")
        for msg, at in BLOCK_ENTRY.findall(match.group(2)):
            expects.append((sev, " ".join(msg.split()), at))
    # Drop block comments first so the per-line strip below cannot leave fragments.
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
    kept = []
    for line in body.splitlines():
        if "XPECT" in line:
            continue
        kept.append(re.sub(r"//.*", "", line))
    model = "\n".join(kept)
    model = re.sub(r"\n{3,}", "\n\n", model).strip() + "\n"
    return model, expects


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    cmd = sys.argv[1]
    if cmd == "list":
        rows = []
        for f in sorted(PILOT.glob("*.xt")):
            _model, expects = split(f.read_text(encoding="utf-8", errors="replace"))
            errors = [e for e in expects if e[0] == "error"]
            if errors:
                rows.append((len(errors), f.name))
        for n, name in sorted(rows, reverse=True):
            print(f"{n:3}  {name}")
        print(f"\n{len(rows)} files carry error assertions")
    elif cmd == "emit":
        model, expects = split(load(sys.argv[2]).read_text(encoding="utf-8", errors="replace"))
        pathlib.Path(sys.argv[3]).write_text(model, encoding="utf-8")
        for sev, msg, at in expects:
            print(f"{sev.upper()}\t{msg}\t{at}")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
