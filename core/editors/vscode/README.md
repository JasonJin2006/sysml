# SysML v2 for VS Code

A thin shell: it launches `sysml-lsp` and lets the language server do the work. There is no SysML
logic in this extension.

## What it gives you

- Diagnostics as you type: syntax, unresolved names, constraint verdicts, the ledger gate.
- Go to definition (`F12`), find references (`Shift+F12`), hover.
- Syntax highlighting and bracket matching for `.sysml` and `.kerml`.

## Install

The extension needs the `sysml-lsp` binary. Build it once:

```bash
cargo build --release --manifest-path core/Cargo.toml
```

The extension looks for it in three places, in this order:

1. `sysml.server.path` in your settings.
2. `sysml-lsp` on `PATH`.
3. `core/target/release/sysml-lsp` — the path a build of this repository produces, relative to the
   extension's own folder.

Then install the extension:

```bash
cd core/editors/vscode
npm install
npm run compile
code --install-extension .   # or press F5 to run it on this folder
```

## Settings

| setting | default | meaning |
|---|---|---|
| `sysml.server.path` | `""` | Where `sysml-lsp` is. Empty means find it on `PATH` or beside the extension. |
| `sysml.trace.server` | `off` | Log the messages exchanged with the server. |

## What it does not do

Nothing the server does not. Completion, rename and code actions are not implemented in
`sysml-lsp` yet, so the extension does not offer them; that is the point of the split — one place
to add a feature, and every editor gets it.
