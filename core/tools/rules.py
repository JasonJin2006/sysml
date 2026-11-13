"""The specification's own OCL, by rule name.

`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs` holds three tables the published
metamodel crate generated from the specification:

  * `RULES` — 180 well-formedness constraints, each with the metaclass it is stated of, its OCL
    body and the sentence the specification gives it;
  * `DERIVATIONS` — 235 derived properties (`type`, `flowEnd`, `featuringType`, …), in OCL;
  * `OPERATIONS` — the metamodel's own operations, also in OCL (`isDistinguishableFrom`, which
    `validateNamespaceDistinguishibility` is written in terms of, is one).

Reading a rule from there beats inferring it from a test model's message: the message says what the
reference implementation *prints*, the OCL says what it *asks*. Four rules in this kernel were
written the wrong way round for want of this file — `validateAssociationEndTypes` read the written
type clause where the rule asks about the derived `type`; `validateClassSpecialization` lost the
half its message does not mention; `validateRedefinitionDirectionConformance` compares two written
directions where the rule asks for a direction computed relative to a featuring type; and
`verify`-in-an-objective was made stricter than the OCL on the strength of a message.

Usage:
    python tools/rules.py validateFeatureInheritedMemberNameNotDuplicate
    python tools/rules.py --grep "inherited member"
    python tools/rules.py --grep isDistinguishableFrom
    python tools/rules.py --derivations --grep "= type"
    python tools/rules.py --all --names-only
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORE = os.path.dirname(HERE)
MODEL = os.path.join(
    os.path.dirname(CORE), "research", "upstream", "mod", "sysmlv2-model-0.1.2", "src", "generated.rs"
)

# One entry of `RULES` or `DERIVATIONS`. `ocl` and `says` are Rust string literals, so a quote
# inside them is written `\"` and a backslash `\\`; the parse below undoes exactly that and nothing
# else.
ENTRY = re.compile(
    r'\{?\s*name: "((?:[^"\\]|\\.)*)",\s*'
    r'metaclass: ElementKind::(\w+),\s*'
    r'ocl: "((?:[^"\\]|\\.)*)",\s*'
    r'(?:says: "((?:[^"\\]|\\.)*)",\s*)?\}',
    re.S,
)

# An entry of `OPERATIONS`: the same shape with a parameter list between the metaclass and the OCL.
OPERATION = re.compile(
    r'\{?\s*name: "((?:[^"\\]|\\.)*)",\s*'
    r'metaclass: ElementKind::(\w+),\s*'
    r'parameters: &\[((?:[^\]])*)\],\s*'
    r'ocl: "((?:[^"\\]|\\.)*)",\s*\}',
    re.S,
)

TABLES = (
    ("rule", "pub const RULES: &[Rule] = &[", ENTRY, False),
    ("derivation", "pub const DERIVATIONS: &[Rule] = &[", ENTRY, False),
    ("operation", "pub const OPERATIONS: &[Operation] = &[", OPERATION, True),
)


def unescape(text):
    return text.replace('\\"', '"').replace("\\\\", "\\").replace("\\n", "\n")


def entries(source, marker, pattern, is_operation):
    """Every entry of the table `marker` names, in file order."""
    start = source.find(marker)
    if start < 0:
        sys.exit(f"no {marker!r} in {MODEL}")
    # The table runs to its closing `];`, which is the first one after the marker.
    end = source.find("];", start)
    block = source[start:end]
    out = []
    for found in pattern.findall(block):
        if is_operation:
            # `Operation` states a parameter list where `Rule` states the sentence.
            name, metaclass, parameters, ocl = found
            says = ""
        else:
            name, metaclass, ocl, says = found
            parameters = ""
        out.append(
            {
                "name": unescape(name),
                "metaclass": metaclass,
                "parameters": parameters,
                "ocl": unescape(ocl),
                "says": unescape(says) if says else "",
            }
        )
    return out


def load():
    if not os.path.exists(MODEL):
        sys.exit(f"no metamodel crate at {MODEL}")
    source = open(MODEL, encoding="utf-8").read()
    out = []
    for kind, marker, pattern, is_operation in TABLES:
        for entry in entries(source, marker, pattern, is_operation):
            out.append({"kind": kind, **entry})
    return out


def show(entry, width=96):
    head = f"== {entry['name']}  [{entry['metaclass']}]"
    if entry["kind"] != "rule":
        head += f"  ({entry['kind']})"
    print(head)
    if entry["parameters"].strip():
        print(f"   parameters: {entry['parameters'].strip()}")
    if entry["says"]:
        print(f"   says: {entry['says']}")
    # The OCL keeps the line breaks the specification wrote; only a line that is too long for a
    # terminal is wrapped, and at a space.
    body = entry["ocl"]
    if not body.strip():
        print("   ocl:  (empty)")
        return
    for index, line in enumerate(body.splitlines()):
        prefix = "   ocl:  " if index == 0 else "         "
        while len(prefix) + len(line) > width:
            cut = line.rfind(" ", 0, width - len(prefix))
            if cut <= 0:
                cut = width - len(prefix)
            print(prefix + line[:cut])
            line = "        " + line[cut:].lstrip()
            prefix = "         "
        print(prefix + line)


def main():
    ap = argparse.ArgumentParser(description="the specification's OCL, by rule name")
    ap.add_argument("name", nargs="*", help="rule names to print")
    ap.add_argument("--grep", "-g", help="substring of the name, the OCL or the sentence")
    ap.add_argument("--derivations", "-d", action="store_true", help="search derived properties only")
    ap.add_argument("--operations", "-o", action="store_true", help="search operations only")
    ap.add_argument("--rules", "-r", action="store_true", help="search constraints only")
    ap.add_argument("--all", "-a", action="store_true", help="print every entry")
    ap.add_argument("--names-only", "-n", action="store_true", help="one name per line")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    everything = load()
    # A narrowing flag selects a table; without one, a search covers all three.
    wanted_kinds = {
        kind
        for kind, flag in (
            ("rule", args.rules),
            ("derivation", args.derivations),
            ("operation", args.operations),
        )
        if flag
    }
    found = [e for e in everything if not wanted_kinds or e["kind"] in wanted_kinds]

    if args.grep:
        needle = args.grep.lower()
        found = [
            e
            for e in found
            if needle in e["name"].lower()
            or needle in e["ocl"].lower()
            or needle in e["says"].lower()
        ]
    elif args.name:
        wanted = set(args.name)
        found = [e for e in found if e["name"] in wanted]
        for name in sorted(wanted - {e["name"] for e in found}):
            print(f"no entry named {name}", file=sys.stderr)
    elif not args.all:
        ap.error("give a name, --grep, or --all")

    if args.json:
        print(json.dumps(found, indent=1))
        return 0
    if args.names_only:
        for entry in found:
            print(entry["name"])
        return 0
    for entry in found:
        show(entry)
        print()
    print(f"{len(found)} entr{'y' if len(found) == 1 else 'ies'}", file=sys.stderr)
    return 0 if found else 1


if __name__ == "__main__":
    sys.exit(main())
