"""Readability metrics for a rendered SVG.

The two existing checkers answer "is anything illegal?" — nothing overlaps, nothing is
clipped. This one answers "is the drawing any good?", which is a different question and the
one that decides whether a human can follow the diagram:

  crossings   how many times two edges cross. Every crossing is a place the eye has to stop
              and decide which strand it is on; it is the single best proxy for how hard a
              node-link diagram is to read.
  bend/edge   mean number of turns per edge. Orthogonal drawings pay for every bend.
  edge/cell   total edge length divided by the number of nodes times the mean node pitch.
              A value near 1 means edges are roughly as long as the distance between
              neighbours; large values mean long runs across the canvas.
  aspect      canvas width / height.
  fill        sum of node areas / canvas area. Very low values mean the drawing is mostly
              empty space, which usually comes from bad column assignment.

Crossings are counted on orthogonal segment pairs, excluding segments that belong to the
same edge and excluding shared endpoints (a junction is not a crossing). Box borders and
compartment separators are not edges and are removed first.

Usage:
    python research/tools/layout_metrics.py out/foo.svg [...]
    python research/tools/layout_metrics.py --json out/*.svg
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

RECT = re.compile(
    r'<rect x="([-\d.]+)" y="([-\d.]+)" width="([-\d.]+)" height="([-\d.]+)"([^/]*)/>')
PATH = re.compile(r'<path d="([^"]+)"([^>]*)/>')
LINE = re.compile(r'<line x1="([-\d.]+)" y1="([-\d.]+)" x2="([-\d.]+)" y2="([-\d.]+)"([^>]*)/>')
ATTR = re.compile(r'([\w-]+)="([^"]*)"')
SVG = re.compile(r'<svg[^>]*width="(\d+)" height="(\d+)"')


def polyline(d: str):
    """Absolute M/V/H/L polylines; one list per subpath."""
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
            continue
        if cmd == "M":
            if len(cur) > 1:
                out.append(cur)
            x, y = float(tokens[i]), float(tokens[i + 1])
            cur = [(x, y)]
            i += 2
        elif cmd == "V":
            y = float(tokens[i])
            cur.append((x, y))
            i += 1
        elif cmd == "H":
            x = float(tokens[i])
            cur.append((x, y))
            i += 1
        elif cmd == "L":
            x, y = float(tokens[i]), float(tokens[i + 1])
            cur.append((x, y))
            i += 2
        elif cmd == "l":
            x, y = x + float(tokens[i]), y + float(tokens[i + 1])
            cur.append((x, y))
            i += 2
        else:
            i += 1
    if len(cur) > 1:
        out.append(cur)
    return out


def segments(polys):
    segs = []
    for poly in polys:
        for a, b in zip(poly, poly[1:]):
            if abs(a[0] - b[0]) > 0.4 or abs(a[1] - b[1]) > 0.4:
                segs.append((a, b))
    return segs


def is_h(seg):
    return abs(seg[0][1] - seg[1][1]) < 0.4


def crossings(edges):
    """Orthogonal crossings between segments of different edges."""
    flat = []
    for idx, segs in enumerate(edges):
        for s in segs:
            flat.append((idx, s))
    n = 0
    for i in range(len(flat)):
        ei, (a1, a2) = flat[i]
        if not is_h((a1, a2)):
            continue
        for j in range(i + 1, len(flat)):
            ej, (b1, b2) = flat[j]
            if ei == ej or is_h((b1, b2)):
                continue
            # a is horizontal, b is vertical
            ax0, ax1 = sorted((a1[0], a2[0]))
            ay = a1[1]
            bx = b1[0]
            by0, by1 = sorted((b1[1], b2[1]))
            if not (ax0 + 0.6 < bx < ax1 - 0.6):
                continue
            if not (by0 + 0.6 < ay < by1 - 0.6):
                continue
            n += 1
    return n


def measure(path: Path) -> dict:
    svg = path.read_text(encoding="utf-8")
    m = SVG.search(svg)
    width, height = (int(m.group(1)), int(m.group(2))) if m else (1, 1)

    frames = []
    for r in RECT.finditer(svg):
        x, y, w, h, extra = r.groups()
        a = dict(ATTR.findall(extra))
        if a.get("stroke") != "#000000" or float(w) < 20 or float(h) < 20:
            continue
        frames.append((float(x), float(y), float(w), float(h)))

    # Compartment separators are lines that span exactly a frame's x-range.
    def is_separator(seg):
        (x1, y1), (x2, y2) = seg
        if abs(y1 - y2) > 0.4:
            return False
        lo, hi = sorted((x1, x2))
        return any(abs(lo - fx) < 1.5 and abs(hi - (fx + fw)) < 1.5 for fx, fy, fw, fh in frames)

    def is_border(seg):
        (x1, y1), (x2, y2) = seg
        for fx, fy, fw, fh in frames:
            if abs(y1 - y2) < 0.4:
                if abs(y1 - fy) < 1.0 or abs(y1 - (fy + fh)) < 1.0:
                    if min(x1, x2) >= fx - 1 and max(x1, x2) <= fx + fw + 1:
                        return True
            elif abs(x1 - x2) < 0.4:
                if abs(x1 - fx) < 1.0 or abs(x1 - (fx + fw)) < 1.0:
                    if min(y1, y2) >= fy - 1 and max(y1, y2) <= fy + fh + 1:
                        return True
        return False

    edges = []
    for p in PATH.finditer(svg):
        d, extra = p.groups()
        a = dict(ATTR.findall(extra))
        if a.get("fill") != "none":
            continue
        # One path element may carry many subpaths (the renderer merges shared runs), so each
        # subpath is its own edge for the purposes of counting.
        for poly in polyline(d):
            segs = [s for s in segments([poly]) if not is_separator(s) and not is_border(s)]
            if segs:
                edges.append(segs)
    for l in LINE.finditer(svg):
        vals = [float(l.group(i)) for i in range(1, 5)]
        extra = l.group(5)
        a = dict(ATTR.findall(extra))
        if a.get("stroke-dasharray"):
            continue
        seg = ((vals[0], vals[1]), (vals[2], vals[3]))
        if abs(seg[0][0] - seg[1][0]) < 0.4 and abs(seg[0][1] - seg[1][1]) < 0.4:
            continue
        if is_separator(seg) or is_border(seg):
            continue
        edges.append([seg])

    hard = []
    for segs in edges:
        kept = []
        for s in segs:
            # arrow heads are 4px decorations, not edges
            if abs(s[0][0] - s[1][0]) + abs(s[0][1] - s[1][1]) >= 6.0:
                kept.append(s)
        if kept:
            hard.append(kept)

    # Collinear overlap is only a defect when the two strands are unrelated. An ownership bus
    # and a typing trunk are drawn as *one* shared run by construction — several edges converge
    # on the same horizontal line, which is the whole point of a bus, and the renderer emits
    # that line once. Counting those as overlap would punish the intended design.
    #
    # Two runs are treated as one bus when they share an endpoint column and sit on the same y:
    # that is a convergence point, not two wires laid on top of each other. What remains is a
    # genuine defect — two independent connections routed through the same corridor.
    overlap = 0.0
    flat = [seg for segs in hard for seg in segs]

    def shares_end(a, b):
        ax0, ax1 = sorted((a[0][0], a[1][0]))
        bx0, bx1 = sorted((b[0][0], b[1][0]))
        if abs(a[0][1] - a[1][1]) < 0.5:
            return abs(ax0 - bx0) < 2 or abs(ax1 - bx1) < 2 or abs(ax0 - bx1) < 2 or abs(ax1 - bx0) < 2
        ay0, ay1 = sorted((a[0][1], a[1][1]))
        by0, by1 = sorted((b[0][1], b[1][1]))
        return abs(ay0 - by0) < 2 or abs(ay1 - by1) < 2 or abs(ay0 - by1) < 2 or abs(ay1 - by0) < 2

    for i in range(len(flat)):
        (a1, a2) = flat[i]
        for j in range(i + 1, len(flat)):
            (b1, b2) = flat[j]
            if is_h((a1, a2)) and is_h((b1, b2)) and abs(a1[1] - b1[1]) < 0.5:
                lo = max(min(a1[0], a2[0]), min(b1[0], b2[0]))
                hi = min(max(a1[0], a2[0]), max(b1[0], b2[0]))
                if hi - lo > 4.0 and not shares_end((a1, a2), (b1, b2)):
                    overlap += hi - lo
            elif (not is_h((a1, a2))) and (not is_h((b1, b2))) and abs(a1[0] - b1[0]) < 0.5:
                lo = max(min(a1[1], a2[1]), min(b1[1], b2[1]))
                hi = min(max(a1[1], a2[1]), max(b1[1], b2[1]))
                if hi - lo > 4.0 and not shares_end((a1, a2), (b1, b2)):
                    overlap += hi - lo

    total_len = sum(
        abs(a[0] - b[0]) + abs(a[1] - b[1]) for segs in hard for a, b in segs)
    # The renderer merges shared runs into one path, so a "edge" here is a maximal run of
    # connected segments, not one model edge. Bends are counted per polyline; the ratio is
    # reported against the number of polylines to stay comparable across renderers.
    bends = sum(max(0, len(s) - 1) for s in hard)
    node_area = sum(fw * fh for _, _, fw, fh in frames)
    mean_pitch = (width / max(1, len(frames))) if frames else 1.0

    return {
        "file": path.name,
        "size": [width, height],
        "nodes": len(frames),
        "edges": len(hard),
        "crossings": crossings(hard),
        "overlap": round(overlap),
        "bends": bends,
        "bends_per_edge": round(bends / len(hard), 2) if hard else 0.0,
        "edge_len": round(total_len),
        "edge_per_cell": round(total_len / max(1e-6, len(frames) * mean_pitch), 2),
        "aspect": round(width / max(1, height), 2),
        "fill": round(node_area / max(1, width * height), 3),
    }


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    as_json = "--json" in sys.argv
    rows = [measure(Path(a)) for a in args]
    if as_json:
        print(json.dumps(rows, indent=2))
        return
    print(f"{'file':34s} {'nodes':>5s} {'edges':>5s} {'cross':>5s} {'overlap':>8s} "
          f"{'bend/e':>6s} {'len':>6s} {'len/cell':>8s} {'aspect':>6s} {'fill':>5s}")
    for r in rows:
        print(f"{r['file']:34s} {r['nodes']:5d} {r['edges']:5d} {r['crossings']:5d} "
              f"{r['overlap']:8d} {r['bends_per_edge']:6.2f} {r['edge_len']:6d} "
              f"{r['edge_per_cell']:8.2f} {r['aspect']:6.2f} {r['fill']:5.3f}")
    tot_c = sum(r["crossings"] for r in rows)
    tot_e = sum(r["edges"] for r in rows)
    print(f"\ntotal crossings {tot_c} over {tot_e} edges")


if __name__ == "__main__":
    main()
