# Using sysml from an AI agent

This file is written for a language model that has been asked to work with SysML v2 models. Every
command, flag, JSON field and exit code below was taken from the binary, not from memory.

## What the tool is

`sysml` reads SysML v2 textual notation, checks it, records where every number came from, and
draws diagrams. It is one executable with the standard library compiled in, so it runs from any
directory.

```
sysml <command> <file.sysml> [options]
sysml <command> [--project path/to/sysml.project.json]
```

A directory is a *project* when `sysml.project.json` sits at or above the working directory; the
commands then take no path.

## Exit codes are the contract

Read the exit code before the text. It tells you which kind of problem you have.

| code | meaning | what to do next |
|---|---|---|
| 0 | passes, or the command succeeded | carry on |
| 1 | the model has a problem | read the diagnostics and fix the model |
| 2 | the call itself was wrong | fix the command line — never retry it unchanged |
| 3 | the model is fine, the ledger is not started | run `sysml next` |

A 2 means an unknown option, a missing value, or a bad path. Retrying it unchanged wastes a turn.

## The order a task takes

### 1. See what is in the model

```
sysml outline model.sysml --json
sysml resolve model.sysml --json
sysml graph   model.sysml --view gv --json
```

`outline` is the element tree. `resolve` lists every written name and what it binds to — use it
when a name is unresolved. `graph` is nodes and edges; `--view` is `gv` general, `iv`
interconnection, `tv` traceability, `pv` packages, `stv` state transition, `afv` action flow,
`sv` sequence.

### 2. Check it

```
sysml check model.sysml --json
```

The JSON carries `ok`, `status`, `issues[]`, `verdicts[]`, and counts. An issue has `code`,
`severity`, `at`, `line`, `message`. Codes are `SEM…` (semantic), `SYN…` (syntax), `PROV…`
(provenance).

A constraint whose verdict is `inconclusive` is well formed but reads a value nothing sets. That
is not an error — it is the tool telling you what to record next.

### 3. Give every number a source

`prov` refuses a model whose numbers have no ledger entry. Four origins, one of which alone needs
no document:

| origin | needs | means |
|---|---|---|
| `spec` | `--cite <source id>`, `--why` | read out of a document |
| `derived` | `--from <element>…` | computed from named inputs |
| `assumed` | `--falsifier`, and a person's approval | a judgement |
| `asked` | `--question <id>` | a person was asked |

Register the document, then cite it. `scope` is required once per model: a ledger that does not
say which jurisdictions were searched cannot be told apart from one that searched one.

```bash
sysml source model.sysml --title "3GPP TS 23.501" --kind standard \
    --quote "<the passage>" --locator "clause 5.31.7.2.1" --jurisdiction INTL \
    --accessed 2026-11-13 --by agent
sysml scope  model.sysml --market global --jurisdictions INTL
sysml decide model.sysml --element "P::x" --value "10 [m/s]" --origin spec \
    --why "one line of why" --cite S001 --by agent
```

An assumption needs its own approval, and **you cannot approve your own**:

```bash
sysml decide  model.sysml --element "P::x" --value "10 [m/s]" --origin assumed \
    --why "…" --falsifier "what observation would show it wrong" --by agent
sysml approve model.sysml --id D003 --by "human:<name>"     # ask the user to run this
```

If `--falsifier` is missing on an `assumed` decision, the command exits 2. Stop and ask the user
for one; do not invent a falsifier to get past the gate.

### 4. Ask what to do next

```
sysml next model.sysml --json
sysml next model.sysml --lang zh
```

Six stages in order: requirements, questions, allocation, values, interfaces, verification. The
answer names the earliest stage the model does not meet and the **one command** worth running.

### 5. Record verdicts

```
sysml judge model.sysml --by "human:<name>" --json
```

Records what each requirement's constraint evaluated to, against a fingerprint of the model. When
the model changes, `prov` reports the verdict as stale (`PROV020`) — re-run `judge` after editing.

### 6. Compare two versions

```
sysml diff before.sysml after.sysml --json
sysml diff before.sysml after.sysml --library deps.sysml --json
sysml impact model.sysml --element "P::Drone::mass" --json
```

`diff` compares by qualified name and reports `requirements reached`: which requirements stop
being true when the change lands. That list is the answer to "does anyone need to re-read this",
and it is usually the only part worth reporting. `--library` is needed when the project is split
across files.

`impact` walks backwards from one element through everything that names it.

### 7. Trace the clauses of a standard

```
sysml trace model.sysml --clauses clauses.jsonl --json
```

A requirement cites its clause in its `doc`:

```sysml
requirement def PtwAppliesAboveThreshold {
    doc /* TS 23.501 5.31.7.2.1: For NR, Paging Time Window applies for extended DRX
           lengths greater than 10.24s as defined in TS 38.304 [50]. */
    subject u : Ue = deployment;
    require constraint { u.cycle > 10.24 [s] implies u.pagingTimeWindowApplies }
}
```

Three reports: a cited clause the ledger lacks (a wrong claim — exit 1), a citation of a clause
that states no requirement, and the normative clauses nothing cites. The last is not a defect and
does not change the exit code.

### 8. Tabulate or draw

```
sysml table  model.sysml --matrix --out m.csv
sysml render model.sysml --view iv --style tree --out p.svg
sysml review model.sysml --out report.html
```

## Writing a model that can be checked

- A requirement needs a `subject` and a `require constraint`.
- **Bind the subject to the instance you mean**: `subject u : Ue = deployment;`. Without the `=
  name` binding the constraint reads the *type*, where attributes have no value, and every verdict
  is inconclusive.
- Give attributes values, or the constraint reading them cannot be decided.
- Write documentation as `doc /* … */`, not `doc "…"`.
- Import what you use: `private import ScalarValues::*;` and `private import ISQ::*;`.
- Put a unit on a number: `20 [min]`, not `20`.

## Ways agents get this wrong

Each of these was hit for real while building the examples.

| Symptom | Cause | Fix |
|---|---|---|
| every verdict `inconclusive` | the subject is not bound to an instance | `subject u : T = instance;` |
| `type unresolved: Real` (or any imported name) | `ScalarValues` / `ISQ` not imported | add `private import ScalarValues::*;` |
| exit 2, "unknown option `--views` for `check`" | the flag belongs to another command | `sysml` with no arguments lists each command's flags |
| `no part usages to draw` | an `iv` render of a model with only definitions | add `part x : TheDef;` |
| `PROV001` on every value | the ledger was never started | `sysml scope`, then `sysml decide` |
| `PROV020` | a verdict is stale | re-run `judge` after changing the model |
| a name resolves nowhere from another package | the declaration is `private` | drop `private`, or import from inside |

## Do not

- Do not read prose output in a script. Every command that matters has `--json`.
- Do not retry a non-zero exit unchanged. 2 is a bad command line, 1 a bad model, 3 a missing
  ledger — three different fixes.
- Do not treat an `inconclusive` verdict as a pass. Nothing was evaluated.
- Do not write a `--falsifier` you invented to get past `decide`. Ask the user; that is what the
  flag is for.
- Do not approve an assumption yourself. `--by` must be `human:<name>`, and that name is the
  person accepting the risk.

## A minimal session

```bash
sysml outline m.sysml --json                 # what is there
sysml check   m.sysml --json                 # is it well formed
sysml next    m.sysml                        # the one next thing
sysml scope   m.sysml --market global --jurisdictions INTL
sysml decide  m.sysml --element "P::x" --value "1" --origin spec --why "…" --cite S001 --by agent
sysml prov    m.sysml --json                 # is every number accounted for
sysml judge   m.sysml --by "human:<name>"    # record the verdicts
sysml diff    before.sysml after.sysml --json
```
