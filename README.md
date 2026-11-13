# sysml

A SysML v2 kernel and toolchain in Rust: its own parser and name resolver, the specification's
validation rules, a ledger that records where every number came from, and SVG views with an
editable canvas.

One executable, no runtime to install. The standard library is compiled into the binary.

```console
$ sysml check drone.sysml
no errors; its ledger has not been started.
10 value(s) and 3 part definition(s) have nothing on record.

$ sysml next drone.sysml
stage     4/6 values
waiting   6 values have no ledger entry (the first is SurveyDrone::Airframe::mass)
next      sysml decide drone.sysml --element "SurveyDrone::Airframe::mass" \
              --value "..." --origin spec|derived|assumed|asked --why "..."
```

Most SysML tools answer "is this model valid". This one also answers **"where did each number come
from", and refuses to pass a model whose numbers have no answer.** A model states `450 [km]`; the
ledger beside it states the clause it was read from, the inputs it was computed from, or the
person who assumed it and what would prove them wrong. That is the part a language model cannot
do on its own, and the part this tool is built around.

## Install

Requires Rust 1.85 or newer (`rustup` is the usual way).

```bash
git clone https://github.com/JasonJin2006/sysml.git
cd sysml
cargo build --release --manifest-path core/Cargo.toml
```

The binary is `core/target/release/sysml`. Put it on your `PATH`, or call it by path.

On Windows, build with the bundled script instead — it loads the MSVC environment first, because
the `link.exe` that ships with Git Bash shadows the MSVC linker and cargo fails at link time:

```bat
cmd //c core\build.bat build --release
```

## Use it

```
sysml outline model.sysml                 the element tree
sysml resolve model.sysml                 every name, and what it binds to
sysml check   model.sysml --json          syntax, names, values, constraints, ledger
sysml next    model.sysml                 which stage the model is in, and the one next command
sysml render  model.sysml --view gv --out picture.svg
sysml studio  model.sysml                 an editable canvas, in a browser
```

`sysml` with no arguments prints all 26 commands. Exit codes are the contract:

| code | meaning |
|---|---|
| 0 | the model passes, or the command succeeded |
| 1 | the model has a problem |
| 2 | the call itself was wrong |
| 3 | the model is fine, its ledger is not started |

Views: `gv` general, `iv` interconnection, `tv` traceability, `pv` packages, `stv` state
transition, `afv` action flow, `sv` sequence. `--style tree` renders the general view as an
indented tree instead of nested boxes. `sysml table --matrix` writes a requirement-by-satisfier
matrix as CSV.

Three examples under [`core/examples/`](core/examples/) are small enough to read in one sitting:
`drone.sysml`, `fuel.sysml`, `rover.sysml`. A fourth, `edrx-threshold.sysml`, carries a complete
ledger: two requirements citing clauses of TS 23.501, both constraints evaluating, every value
accounted for. Run `sysml check core/examples/edrx-threshold.sysml` and it prints nothing, which
is the point.

## Editing

Two ways, and both use the same kernel.

**The studio** — `sysml studio model.sysml` opens a browser with four panes: the outline, the
diagram, a Monaco editor on the source, and the inspector. The diagram and the editor are two views
of one model, so an edit is checked by the same loader the command line uses.

**Any editor, through the language server** — `sysml-lsp` speaks LSP over stdio, so diagnostics,
go to definition, find references and hover work wherever a client does. It is the eleventh crate
and it is thin: `sysml-resolve` answers what a name binds to, `sysml-semantics` answers what is
wrong, and the server translates positions.

A VS Code extension is in [`core/editors/vscode/`](core/editors/vscode/): about sixty lines that
start the server and nothing else. Build `sysml-lsp` once and it finds it.

## The ledger

Four origins, and only one of them may be written without evidence:

| origin | needs | means |
|---|---|---|
| `spec` | `--cite <source>` and `--why` | read out of a document |
| `derived` | `--from <element>...` | computed from named inputs |
| `assumed` | `--falsifier` and `human:` approval | a judgement, with what would refute it |
| `asked` | `--question <id>` | a person was asked |

```bash
sysml source model.sysml --title "3GPP TS 23.501" --kind standard \
    --quote "…" --locator "clause 5.31.7.2.1" --jurisdiction INTL --accessed 2026-11-13 --by agent
sysml scope  model.sysml --market global --jurisdictions INTL
sysml decide model.sysml --element "P::x" --value "10 [m/s]" --origin spec \
    --why "one line of why" --cite S001 --by agent
```

An agent can propose an assumption; only a person accepts it. `sysml approve` takes
`--by human:<name>` and nothing else.

## Use it from an AI agent

[`AGENT-GUIDE.md`](AGENT-GUIDE.md) is written for a language model rather than a person: the
commands in the order a task needs them, the JSON shape of each, the exit-code contract, and the
ways a model written by an agent usually goes wrong.

A prompt that gets an agent started:

```
Clone https://github.com/JasonJin2006/sysml and read AGENT-GUIDE.md. Build the
binary (`cargo build --release --manifest-path core/Cargo.toml`, or
`cmd //c core\build.bat build --release` on Windows). Then use
core/target/release/sysml to check and account for the SysML v2 model I am about
to describe. Ask me before recording any decision that needs my approval.
```

## Where it stands

Verified by `core/tools/gate.sh` on every commit — formatting, lints, tests, UI tests, coverage,
the rule ledger, the parser corpus, the OMG pilot, a benchmark, and the unmodelled-construct count.

| Measure | Value |
|---|---|
| Tests | 1545 pass, 0 fail (plus 45 front-end tests) |
| Specification rules enforced | 136 of 180 (41 hold by construction, 2 conflict, 1 blocked) |
| Reference-implementation rules complete | 3 of 9 |
| OMG pilot assertions | 382/387, 0 false positives |
| Parser corpus | 151/151 files clean |
| Semantic coverage | 56/56 |

**What it does not do, measured rather than guessed.** Of the 359 normative clauses of
TS 23.501, two can be written as a constraint — the same 10.24 s threshold on the extended DRX
cycle. Clause *traceability* (which clause nothing covers) works for all of them;
clause *decidability* works for two. `core/examples/edrx-threshold.sysml` is the working end of
that line.

The diagrams are drawn to be read: a box is coloured by the kind of element it is, its members sit
in named compartments, and every picture carries its own `«view»` tab and `expose` footer. They are
still below the commercial renderers — no per-kind node shapes, and a large model's connector
routing still grazes and overlaps. [`core/KNOWN-GAPS.md`](core/KNOWN-GAPS.md) lists the gaps that
matter, with the measured numbers.

## Repository layout

```
core/        the Rust workspace (eleven crates), the editors and the validation tools
research/    the clause ledger, the OMG pilot models, probe scripts
docs/        vision, product plan and design notes
```

`core/README.md` documents the crates, the commands and the design invariants.
[`CONTRIBUTING.md`](CONTRIBUTING.md) is the working agreement: layout, language, Rust style,
commits, and how the gate is run.

## Licence

MIT. See [`LICENSE`](LICENSE).
