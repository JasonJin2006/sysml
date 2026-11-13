# Contributing

This is the working agreement for code and documents in this repository. Where a tool can check a
rule, the gate checks it. The rest are review rules.

## Layout

- `core/`: the kernel. `crates/` is the Rust workspace, `stdlib/` the vendored standard library,
  `tools/` the gate and measurement scripts, `gate/` the recorded baselines, `examples/` the
  sample models.
- `research/`: the material the kernel is validated against. `pilot/` holds the OMG validation
  models, `clauses/` the clause ledger of TS 23.501, `tools/` the probe scripts and `experiments/`
  the model samples.
- `docs/`: product and design notes. Not needed to build or use the tool.

## Language

- Code, comments, identifiers, diagnostics and command-line output are written in English.
  English is the primitive text.
- Chinese appears only in translation tables (`i18n.ts` in the studio, and the server-side table).
- Recorded human input, such as the reasons in a ledger, is kept as it was written.

## Rust

- Format with rustfmt's default configuration; there is no `rustfmt.toml`. `build.bat fmt --all`
  formats the tree. The gate runs `fmt --all -- --check`.
- A formatting change is a commit of its own and is listed in `.git-blame-ignore-revs`.
- Clippy: the gate counts distinct warnings for the library and binary targets and compares the
  count with `gate/clippy.baseline`. Fix the correctness warnings. Lower the baseline when the
  count falls. Never raise it to let a change pass, except when a warning moves to a new line —
  the counter keys a warning by its line, so an unrelated edit above it can make the same warning
  count as new; say so in the commit when that happens.
- Naming follows RFC 430: types `UpperCamelCase`, functions and variables `snake_case`, constants
  `SCREAMING_SNAKE_CASE`.
- Public items of the library crates (`sysml-model`, `sysml-parser`, `sysml-syntax`,
  `sysml-resolve`, `sysml-semantics`, `sysml-view`) follow the Rust API Guidelines: `as_`, `to_`
  and `into_` naming, the common trait implementations, and error types that implement
  `std::error::Error`.
- Avoid glob imports outside test modules. `use super::*` is fine inside a test module.
- The target layout is that a crate root declares its modules and re-exports, and that the
  implementation lives in leaf modules. Several crate roots do not meet this yet. That refactor
  is done on its own, never mixed into a feature change.

## Commits and checks

- One concern per commit. Formatting, moves and behaviour changes are separate commits.
- The gate must pass before a commit: `cd core && ./tools/gate.sh`. It runs formatting, lints,
  tests, the front-end tests, coverage, the rule ledger, the corpus, the OMG pilot, a benchmark
  and the unmodelled-construct count.
- Read a tool's exit code rather than a piped summary. Do not pipe a build or test command into
  `grep` or `head` to decide whether it succeeded.
- On Windows, call `build.bat` by absolute path through `cmd`; a bare `cargo build` fails at link
  time because the `link.exe` from Git Bash shadows the MSVC one.
- Text files use LF. `.gitattributes` sets `* text=auto eol=lf`; keep `core.autocrlf` false.
- Commit messages are in English and say what changed and why.

## Documents

- Documents are written in English first.
- A document that describes the product must stand on its own: a reader with a clone and a built
  binary should be able to follow it without any file that is not in the repository.
