# SysML v2 Standard Library (Vendored)

> Chinese translation: [docs/zh/core/stdlib/VENDORED.md](../../docs/zh/core/stdlib/VENDORED.md)

This library was extracted from OMG's official per-library `.kpar` packages. It is unchanged, so that a single `git clone` gives a complete standard library, instead of quietly falling back to the 68-line stub at `crates/sysml-syntax/stdlib.sysml`.

## Source and License

- Upstream: the ten standard libraries in OMG's 2025-02 release, as `.kpar` files. The URL for each one is in `README.md` in the same directory.
- License: GNU Lesser General Public License v3.0. The full text is in `LICENSE` in the same directory.
- Contents: 94 source files (58 `.sysml`, 36 `.kerml`), 28433 lines, including the three layers of the Kernel, 21 Systems Library files, and six domain libraries.
- Verification: `MANIFEST.sha256` records the sha256 of each of the 94 source files. `sha256sum -c MANIFEST.sha256` can recheck them.

Only the `.sysml` and `.kerml` source files are kept. `.project.json`, `.meta.json`, and `.DS_Store` are dropped. The directory layout is this project's own, not the one inside the kpar.

LGPL v3 permits redistribution as is. Here, no library file is modified. The files are only placed in the repository, and the kernel reads them at runtime without changing them. If the library contents ever need to change, the licensing impact must be evaluated separately. Do not edit the files in place.

## Replaced Once: The Version Changed, Not the Source

Before 2026-11-01, this library came from the `fixes` branch of `SysML-v2-Release`. That branch was maintained personally, and its copy path went through syside's wheel. Two things were done before switching to the official kpar, and both are recorded here.

**One: How far apart the two versions are.** Compared file by file: of the 94 files, 50 are identical and 44 differ, for 1011 lines in total. The differences are in content, not formatting. The direction is that **the official version is the earlier one**:

| Location | Official 2025-02 | Original `fixes` version |
|---|---|---|
| `Systems Library/SysML.sysml` | Metadata features carry no subsetting | Each one appends `subsets Metadata::metadataItems` (194 occurrences in the whole library) |
| `Systems Library/Parts.sysml` | `ref part :>> this : Part = that as Part` | `ref part this : Part :>> Action::this, ownedPerformances::this = ...` |
| `Domain Libraries/Geometry/ShapeItems.sysml` | 181 differing lines, due to revision progress | Same as left, with the direction reversed |

`metadataItems` is defined in the official version at `Systems Library/Metadata.sysml:21`. Only the metadata features do not subset it. So this is **a difference in revision progress**, not a mistake on either side.

**Two: Whether behavior changed after the switch.** The full gate was run with the official library placed under `SYSML_LIBRARY`. All five figures matched:

```
tests 1100   coverage 56/56   ledger 126/53/1/0   corpus 151/151   pilot 298/386 (spurious 45)
```

No current verdict depends on the 1011 lines of difference between the two versions. This does not mean the difference went unmeasured. Each item was compared one by one. After the switch, the same models, the same assertions, and the same rules produced figures that did not change at all.

**Why the switch was made anyway.** The original source was a personal branch that might not exist in five years. The official kpar URLs include the release version number, so they can be fetched again and checked. The cost is giving up the `metadataItems` subsetting that was added after 2025-02. That cost is recorded here: if a future rule needs it (for example, the subsetting relation of metadata members), first add that semantics, or move the library to a newer official version, rather than quietly returning to the `fixes` branch.

## Why It Is in the Repository

Previously, this directory existed only under `research/upstream/`, and `.gitignore` excluded that path. As a result, after a clean clone, `library::discover` could not find the library and fell back to the stub, while `check` still reported the model as clean. The difference between the two shows up only in `resolve` (the real library has over 600 lines of import bindings, while the stub has 4).

The lookup order in `discover` is now: the `SYSML_LIBRARY` environment variable, then `core/stdlib` inside the repository, then `sysml.library` or `library` next to the binary, then the copy in `research/upstream`. The copy inside the repository comes first, so under normal conditions that is the one used.

`Loaded::library_source` records which copy was actually read. When `check` reads the stub, it prints a one-line notice.

## How to Confirm Which Copy Is Used

```bash
cd core
./target/release/sysml.exe resolve examples/drone.sysml | grep MassValue
# SurveyDrone::Battery::mass : MassValue -> ISQBase::MassValue
```

A result pointing to `ISQBase::` shows that the full library was read. If the `check` output contains `note: standard library: the bundled stub, ...`, the stub was used instead, and the lookup order above needs checking.

## How to Update

When upstream publishes a new version, replace this directory as a whole, keep `LICENSE`, `README.md`, and `VENDORED.md`, and then:

```bash
cd core/stdlib && sha256sum -c MANIFEST.sha256   # first confirm the replacement is complete
cd .. && ./tools/gate.sh                          # a library change moves the pilot and corpus pass counts
```

After the replacement, regenerate `MANIFEST.sha256`. Otherwise, the next verification will report errors against the correct library:

```bash
cd core/stdlib
find . -type f \( -name "*.kerml" -o -name "*.sysml" \) | sed 's|^\./||' | sort \
  | while IFS= read -r f; do printf '%s  %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$f"; done \
  > MANIFEST.sha256
```

The gate's baseline check catches any drop in the pass count.
