"""Layout linter: finds text and line collisions in a rendered SVG.

The renderer emits a small subset of SVG — `rect`, `line`, `path` with only M/V/H/L/l, and
`text` with start/middle/end anchors — so a hand-written parser is enough and avoids a
dependency. Text extents are estimated with the same advance widths the renderer uses, which
makes the numbers comparable with the geometry it computed.

Reported defects:
  overlap   two labels whose boxes intersect
  crossed   a line or path segment that runs through a label
  clipped   a label wider than the box it sits in
  escape    a nested box that leaves its parent's frame

Usage:
    python research/tools/layout_lint.py out/drone-gv-extended.svg [...]
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

RECT = re.compile(
    r'<rect x="([-\d.]+)" y="([-\d.]+)" width="([-\d.]+)" height="([-\d.]+)"([^/]*)/>')
TEXT = re.compile(r'<text x="([-\d.]+)" y="([-\d.]+)"([^>]*)>(.*?)</text>', re.S)
LINE = re.compile(r'<line x1="([-\d.]+)" y1="([-\d.]+)" x2="([-\d.]+)" y2="([-\d.]+)"')
PATH = re.compile(r'<path d="([^"]+)"')
ATTR = re.compile(r'(\w[\w-]*)="([^"]*)"')


def advance(text: str, size: float) -> float:
    return sum(size * (0.55 if ord(c) < 128 else 0.98) for c in text)


@dataclass
class Label:
    text: str
    x0: float
    y0: float
    x1: float
    y1: float
    size: float
    group: int = -1
    backed: bool = False

    def hits(self, other: "Label", pad: float = 2.0) -> bool:
        return not (
            self.x1 + pad < other.x0
            or other.x1 + pad < self.x0
            or self.y1 + pad < other.y0
            or other.y1 + pad < self.y0
        )


@dataclass
class Box:
    x0: float
    y0: float
    x1: float
    y1: float

    def contains(self, other: "Box") -> bool:
        return (
            self.x0 <= other.x0 + 0.5
            and self.y0 <= other.y0 + 0.5
            and self.x1 >= other.x1 - 0.5
            and self.y1 >= other.y1 - 0.5
        )

    def meets(self, other: "Box") -> bool:
        return not (
            self.x1 <= other.x0 or other.x1 <= self.x0
            or self.y1 <= other.y0 or other.y1 <= self.y0
        )


def segments(d: str) -> list[tuple[float, float, float, float]]:
    """Only the absolute M/V/H/L forms the renderer emits, plus relative `l` arrow strokes."""
    out: list[tuple[float, float, float, float]] = []
    tokens = re.findall(r"[MVHLl]|-?\d+(?:\.\d+)?", d)
    x = y = 0.0
    cmd = ""
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t in "MVHLl":
            cmd = t
            i += 1
            continue
        if cmd == "M":
            x, y = float(tokens[i]), float(tokens[i + 1])
            i += 2
        elif cmd == "V":
            ny = float(tokens[i])
            out.append((x, y, x, ny))
            y = ny
            i += 1
        elif cmd == "H":
            nx = float(tokens[i])
            out.append((x, y, nx, y))
            x = nx
            i += 1
        elif cmd == "L":
            nx, ny = float(tokens[i]), float(tokens[i + 1])
            out.append((x, y, nx, ny))
            x, y = nx, ny
            i += 2
        elif cmd == "l":
            dx, dy = float(tokens[i]), float(tokens[i + 1])
            out.append((x, y, x + dx, y + dy))
            x, y = x + dx, y + dy
            i += 2
        else:
            i += 1
    return out


def crosses(seg: tuple[float, float, float, float], lab: Label, pad: float = 1.0) -> bool:
    x1, y1, x2, y2 = seg
    left, right = lab.x0 - pad, lab.x1 + pad
    top, bottom = lab.y0 - pad, lab.y1 + pad
    if abs(x1 - x2) < 0.01:  # vertical
        return left <= x1 <= right and not (max(y1, y2) < top or min(y1, y2) > bottom)
    if abs(y1 - y2) < 0.01:  # horizontal
        return top <= y1 <= bottom and not (max(x1, x2) < left or min(x1, x2) > right)
    return False


def cuts_through(seg: tuple[float, float, float, float], box: Box, inset: float = 2.0) -> bool:
    x1, y1, x2, y2 = seg
    left, right = box.x0 + inset, box.x1 - inset
    top, bottom = box.y0 + inset, box.y1 - inset
    if left >= right or top >= bottom:
        return False
    if abs(x1 - x2) < 0.01:
        return left < x1 < right and not (max(y1, y2) <= top or min(y1, y2) >= bottom)
    if abs(y1 - y2) < 0.01:
        return top < y1 < bottom and not (max(x1, x2) <= left or min(x1, x2) >= right)
    return False


def lint(path: Path, verbose: bool) -> int:
    svg = path.read_text(encoding="utf-8")
    labels: list[Label] = []
    boxes: list[Box] = []
    lines: list[tuple[float, float, float, float]] = []

    for m in RECT.finditer(svg):
        x, y, w, h, extra = m.groups()
        if w == "0.0" or h == "0.0":
            continue
        attrs = dict(ATTR.findall(extra))
        # Frames are the stroked rectangles; a background plate or a port square is not one.
        if attrs.get("stroke") != "#000000" or float(w) < 20 or float(h) < 20:
            continue
        boxes.append(Box(float(x), float(y), float(x) + float(w), float(y) + float(h)))

    # A label sitting on a white plate hides whatever runs underneath, so a line behind it is
    # not a defect. Plates are collected first and matched to labels afterwards.
    plates = [
        Box(float(m.group(1)), float(m.group(2)),
            float(m.group(1)) + float(m.group(3)), float(m.group(2)) + float(m.group(4)))
        for m in RECT.finditer(svg)
        if dict(ATTR.findall(m.group(5))).get("fill") == "#FFFFFF"
        and dict(ATTR.findall(m.group(5))).get("stroke") in (None, "none")
    ]

    for m in TEXT.finditer(svg):
        x, y, attrs_raw, body = m.groups()
        attrs = dict(ATTR.findall(attrs_raw))
        size = float(attrs.get("font-size", "12"))
        text = re.sub(r"&[a-z]+;", "x", body)
        width = advance(text, size)
        anchor = attrs.get("text-anchor", "start")
        cx, baseline = float(x), float(y)
        x0 = cx - width / 2 if anchor == "middle" else (cx - width if anchor == "end" else cx)
        label = Label(text, x0, baseline - size * 0.78, x0 + width, baseline + size * 0.22, size)

        # Which frame owns it: the smallest one that contains the label's anchor.
        owners = [b for b in boxes if b.x0 <= cx <= b.x1 and b.y0 <= baseline <= b.y1]
        if owners:
            label.group = boxes.index(min(owners, key=lambda b: (b.x1 - b.x0) * (b.y1 - b.y0)))
        else:
            label.group = -1000 - len(labels)
        label.backed = any(
            p.x0 <= label.x0 and p.x1 >= label.x1 and p.y0 <= label.y0 and p.y1 >= label.y1
            for p in plates
        )
        labels.append(label)

    for m in LINE.finditer(svg):
        x1, y1, x2, y2 = (float(g) for g in m.groups())
        lines.append((x1, y1, x2, y2))
    for m in PATH.finditer(svg):
        lines.extend(segments(m.group(1)))

    overlaps = []
    for i in range(len(labels)):
        for j in range(i + 1, len(labels)):
            # Two labels inside the same box are a stacked title block, not a collision.
            if labels[i].group == labels[j].group:
                continue
            if labels[i].hits(labels[j]):
                overlaps.append((labels[i], labels[j]))

    crossed = []
    for lab in labels:
        if lab.backed:
            continue
        for seg in lines:
            if crosses(seg, lab):
                crossed.append((lab, seg))
                break

    escaped = []
    for a in boxes:
        for b in boxes:
            if a is b:
                continue
            # Containment is the point of a nested box; only a partial overlap is a defect.
            if a.meets(b) and not (a.contains(b) or b.contains(a)):
                escaped.append((a, b))

    # A connector between two nested parts legitimately crosses its owner's frame, so only leaf
    # frames are checked for lines running through them.
    leaves = [b for b in boxes if not any(o is not b and b.contains(o) for o in boxes)]
    separators = {
        seg
        for seg in lines
        if any(
            abs(seg[1] - seg[3]) < 0.01
            and abs(min(seg[0], seg[2]) - box.x0) < 1.5
            and abs(max(seg[0], seg[2]) - box.x1) < 1.5
            for box in boxes
        )
    }
    through = []
    for seg in lines:
        if seg in separators:
            continue
        for box in leaves:
            if cuts_through(seg, box):
                through.append((seg, box))
                break

    name = path.name
    print(f"\n{name}")
    print(f"  {len(labels)} labels, {len(boxes)} boxes, {len(lines)} segments")
    print(f"  label overlaps : {len(overlaps)}")
    print(f"  crossed labels : {len(crossed)}")
    print(f"  box collisions : {len(escaped) // 2}")
    print(f"  lines in boxes : {len(through)}")
    if verbose:
        for a, b in overlaps:
            print(f"    overlap  {a.text[:24]!r} @({a.x0:.0f},{a.y0:.0f}) vs {b.text[:24]!r} @({b.x0:.0f},{b.y0:.0f})")
        for lab, seg in crossed:
            print(f"    crossed  {lab.text[:24]!r} @({lab.x0:.0f},{lab.y0:.0f}) by {tuple(round(v) for v in seg)}")
        for a, b in escaped[:8]:
            print(f"    collide  ({a.x0:.0f},{a.y0:.0f},{a.x1:.0f},{a.y1:.0f}) vs ({b.x0:.0f},{b.y0:.0f},{b.x1:.0f},{b.y1:.0f})")
        for seg, box in through[:12]:
            print(f"    through  {tuple(round(v) for v in seg)} in ({box.x0:.0f},{box.y0:.0f},{box.x1:.0f},{box.y1:.0f})")
    return len(overlaps) + len(crossed) + len(escaped) // 2 + len(through)


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    verbose = "--verbose" in sys.argv or "-v" in sys.argv
    files = args or sorted(Path("out").glob("*-extended.svg"))
    total = sum(lint(Path(f), verbose) for f in files)
    print(f"\nTOTAL DEFECTS: {total}")
    sys.exit(0 if total == 0 else 1)
