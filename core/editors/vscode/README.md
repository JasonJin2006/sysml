# SysML v2 for VS Code

Write SysML v2 models, check them as you type, and explore diagrams beside the source. The extension includes a native model engine and its standard library: installing a packaged VSIX does not require Rust, Java, Python, or a separate server.

**Preview release.** The downloadable package is currently built and tested for Windows x64. Other platforms require a native build. This is an independent implementation, not a certified replacement for every SysML v2 tool.

## Install and open your first diagram

1. Download `sysml-v2-win32-x64.vsix` from the [GitHub releases](https://github.com/JasonJin2006/sysml/releases).
2. In VS Code, open Extensions, choose **… → Install from VSIX…**, and select the download. Reload VS Code if prompted.
3. Open a folder containing `.sysml` or `.kerml` files. Open a model and run **SysML: Open Diagram** from the Command Palette.

Or install the downloaded file from a terminal:

```bash
code --install-extension sysml-v2-win32-x64.vsix
```

Updates distributed as VSIX files must be installed manually. A GitHub release is not a Marketplace listing and does not enable automatic extension updates.

## Work with the model

- **Check while editing.** Syntax, name-resolution, semantic and provenance diagnostics appear in VS Code's Problems panel. Unstarted provenance tracking is an informational notice, not an error on every value.
- **Navigate and edit.** Go to definition, find references, hover, Outline, completion, rename, formatting, code actions, semantic highlighting and inlay hints are provided by the language server. Results depend on the constructs the engine supports.
- **Inspect diagrams.** Switch among general, interconnection, package, state-transition, action-flow, sequence and requirement-traceability projections. Select a box to inspect it or navigate to its source; drag boxes to save manual positions.
- **Focus on one element.** Place the cursor inside a declaration and press `Ctrl+Alt+E`, or run **SysML: Visualize Element**. Choose nested or tree layout and a descendant depth. The diagram's **Diagram options** also lets you visualize the selected element, change depth, export SVG, reset positions or return to the whole model.
- **Read tables and matrices.** Open a searchable, sortable element table or requirement-satisfaction matrix. Jump from a row to its source or export CSV. These views are currently read-only.

### Commands

| Command | Use it to |
|---|---|
| `SysML: Open Diagram` | Open or reveal the diagram beside the source. |
| `SysML: Choose Diagram` | Pick a built-in projection or a view declared in the model. |
| `SysML: Visualize Element` | Draw the element at the cursor and its descendants. |
| `SysML: Change Diagram Depth` | Change how much of an element-scoped diagram is shown. |
| `SysML: Model Elements` | Search, sort and export model elements. |
| `SysML: Requirement Satisfaction Matrix` | Inspect requirement-to-satisfier relationships. |

### What does “view” mean?

A model is not tied to one diagram. A **projection** chooses what to show: structure, connections, states, actions, messages or traceability. Switching projection does not rewrite the model.

A **model-declared view** is different: its declaration identifies a rendering and its `expose` clauses select model elements. Choose it by name to use that declared selection. Current support is a subset of the SysML view language, not a full implementation of every `expose` and `filter` expression.

**Scope** chooses one element and its subtree; **depth** limits the descendants drawn below each root. Deeper elements are summarized by a collapsed-count row. This is a diagram-wide depth setting, not independent expand/collapse state for each box. Scoped diagrams and depth controls currently support general, state and action-flow projections; nested/tree layout selection applies to the general projection.

### Source and diagram synchronization

The language server checks editor buffers. The diagram service reads files on disk, so save before expecting text changes to appear in the diagram. Opening a diagram from a dirty model offers **Save and Open** or cancellation. Inspector edits write model files; save outstanding editor changes before using them.

The model service runs locally on a loopback port and stops when its panel closes. If it fails, reopen the diagram and inspect **Output → SysML** for details.

## Settings

| Setting | Default | Purpose |
|---|---|---|
| `sysml.server.path` | `""` | Override the bundled `sysml-lsp` executable. |
| `sysml.binary.path` | `""` | Override the bundled `sysml` executable used by diagrams. |
| `sysml.trace.server` | `off` | Trace language-server messages. |
| `sysml.diagram.defaultElementRendering` | `nested` | Initial layout offered by Visualize Element. |
| `sysml.diagram.defaultDepth` | `3` | Initial descendant depth, from 1 to 64. |

Executable lookup uses the explicit setting first, then the extension's `bin/<platform>-<arch>/`, then a local `core/target/release/` build beside the extension source.

## Known limitations

- Large diagrams can still have crossings or crowded labels; routing is not equivalent to a commercial layout engine.
- Descendant depth summarizes hidden content instead of providing independent folding for each node.
- Tables and matrices cannot edit model cells. Multiple simultaneous diagram tabs are not supported.
- Not every SysML v2 validation rule, expression or graphical notation is implemented. See [the repository](https://github.com/JasonJin2006/sysml) for measured coverage and examples.
- This release ships a Windows x64 native payload; it is not a universal or browser-only extension.

## Build and distribute

From the repository root, build the native binaries:

```bash
cargo build --release --manifest-path core/Cargo.toml
```

On Windows, use an absolute path to the build script so the MSVC linker is selected:

```bash
cmd //c "C:/path/to/sysml/core/build.bat" build --release
```

Install extension dependencies and package for the current platform:

```bash
npm --prefix core/editors/vscode ci
```

```bash
npm --prefix core/editors/vscode run package
```

The packer compiles the extension, copies both release binaries and the versioned standard library, and writes `core/editors/vscode/dist/sysml-v2-<platform>-<arch>.vsix`. Build on each target platform before claiming support for it. The Studio bundle is embedded in the native binary; after UI changes, build `core/crates/sysml-studio/ui` before rebuilding Rust.

### Marketplace publication

A publisher named in `package.json` is metadata, not proof of a registered Marketplace publisher. To enable search-based installation and automatic updates, the maintainer must create or verify that publisher, authenticate with the publishing service, and publish the platform-specific VSIX. Do not commit access tokens or paste them into an issue.

After authenticating with `vsce login JasonJin2006`, publish the already-tested package:

```bash
npx --yes @vscode/vsce publish --packagePath core/editors/vscode/dist/sysml-v2-win32-x64.vsix
```

Until publication is confirmed, use the GitHub release instructions above; do not advertise the extension as available in Marketplace.

## Feedback

Report reproducible problems in [GitHub Issues](https://github.com/JasonJin2006/sysml/issues). Include the extension version, operating system, a small model, the expected result and the relevant Output log. Remove private model content before posting.
