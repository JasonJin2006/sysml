# pilot

The OMG SysML v2 Pilot Implementation's own validation test models, downloaded
from `Systems-Modeling/SysML-v2-Pilot-Implementation` (261 stars, Apache-2.0).

## What is here

`models/` holds 136 `.xt` files, each an Xpect test: 66 carry `_invalid` in the
name, 70 do not. The naming is not a reliable signal — some valid-named files
assert errors too — so the assertions below are counted directly rather than
inferred from file names. An `_invalid` file carries inline assertions of the
form

```
// XPECT errors --> "An occurrence, item or part must be typed by occurrence definitions." at "part p1: Real;"
```

That is the authoritative implementation saying *this construct is a violation*
and *this is the message*. 70 files carry 235 such assertions over 97 distinct
messages.

## Why it matters

This is the judging instrument the kernel was missing. Before it, deciding
whether a rule was decidable meant reading its OCL and guessing; the guesses
were wrong in at least one case that mattered — `validatePartUsagePartDefinition`
was written, produced 98 violations against conforming models, and was withdrawn
on the theory that the notation permits an untyped part. It does, but the rule is
not about untyped parts: `PartUsage_invalid.sysml.xt` marks `part p1: Real;` and
`part p4: AttDef;` as errors and leaves `part aPart;` alone in the same file. The
distinction took one grep to find once the file was here.

## How to use it

Extract the model from an `.xt` file (drop the `XPECT_SETUP` block and the
comments), run the kernel over it, and compare the count against the number of
`XPECT errors` assertions in the file. `crates/sysml-semantics/tests/rules_part_type.rs`
mirrors four of them.

`models/` is checked in because it is small and it is the reference. The
statements *in* it are the pilot implementation's, not this project's.

## Licence

Apache-2.0, from the upstream repository. `paths.txt` records where each file
came from.
