"""Classify every rule the kernel does not enforce by the one thing it is waiting on.

The ledger has four states: enforced, not applicable, in conflict, and blocked. This script
produces the last: for each rule in none of the first three lists, the metaclass, derived property
or element the rule needs before a check can decide it.

The point of writing it down is that "blocked" must not be a synonym for "not done". Each name here
is either a metaclass from `crates/sysml-model/data/metaclasses.rs` (Layer 1), a derived property
from `DERIVED` below (Layer 2), or a metaclass the notation implies but the lowering does not build
(Layer 3). A name outside those three is a refusal to classify, and the script exits non-zero.

Usage:  python tools/blocked_by.py [--check]
        --check verifies the committed table is current without writing.
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORE = os.path.dirname(HERE)
RULES = os.path.join(CORE, "crates", "sysml-semantics", "data", "spec_rules.rs")
METAS = os.path.join(CORE, "crates", "sysml-model", "data", "metaclasses.rs")
CONFORMANCE = os.path.join(CORE, "crates", "sysml-semantics", "src", "conformance.rs")
OUT = os.path.join(CORE, "crates", "sysml-semantics", "data", "blocked_by.rs")

# Derived properties and operations the specification's OCL reads, which the tree does not carry.
# Layer 2 builds these; a rule naming one is waiting for that property, not for a metaclass.
#
# A name here must be a property the metamodel declares (`research/upstream/SysML.json`), not an
# OCL helper operation. `isDistinguishableFrom` and `canAccess` are operations, not properties: the
# first's OCL body is stated (`generated.rs:9731-9735`) and `member_duplicates` decides it as a
# warning, the second is a visibility query whose direction the corpus contradicts. Neither can be
# a Layer 2 deliverable, so a rule that reads one is classified not-applicable instead.
DERIVED = [
    "chainingFeature",
    "condition",
    "crossFeature",
    "featureTarget",
    "featuringType",
    "featureValue",
    "instantiatedType",
    "isAbstract",
    "isImplied",
    "isModelLevelEvaluable",
    "isPortion",
    "multiplicity",
    "portionKind",
    "referent",
    "referencedElement",
    "result",
    "visibility",
]

# OCL helper operations on a metaclass, which no field can carry. Listed so a rule that reads one
# is not mistaken for one waiting on a missing property.
OPERATIONS = [
    "canAccess",
    "isDistinguishableFrom",
]

# One entry per rule that is neither enforced nor not applicable. The name is what the rule is
# waiting on; the comment above each group says which layer supplies it.
BLOCKED = {
    # -- Layer 1: a metaclass the projection does not produce --------------------------------
    # -- Layer 2: a derived property the OCL reads ------------------------------------------
    # The written type reference and the resolver hold the data; the derivations are not built.
    "validateInstantiationExpressionResult": "result",
    # The OCL guards this on `chainingFeatures->size() = 2 and endFeatures->size() = 2`, so a
    # crossing whose target has no chain satisfies no antecedent and reports nothing. The pilot's
    # own assertion (`CrossFeatures_invalid.kerml.xt:39-41`, "Cross subsetting must chain through an
    # opposite end feature") fires on exactly that model, so the two authorities disagree about the
    # same construct. Implementing the pilot's reading would contradict the OCL the ledger is
    # measured against; the OCL reading reports nothing and cannot be told from a missing check.
    # Recorded here rather than implemented, per the rule that a corpus defect is never a reason to
    # withdraw a rule — and neither is a pilot assertion a reason to write one the OCL denies.
    # -- Layer 3: an element the notation implies and the lowering does not build ------------
    # `validateTransitionUsageParameters` stays here for its second branch only. Every
    # `TransitionUsage` production writes an `EmptyParameterMember` (`SysML.xtext:1708-1724`), so
    # `inputParameters()->size() >= 1` holds by construction; the `>= 2` branch needs the trigger
    # counted as a parameter, and a trigger reaches the tree as `Element::trigger_expr`, not as a
    # member.
    # These four were not applicable on the argument that the production always writes the members
    # the rule counts. The argument is sound and the premise was not: this parser accepts
    # `if then b;`, `while;` and `for in 1..3 { }`, each of which the grammar forbids, and lowers
    # them to control flows with the members missing. So the rules have something to reject after
    # all — once the front end stops accepting the forms.
}

LAYER3 = [
    "ReturnParameterMembership",
    "TransitionFeatureMembership",
    # The parameters an action's notation implies, which come from the library base it specialises
    # (`AcceptAction.payload`, `SendAction.sender`/`receiver`) rather than from the model text.
    # `derived::input_parameters` already answers the OCL's `inputParameters()` for what the model
    # wrote; these rules count the implied ones as well.
    "inputParameters",
    # `Membership::Parameter` — the `ParameterMembership` a message payload and a call argument are
    # owned through (`PayloadParameterMember`, `ArgumentExpressionMember`). The enum has the variant
    # (`sysml-model/src/lib.rs`), and no lowering fills it, because the notation that would write it
    # — the payload of an `accept`/`send`, the arguments of a call — reaches the tree as text.
    "parameterMembership",
    # A production that requires a member, which this parser does not yet require. The rule reads
    # like a restatement of the grammar — "`IfNode` writes a condition and a body, so an if has two
    # parameters" — and that reading is sound only if the parser rejects `if then b;`. It does not:
    # the form lowers to a control flow with no condition, so the rule's `>= 2` is decidable after
    # all, once the front end stops accepting the form.
    "grammarEnforcement",
]


def read_rules(path):
    """Rule name → metaclass, from the generated table (no regex: the OCL has escapes)."""
    out = {}
    text = open(path, encoding="utf-8").read()
    for chunk in text.split("SpecRule {")[1:]:
        name = _field(chunk, "name:")
        meta = _field(chunk, "metaclass:")
        if name:
            out[name] = meta
    return out


def _field(chunk, key):
    i = chunk.find(key)
    if i < 0:
        return None
    a = chunk.index('"', i) + 1
    b = chunk.index('"', a)
    return chunk[a:b]


def read_names(path, opener, closer, marker):
    """Rule names in one of conformance's two tables, between `opener` and `closer`."""
    text = open(path, encoding="utf-8").read()
    start = text.find(opener)
    if start < 0:
        return set()
    end = text.find(closer, start)
    body = text[start:end]
    out = set()
    for chunk in body.split(marker)[1:]:
        parts = chunk.split('"')
        if len(parts) > 1:
            out.add(parts[0] if marker == '("' else parts[2])
    return out


def read_not_applicable(path):
    """Rule names in `NOT_APPLICABLE`, which is formatted one field per line."""
    text = open(path, encoding="utf-8").read()
    start = text.find("pub const NOT_APPLICABLE")
    end = text.find("pub enum Status", start)
    out = set()
    for line in text[start:end].split("\n"):
        stripped = line.strip()
        if stripped.startswith('"validate') and stripped.endswith('",'):
            out.add(stripped[1:-2])
    return out


def read_conflict(path):
    """Rule names in `CONFLICT`, which record a disagreement rather than a blocker.

    The list sits after `Status`, so `read_not_applicable` — which stops at the enum — cannot see
    it. It is read separately here because a conflict is not waiting on anything: filing it as
    blocked would claim the tree is missing a fact, and the tree is not the side that disagrees.
    """
    text = open(path, encoding="utf-8").read()
    start = text.find("pub const CONFLICT")
    if start < 0:
        return set()
    end = text.find("\n];", start)
    out = set()
    for line in text[start:end].split("\n"):
        stripped = line.strip()
        if stripped.startswith('"validate') and stripped.endswith('",'):
            out.add(stripped[1:-2])
    return out


def main():
    check = "--check" in sys.argv
    rules = read_rules(RULES)
    metas = set(re.findall(r'name: "(\w+)"', open(METAS, encoding="utf-8").read()))
    # `implemented()` entries are `("SEM123", "rule")` on one line.
    enforced = read_names(CONFORMANCE, "pub fn implemented()", "pub const NOT_APPLICABLE", '"SEM')
    na = read_not_applicable(CONFORMANCE)
    conflict = read_conflict(CONFORMANCE)
    outstanding = [
        n for n in rules if n not in enforced and n not in na and n not in conflict
    ]

    errors = []
    for name in outstanding:
        if name not in BLOCKED:
            errors.append(f"{name} is outstanding and unclassified")
    for name, blocker in BLOCKED.items():
        if name not in rules:
            errors.append(f"{name} is not a rule the specification states")
        if name in enforced:
            errors.append(f"{name} is enforced, not blocked")
        if name in na:
            errors.append(f"{name} is not applicable, not blocked")
        if name in conflict:
            errors.append(f"{name} is in conflict, not blocked")
        if blocker not in metas and blocker not in DERIVED and blocker not in LAYER3:
            errors.append(f"{name} is blocked by `{blocker}`, which is not a Layer 1-3 name")
    if errors:
        for e in errors:
            print(e)
        sys.exit(1)

    rows = [f'    ("{name}", "{BLOCKED[name]}"),' for name in sorted(BLOCKED)]
    text = "\n".join(
        [
            "// Generated by tools/blocked_by.py: every rule the kernel does not enforce and does",
            "// not record as not applicable or in conflict, with the one thing it is waiting on.",
            "// Each name is a metaclass (Layer 1), a derived property (Layer 2), or an element the",
            "// notation implies and the lowering does not build (Layer 3).",
            f"// {len(rows)} blocked rules.",
            "&[",
        ]
        + rows
        + ["]", ""]
    )

    if check:
        current = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        if current != text:
            sys.exit(f"{OUT} is stale; run `python tools/blocked_by.py`")
        print(f"up to date: {len(rows)} blocked rules")
        return

    open(OUT, "w", encoding="utf-8", newline="\n").write(text)
    print(
        f"wrote {OUT}: {len(rows)} blocked, {len(enforced)} enforced, "
        f"{len(na)} not applicable, {len(conflict)} in conflict, {len(rules)} total"
    )


if __name__ == "__main__":
    main()
