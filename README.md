# SysML v2 tools for models you can inspect

Edit, validate and visualize SysML v2 models with a native Rust engine. Use the command line,
open an interactive diagram in the browser, or work beside your source in VS Code. A provenance
ledger records the sources, derivations and assumptions behind model values.

**Preview software.** The project implements a measured subset of SysML v2; it does not claim
complete language or graphical-notation conformance. Packaged builds need no separate runtime.

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

The ledger answers a separate question from language validation: **where did each number come
from?** A model states `450 [km]`; its ledger records a source clause, named inputs in a derivation,
or an approved assumption. A model without a ledger can still be checked: the tool reports that
provenance tracking has not started rather than marking every value as a language error.

## Install

### Try it in VS Code — Windows x64

Download `sysml-v2-win32-x64.vsix` from [GitHub Releases](https://github.com/JasonJin2006/sysml/releases).
In VS Code, choose **Extensions → … → Install from VSIX…**, then open a `.sysml` file and run
**SysML: Open Diagram**. The package includes the native engine and standard library; no Rust,
Java or Python installation is needed.

```bash
code --install-extension sysml-v2-win32-x64.vsix
```

This preview is distributed as a VSIX, not a confirmed Marketplace listing. Updates are manual.
See the [extension guide](core/editors/vscode/README.md) for commands, settings and limitations.

### Build the CLI or another platform

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
sysml format  model.sysml                 re-indent the source, comments and all
sysml format  model.sysml --check         report without writing (exit 4 when a file needs it)
sysml resolve model.sysml                 every name, and what it binds to
sysml check   model.sysml --json          syntax, names, values, constraints, ledger
sysml next    model.sysml                 which stage the model is in, and the one next command
sysml render  model.sysml --view gv --out picture.svg
sysml views   model.sysml                 the views the model declares, and how to draw each
sysml render  model.sysml --expose P::myView --out picture.svg
sysml studio  model.sysml                 an editable canvas, in a browser
```

`sysml` with no arguments prints all 29 commands. Exit codes are the contract:

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

**In VS Code** — the [extension](core/editors/vscode/README.md) provides diagnostics, completion,
rename, formatting, navigation, hover and Outline. **SysML: Open Diagram** opens a diagram and
inspector beside the source. **Visualize Element** (`Ctrl+Alt+E`) focuses on a declaration;
**Choose Diagram** switches projections or opens a model-declared view. Diagram options expose
layout, descendant depth, SVG export and position reset. Searchable element tables and a
read-only requirement-satisfaction matrix can export CSV.

One model can have several projections; choosing a state diagram does not turn the model into a
state-only model. A declared `view` instead selects content with `expose` clauses and identifies
its rendering. Current descendant folding is a depth limit with a summary row, not independent
expand/collapse state on every node. Save source edits before refreshing the diagram.

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
| Tests (2026-11-15) | 1753 Rust, 166 Studio UI and 22 extension tests pass |
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

The renderer includes actor figures, initial/final nodes, decisions, synchronization bars and
nested state/action frames. Large models can still produce crossing connectors or crowded
labels. The view-language subset, diagram-wide folding and read-only tables are current limits,
not full parity with commercial tools. [`core/KNOWN-GAPS.md`](core/KNOWN-GAPS.md) records further
coverage gaps; [`core/examples/showcase/`](core/examples/showcase/) contains diagram examples.

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
