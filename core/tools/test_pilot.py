"""The scorer's own tests: how a `//* … */` block is anchored.

The scorer decides whether this kernel agrees with the reference implementation, so a bug here is
invisible in every other number: it makes the kernel look wrong where it is right, or right where it
is wrong. This file covers the one such bug found so far.

Run with `python tools/test_pilot.py`.

# The bug

Xpect writes a multi-assertion block **above** the declaration it describes:

    //* XPECT warnings ---
       "Duplicate of inherited member name 'self' from Action, Part" at "action a: ABlock;"
       --- */
    action a: ABlock;

The block occupied lines 40-44 and the declaration was line 45. `split()` anchored every assertion in
the block on line 40 — the block's first line — so the kernel's diagnostic on 45 could never match,
no matter how correct it was. Thirteen warnings in `ActionUsage_invalid`, `PartUsage_invalid` and
`FlowUsage_Invalid` were scored as misses for this reason alone. Anchoring on the first line after
the block moved the score from 344/387 to 345/387, files exact 70 to 71, and spurious 11 to 10.
"""

import io
import pathlib
import sys

sys.path.insert(0, "tools")
import pilot  # noqa: E402


def block_lines(body):
    """The line each block assertion is anchored on, for a model body written as `body`."""
    _model, expects = pilot.split(body)
    return [(msg.split()[0], line) for _sev, msg, _at, line, _c in expects]


def test_a_block_is_anchored_on_the_declaration_below_it():
    src = """//*
XPECT_SETUP something
*/
package P {
    //* XPECT warnings ---
       "Duplicate of inherited member name 'self' from Action, Part" at "action a: ABlock;"
       --- */
    action a: ABlock;
}
"""
    found = block_lines(src)
    assert found, "the block assertion was not read at all"
    # `action a: ABlock;` is line 6 of the file: the setup block is 1-3, `package P {` is 4, the
    # block comment is 5-7, and the declaration is 8.
    expected = 8
    assert found[0][1] == expected, (
        f"block anchored on line {found[0][1]}, but the declaration is on {expected}"
    )


def test_a_block_followed_by_a_blank_line_still_finds_the_declaration():
    # Some files separate the block from the declaration. The anchor must follow the text, not the
    # line immediately after `--- */`.
    src = """//*
XPECT_SETUP something
*/
package P {
    //* XPECT warnings ---
       "Duplicate of inherited member name 'self' from Action, Part" at "action a: ABlock;"
       --- */

    action a: ABlock;
}
"""
    found = block_lines(src)
    assert found, "the block assertion was not read at all"
    assert found[0][1] == 9, f"expected line 9, got {found[0][1]}"


def test_a_line_comment_is_not_moved():
    # The fix only applies to blocks. A `// XPECT errors --> …` line comment describes the line
    # below it too, but it has always been anchored on its own line, and every other assertion in
    # the suite depends on that staying true.
    src = """//*
XPECT_SETUP something
*/
package P {
    // XPECT errors --> "An action must be typed by action definitions." at "action a: ABlock;"
    action a: ABlock;
}
"""
    found = block_lines(src)
    assert found, "the line assertion was not read at all"
    assert found[0][1] == 5, f"a line comment must keep its own line, got {found[0][1]}"


def test_every_assertion_is_still_read():
    # Guard against the fix swallowing assertions: the suite has 387, and a change to `split` that
    # drops some would raise the match *rate* while lowering the count.
    root = pathlib.Path(pilot.PILOT)
    if not root.exists():
        print("  (pilot models not present; skipping the corpus count)")
        return
    total = 0
    for path in sorted(root.glob("*.xt")):
        _model, expects = pilot.split(path.read_text(encoding="utf-8", errors="replace"))
        total += len(expects)
    assert total == 387, f"expected 387 assertions across the suite, found {total}"


def test_attribution_never_changes_the_score():
    # `unmatchable` says *why* an assertion is missed. It must not say the assertion matched: the
    # temptation is to treat "we know the reference is wrong here" as a pass, which would raise the
    # fraction by redefining it. This checks that every attributed row is still `matched: False` and
    # that attribution never removes a row from the misses.
    root = pathlib.Path(pilot.PILOT)
    if not root.exists():
        print("  (pilot models not present; skipping)")
        return
    attributed = 0
    missed = 0
    for path in sorted(root.glob("*.xt")):
        rows, _spurious = pilot.score(path)
        if rows is None or rows is pilot.UNSCORABLE:
            continue
        for row in rows:
            if row["matched"]:
                assert row["unmatchable"] is None, (
                    f"{path.name}: a matched assertion was attributed "
                    f"({row['unmatchable']}); attribution must never claim a match"
                )
                continue
            missed += 1
            if row["unmatchable"]:
                attributed += 1
    # Two buckets. The `/src` dependency assertions match now that their files are vendored, and
    # `official-duplicate`, `reference-connector-rule` and `specified-once` are answered by the kernel.
    assert attributed == 5, (
        f"expected 5 attributed misses (4 no-library-setup + "
        f"1 reference-recovery-cascade), found {attributed}"
    )
    assert attributed <= missed, "an attribution must describe an unmatched assertion"
    if attributed == missed:
        assert missed == 5, "closing the real gaps must leave the established attributions visible"


def test_the_attribution_lists_are_exact():
    assert len(pilot.NO_LIBRARY_SETUP) == 2, "two models list no /library file and assert a library base"
    # The lists for assertions the kernel now answers are gone, not merely empty.
    for retired in ("OFFICIAL_DUPLICATE", "REFERENCE_CONNECTOR_RULE", "SPEC_REPORTED_ONCE"):
        assert not hasattr(pilot, retired), retired


def test_a_recovery_cascade_is_attributed_only_at_the_exact_fixture_anchor():
    stem, line, message, at = next(iter(pilot.REFERENCE_RECOVERY_CASCADE))
    assert pilot.unmatchable(stem, [line], message, at) == "reference-recovery-cascade"
    assert pilot.unmatchable(stem, [line - 2], message, at) is None
    assert pilot.unmatchable(stem, [line], "a genuinely extra brace", at) is None
    assert pilot.unmatchable("another_Import_Visibility_Invalid.sysml", [line], message, at) is None
    assert pilot.unmatchable(stem, [line], message, "import") is None


def test_a_recovery_attribution_keeps_its_row_unmatched():
    from unittest.mock import patch

    fixture = pilot.PILOT / (
        "org.omg.sysml.xpect.tests_src_org_omg_sysml_xpect_tests_validation_invalid_"
        "Import_Visibility_Invalid.sysml.xt"
    )
    with patch.object(pilot, "check", return_value=[("error", 24, "import requires visibility")]):
        rows, spurious = pilot.score(fixture)
    assert len(rows) == 2
    assert rows[0]["matched"]
    assert not rows[1]["matched"]
    assert rows[1]["lines"] == [26]
    assert rows[1]["unmatchable"] == "reference-recovery-cascade"
    assert not spurious


if __name__ == "__main__":
    failures = 0
    for name, fn in sorted(globals().items()):
        if not name.startswith("test_") or not callable(fn):
            continue
        try:
            fn()
            print(f"ok   {name}")
        except AssertionError as e:
            failures += 1
            print(f"FAIL {name}: {e}")
    raise SystemExit(1 if failures else 0)
