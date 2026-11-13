"""Independent review of an IV diagram's routing. Deliberately does not reuse layout_lint's
predicates, so agreement between the two means something.

Measures, per SVG:
  grazes     a line that runs within `pad` px of a frame border without entering it. The lint
             calls this clean; on a drawing it reads as a line hugging a box, and two parallel
             grazes read as a double border.
  overlaps   two line segments that collinearly overlap (a run doubled back on itself), or a
             vertical and a horizontal segment crossing at a point no port sits on.
  detours    per connector: path length / straight-line distance. A route that is more than
             2.2x its displacement is walking a long way round.
  ink        total path length as a multiple of canvas diagonal: a cheap "how busy" number.

Usage:
    python research/tools/route_review.py out/foo.svg [-v]
"""

from __future__ import annotations

import math
import re
import sys
from pathlib import Path

RECT = re.compile(
    r'<rect x="([-\d.]+)" y="([-\d.]+)" width="([-\d.]+)" height="([-\d.]+)"([^/]*)/>')
PATH = re.compile(r'<path d="([^"]+)"([^>]*)/>')
CIRCLE = re.compile(r'<circle ')
ATTR = re.compile(r'(\w[\w-]*)="([^"]*)"')
SVG = re.compile(r'<svg[^>]*width="(\d+)" height="(\d+)"')


def segments(d: str):
    """Absolute M/V/H/L polylines, one list per subpath."""
    out, cur = [], []
    tokens = re.findall(r"[MVHLl]|-?\d+(?:\.\d+)?", d)
    x = y = 0.0
    cmd = ""
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t in "MVHLl":
            cmd = t
            i += 1
            if cmd == "M" and cur:
                out.append(cur)
                cur = []
            continue
        if cmd == "M":
            x, y = float(tokens[i]), float(tokens[i + 1]); i += 2
            if cur:
                out.append(cur)
            cur = [(x, y)]
        elif cmd == "V":
            ny = float(tokens[i]); cur.append((x, ny)); y = ny; i += 1
        elif cmd == "H":
            nx = float(tokens[i]); cur.append((nx, y)); x = nx; i += 1
        elif cmd == "L":
            nx, ny = float(tokens[i]), float(tokens[i + 1])
            cur.append((nx, ny)); x, y = nx, ny; i += 2
        elif cmd == "l":
            dx, dy = float(tokens[i]), float(tokens[i + 1])
            x, y = x + dx, y + dy
            cur.append((x, y)); i += 2
        else:
            i += 1
    if cur:
        out.append(cur)
    return [p for p in out if len(p) > 1]


def segs_of(poly):
    return [(poly[i], poly[i + 1]) for i in range(len(poly) - 1)]


def length(seg) -> float:
    (x1, y1), (x2, y2) = seg
    return math.hypot(x2 - x1, y2 - y1)


def is_h(seg) -> bool:
    return abs(seg[0][1] - seg[1][1]) < 0.01


def is_v(seg) -> bool:
    return abs(seg[0][0] - seg[1][0]) < 0.01


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    verbose = "-v" in sys.argv
    for name in args:
        path = Path(name)
        svg = path.read_text(encoding="utf-8")
        m = SVG.search(svg)
        width, height = (int(m.group(1)), int(m.group(2))) if m else (1, 1)

        # Frames: stroked rects big enough to be a box (not port squares, not label plates).
        frames = []
        for r in RECT.finditer(svg):
            x, y, w, h, extra = r.groups()
            a = dict(ATTR.findall(extra))
            if a.get("stroke") != "#000000" or float(w) < 20 or float(h) < 20:
                continue
            frames.append((float(x), float(y), float(x) + float(w), float(y) + float(h)))

        # Connector paths: thick stroke, unfilled, made of absolute segments only.
        conns = []
        for p in PATH.finditer(svg):
            d, extra = p.groups()
            a = dict(ATTR.findall(extra))
            if a.get("fill") == "none" and a.get("stroke-width", "1") not in ("1", "1.1", "0.8"):
                for poly in segments(d):
                    conns.append(poly)

        grazes = []
        for poly in conns:
            for (x1, y1), (x2, y2) in segs_of(poly):
                if is_v(((x1, y1), (x2, y2))):
                    lo, hi = min(y1, y2), max(y1, y2)
                    for fx0, fy0, fx1, fy1 in frames:
                        for border in (fx0, fx1):
                            if abs(x1 - border) < 6.0 and hi > fy0 and lo < fy1:
                                grazes.append((x1, y1, x2, y2, "v", border))
                elif is_h(((x1, y1), (x2, y2))):
                    lo, hi = min(x1, x2), max(x1, x2)
                    for fx0, fy0, fx1, fy1 in frames:
                        for border in (fy0, fy1):
                            if abs(y1 - border) < 6.0 and hi > fx0 and lo < fx1:
                                grazes.append((x1, y1, x2, y2, "h", border))

        # Overlaps: two collinear runs sharing more than a point; and crossings with no port.
        overlaps = []
        allsegs = [(s, i) for i, poly in enumerate(conns) for s in segs_of(poly)]
        for a in range(len(allsegs)):
            for b in range(a + 1, len(allsegs)):
                (s1, i1), (s2, i2) = allsegs[a], allsegs[b]
                if i1 == i2:
                    continue
                if is_h(s1) and is_h(s2) and abs(s1[0][1] - s2[0][1]) < 0.5:
                    lo = max(min(s1[0][0], s1[1][0]), min(s2[0][0], s2[1][0]))
                    hi = min(max(s1[0][0], s1[1][0]), max(s2[0][0], s2[1][0]))
                    if hi - lo > 4.0:
                        overlaps.append(("collinear-h", round(s1[0][1]), round(lo), round(hi)))
                if is_v(s1) and is_v(s2) and abs(s1[0][0] - s2[0][0]) < 0.5:
                    lo = max(min(s1[0][1], s1[1][1]), min(s2[0][1], s2[1][1]))
                    hi = min(max(s1[0][1], s1[1][1]), max(s2[0][1], s2[1][1]))
                    if hi - lo > 4.0:
                        overlaps.append(("collinear-v", round(s1[0][0]), round(lo), round(hi)))

        detours = []
        for poly in conns:
            total = sum(length(s) for s in segs_of(poly))
            disp = math.hypot(poly[-1][0] - poly[0][0], poly[-1][1] - poly[0][1])
            ratio = total / disp if disp > 1 else float("inf")
            detours.append((ratio, total, disp, poly))

        ink = sum(length(s) for poly in conns for s in segs_of(poly))
        diag = math.hypot(width, height)
        bends = [len(poly) - 2 for poly in conns]

        print(f"\n{path.name}")
        print(f"  canvas {width}x{height}, {len(frames)} frames, {len(conns)} connectors")
        print(f"  grazes (line within 6px of a border) : {len(grazes)}")
        print(f"  collinear overlaps                   : {len(overlaps)}")
        print(f"  ink / diagonal                       : {ink / diag:.2f}")
        worst = sorted(detours, reverse=True)[:3]
        print(f"  long detours (>2.2x displacement)    : {sum(1 for d in detours if d[0] > 2.2)}")
        print(f"  bends per connector                  : max {max(bends) if bends else 0}, "
              f"mean {(sum(bends) / len(bends)) if bends else 0:.1f}")
        if verbose:
            for g in grazes[:12]:
                print(f"    graze   seg {tuple(round(v) for v in g[:4])} hugs {'x' if g[4] == 'v' else 'y'}={g[5]:.0f}")
            for o in overlaps[:12]:
                print(f"    overlap {o}")
            for ratio, total, disp, poly in worst:
                if ratio > 2.2:
                    pts = " -> ".join(f"({p[0]:.0f},{p[1]:.0f})" for p in poly)
                    print(f"    detour  {ratio:.1f}x  {total:.0f}px for {disp:.0f}px: {pts}")


if __name__ == "__main__":
    main()
