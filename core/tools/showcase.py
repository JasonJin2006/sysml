#!/usr/bin/env python3
"""Render the maintained example models into a self-contained local gallery."""

import argparse
import csv
import html
import io
from pathlib import Path
import subprocess
import sys

VIEWS = [
    ("general", "gv", "General view", "Definitions, usages, shared types, variants and satisfaction relationships."),
    ("interconnection", "iv", "Interconnection view", "Nested equipment and binary connections through boundary ports."),
    ("packages", "pv", "Package view", "Nested namespace frames. Import relationships are not drawn."),
    ("states", "stv", "State transitions", "Two state machines, branching transitions, cycles and guards, laid out by transition topology."),
    ("actions", "afv", "Action flow", "Action successions with branches and cycles. Not a demonstration of specialized if/loop or fork/join notation."),
    ("sequence", "sv", "Sequence view", "Four participants and source-ordered sends. Accept pairing, event markers and nested sequence references are not implemented."),
    ("traceability", "tv", "Requirement traceability", "Many-to-many satisfaction relationships and a deliberately unmet requirement."),
]


def run(exe, *args):
    result = subprocess.run([str(exe), *map(str, args)], capture_output=True, encoding="utf-8", check=True)
    return result.stdout


def table_markup(text):
    rows = list(csv.reader(io.StringIO(text)))
    if not rows:
        return ""
    heading = "".join(f"<th>{html.escape(cell)}</th>" for cell in rows[0])
    body = "".join("<tr>" + "".join(f"<td>{html.escape(cell)}</td>" for cell in row) + "</tr>" for row in rows[1:])
    return f"<div class='scroll'><table><thead><tr>{heading}</tr></thead><tbody>{body}</tbody></table></div>"


def main():
    core = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    binary = "sysml.exe" if sys.platform == "win32" else "sysml"
    parser.add_argument("--binary", type=Path, default=core / "target/release" / binary)
    parser.add_argument("--out", type=Path, default=core / "target/tmp/showcase")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    examples = core / "examples/showcase"
    sections = []
    links = []
    for name, view, title, detail in VIEWS:
        model = examples / f"{name}.sysml"
        target = args.out / f"{name}.svg"
        run(args.binary, "render", model, "--view", view, "--mode", "extended", "--out", target)
        svg = target.read_text(encoding="utf-8")
        svg = svg[svg.index("<svg"):]
        sections.append(f"<section id='{name}'><h2>{html.escape(title)}</h2><p>{html.escape(detail)}</p><div class='diagram'>{svg}</div></section>")
        links.append(f"<a href='#{name}'>{html.escape(title)}</a>")
        print(f"Rendered {name}: {target}")
    for view, title in [("gv", "Actors and stakeholders"), ("stv", "Nested states and state actions"), ("afv", "Decision, merge, fork and join")]:
        key = f"notation-{view}"
        target = args.out / f"{key}.svg"
        run(args.binary, "render", examples / "notation.sysml", "--view", view, "--mode", "extended", "--out", target)
        svg = target.read_text(encoding="utf-8")
        svg = svg[svg.index("<svg"):]
        sections.append(f"<section id='{key}'><h2>{title}</h2><p>Notation details: distinct actor/stakeholder forms, entry and done symbols, state-action compartments and control nodes. Pixel sizes are local rendering choices, not mandatory standard dimensions.</p><div class='diagram'>{svg}</div></section>")
        links.append(f"<a href='#{key}'>{title}</a>")
    trace = examples / "traceability.sysml"
    for name, flag in [("elements", []), ("matrix", ["--matrix"])]:
        text = run(args.binary, "table", trace, *flag)
        (args.out / f"{name}.csv").write_text(text, encoding="utf-8")
        sections.append(f"<section id='{name}'><h2>{'Element table' if name == 'elements' else 'Satisfaction matrix'}</h2><p>Fixed projections exported by the table command, not configurable GridView parity.</p>{table_markup(text)}</section>")
        links.append(f"<a href='#{name}'>{'Table' if name == 'elements' else 'Matrix'}</a>")
    outline = run(args.binary, "outline", trace)
    (args.out / "outline.txt").write_text(outline, encoding="utf-8")
    sections.append(f"<section id='browser'><h2>Containment tree</h2><p>The CLI outline projection. Interactive navigation is available in Studio and the VS Code Outline panel.</p><pre>{html.escape(outline)}</pre></section>")
    links.append("<a href='#browser'>Containment tree</a>")
    page = """<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>SysML rendering showcase</title>
<style>
:root{font-family:'Segoe UI',Arial,sans-serif;color:#222;background:#f8f8f8;color-scheme:light}*{box-sizing:border-box}body{margin:0}header,main,nav{max-width:1440px;margin:auto;padding:20px 28px}header{padding-top:36px}h1{font-size:28px;font-weight:600;margin:0 0 12px}h2{font-size:20px;font-weight:600}p{max-width:80ch;line-height:1.6;color:#4d4d4d}nav{display:flex;gap:8px 18px;flex-wrap:wrap;background:#fff;border-block:1px solid #ddd}a{color:#005fb8;text-underline-offset:3px}section{scroll-margin:20px;margin:0 0 48px}.diagram{overflow:auto;background:#fff;border:1px solid #ddd;padding:16px}.diagram svg{display:block;width:100%;height:auto;min-width:660px}table{border-collapse:collapse;background:#fff;font-size:13px}td,th{border:1px solid #ddd;padding:7px 10px;text-align:left;white-space:nowrap}th{background:#f0f0f0;font-weight:600}.scroll{overflow:auto}pre{background:#fff;border:1px solid #ddd;padding:20px;overflow:auto;font-size:13px;line-height:1.6}@media(max-width:600px){header,main,nav{padding:16px}.diagram{padding:8px}}
</style><header><h1>SysML rendering showcase</h1><p>Seven implemented diagram projections, plus tables, a matrix and a containment tree. These examples demonstrate current capabilities, not complete parity with Syside's graphical notation coverage. GeometryView, swim lanes and nested sequence references are not implemented.</p></header>"""
    page += "<nav aria-label='Examples'>" + "".join(links) + "</nav><main>" + "".join(sections) + "</main></html>"
    gallery = args.out / "gallery.html"
    gallery.write_text(page, encoding="utf-8")
    print(f"Gallery: {gallery}")


if __name__ == "__main__":
    main()
