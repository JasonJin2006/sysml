"""Checks the arrow decorations in a rendered SVG against the notation they claim to follow.

Nothing here looks at pixels. The SVG is parsed as XML and the drawing primitives are counted:
a filled diamond is a closed 4-point path with a black fill, a hollow triangle is a closed
3-point path with a white fill, a stick arrowhead is a path made only of `l` segments, and the
colon of `:|>` is two `<circle>` elements near the triangle.

The expected counts come from the projection itself (`sysml graph`), not from the picture:
one diamond per ownership edge, one triangle per typing trunk, one stick head per satisfy edge.

Sources for the shapes, archived under `research/archive/java-probes/`:
  OMG pilot `SysML2PlantUMLStyle.java`
      caseFeatureTyping            -> " --:|> "
      caseSatisfyRequirementUsage  -> " --> "
      caseSpecialization           -> " --|> "
      caseFeatureMembership        -> composite ? " *-- " : " o-- "
  PlantUML `LinkDecor.java:72,78` and `ExtremityExtendsLike.DefinedBy`
      DEFINEDBY(decors2(":|>"), 30, fill=false)  hollow triangle + two dots
      COMPOSITION(decors2("*"), 15, fill=true)   filled diamond
      ARROW(decors2(">"), 10)                    stick arrow, stroke not dashed

Usage:
    python research/tools/arrow_check.py out/drone-gv-extended.svg [...]
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / "core"
CLI = CORE / "target" / "debug" / "sysml.exe"

PATH = re.compile(r'<path d="([^"]+)"([^>]*)/>')
CIRCLE = re.compile(r"<circle ")
STYLE = re.compile(r'(\w[\w-]*)="([^"]*)"')


@dataclass
class Shape:
    points: int
    fill: str
    relative: bool


def shapes(svg: str) -> list[Shape]:
    out = []
    for m in PATH.finditer(svg):
        d, attrs = m.groups()
        style = dict(STYLE.findall(attrs))
        absolute = len(re.findall(r"[MVHL]", d))
        relative = len(re.findall(r"l", d))
        out.append(Shape(absolute, style.get("fill", "none"), relative > 0))
    return out


def expected(example: str, view: str, mode: str) -> tuple[int, int, int]:
    """(ownership diamonds, distinct typing targets, satisfying elements) from the projection.

    A diamond is drawn once per owner, not once per ownership edge, because all the features of
    one owner share a bus. A typing head is drawn once per trunk, and a trunk exists per
    (gap, definition); a typing edge that has to cross another row leaves on its own, so the
    number of triangles lies between the distinct targets and the number of typing edges.

    The view matters: the interconnection projection carries connection lines, which are
    drawn thick and headless, so its arrow expectations are its own. The mode matters more:
    compat projects the same model differently (anonymous placeholder nodes for satisfy), so a
    compat SVG has to be checked against the compat projection.
    """
    out = subprocess.run(
        [str(CLI), "graph", str(CORE / "examples" / example), "--view", view, "--mode", mode],
        capture_output=True, text=True, check=True).stdout
    owners: set[str] = set()
    targets: set[str] = set()
    typing = 0
    satisfy = 0
    for line in out.splitlines():
        parts = line.split()
        if len(parts) < 4 or parts[0] != "edge":
            continue
        src, notation, dest = parts[1], parts[2], parts[3]
        if notation == "*--":
            owners.add(src)
        elif notation == "--:|>":
            targets.add(dest)
            typing += 1
        elif notation == "-->":
            satisfy += 1
    return len(owners), (len(targets), typing), satisfy


def check(svg_path: Path, example: str, view: str, mode: str) -> bool:
    svg = svg_path.read_text(encoding="utf-8")
    got = shapes(svg)
    diamonds = [s for s in got if s.points == 4 and s.fill == "#000000" and not s.relative]
    triangles = [s for s in got if s.points == 3 and s.fill == "#FFFFFF" and not s.relative]
    sticks = [s for s in got if s.relative and s.fill == "none"]
    circles = len(CIRCLE.findall(svg))
    dashed = svg.count("stroke-dasharray")

    want_diamond, (want_lo, want_hi), want_stick = expected(example, view, mode)
    ok = (
        len(diamonds) == want_diamond
        and want_lo <= len(triangles) <= want_hi
        and len(sticks) == want_stick
        and circles == 2 * len(triangles)
        and dashed == 0
    )

    print(f"\n{svg_path.name}")
    print(f"  filled diamonds (composition) : {len(diamonds):3d}  expected {want_diamond}")
    print(f"  hollow triangles (typing)     : {len(triangles):3d}  expected {want_lo}..{want_hi}")
    print(f"  open stick heads (satisfy)    : {len(sticks):3d}  expected {want_stick}")
    print(f"  colon dots (2 per triangle)   : {circles:3d}  expected {2 * len(triangles)}")
    print(f"  dashed strokes                : {dashed:3d}  expected 0")
    print(f"  => {'ok' if ok else 'MISMATCH'}")
    return ok


if __name__ == "__main__":
    ok = True
    for arg in sys.argv[1:]:
        path = Path(arg)
        # `drone-gv-extended.svg` -> example `drone.sysml`, view `gv`, mode `extended`.
        match = re.match(r"^(.*)-(gv|iv)-(compat|extended)\.svg$", path.name)
        if match is None:
            print(f"\n{path.name}\n  => skipped: name does not encode a view and mode")
            continue
        ok &= check(path, match.group(1) + ".sysml", match.group(2), match.group(3))
    print("\nARROWS OK" if ok else "\nARROWS: mismatch")
    sys.exit(0 if ok else 1)
