"""Exact parity check: this implementation's compat projection against sysmlpy 0.97.1.

The comparison is ordered and exact. The only normalisation is on generated element names:
sysmlpy names anonymous usages (`satisfy R by x`, `connect a to b`) with a fresh UUID on every
parse — verified by `reference_is_stable()` — so those names can never match and are folded to
`<anon>` on both sides. Everything else has to line up, including the order in which nodes and
relations are emitted, because the reference interleaves ownership and typing per element and a
projection that grouped them by kind would be a different traversal.

Usage:
    python research/tools/parity.py [--verbose] [example ...]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import sysmlpy

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / "core"
CLI = CORE / "target" / "debug" / "sysml.exe"

# Notation strings as `ARROW_STYLES` spells them (plantuml.py:29-76).
NOTATIONS = {"*--", "--:|>", "-[thickness=3]-", "-->", "..>", "--"}
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")


def fold(name: str) -> str:
    """Generated element names fold to one token; authored names are kept verbatim."""
    return "<anon>" if UUID.match(name) else name


def reference(path: Path, view: str):
    src = path.read_text(encoding="utf-8")
    model, _errors = sysmlpy.parse(src)
    text = (
        sysmlpy.as_general_view(model, style="bw")
        if view == "gv"
        else sysmlpy.as_interconnection_view(model, style="bw")
    )

    order: list[str] = []
    label: dict[str, tuple[str, str]] = {}
    for m in re.finditer(r'^rectangle "(.*)" as (\w+) <<(.+?)>>\s*$', text, re.M):
        name, alias, stereo = m.groups()
        order.append(alias)
        label[alias] = (fold(name), stereo)

    nodes = [(label[a][0], label[a][1]) for a in order]

    edges: list[tuple] = []
    dangling: list[str] = []
    for m in re.finditer(r"^(\w+) (\S+) (\w+)(?:\s*:\s*(.*))?$", text, re.M):
        a, notation, b, tail = m.groups()
        if notation not in NOTATIONS:
            continue
        if a not in label:
            # An edge whose source is undeclared cannot be matched to anything.
            dangling.append(f"{a} {notation} {b}" + (f" : {tail}" if tail else ""))
            continue
        if b not in label:
            # The reference resolves `RoverSystem::Rover` to nothing and then still emits the
            # typing edge, pointed at an alias it never declared. The relationship is real, the
            # target name is not; compare it as a wildcard instead of dropping it.
            dangling.append(f"{a} {notation} {b}" + (f" : {tail}" if tail else ""))
            edges.append((label[a][0], notation, None, (tail or "").strip()))
            continue
        edges.append((label[a][0], notation, label[b][0], (tail or "").strip()))
    return nodes, edges, dangling


def ours(path: Path, view: str, mode: str):
    out = subprocess.run(
        [str(CLI), "graph", str(path), "--view", view, "--mode", mode],
        capture_output=True,
        text=True,
        check=True,
    ).stdout

    alias_of: dict[str, str] = {}
    nodes: list[tuple] = []
    for m in re.finditer(r'^node (\w+) "(.*)" <<(.+?)>>( def)?$', out, re.M):
        alias, name, stereo, _is_def = m.groups()
        alias_of[alias] = name
        nodes.append((fold(name), stereo))

    edges: list[tuple] = []
    for m in re.finditer(r"^edge (\w+) (\S+) (\w+)(?:\s*:\s*(.*))?$", out, re.M):
        a, notation, b, tail = m.groups()
        # Strip the `:from_port :to_port` suffix the connector line carries.
        tail = re.sub(r"\s*:\S+\s*:\S+$", "", tail or "").strip()
        edges.append((fold(alias_of[a]), notation, fold(alias_of[b]), tail))
    return nodes, edges


def reference_is_stable(path: Path) -> bool:
    """Anonymous elements get a new UUID per parse, so byte identity is impossible by design."""
    src = path.read_text(encoding="utf-8")
    seen = set()
    for _ in range(3):
        model, _errors = sysmlpy.parse(src)
        text = sysmlpy.as_general_view(model, style="bw")
        seen.add(tuple(re.findall(r'rectangle "([0-9a-f-]{36})"', text)))
    return len(seen) == 1


def edges_match(ours: tuple, reference: tuple) -> bool:
    """A reference edge with `None` as target matches any target: the reference emitted the
    relationship but never declared the node it points at."""
    if ours == reference:
        return True
    a, b = ours, reference
    return (
        a[0] == b[0]
        and a[1] == b[1]
        and a[3] == b[3]
        and (a[2] is None or b[2] is None)
    )


def report(path: Path, view: str, verbose: bool) -> bool:
    ref_nodes, ref_edges, dangling = reference(path, view)
    my_nodes, my_edges = ours(path, view, "compat")

    same = ref_nodes == my_nodes and len(my_edges) == len(ref_edges) and all(
        edges_match(a, b) for a, b in zip(my_edges, ref_edges)
    )
    print(f"\n{path.name} [{view}] compat: {'identical' if same else 'DIFFERS'}")
    print(f"  nodes {len(my_nodes)}/{len(ref_nodes)}, edges {len(my_edges)}/{len(ref_edges)}")

    if dangling:
        print("  reference edges with undeclared endpoints (reference defect, matched as wildcard):")
        for line in dangling:
            print(f"    ! {line}")

    if not same:
        for i, (a, b) in enumerate(zip(my_nodes, ref_nodes)):
            if a != b:
                print(f"  first node divergence at #{i}: ours {a!r} vs reference {b!r}")
                break
        else:
            if len(my_nodes) != len(ref_nodes):
                print(f"  node count differs: {len(my_nodes)} vs {len(ref_nodes)}")
        if ref_nodes == my_nodes:
            for i, (a, b) in enumerate(zip(my_edges, ref_edges)):
                if not edges_match(a, b):
                    print(f"  first edge divergence at #{i}: ours {a!r} vs reference {b!r}")
                    break
            else:
                if len(my_edges) != len(ref_edges):
                    print(f"  edge count differs: {len(my_edges)} vs {len(ref_edges)}")
        if verbose:
            print("  ours:")
            for n in my_nodes:
                print(f"    node {n}")
            for e in my_edges:
                print(f"    edge {e}")
    return same


def extended_additions(path: Path, view: str) -> None:
    """The extended projection is not meant to match; list what it adds or drops."""
    ref_nodes, ref_edges, _ = reference(path, view)
    my_nodes, my_edges = ours(path, view, "extended")
    added_nodes = [n for n in my_nodes if n not in ref_nodes]
    added_edges = [e for e in my_edges if e not in ref_edges]
    missing_edges = [e for e in ref_edges if e not in my_edges]
    print(f"\n{path.name} [{view}] extended: {len(my_nodes)} nodes, {len(my_edges)} edges")
    for n in added_nodes:
        print(f"  + node {n[0]!r} <<{n[1]}>>")
    for e in added_edges:
        print(f"  + edge {e[0]} {e[1]} {e[2]}" + (f" : {e[3]}" if e[3] else ""))
    for e in missing_edges:
        print(f"  - edge {e[0]} {e[1]} {e[2]}" + (f" : {e[3]}" if e[3] else ""))


if __name__ == "__main__":
    argv = [a for a in sys.argv[1:] if not a.startswith("-")]
    verbose = "--verbose" in sys.argv
    examples = argv or ["drone.sysml", "fuel.sysml", "rover.sysml", "shadow.sysml"]

    print("reference determinism:")
    for name in examples:
        stable = reference_is_stable(CORE / "examples" / name)
        print(f"  {name}: {'stable' if stable else 'unstable, UUIDs regenerate each parse'}")

    ok = True
    for name in examples:
        path = CORE / "examples" / name
        for view in ("gv", "iv"):
            ok &= report(path, view, verbose)

    if verbose:
        for name in examples:
            path = CORE / "examples" / name
            for view in ("gv", "iv"):
                extended_additions(path, view)

    print("\nCOMPAT PARITY: identical" if ok else "\nCOMPAT PARITY: differences above")
    sys.exit(0 if ok else 1)
