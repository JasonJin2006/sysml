"""Generate the metaclass table: every SysML/KerML metaclass and its direct supertypes.

Two sources, and they must agree edge for edge before anything is written.

`research/upstream/SysML.json` is the published metamodel schema. Each `$defs` entry lists the
definitions that may describe it under `anyOf`, and every one of those branches is a `$ref` to
another entry: `PartUsage`'s branches name `ConnectionUsage`, `ViewUsage`, `RenderingUsage`, and
the crate's own table (`generated.rs:1640`) gives `ConnectionUsage` the supertypes
`[PartUsage, ConnectorAsUsage]`. So the branch means "this entry is a direct subtype of the entry
that lists it", and the relation is the inverse of `anyOf`.

`research/upstream/sem/mod/sysmlv2-model-0.1.2/src/generated.rs` carries the same relation as a
Rust match: `ElementKind::X => &[ElementKind::A, ...]` is X's direct supertype list. Read both,
refuse to write on any disagreement, and emit the JSON's edges — the published schema is the
primary source, the crate is the check.

The 8 entries that are not metaclasses (`Identified`, `VisibilityKind`, `FeatureDirectionKind`,
`PortionKind`, `RequirementConstraintKind`, `StateSubactionKind`, `TransitionFeatureKind`,
`TriggerKind`) are datatypes; they appear only in the crate's kind list if the crate models them
as kinds, and never as the target of a subtype edge, so filtering by "is an entry in the crate's
metaclass list" drops exactly them.

Usage:  python tools/metaclasses.py [--check]
        --check verifies the committed table is current without writing.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORE = os.path.dirname(HERE)
PROJECT = os.path.dirname(CORE)
JSON_SRC = os.path.join(PROJECT, "research", "upstream", "SysML.json")
CRATE_SRC = os.path.join(
    PROJECT,
    "research",
    "upstream",
    "sem",
    "mod",
    "sysmlv2-model-0.1.2",
    "src",
    "generated.rs",
)
OUT = os.path.join(CORE, "crates", "sysml-model", "data", "metaclasses.rs")


def crate_table(path):
    """The crate's direct supertype list per kind, plus its kind names."""
    text = open(path, encoding="utf-8").read()
    supers = {}
    for m in re.finditer(r"ElementKind::(\w+) => &\[([^\]]*)\],", text):
        if "FeatureMeta" in m.group(2):
            continue  # own_features, not the supertype table
        name = m.group(1)
        if name in supers:
            continue  # the name and keyword tables share the shape; the first is the hierarchy
        supers[name] = re.findall(r"ElementKind::(\w+)", m.group(2))
    return supers


def json_edges(path):
    """(subtype, supertype) pairs from the schema's `anyOf` branches."""
    defs = json.load(open(path, encoding="utf-8"))["$defs"]
    edges = set()
    for name, body in defs.items():
        for branch in body.get("anyOf", []):
            ref = branch.get("$ref", "")
            if ref:
                # The entry listing the branch is the supertype; the branch is a subtype of it.
                edges.add((ref.rsplit("/", 1)[-1], name))
    return edges, set(defs)


def main():
    check = "--check" in sys.argv
    crate = crate_table(CRATE_SRC)
    edges, defs = json_edges(JSON_SRC)
    names = set(crate)

    if not names:
        sys.exit(f"no metaclasses read from {CRATE_SRC}")
    only_json = defs - names
    only_crate = names - defs
    if only_crate:
        sys.exit(f"metaclasses the schema does not define: {sorted(only_crate)}")

    # Every schema edge must be a direct supertype the crate states, and the two sets must be
    # equal: a subset would mean one source is stale, and reading a partial hierarchy is worse
    # than refusing to generate.
    crate_edges = {(child, parent) for child, parents in crate.items() for parent in parents}
    unknown = {(c, p) for c, p in edges if c not in names or p not in names}
    if unknown:
        sys.exit(f"edges naming a non-metaclass: {sorted(unknown)[:5]}")
    missing = edges - crate_edges
    extra = crate_edges - edges
    if missing or extra:
        print(f"schema edges not in the crate: {len(missing)}", sorted(missing)[:10])
        print(f"crate edges not in the schema: {len(extra)}", sorted(extra)[:10])
        sys.exit("the two sources disagree; refusing to write a table either one would call wrong")

    rows = []
    for name in sorted(names):
        parents = sorted(crate[name])
        if parents:
            joined = ", ".join(f'"{p}"' for p in parents)
            rows.append(f'    Meta {{ name: "{name}", supers: &[{joined}] }},')
        else:
            rows.append(f'    Meta {{ name: "{name}", supers: &[] }},')

    header = [
        "// Generated from the SysML v2 metamodel: each row is one metaclass and its direct",
        "// supertypes. Source is research/upstream/SysML.json, whose `anyOf` branches are the",
        "// subtypes of the entry listing them; every edge was checked against the published",
        "// crate's own hierarchy (sysmlv2-model generated.rs) and the two sets are equal.",
        "// Regenerate with `python tools/metaclasses.py` after a metamodel update.",
        f"// {len(names)} metaclasses, {len(edges)} supertype edges.",
        "&[",
    ]
    body = header + rows + ["]"]
    text = "\n".join(body) + "\n"

    if check:
        current = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        if current != text:
            sys.exit(f"{OUT} is stale; run `python tools/metaclasses.py`")
        print(f"up to date: {len(names)} metaclasses, {len(edges)} edges")
        return

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    open(OUT, "w", encoding="utf-8", newline="\n").write(text)
    print(f"wrote {OUT}: {len(names)} metaclasses, {len(edges)} edges")
    if only_json - {
        "Identified",
        "VisibilityKind",
        "FeatureDirectionKind",
        "PortionKind",
        "RequirementConstraintKind",
        "StateSubactionKind",
        "TransitionFeatureKind",
        "TriggerKind",
    }:
        print("schema-only entries kept out:", sorted(only_json))


if __name__ == "__main__":
    main()
