#!/usr/bin/env python
"""Mutation test for the gate.

Fourteen defects are injected into a model that otherwise passes. Each is a thing an AI writing
a SysML model actually does — not an artificial corruption (a truncated brace would only prove
the parser works). The gate is credited for a defect when `check` or `prov` reports it as an
**error**, because a warning does not stop a build and therefore does not stop the defect.

This is the one mutation test in the repository. An earlier study collected twelve mutants in a
separate script during the five-dimension experiment; the three kinds it had that were missing
here became s04, s05 and m03, and the rest duplicated a case below.

    python tools/mutation_test.py [--binary <sysml.exe>] [--json]

Exit code is 0 when every class is caught, 1 otherwise, so it can gate a release.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

DEFAULT_BINARY = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "target", "release", "sysml.exe"
)

# The clean model every mutation is applied to. It must pass both gates untouched; the runner
# asserts that first, because a mutant of a broken model proves nothing.
BASE = """
package Mut {
    private import ScalarValues::*;
    private import ISQ::*;

    part def Battery {
        attribute mass : MassValue = 4.0 [kg];
        attribute capacity : EnergyValue = 5000.0 [J];
    }

    part def Motor {
        attribute mass : MassValue = 2.0 [kg];
        attribute power : PowerValue = 250.0 [W];
    }

    part def Chassis {
        port left : MountPoint;
        port right : MountPoint;
    }

    port def MountPoint {
        in load : Real;
    }

    part def Vehicle {
        part battery : Battery;
        part motor : Motor;
        part chassis : Chassis;
        attribute dryMass : MassValue = battery.mass + motor.mass;
        attribute endurance : TimeValue = battery.capacity / motor.power;
    }

    requirement def MassBudget {
        subject v : Vehicle;
        require constraint { v.dryMass <= 20 [kg] }
    }

    requirement def Endurance {
        subject v : Vehicle;
        require constraint { v.endurance >= 10 [s] }
    }
}
"""

# Every mutation states what an AI would plausibly have done, so the report can say which real
# failure modes are covered rather than which strings happen to be broken.
MUTATIONS = [
    # --- structural: name resolution ---
    ("s01", "structure", "typo in a type name",
     [("mass : MassValue = 4.0 [kg]", "mass : MasValue = 4.0 [kg]")]),
    ("s02", "structure", "typo in a feature reference",
     [("v.dryMass <= 20", "v.dryMasss <= 20")]),
    # The rig is a part *usage* inside Vehicle rather than a new part def, so PROV017 does not
    # also fire. A mutation must be caught by the check it targets, or it stops measuring that
    # check at all.
    ("s03", "structure", "connection to a port that does not exist",
     [("part chassis : Chassis;",
       "part chassis : Chassis;\n        connect battery.nonexistent to chassis.left;")]),
    ("s04", "structure", "a value computed from a name that does not exist",
     [("attribute dryMass : MassValue = battery.mass + motor.mass",
       "attribute dryMass : MassValue = battery.mas + motor.mass")]),
    ("s05", "structure", "a part typed by a definition that does not exist",
     [("part chassis : Chassis;", "part chassis : Chassiss;")]),

    # --- semantic: does the claim hold ---
    ("m01", "semantic", "a threshold that is false",
     [("v.dryMass <= 20 [kg]", "v.dryMass <= 2 [kg]")]),
    ("m02", "semantic", "a second constraint clause that is false",
     [("require constraint { v.dryMass <= 20 [kg] }",
       "require constraint { v.dryMass <= 20 [kg] }\n        require constraint { v.dryMass <= 1 [kg] }")]),
    ("m03", "semantic", "a placeholder constraint that is trivially true",
     [("v.endurance >= 10 [s]", "1 > 0")]),

    # --- provenance: where the number came from ---
    ("p01", "provenance", "a value with no ledger entry",
     [("mass : MassValue = 2.0 [kg]", "mass : MassValue = 2.5 [kg]")]),
    ("p02", "provenance", "a spec value citing nothing",
     [],  # handled by ledger mutation below
     ),
    ("p03", "provenance", "an assumption with no falsifier",
     [],
     ),
    ("p04", "provenance", "a derivation whose input is an assumption",
     [],
     ),
    ("p05", "provenance", "claiming a human answered a question that is open",
     [],
     ),
    ("p06", "provenance", "an assumption nobody approved",
     [],
     ),
    ("p07", "provenance", "a part with no record of why it exists",
     [],
     ),
    ("p08", "provenance", "a part row that is present but empty",
     [],
     ),
    ("p09", "provenance", "one jurisdiction cited with the search scope unstated",
     [],
     ),
    ("p10", "provenance", "regulations cited without naming the market",
     [],
     ),

    # --- type: is the claim even about the right thing ---
    ("t01", "type", "a subsystem satisfying a whole-system requirement",
     [("requirement def MassBudget {\n        subject v : Vehicle;\n        require constraint { v.dryMass <= 20 [kg] }\n    }",
       "requirement def MassBudget {\n        subject v : Vehicle;\n        require constraint { v.dryMass <= 20 [kg] }\n    }\n\n    part oneBattery : Battery;\n    satisfy MassBudget by oneBattery;")]),
]

# A ledger that passes: two spec values citing a real source, two assumptions approved by a
# person. Mutations below change one field at a time.
BASE_LEDGER = {
    "model": "mut.sysml",
    "market": "CN",
    "jurisdictions": "CN,US",
    "sources": [{
        "id": "S001", "kind": "datasheet", "title": "reference module",
        "locator": "https://example.invalid", "quote": "mass 4.0 kg, capacity 5000 J",
        "accessed": "2026-10-25", "jurisdiction": "US", "added_by": "agent",
    }],
    "questions": [],
    "decisions": [
        {"id": "D001", "element": "Mut::Battery::mass", "value": "4.0 [kg]", "origin": "spec",
         "why": "datasheet", "falsifier": "", "inputs": [], "question": None,
         "decided_by": "agent", "approved_by": None, "cites": ["S001"]},
        {"id": "D002", "element": "Mut::Battery::capacity", "value": "5000.0 [J]", "origin": "spec",
         "why": "datasheet", "falsifier": "", "inputs": [], "question": None,
         "decided_by": "agent", "approved_by": None, "cites": ["S001"]},
        {"id": "D003", "element": "Mut::Motor::mass", "value": "2.0 [kg]", "origin": "assumed",
         "why": "typical for this class", "falsifier": "if the selected motor weighs more",
         "inputs": [], "question": None, "decided_by": "agent",
         "approved_by": "human:tester", "cites": []},
        {"id": "D004", "element": "Mut::Motor::power", "value": "250.0 [W]", "origin": "assumed",
         "why": "typical", "falsifier": "if the datasheet says otherwise", "inputs": [],
         "question": None, "decided_by": "agent", "approved_by": "human:tester", "cites": []},
        {"id": "D005", "element": "Mut::Vehicle::dryMass", "value": "6.0 [kg]",
         "origin": "derived", "why": "sum of parts", "falsifier": "",
         "inputs": ["Mut::Battery::mass"], "question": None,
         "decided_by": "agent", "approved_by": None, "cites": []},
        {"id": "D006", "element": "Mut::Vehicle::endurance", "value": "20.0 [s]",
         "origin": "derived", "why": "capacity over power", "falsifier": "",
         "inputs": ["Mut::Battery::capacity"], "question": None,
         "decided_by": "agent", "approved_by": None, "cites": []},
        # One row per part definition, stating why the design contains that kind of thing.
        # PROV017 asks for these; without them the base model is not clean and every mutation
        # below would be measuring the wrong thing.
        {"id": "D007", "element": "Mut::Battery", "value": "one pack",
         "origin": "assumed", "why": "the vehicle needs stored energy",
         "falsifier": "if the design turns out tethered", "inputs": [],
         "question": None, "decided_by": "agent", "approved_by": "human:tester",
         "cites": []},
        {"id": "D008", "element": "Mut::Motor", "value": "one motor",
         "origin": "assumed", "why": "the vehicle needs propulsion",
         "falsifier": "if it turns out to be a glider", "inputs": [],
         "question": None, "decided_by": "agent", "approved_by": "human:tester",
         "cites": []},
        {"id": "D009", "element": "Mut::Chassis", "value": "one frame",
         "origin": "assumed", "why": "battery and motor have to attach to something",
         "falsifier": "if the battery and motor are integrated into one housing",
         "inputs": [], "question": None, "decided_by": "agent",
         "approved_by": "human:tester", "cites": []},
        {"id": "D010", "element": "Mut::Vehicle", "value": "one vehicle",
         "origin": "spec", "why": "the requirement is written about a vehicle",
         "falsifier": "", "inputs": [], "question": None,
         "decided_by": "agent", "approved_by": None, "cites": ["S001"]},
    ],
}

LEDGER_MUTATIONS = {
    # a spec value that cites nothing
    "p02": lambda d: [x for x in d if x["id"] != "D001"] + [
        dict(next(x for x in d if x["id"] == "D001"), cites=[])],
    # an assumption with no falsifier
    "p03": lambda d: [dict(x, falsifier="") if x["id"] == "D003" else x for x in d],
    # a derivation whose input is an assumption
    "p04": lambda d: [dict(x, inputs=["Mut::Motor::mass"]) if x["id"] == "D005" else x for x in d],
    # claiming a human answered while the question is open. The element has to be a value the
    # model actually writes: a port carries no value, so nothing would check it and the mutation
    # would prove nothing about the gate.
    "p05": lambda d: [
        dict(x, origin="asked", question="Q001", cites=[]) if x["id"] == "D003" else x
        for x in d
    ],
    # an assumption nobody approved
    "p06": lambda d: [dict(x, approved_by=None) if x["id"] == "D003" else x for x in d],
    # a part with no record of why it exists. Deleting the row is the honest form: the model
    # still declares the part, and nothing says who decided it should be there.
    "p07": lambda d: [x for x in d if x["id"] != "D009"],
    # a part row that exists but says nothing. This is how the first version of PROV017 was
    # defeated: the gate checked that a row named the part, not that the row meant anything.
    "p08": lambda d: [
        dict(x, why="", falsifier="", approved_by=None) if x["id"] == "D009" else x
        for x in d
    ],
}

# Mutations that change the ledger's scope fields rather than its decisions.
SCOPE_MUTATIONS = {
    # Everything cited is US, and the ledger does not say the search stopped there. This is the
    # flight deck's real defect: 9 US instruments, 3 EU, 0 CN, and nothing recording that China
    # was never queried.
    "p09": lambda l: dict(l, jurisdictions=""),
    # Regulations chosen without a market to choose them. The load factors then size real parts
    # while the question "who is this sold to" is still unanswered.
    "p10": lambda l: dict(l, market=""),
}


def run(binary, model, ledger, workdir):
    mpath = os.path.join(workdir, "mut.sysml")
    lpath = os.path.join(workdir, "mut.ledger.json")
    with open(mpath, "w", encoding="utf8") as fh:
        fh.write(model)
    with open(lpath, "w", encoding="utf8") as fh:
        json.dump(ledger, fh, ensure_ascii=False, indent=2)

    results = {}
    for command in ("check", "prov"):
        proc = subprocess.run([binary, command, mpath], capture_output=True, text=True)
        results[command] = {
            "code": proc.returncode,
            "output": proc.stdout + proc.stderr,
        }
    return results


def errors_in(text):
    """Diagnostics that actually fail a build.

    Two shapes exist: the semantic and provenance checks carry a code (`error[SEM005]`), while
    the reference pass prints `type unresolved: X` / `predicate unresolved: X` without one.
    Both fail the build, so both count; reading only the bracketed form would miss the name
    resolution failures the tool has always caught.
    """
    out = []
    for line in text.splitlines():
        m = re.search(r"error\[([A-Z]+\d+)\]", line)
        if m:
            out.append(m.group(1))
            continue
        if re.search(r":\s*(?:type|predicate)\s+unresolved:", line):
            out.append("REF")
            continue
        if ": ambiguous:" in line:
            out.append("AMB")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=DEFAULT_BINARY)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    if not os.path.exists(args.binary):
        print(f"no binary at {args.binary}", file=sys.stderr)
        return 2

    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        # 1. The clean model must pass, or a mutant of it proves nothing.
        base = run(args.binary, BASE, BASE_LEDGER, tmp)
        if base["check"]["code"] != 0 or base["prov"]["code"] != 0:
            print("the base model does not pass; every mutation below would be meaningless",
                  file=sys.stderr)
            print(base["check"]["output"][-800:], file=sys.stderr)
            print(base["prov"]["output"][-800:], file=sys.stderr)
            return 2

        for entry in MUTATIONS:
            mid, family, description = entry[0], entry[1], entry[2]
            patches = entry[3] if len(entry) > 3 else []

            model = BASE
            for old, new in patches:
                if old not in model:
                    print(f"{mid}: mutation anchor not found: {old[:60]!r}", file=sys.stderr)
                    return 2
                model = model.replace(old, new, 1)

            ledger = json.loads(json.dumps(BASE_LEDGER))
            if mid in LEDGER_MUTATIONS:
                ledger["decisions"] = LEDGER_MUTATIONS[mid](ledger["decisions"])
            if mid in SCOPE_MUTATIONS:
                ledger = SCOPE_MUTATIONS[mid](ledger)
            if mid == "p05":
                ledger["questions"] = [{
                    "id": "Q001", "at": "Mut::Chassis", "text": "which side?",
                    "blocks": [], "state": "open", "answer": None, "answered_by": None,
                    "asked_by": "agent", "raised_at": None}]

            res = run(args.binary, model, ledger, tmp)
            caught_by = sorted(
                set(errors_in(res["check"]["output"])) | set(errors_in(res["prov"]["output"]))
            )
            caught = bool(caught_by)
            rows.append({
                "id": mid,
                "family": family,
                "defect": description,
                "caught": caught,
                "codes": caught_by,
                "check_code": res["check"]["code"],
                "prov_code": res["prov"]["code"],
            })

    caught = sum(1 for r in rows if r["caught"])
    if args.json:
        print(json.dumps({"caught": caught, "total": len(rows), "rows": rows},
                         ensure_ascii=False, indent=2))
    else:
        print(f"defects caught as errors: {caught}/{len(rows)}\n")
        by_family = {}
        for r in rows:
            by_family.setdefault(r["family"], []).append(r)
        for family, items in by_family.items():
            hit = sum(1 for i in items if i["caught"])
            print(f"{family:12s} {hit}/{len(items)}")
            for i in items:
                mark = "caught" if i["caught"] else "MISSED"
                codes = ",".join(i["codes"]) if i["codes"] else "-"
                print(f"  {i['id']}  {mark:7s} {codes:12s} {i['defect']}")
    return 0 if caught == len(rows) else 1


if __name__ == "__main__":
    sys.exit(main())
