import json
import os

SRC = r"C:\Users\JasonJin06\Desktop\sysml\core\crates\sysml-semantics\data\spec_rules.json"
OUT = r"C:\Users\JasonJin06\Desktop\sysml\core\crates\sysml-semantics\data\spec_rules.rs"


def esc(text):
    return (
        text.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\t", "\\t")
    )


rules = json.load(open(SRC, encoding="utf-8"))
lines = [
    "// Generated from the SysML v2 metamodel's own OCL constraint table.",
    "// Regenerate with `python tools/spec_rules.py --emit-rust` after a metamodel update.",
    "&[",
]
for r in rules:
    lines.append("    SpecRule {")
    lines.append(f'        name: "{esc(r["name"])}",')
    lines.append(f'        metaclass: "{esc(r["metaclass"])}",')
    lines.append(f'        ocl: "{esc(r["ocl"])}",')
    lines.append(f'        says: "{esc(r["says"])}",')
    lines.append("    },")
lines.append("]")
open(OUT, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
print("wrote", OUT, len(rules))
