#!/bin/sh
# One gate, five checks, one exit code. Every change runs through here.
#
# The five are the ones a change can break independently: the parser corpus, the
# semantic coverage of the official validation set, the rule ledger, the
# not-modelled count, and the tests. Running them by hand took four minutes and
# was skipped exactly once — which is when a commit went in with
# `test result: FAILED` on a guard test.
set -e
cd "$(dirname "$0")/.."

CORPUS="${CORPUS:-$(cd .. && pwd -W)/research/upstream}"
BASELINE_FILE="gate/tests.baseline"

echo "== format"
# The default rustfmt style, with no rustfmt.toml. A file that differs fails here instead of being
# reformatted, so formatting changes stay in commits of their own (see .git-blame-ignore-revs).
if ! cmd //c build.bat fmt --all -- --check > /tmp/gate-fmt.log 2>&1; then
    tail -20 /tmp/gate-fmt.log
    echo "GATE FAIL: rustfmt (run: build.bat fmt --all)"; exit 1
fi
echo "   clean"

echo "== lints"
# Clippy over the library and binary targets. Warnings may fall but not rise above the baseline in
# gate/clippy.baseline; the baseline is lowered as warnings are fixed (tools/clippy_count.py).
if ! python tools/clippy_count.py > /tmp/gate-clippy.log 2>&1; then
    tail -20 /tmp/gate-clippy.log
    echo "GATE FAIL: clippy"; exit 1
fi
echo "   $(tail -1 /tmp/gate-clippy.log)"

echo "== tests"
# nextest runs the 112 test binaries in parallel; cargo runs them one after another, and that
# queue was 54 of the gate's 90 seconds — each binary pays its own start-up and library load, and
# the slowest single suite is 2.6s.
#
# Two things the switch must not lose. A missing nextest falls back to cargo rather than skipping:
# a check that goes quiet because a tool is absent is worse than no check. And nextest does not run
# doctests, so those run on their own — nine targets, every one holding zero tests today, which is
# exactly why the line has to exist before someone writes the first one.
#
# The count is the same either way. It used to read 1020 because two tests carried a duplicated
# `#[test]` attribute and cargo registered each twice; nextest keys by name and ran 1018. The
# attributes are gone, so both tools now report 1018.
if cmd //c build.bat nextest --version >/dev/null 2>&1; then
    if ! cmd //c build.bat nextest run --release > /tmp/gate-test.log 2>&1; then
        grep -E "^ *FAIL|^error" -A 3 /tmp/gate-test.log | head -30
        echo "GATE FAIL: tests"; exit 1
    fi
    # `build.bat nextest` can exit 0 with a failed test in the log, exactly as `build.bat test`
    # does — the count check below caught one such run, but reported it as "test count dropped"
    # when the log named the two failing tests. Read the verdict from the log as well.
    if grep -qE "^ *FAIL|^error: test run failed" /tmp/gate-test.log; then
        grep -E "^ *FAIL|^ *error: test run failed" -A 3 /tmp/gate-test.log | head -30
        echo "GATE FAIL: tests"; exit 1
    fi
    PASSED=$(sed -n 's/.*tests run: \([0-9]*\) passed.*/\1/p' /tmp/gate-test.log | tail -1)
else
    # build.bat can exit 0 while a test binary failed, so the log is what gets read:
    # a FAILED line, a panic, or fewer passing tests than the baseline all mean the
    # same thing. The count check is what caught a guard test failing while an
    # earlier version of this script reported OK.
    if ! cmd //c build.bat test --release > /tmp/gate-test.log 2>&1; then
        grep -E "^test result: FAILED|panicked at|^error" -A 3 /tmp/gate-test.log | head -30
        echo "GATE FAIL: tests"; exit 1
    fi
    if grep -qE "^test result: FAILED|panicked at|^error" /tmp/gate-test.log; then
        grep -E "^test result: FAILED|panicked at" -A 3 /tmp/gate-test.log | head -30
        echo "GATE FAIL: tests"; exit 1
    fi
    PASSED=$(grep -oE "test result: ok\. [0-9]+ passed" /tmp/gate-test.log \
        | awk '{p+=$4} END {printf "%d", p+0}')
fi
if cmd //c build.bat test --release --doc > /tmp/gate-doc.log 2>&1; then :; else
    grep -E "^test result: FAILED|panicked at|^error" -A 3 /tmp/gate-doc.log | head -20
    echo "GATE FAIL: doctests"; exit 1
fi
BASELINE=$(cat "$BASELINE_FILE" 2>/dev/null || echo 0)
if [ -z "$PASSED" ]; then
    tail -20 /tmp/gate-test.log
    echo "GATE FAIL: tests produced no count"
    exit 1
fi
if [ "$PASSED" -lt "$BASELINE" ]; then
    echo "   $PASSED passed, baseline $BASELINE"
    echo "GATE FAIL: test count dropped"; exit 1
fi
echo "   $PASSED passed"

echo "== ui"
# The front end has logic that can be wrong on its own: the alignment arithmetic and the
# comparison that decides what to mark as changed. Both are pure and tested with `node --test`.
# A missing node is a failure, not a skip: a check that silently does not run is worse than no
# check, and this gate already requires cargo, python and the MSVC toolchain.
UI_OUT=$(cd crates/sysml-studio/ui && npm test 2>&1) || true
UI=$(echo "$UI_OUT" | sed -n 's/^ℹ pass \([0-9]*\)$/\1/p')
if [ -z "$UI" ]; then
    echo "$UI_OUT" | tail -20
    echo "GATE FAIL: ui tests produced no result"
    exit 1
fi
UI_FAIL=$(echo "$UI_OUT" | sed -n 's/^ℹ fail \([0-9]*\)$/\1/p')
if [ "${UI_FAIL:-0}" != "0" ]; then
    echo "$UI_OUT" | grep -E "^✖|not ok" | head -10
    echo "GATE FAIL: ui tests"
    exit 1
fi
echo "   $UI passed"

echo "== coverage"
COV=$(python tools/coverage.py 2>&1 | head -1)
echo "   $COV"
case "$COV" in 56/56*) ;; *) echo "GATE FAIL: coverage"; exit 1;; esac

echo "== ledger"
# Four lines: the 180's enforced/not-applicable counts, the percentage, the Xtext validator's own
# rules, and the metamodel property checks. This read three until the reference-rule line was added
# above the property line, which pushed the property count to line four and made the grep below
# unreachable — the check could not pass, and only stopped being invisible when coverage started
# passing.
LEDGER=$(./target/release/sysml.exe conformance | head -4)
echo "$LEDGER" | sed 's/^/   /'
# The three counts are three different kinds of check: the specification's 180 constraints, the
# Xtext validator's own grammar rules, and the declared type of a metamodel property. The last line
# is asserted because a property check that stops running looks exactly like a clean model.
echo "$LEDGER" | grep -q "^metamodel properties    5   checked 5$"     || { echo "GATE FAIL: property checks"; exit 1; }

echo "== corpus"
COR=$(cmd //c build.bat run --release -p sysml-parser --example corpus -- "$CORPUS" 2>/dev/null | tail -1)
echo "   $COR"
case "$COR" in "files 151  clean 151  with-errors 0  hard-fail 0  errors 0") ;;
    *) echo "GATE FAIL: corpus"; exit 1;; esac

echo "== pilot"
# The scorer has its own tests because a bug in it is invisible in every other number: it decides
# whether this kernel agrees with the reference, so anchoring an assertion on the wrong line makes
# the kernel look wrong where it is right. Run them before the score they protect.
python tools/test_pilot.py || { echo "GATE FAIL: the pilot scorer's own tests"; exit 1; }
# The official implementation's own verdicts. The count is a floor, not a target: a change that
# lowers it is a regression the other checks cannot see.
# One run, three fields. This used to invoke pilot.py once per field, and every invocation
# re-checks all 136 models: three copies of the same 85 seconds. Capture the text once and parse
# it three ways. The empty-output guard matters on its own: a pilot that dies printed nothing,
# every `[ -n ... ]` test below then skipped its floor, and the gate passed on a tool that
# never ran.
PILOT_OUT=$(python tools/pilot.py 2>&1) || true
PILOT=$(echo "$PILOT_OUT" | head -1)
echo "   $PILOT"
case "$PILOT" in
    "pilot assertions "*) ;;
    *) echo "$PILOT_OUT" | tail -20; echo "GATE FAIL: pilot produced no summary line"; exit 1;;
esac
PILOT_GOT=$(echo "$PILOT" | awk '{print $3}' | cut -d/ -f1)
PILOT_FLOOR=$(cat gate/pilot.floor 2>/dev/null || echo 0)
if [ -n "$PILOT_GOT" ] && [ "$PILOT_GOT" -lt "$PILOT_FLOOR" ]; then
    echo "GATE FAIL: pilot assertions dropped below $PILOT_FLOOR"; exit 1
fi
# The false-positive direction, on the models the pilot states are clean. Those files carry
# `noErrors` and no error assertion, so the matched/unmatched score above cannot see them: a model
# we reject and the pilot accepts used to cost nothing. The floor is how many of them we accept.
CLEAN=$(echo "$PILOT_OUT" | sed -n 's/^  models the pilot calls clean: \([0-9]*\)\/.*/\1/p')
CLEAN_FLOOR=$(cat gate/clean.floor 2>/dev/null || echo 0)
if [ -n "$CLEAN" ] && [ "$CLEAN" -lt "$CLEAN_FLOOR" ]; then
    echo "GATE FAIL: clean models accepted dropped below $CLEAN_FLOOR"; exit 1
fi
# The false-positive count on the whole suite: a change that reports more on models the pilot
# leaves alone is a regression even when the matched count rises.
SPUR=$(echo "$PILOT_OUT" | awk '/^pilot assertions/ {print $8}')
SPUR_CEIL=$(cat gate/spurious.ceiling 2>/dev/null || echo 99999)
if [ -n "$SPUR" ] && [ "$SPUR" -gt "$SPUR_CEIL" ]; then
    echo "GATE FAIL: spurious diagnostics rose above $SPUR_CEIL"; exit 1
fi

echo "== bench"
# The scale line: a thousand elements through parse, check and render, and through the two requests
# the canvas makes (its document, and the fingerprint its poll asks for), against the recorded
# ceiling. It is here because nothing else in the gate grows with the model — every other check runs
# on models of a hundred-odd elements, so a change that makes the kernel twice as slow per element
# would pass them all.
#
# Only the thousand. The ten-thousand run is on demand (`--sizes 1000,10000`) and takes six seconds
# in render alone, because the arc routing in `jumps::hops` compares every horizontal run with every
# vertical one. That is measured and recorded, not yet fixed.
#
# Best of five, not best of two. This step runs after five minutes of the gate's own CPU work, so a
# run that catches the machine still busy reports every stage at three to four times its baseline —
# measured: one gate run showed `outline 350ms` against a baseline of 89, and the same binary alone
# measured `check 483ms` against 555. The reading is a minimum over the repeats, so more repeats is
# the variance fix that does not touch the ceiling, which stays at twice the baseline.
BENCH_OUT=$(PYTHONUTF8=1 python tools/bench.py --sizes 1000 --repeat 5 --check --quiet 2>&1) || {
    echo "$BENCH_OUT" | tail -10
    echo "GATE FAIL: bench"; exit 1
}
echo "   $BENCH_OUT"

echo "== not-modelled"
# The whole run is captured, not just line 3. When the binary was missing — a concurrent build
# deletes target/release/sysml.exe mid-gate — conformance.py printed a one-line `no binary at …`
# on line 1, line 3 was empty, and the case below reported "GATE FAIL: not-modelled". That reads
# as "the model has unmodelled constructs" and sends the reader into the semantics crates for a
# failure that was a missing executable.
NM_OUT=$(python tools/conformance.py 2>&1)
NM=$(echo "$NM_OUT" | sed -n '3p')
echo "   $NM"
case "$NM" in
    "not-modelled total 0"*) ;;
    "not-modelled total "*) echo "GATE FAIL: not-modelled"; exit 1;;
    *) echo "$NM_OUT" | tail -20; echo "GATE FAIL: conformance.py produced no summary (see above)"; exit 1;;
esac

echo "OK"
