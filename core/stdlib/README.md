# `sysml.library`

This directory contains the SysML v2 and KerML standard library as published by the OMG, taken
from the per-library `.kpar` archives of the 2025-02 release:

| Library | Archive |
|---|---|
| Kernel Semantic | `https://www.omg.org/spec/KerML/20250201/Semantic-Library.kpar` |
| Kernel Data Type | `https://www.omg.org/spec/KerML/20250201/Data-Type-Library.kpar` |
| Kernel Function | `https://www.omg.org/spec/KerML/20250201/Function-Library.kpar` |
| Systems | `https://www.omg.org/spec/SysML/20250201/Systems-Library.kpar` |
| Analysis | `https://www.omg.org/spec/SysML/20250201/Analysis-Domain-Library.kpar` |
| Cause and Effect | `https://www.omg.org/spec/SysML/20250201/Cause-and-Effect-Domain-Library.kpar` |
| Geometry | `https://www.omg.org/spec/SysML/20250201/Geometry-Domain-Library.kpar` |
| Metadata | `https://www.omg.org/spec/SysML/20250201/Metadata-Domain-Library.kpar` |
| Quantities and Units | `https://www.omg.org/spec/SysML/20250201/Quantities-and-Units-Domain-Library.kpar` |
| Requirement Derivation | `https://www.omg.org/spec/SysML/20250201/Requirement-Derivation-Domain-Library.kpar` |

Only the `.sysml` and `.kerml` sources are kept; the archives' `.project.json`, `.meta.json` and
`.DS_Store` members are dropped. The directory layout is this project's, not the archives'.

The library is licensed under GNU Lesser General Public License (LGPL) v3.0, see file `LICENSE`.
`MANIFEST.sha256` pins every source file; `VENDORED.md` records where it came from and what changed
when it was last replaced.
