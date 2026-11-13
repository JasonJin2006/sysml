# Known Gaps in the Kernel

> Chinese translation: [docs/zh/core/KNOWN-GAPS.md](../docs/zh/core/KNOWN-GAPS.md)

Updated 2026-11-13. Ordered by impact on the product. Each item was verified by running it. "Location" names the place to change.

## Counting Basis

- Gate figures: 1545 tests pass (plus 45 UI tests); official corpus 151/151 clean; pilot assertions 382/387, spurious 0; semantic coverage 56/56; normative rules 136 enforced / 41 not applicable / 2 conflict / 1 blocked; reference rules 3 of 9 complete.
- The corpus covers only the syntax that the official examples use. Parsing gaps may include syntax the corpus does not cover.
- The pilot matches by line number and severity, and it does not compare message text. If the wording differs, the pilot score cannot show it.

## 1. Fixed in This Round

- **Valid models reported falsely**: a feature without a keyword, such as `out image : Payload;`, was treated as an attribute. When its type was an item, part, or port definition, it was reported as `SEM090`. It is now judged by its reference usage (`Element::is_bare`). The 31 false reports across four experimental models are gone, and the pilot is unchanged.
- **n-ary connections**: `connect (a, b, c)` and `connection c connect (a, b, c)` now parse. All three ends are parsed and checked (`Element::extra_ends`). Anonymous `connection connect (...)` also parses now.
- **`include use case i : T;`**: now parses as a use case usage typed by T, and is no longer treated as including a definition.
- **Outline display**: usages of concern, case, analysis, and interface show as usages, without "def" (`Kind::keyword_as`).
- **Type name resolution**: a redefined member is hidden from type-name lookup for that feature (`Index::resolve_typing`), so `sysml resolve` and type checking agree. Only type names changed. Other kinds of reference still use `resolve`.
- **A value from the wrong family**: `attribute mass : Real = wheels` (a part) and `part p : Wheel = 5` (a number) both passed silently. `SEM015` compared literals only, so a value written as a reference had nothing to compare against, and a literal against a non-scalar type had no opinion to state. `typing::family` now sorts a resolved type into three families — scalar (specializes `ScalarValues::ScalarValue`), structural (an occurrence definition, read from the compiled metaclass table), and unknown — and reports the mixed pairs. Two structural types are compared too, by `typing::conforms`, except in one direction (see under-reporting below). A type above either family, a project's own `attribute def`, and a checkout with no library loaded are all left undecided. Pilot 382/387, spurious 0, corpus 151/151 and coverage 56/56 are unchanged.

## 2. Decisions Needed

- **`examples/shadow.sysml` lacks `import ScalarValues`**, so `check` cannot resolve `Real`. The kernel is correct: every file in the official corpus that uses `Real` imports it explicitly. The example was left unchanged, because the parity table in the README was produced by comparing it with sysmlpy, and changing it would mean rerunning that comparison. sysmlpy is not installed, and installing it requires a download, so it needs your approval.

## 3. Under-Reporting: Checks Not Fully Done

### Normative Rules (3 of 180)

- **Blocked**: `validateInstantiationExpressionResult`. An expression is not an element in the tree, so nothing can own a result parameter. The expression would first have to be built as an element. That is a lot of work, and the result might still be recorded only as not applicable. The reference implementation's post-processing fills in result parameters. Which subclasses pass through it, however, depends on the EMF-generated dispatch code, and that code is not available locally.
- **Conflict (decision pending)**:
  - `validateAcceptActionUsageParameters`: counted literally from the specification, the 33 lines in the official corpus (8 files) and the 3 lines in the pilot's error-free model `StateUsage.sysml.xt` that use `accept` without `via` would all be errors. The kernel currently follows the reference implementation and does not report them.
  - `validateNamespaceDistinguishibility`: narrowing or upgrading it to match the specification conflicts with the pilot assertions in both directions. This conclusion was reached by reasoning from how pilot.py does its comparison. It has not been run in practice.

### Reference Rules (6 of 9 unfinished)

| Rule | What is missing |
|---|---|
| `validateUsageType_` | The type of `require`/`assume constraint c : T` is dropped when converted into the model tree |
| Connector featuring and association features | KerML `connector` declarations and succession endpoints are not checked for accessibility |
| Namespace distinguishability (inherited parts) | Feature chains written outside perform, exhibit, and event are not yet treated as independent namespaces |
| Parameter redefinition by position | Reported only through `SEM081`. The reference implementation's own message is not emitted, and features that construct results are not paired |
| Crossing feature specialization | The full syntax for owned crossings is missing, and so is the implicit inherited crossing chain beyond the explicit target |
| Binding connector type consistency | Unit operations, short-circuit operations, and other implicit bindings are not represented. Nothing is reported when the type is unknown |

### Other Known Under-Reporting

- An inline `featured by X` (`OwnedTypeFeaturing`) does not parse. Only the standalone form `featuring … of … by …;` is supported.
- **One structural direction is accepted on purpose.** `SEM015` now compares two structural types in both directions, but when the *declared* type specializes the value's type it stays silent: `out item assembledVehicle : AssembledVehicle = 'vehicle assy without engine' { part engine : Engine = …; }` with `AssembledVehicle :> VehicleAssembly` is in the official corpus and stated clean. The `= ref { … }` form adds the members that make up the difference, and the reference implementation asks no question about it. Enforcing it would take the coverage from 56/56 to 55/56. The value's type is read from `typing::type_of`, which does not follow redefinition or subsetting chains, so a value whose type arrives that way is still skipped rather than guessed.
- When a comment's or metadata's `about` target cannot be resolved, no error is reported.
- `OwnedAnnotation` inside a dependency body is not modeled.
- `SEM122` (instantiated type) checks only value expressions and return parameters, not constraints or guards.
- The n-ary connection checks and the outline recognize all three ends. However, the interconnection view (`iv`) draws only the first two ends: `connect (a, b, e)` is drawn as the link a to b, and e has no line. It is still to be decided how to draw it (a star node, or pairwise links).

## 4. Deliberate Differences from the Reference Implementation

- `SEM123`: `send x;` and `send x via p;` are reported as errors, because the specification requires a send to have 3 input parameters. The reference implementation does not check this count. The 6 sends in the official corpus all write `to`, and the pilot has no sends.

## 5. For the Product Layer: Conventions in the Tree

- Definitions and usages that share a Kind are displayed with `Kind::keyword_as(is_definition)`. `keyword()` is still the word for definitions, and the semantic rules use it in their error text.
- Bare features (`x : T;`, `in x : T;`) are still `AttributeUsage` in the tree, and `Element::is_bare` marks them. Type rules judge them by that flag. Value and unit rules still read them as attributes.
- `Element::extra_ends`: the ends of an n-ary connection from the third end onward. Connection ends are all projected as child ends, and the SEM096 count uses the projected ends. Binding connections also use this field.
- Type names use `Index::resolve_typing`, which hides redefined members. Other names still use `Index::resolve`.
- For accept and send elements, `to` holds the payload name, and `addressed` holds the end that was written last. For send, `via` and `to` are in `sender` and `receiver`, respectively. For accept, `via` is in `receiver`.
- `type_refs` is the list of types as written. Subject, objective, frame, perform, exhibit, entry/do/exit, and return fill only `type_ref`.
- When a subject binds only a value and gives no type (`subject vehicle = vehicle1;`), `type_ref` holds the bound reference. The `type_ref` of an ordinary parameter may be a subset target (`:>`), which is not necessarily a type.
- The kernel's Rust structures are still changing. Recently added are `Kind::TypeFeaturing`, `Kind::Comment`, and the `sender`, `receiver`, and `has_payload` fields of `Element`. The product layer should use the kernel through `--json` output where possible. `sysml-cli/tests/json_shape.rs` fixes the output shape.

## 6. Development Environment and Workflow

- On this machine, Rust programs cannot write to the system temp directory (access is denied), so before you run the tests and the gate, point the temp directory to a location inside the repository. Also, in this terminal, `cmd //c build.bat` cannot find `build.bat` in the current directory, so put `core` on `PATH`. From the `core` directory:

  ```bash
  export TEMP='C:\Users\JasonJin06\Desktop\sysml\core\target\tmp' TMP='C:\Users\JasonJin06\Desktop\sysml\core\target\tmp'
  export PATH="/c/Users/JasonJin06/Desktop/sysml/core:$PATH"
  ./tools/gate.sh
  ```

- The gate does not run `python tools/blocked_by.py --check`, so an out-of-date blocked table is not caught. Run it by hand after you change the ledger.
- `deep_nesting_is_refused_rather_than_overflowing_the_stack` overflows the stack in debug builds and passes in release builds. The gate uses release. Whether this is an old problem has not been verified.

## 7. The Diagrams

Functionally each view draws the right shapes; visually they are below the commercial renderers.
Measured against Syside's published renderings (`research/syside-diagrams/`, kept local):

- **No per-kind colour.** Every box is white with a black outline. Syside colours a definition, a
  usage, an enumeration and a requirement differently, and a reader scans a large diagram by
  colour before reading any label.
- **No compartment subdivision.** A definition's attributes, its parts and its ports are drawn as
  one flat list. Syside draws an `attributes` compartment, a `features` compartment and a
  separator rule, which is what makes a 200-line model readable.
- **No view frame.** Syside draws a `«view» Name` tab and an `expose ModelContext::**` footer. The
  renderer has neither, so a picture does not say which view it is.
- **No inherited-member marks.** Syside writes `^name` and `:>>` on a redefinition; the renderer
  shows the member with no marker at all.
- **Connector routing is axis-aligned only.** A route may still overlap another line; there is no
  crossing minimisation beyond the hop arcs.
- **Ports are drawn as squares on the border with the name outside.** Syside draws the port box on
  the border with a direction arrow inside it, so a port reads as part of the box.

The layout linter (`research/tools/layout_lint.py`) reports zero defects on all eight sample models
as of 2026-11-13: no overlapping label, no line through a label, no box escaping its parent. The
route reviewer (`research/tools/route_review.py`) is clean on the small ones and still finds grazes
and collinear overlaps on the large ones — 7 grazes and 74 overlaps on `spacecraft`, 3 and 21 on
`patrol-inspection`, 4 and 5 on `AutoPatrol`. Those counts went down by roughly a factor of ten
when the router learned to keep a shared container's lead-outs inside it, but they are not zero.
Both tools check correctness, not appearance.
