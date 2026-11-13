"""Score the kernel against the OMG pilot implementation's own verdicts.

`research/pilot/models/` is the authoritative implementation's validation test suite: each `.xt`
model carries inline `// XPECT errors --> "message" at "source text"` assertions, plus the grouped
form `//* XPECT warnings ---` … `--- */` with one assertion per line. Together they are 275 error
and 112 warning assertions across 90 scored files. This tool runs `sysml check` on each model and asks,
assertion by assertion, whether the kernel reports the asserted severity at the line the assertion
points at.

What "matched" means, and what it does not:

  * A match is a diagnostic at a line the assertion can be read as pointing at. Message text is not
    compared: the two implementations word findings differently, and comparing sentences would
    measure wording rather than verdict.
  * The model text keeps every line, with comment bodies blanked rather than removed, so a line
    number in the model is a line number in the file. Without that, the assertion's own position —
    which is how Xpect disambiguates a snippet that repeats — would be lost.
  * When the quoted snippet occurs on exactly one line, that line is the target. When it occurs on
    several, the nearest one at or after the assertion wins, and failing that the nearest one before
    it: Xpect's assertion applies to the text that follows it, and `at "A"` in a file that declares
    `A` four times can only mean the next one.
  * Severity is compared — an assertion that says `error` needs an error.
  * One diagnostic matches at most one assertion, so one finding cannot satisfy ten assertions.
  * Diagnostics with no assertion behind them are counted as `spurious`: the official suite says
    that model is clean there, so reporting there is the false-positive side.

Usage:
    python tools/pilot.py              # score every file, print the total and the worst files
    python tools/pilot.py -v FILE      # one file, assertion by assertion
    python tools/pilot.py --misses     # group unmatched assertions and spurious findings by message
    python tools/pilot.py --json
"""
import argparse
import concurrent.futures
import functools
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
PILOT = HERE.parent.parent / "research" / "pilot" / "models"
BIN = HERE.parent / "target" / "release" / "sysml.exe"

# Files an `XPECT_SETUP` block loads besides the standard library: five `/src/*.kerml` files from
# `org.omg.kerml.xpect.tests/src` in the upstream pilot repository, vendored into `models/src` with
# their git blob ids checked. They are attached to each model as libraries (`present_libraries`).
# A checkout without them would leave the names they define unresolvable, and a model whose
# dependency is absent is then left out of both directions rather than counted as a false positive.
DEPENDENCY = re.compile(r'File\s*\{\s*from\s*=\s*"([^"]+)"')

# Whether `XPECT_SETUP` loads the standard library. Three of the 136 models do not, and their
# assertions describe the reference implementation running without a library — see `unscorable`.
LIBRARY_LOADED = re.compile(r'File\s*\{\s*from\s*=\s*"/library')

# Two models whose `XPECT_SETUP` lists no `/library` file at all — measured, only three of the 136
# do, and the third asserts something unrelated that this kernel checks correctly. Their assertions
# describe the reference implementation running without `Base.kerml`; see `unmatchable`.
NO_LIBRARY_SETUP = frozenset(
    {
        "org.omg.kerml.xpect.tests_src_org_omg_kerml_xpect_tests_validation_Feature_invalid_noType.kerml",
        "org.omg.sysml.xpect.tests_src_org_omg_sysml_xpect_tests_validation_invalid_Feature_invalid_noType.sysml",
    }
)

# `score` returns this as its first element for a model whose `XPECT_SETUP` needs a file this
# checkout does not have. `None` already means "the emitted model does not parse", and conflating
# the two would report an environment gap as a parser failure.
UNSCORABLE = object()

# The connector findings and the second reference-subsetting anchor used to be attributed here
# (`reference-connector-rule`, `specified-once`). They are answered now: the kernel reports what the
# reference reports for an unidentified flow end, a redundant reference subsetting clause, and a
# parameter that redefines its general's parameter by position.


# One parser-recovery cascade, not a second independent syntax defect. In this exact fixture,
# line 24's unqualified import is rejected by the reference grammar and recovery closes the
# package early; line 26's brace then becomes extraneous to that recovered parse. In the source
# the brace closes the package normally. Keep the assertion unmatched and in the denominator.
REFERENCE_RECOVERY_CASCADE = frozenset(
    {
        (
            "org.omg.sysml.xpect.tests_src_org_omg_sysml_xpect_tests_validation_invalid_Import_Visibility_Invalid.sysml",
            26,
            "extraneous input '}' expecting EOF",
            "}",
        ),
    }
)


# The tokens an assertion's `at "..."` snippet is split into when deciding which diagnostic on a
# line it belongs to. Notation keywords are dropped so that `at "render rendering r2;"` is asked
# after `r2` (the name) rather than after `render` (the syntax every rendering line carries).
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NOTATION_KEYWORDS = frozenset(
    """part def action state attribute port item interface connection flow message perform
    exhibit ref in out inout end transition first then if do accept when after assert constraint
    require assume package import alias comment doc abstract readonly derived variable constant
    composite reference individual portion variation variant view rendering expose filter
    allocate bind connect to from of at new null true false and or not xor implies
    feature var struct class datatype type assoc subcalc calc expr subject actor stakeholder
    satisfy include connect terminate merge decide join fork send accept""".split()
)
# The snippet is delimited by `"`, and a snippet that is itself a quoted literal is written
# `at ""x""` — Xpect does not escape it. A non-greedy `"(.*?)"` stops at the inner quote, so the
# assertion lands on an empty string and can never match; that is what happened to
# `MultiplicityRange_invalid.kerml.xt`'s `at ""x""`. Taking everything up to the last quote on the
# assertion's own line fixes that, and it also recovers snippets that contain a `""` literal
# argument (`filter new A(null, 1, "", false);`), which the same defect truncated.
#
# The message is matched as one quoted run for the same reason: it may contain a `"`.
PAT = re.compile(r'XPECT\s+(errors?|warnings?)\s*-+>\s*"(.*?)"\s+at\s+"(.*)"\s*$', re.M)
# Xpect's other spelling: one severity for a block, one assertion per line, closed by `--- */`.
#
#     //* XPECT warnings ---
#        "Duplicate of inherited member name 'self' from Action, Part" at "action a: ABlock;"
#     --- */
#
# The arrow form above cannot see this one, and a missed assertion is worse than it looks: a
# diagnostic that *does* satisfy it has no assertion to consume it, so it is counted as
# `spurious`. The same model then scores both low and dirty because of a defect in this file.
# 94 of the suite's assertions (38 blocks) are written this way.
#
# The closing `---` is not always on the same line as `*/` — `Specialization_invalid.kerml.xt`
# ends one block with `---` and the `*/` on the next line — so both spellings are accepted.
#
# A third spelling puts the snippet on the header instead of quoting it in each entry:
#
#     //* XPECT errors at A::a1 ---
#         "Couldn't resolve reference to Type 'A::a1'." at "A::a1"
#     --- */
#
# `ShadowingTests_SameNamesBadCaseWithAlias_FT.kerml.xt:26` is the suite's one instance. The
# snippet there is unquoted and sits before the dashes, which is why neither pattern above sees it:
# the model's only assertion went unread, and the diagnostic that satisfied it was counted as
# spurious instead.
BLOCK = re.compile(
    r'XPECT\s+(errors?|warnings?)\s*(?:at\s+[^\n]*?)?\s*-+\s*\n(.*?)\n[ \t]*-+[ \t]*(?:\*/|\n[ \t]*\*/)',
    re.S,
)
BLOCK_ENTRY = re.compile(r'"([^"]*)"\s*at\s*"([^"]*)"')


def after_block(lines, end_line):
    """The body lines from the line after a `//* … */` block, inclusive.

    `end_line` is 1-based and counted in file lines; `lines` excludes the `XPECT_SETUP` header, whose
    length is the difference between the two numbering systems. The block's closing `--- */` occupies
    its own line, so the first line a diagnostic can be reported on is the one *after* it.
    """
    return lines[end_line:]


def target_line(remaining_lines, fallback):
    """The first line after a comment block that carries model text, as a file line number.

    A `//* … */` block sits directly above the declaration it describes, so that declaration — not
    the block — is the line a diagnostic is reported on. `fallback` is the line after the block's
    closing marker: if the block is followed by blank or comment-only lines they are skipped, and if
    nothing model-bearing follows, `fallback` is returned unchanged so a malformed file stays
    readable rather than having its assertions moved somewhere arbitrary.
    """
    line = fallback
    for text in remaining_lines:
        if text.strip():
            return line
        line += 1
    return fallback


def split(text: str):
    """Return (model text, [(severity, message, at, line, target lines)]).

    Line numbers in the returned model equal line numbers in the file after the `XPECT_SETUP`
    header, so an assertion's own position stays meaningful.
    """
    end = text.find("*/")
    offset = text[: end + 2].count("\n") if end >= 0 else 0
    body = text[end + 2:] if end >= 0 else text
    lines = body.split("\n")

    # Every assertion, with the line its comment starts on — counted in *file* lines, because that
    # is the coordinate system the emitted model uses and therefore the one the checker reports in.
    expects = []
    for index, line in enumerate(lines, start=1):
        for sev, msg, at in PAT.findall(line):
            expects.append((sev.rstrip("s"), " ".join(msg.split()), at, index + offset))
    # A block comment spanning lines: attribute to its first line, which is where the reader sees it.
    for match in PAT.finditer(body):
        start_line = body[: match.start()].count("\n") + 1 + offset
        sev, msg, at = match.group(1), " ".join(match.group(2).split()), match.group(3)
        # The snippet is kept exactly as written (`at "if \"test\""`), because it is also what
        # `-v` prints and what a reader compares against the assertion file. Unescaping belongs to
        # the locator, which does it on both sides — see `locate`.
        entry = (sev.rstrip("s"), msg, at, start_line)
        if entry not in expects:
            expects.append(entry)
    # The block form carries one severity for the whole group. Xpect writes the block *above* the
    # declaration it describes:
    #
    #     //* XPECT warnings ---
    #        "Duplicate of inherited member name 'self' from Action, Part" at "action a: ABlock;"
    #        --- */
    #     action a: ABlock;
    #
    # so the block is anchored on the first non-comment line after it, not on the line it starts on.
    # Anchoring it on the block's own first line made every assertion in a block a line-off miss:
    # `ActionUsage_invalid.sysml.xt` asserts on block line 40 while the declaration is on 45, and one
    # diagnostic cannot satisfy a set of lines it does not share. That accounted for most of the
    # unmatched `Duplicate of inherited member name` warnings — a defect in this scorer, not in the
    # kernel, since the kernel was reporting the right line all along.
    for match in BLOCK.finditer(body):
        start_line = body[: match.start()].count("\n") + 1 + offset
        end_line = body[: match.end()].count("\n") + 1 + offset
        sev = match.group(1).rstrip("s")
        ats = BLOCK_ENTRY.findall(match.group(2))
        if ats:
            line = target_line(after_block(lines, end_line - offset), end_line + 1)
        else:
            line = start_line
        for message, at in ats:
            expects.append((sev, " ".join(message.split()), at, line))

    # Blank the comment bodies but keep the lines, so numbering survives.
    blanked = []
    in_block = False
    for line in lines:
        out = []
        i = 0
        while i < len(line):
            if in_block:
                close = line.find("*/", i)
                if close < 0:
                    i = len(line)
                else:
                    in_block = False
                    i = close + 2
                out.append(" ")
                continue
            opened = line.find("/*", i)
            slashes = line.find("//", i)
            # `//*` opens a *block*: Xpect writes a multi-assertion body that way, and the lines
            # after it are comment text up to `*/`. Reading it as a line comment kept the body —
            # `"Duplicate …" at "state s1 : A;"` — as model text, which made `at` a member of the
            # namespace and produced a warning about nine members named `at`.
            if slashes >= 0 and line[slashes:].startswith("//*"):
                out.append(line[i:slashes])
                in_block = True
                i = slashes + 3
                continue
            if slashes >= 0 and (opened < 0 or slashes < opened):
                out.append(line[i:slashes])
                i = len(line)
                continue
            if opened < 0:
                out.append(line[i:])
                i = len(line)
                continue
            out.append(line[i:opened])
            in_block = True
            i = opened + 2
        blanked.append("".join(out).rstrip())

    # The header occupies `offset` lines and is not model text, but the checker reports line
    # numbers against what it is given: emitting `offset` blank lines first keeps its numbers
    # aligned with the file's, which is what the assertions are written against.
    model = "\n".join([""] * offset + blanked) + "\n"
    numbered = model.split("\n")

    compact = ["".join(line.split()) for line in numbered]

    def spanning(needle: str) -> list:
        """Lines where `needle` starts, allowing it to run across line breaks.

        Xpect points at a text range, and a range can cover several lines: the suite writes
        `interface opposite : Fuel connect x ::> … to y ::> …;` across three. Collapsing whitespace
        per line cannot see that, so the assertion had no target at all — it counted as missing
        while the diagnostic that satisfied it counted as spurious. The answer is the first line of
        the run, which is where a reader sees the construct begin.
        """
        if not needle:
            return []
        found = []
        for start in range(len(compact)):
            # A run starts on the line that carries the text. Starting on the blank line above it
            # answers that line, which is one before the construct — the assertion then points at
            # the comment that wrote it rather than at the model.
            if not compact[start]:
                continue
            joined = ""
            for end in range(start, min(start + 8, len(compact))):
                joined += compact[end]
                if len(joined) > len(needle) + 64:
                    break
                if needle in joined:
                    found.append(start + 1)
                    break
        return found

    placed = []
    for sev, msg, at, line in expects:
        # `at "if \"test\""` carries the assertion file's own escaping, so the snippet as written
        # is `if \"test\"` while the model says `if "test"`. Both sides are unescaped here, which
        # is the only way the needle can match: unescaping the snippet alone (the obvious fix, and
        # the one this comment used to say had failed) left the model side escaped and still found
        # nothing. Across all 136 models exactly one `XPECT` line contains a backslash — this one —
        # so the sweep cannot disturb any other assertion.
        needle = "".join(at.split()).replace("\\", "")
        found = [
            n
            for n, hay in enumerate(compact, start=1)
            if needle and needle in hay.replace("\\", "")
        ]
        if not found:
            found = spanning(needle)
        if not found:
            target = []
        elif len(found) == 1:
            target = found
        else:
            after = [n for n in found if n >= line]
            target = [after[0]] if after else [found[-1]]
        placed.append((sev, msg, at, line, target))
    return model, placed


@functools.lru_cache(maxsize=None)
def check(model: str, libraries: tuple = ()):
    """Diagnostics for a model, cached by its text.

    The same file is checked twice per run: once by `score` and again by `clean_model_errors`,
    which re-derives the identical model string for the 44 `noErrors` files. That was 44 extra
    processes out of 180 on every invocation, and the gate invokes this script for three separate
    fields. The list is read-only at every call site (`score` indexes it, `clean_model_errors`
    filters into a new one), so handing back the same object is safe.
    """
    with tempfile.NamedTemporaryFile("w", suffix=".sysml", delete=False, encoding="utf-8") as f:
        f.write(model)
        path = f.name
    try:
        # A checker that cannot be started is not a model that fails to parse. Before this
        # distinction the OSError propagated out of the worker, killed the run with a traceback,
        # and — worse, on the paths that caught it — turned one file into `unparsed` and moved the
        # score from 278/386 to 277/385 without a word. Rebuilding the binary under a running gate
        # produces exactly that. Raising here makes the failure say what happened instead.
        # The `/src` files the model's setup loads are attached as libraries, as `coverage.py`
        # attaches the other corpus files: their names resolve through the model's imports, and
        # their own diagnostics are not reported.
        cmd = [str(BIN), "check", path]
        for lib in libraries:
            cmd += ["--library", lib]
        try:
            run = subprocess.run(cmd + ["--json"], capture_output=True, text=True)
        except OSError as e:
            raise SystemExit(f"cannot run {BIN}: {e}") from e
        try:
            doc = json.loads(run.stdout.lstrip("\ufeff"))
        except json.JSONDecodeError:
            # A model that does not parse has no diagnostics to compare; the caller counts it
            # separately rather than scoring it as zero matches and hiding why.
            return None
        return [(i["severity"], i["line"], i.get("message", "")) for i in doc.get("issues", [])]
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


def clean_model_errors(path: pathlib.Path):
    """Errors reported on a model the pilot says is clean, or `None` when it says nothing.

    `// XPECT noErrors --> ""` is the author's statement that the whole file conforms. Those
    files carry no error assertion, so nothing in the matched/unmatched score sees them: a model
    we reject and the pilot accepts costs nothing on the board. That is the false-positive
    direction — the expensive one, because it is what makes a tool unusable on real models — so
    it gets its own count rather than riding along as `spurious`.

    A file carrying `noErrors` *and* error assertions is skipped: the two contradict each other,
    and guessing which the author meant would invent a verdict.
    """
    text = path.read_text(encoding="utf-8", errors="replace")
    if "noErrors" not in text:
        return None
    if unscorable(text):
        return None
    model, expects = split(text)
    if any(sev == "error" for sev, *_ in expects):
        return None
    issues = check(model, present_libraries(text))
    if issues is None:
        return None
    return [issue for issue in issues if issue[0] == "error"]


def subject_names(text):
    """The identifiers an assertion's `at "..."` text names, minus notation keywords.

    Used to choose between several diagnostics on one line. It is not a message comparison: the
    question is only whether a diagnostic is *about* the construct the assertion points at, which
    is a weaker and more defensible test than matching prose — the reference implementation writes
    its own diagnostic wording (0 of 146 assertion messages equal any of the 412 `says` texts in the
    metamodel's rule table, measured), so comparing sentences would measure style, not substance.
    """
    return set(IDENT.findall(text)) - NOTATION_KEYWORDS


def unscorable(text: str):
    """Why this model's score cannot be compared with this kernel's, or `[]` when it can.

    Two different environment gaps, both of which make an assertion measure the setup rather than the
    kernel. Neither is a defect in the model, and neither can be fixed here.

    **A file this checkout does not have.** `XPECT_SETUP` loads `/src/Dependency*.kerml` beside the
    model; five of those files were absent from this repository until they were vendored (see `present_libraries`). Every name a model takes from one is
    unresolvable here, and we report it, while the suite asserts nothing because in its harness the
    name resolves. Measured: fourteen models depend on one of those files, eleven of them score
    correctly anyway — their assertions are about duplicate names and arities that need nothing from
    the absent file — so only a model that carries **no assertions of its own** is excluded. Excluding
    all fourteen threw away twelve assertions that were being scored correctly.

    **A model that loads no standard library at all.** Three of the 136 list no `/library` file.
    `Feature_invalid_noType` is one: it asserts that `classifier A;` must specialize `Base::Anything`
    and that `feature f;` must have a type. Measured: the same bare shapes appear in 23 (feature), 84
    (part def) and 9 (classifier) places across models that do load the library and draw no assertion
    there; and `noErrors` models — the author's statement that the file conforms — contain bare
    `feature x;` and `part def Fuel;`. So the messages describe the reference implementation failing
    without a library, not a rule. SysML 7.6.8 says a declaration that does not specialize the
    required base "is considered to include an implicit subclassification or subsetting": a completion
    rule, not a constraint a model can violate. This kernel always loads the library, so the
    situation cannot arise, and imitating the message would mean inventing a diagnostic for something
    the specification says is automatically satisfied.

    Excluded only when the model also asserts nothing that could match here — which is the only case
    where exclusion is free, because such a model contributes no assertions to the denominator
    anyway. A model that does assert something is scored, and its unmet assertions are *attributed*
    rather than removed, so the denominator stays comparable.
    """
    head = text[: text.find("*/")] if "*/" in text else text
    deps = [d for d in DEPENDENCY.findall(head) if not d.startswith("/library")]
    missing = [d for d in deps if not (PILOT / d.lstrip("/")).exists()]
    if missing:
        _model, expects = split(text)
        if expects:
            return []
        return missing
    return []


def absent_setup_files(text: str):
    """The `/src` files a model's `XPECT_SETUP` names that this checkout does not have.

    `unscorable` asks the same question to decide whether a model can be scored at all. This is the
    other half of it: a model that does carry assertions is scored, and the names it takes from an
    absent file are the one thing its assertions can measure about the harness instead of the kernel.
    """
    head = text[: text.find("*/")] if "*/" in text else text
    deps = [d for d in DEPENDENCY.findall(head) if not d.startswith("/library")]
    return [d for d in deps if not (PILOT / d.lstrip("/")).exists()]


def present_libraries(text: str):
    """The `/src` files a model's `XPECT_SETUP` loads that this checkout has, as paths.

    Each file is listed twice in a setup (in `ResourceSet` and in `Workspace`), so duplicates are
    dropped, keeping the first order: the kernel is given each file once.
    """
    head = text[: text.find("*/")] if "*/" in text else text
    deps = [d for d in DEPENDENCY.findall(head) if not d.startswith("/library")]
    present = [str(PILOT / d.lstrip("/")) for d in deps if (PILOT / d.lstrip("/")).exists()]
    return tuple(dict.fromkeys(present))


def unmatchable(stem: str, lines, message: str, at: str, absent=()):
    """Why this assertion cannot be satisfied here, or `None` when it is a real gap.

    Attributions describe measured reference or setup differences. They are *labels on
    the miss*, not exemptions from it: `matched` stays false, the denominator stays 387, and a file
    carrying one still fails to be exact. Removing them from the count would be crossing out the
    question instead of answering it, and the gate's own test asserts the total is 387.

    There used to be an `"official-duplicate"` label for fourteen inherited-duplicate assertions at
    `perform b.a;`, `exhibit s.sa;` and `event a.areal;`, read as the reference reporting one conflict
    twice. The reference's own check says otherwise: `KerMLValidator.checkNamespace`
    (`KerMLValidator.xtend:421-448`) reports on every `Type`, and the performance and the feature
    chain `b.a` are two of them. The kernel now reports both, and the label is gone.

    `"no-library-setup"` — the model's `XPECT_SETUP` loads no standard library, so the reference was
    running without `Base.kerml`. Measured: bare `feature f;` appears in 23 places across models that
    do load the library and draws no assertion there; `noErrors` models contain bare `feature x;` and
    `part def Fuel;`. SysML 7.6.8 makes the required base specialization implicit, so there is nothing
    for a model to violate — the messages describe the reference failing, not a rule.
    """
    # A name the model takes from a `/src` file this checkout does not ship. Two assertions:
    # `ShadowingTests_ImportAndInnerClassesNamesAreTheSameGoodCase3` and its `_FT` twin both assert
    # `Couldn't resolve reference to … 'a2'` at `classifier b specializes a2{}` inside
    # `classifier inner { public import OuterPackage::*; … }`. `OuterPackage` lives in
    # `/src/DependencyOuterPackage.kerml`, which is not in this repository, so here the import binds
    # nothing, `a2` reaches the inner `A::a2` unshadowed, and there is no unresolved reference to
    # report. Their setups name the absent file, which is what `absent` holds.
    if absent and message.startswith("Couldn't resolve reference to"):
        return "missing-dependency-file"
    if stem in NO_LIBRARY_SETUP:
        return "no-library-setup"
    for fixture, line, expected_message, expected_at in REFERENCE_RECOVERY_CASCADE:
        if stem == fixture and list(lines) == [line] and message == expected_message and at == expected_at:
            return "reference-recovery-cascade"
    return None


def score(path: pathlib.Path):
    text = path.read_text(encoding="utf-8", errors="replace")
    if unscorable(text):
        # A distinct value from `None`, which means "the emitted model does not parse". Both are
        # excluded from the totals, but they are different facts and the report says which.
        return UNSCORABLE, None
    model, expects = split(text)
    # Names this model takes from a `/src` file the checkout does not ship. `unscorable` asks the
    # same question to decide whether the model can be scored at all; a model carrying assertions is
    # scored, and this is what tells the attribution which of its misses the harness owns.
    absent = absent_setup_files(text)
    issues = check(model, present_libraries(text))
    if issues is None:
        return None, None
    used = [False] * len(issues)
    rows = []
    for sev, msg, at, _line, candidates in expects:
        pool = [
            n
            for n, (isev, line, _imsg) in enumerate(issues)
            if not used[n] and isev == sev and line in candidates
        ]
        # Among the diagnostics on the asserted line, prefer one that names the construct the
        # assertion points at. Pairing by output order instead crossed two assertions with two
        # diagnostics on `Connector_Invalid.sysml.xt:45`: the assertion at `A::x` was answered by
        # the diagnostic about `y::z` and the other way round, so the line scored while neither
        # pair was about the same thing. This only *reorders* candidates within one line — it never
        # admits a pair the severity-and-line rule rejects, so it cannot inflate the score.
        want = subject_names(at)
        hit = None
        if want:
            for n in pool:
                if want & set(IDENT.findall(issues[n][2])):
                    hit = n
                    break
        if hit is None and pool:
            hit = pool[0]
        if hit is not None:
            used[hit] = True
        rows.append(
            {
                "severity": sev,
                "message": msg,
                "at": at,
                "lines": candidates,
                "matched": hit is not None,
                # Why an unmatched assertion is not a gap in this kernel, or `None` when it is one.
                # Attribution only: it never changes `matched`, the denominator, or `filesExact`.
                "unmatchable": None
                if hit is not None
                else unmatchable(path.stem, candidates, msg, at, absent),
            }
        )
    spurious = [issues[n] for n in range(len(issues)) if not used[n]]
    return rows, spurious


def main():
    global BIN
    ap = argparse.ArgumentParser()
    ap.add_argument("-v", "--verbose", metavar="FILE", help="score one file and list each assertion")
    ap.add_argument("--json", action="store_true")
    ap.add_argument(
        "--misses",
        action="store_true",
        help="group the unmatched assertions and the spurious diagnostics by message",
    )
    ap.add_argument("--bin", default=str(BIN))
    # Every model is one `check` process that reloads the checker, so this fans out over CPU, not
    # I/O. Sequentially it was 85s of the gate's 263s for 178 launches that are almost all
    # independent.
    ap.add_argument("--jobs", "-j", type=int, default=min(os.cpu_count() or 4, 16))
    args = ap.parse_args()
    BIN = pathlib.Path(args.bin)

    files = sorted(PILOT.glob("*.xt"))
    if not files:
        sys.exit(f"no .xt files under {PILOT}")

    def scan(paths):
        """`[(path, score(path))]`, in the given order.

        `executor.map` yields in submission order, so every list built below keeps corpus order
        and two runs of this script produce byte-identical output.
        """
        if args.jobs <= 1:
            return [(p, score(p)) for p in paths]
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            return list(zip(paths, pool.map(score, paths)))

    if args.verbose:
        selected = [f for f in files if args.verbose in f.name]
        if not selected:
            sys.exit(f"no file matching {args.verbose!r}")
        for path in selected:
            rows, spurious = score(path)
            if rows is UNSCORABLE:
                print(f"== {path.name}: UNSCORABLE (needs a file this checkout does not have)")
                continue
            if rows is None:
                print(f"== {path.name}: UNPARSED (the emitted model does not parse)")
                continue
            print(f"== {path.name}")
            for row in rows:
                if row["matched"]:
                    mark = "ok  "
                elif row["unmatchable"]:
                    mark = f"SKIP({row['unmatchable']})"
                else:
                    mark = "MISS"
                print(f" {mark} {row['severity']:7} {row['at'][:40]:42} lines {row['lines']}")
            for sev, line, msg in spurious:
                print(f" SPUR {sev:7} line {line}: {msg[:100]}")
        return

    if args.misses:
        import collections

        missing = collections.Counter()
        spurious = collections.Counter()
        attributed = collections.Counter()
        for path in files:
            rows, spur = score(path)
            if rows is None or rows is UNSCORABLE:
                continue
            for row in rows:
                if not row["matched"]:
                    key = (row["severity"], row["message"])
                    if row["unmatchable"]:
                        attributed[(row["unmatchable"],) + key] += 1
                    else:
                        missing[key] += 1
            for sev, _line, msg in spur:
                spurious[(sev, msg)] += 1
        print("unmatched assertions, by message:")
        for (sev, msg), count in missing.most_common():
            print(f"  {count:3}  {sev:7} {msg[:110]}")
        if attributed:
            print("unmatched but attributed to the reference or its setup, not to this kernel:")
            for (why, sev, msg), count in attributed.most_common():
                print(f"  {count:3}  {why:18} {sev:7} {msg[:96]}")
        print("spurious diagnostics, by message:")
        for (sev, msg), count in spurious.most_common():
            print(f"  {count:3}  {sev:7} {msg[:110]}")
        return

    total = matched = 0
    files_exact = 0
    spurious_total = 0
    unparsed = 0
    unscorable_count = 0
    by_attribution = {}
    worst = []
    by_kind = {"error": [0, 0], "warning": [0, 0]}
    for path, (rows, spurious) in scan(files):
        if rows is UNSCORABLE:
            unscorable_count += 1
            continue
        if rows is None:
            unparsed += 1
            continue
        if not rows:
            # A file with no assertions still has diagnostics, and every one of them is a false
            # positive: the official suite states nothing there, so anything this kernel says is
            # extra. Skipping the file hid exactly that — `ConnectorTest_..._GoodCase` carried nine
            # diagnostics that neither headline number could see, and fixing them moved nothing.
            spurious_total += len(spurious)
            continue
        got = sum(1 for r in rows if r["matched"])
        total += len(rows)
        matched += got
        spurious_total += len(spurious)
        if got == len(rows):
            files_exact += 1
        for row in rows:
            by_kind[row["severity"]][1] += 1
            by_kind[row["severity"]][0] += 1 if row["matched"] else 0
            if row["unmatchable"]:
                by_attribution[row["unmatchable"]] = by_attribution.get(row["unmatchable"], 0) + 1
        if got < len(rows):
            worst.append((len(rows) - got, path.name, got, len(rows)))

    # The false-positive direction, counted on its own. `noErrors` files carry no assertions, so
    # nothing above can see them.
    clean_files = 0
    clean_bad = []
    if args.jobs <= 1:
        clean_results = [(p, clean_model_errors(p)) for p in files]
    else:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            clean_results = list(zip(files, pool.map(clean_model_errors, files)))
    for path, errs in clean_results:
        if errs is None:
            continue
        clean_files += 1
        if errs:
            clean_bad.append((path.name, errs))

    if args.json:
        print(
            json.dumps(
                {
                    "assertions": total,
                    "matched": matched,
                    "files": len(files),
                    "filesExact": files_exact,
                    "spurious": spurious_total,
                    "unparsed": unparsed,
                    "unscorable": unscorable_count,
                    "attributed": by_attribution,
                    "bySeverity": by_kind,
                    "cleanFiles": clean_files,
                    "cleanFilesRejected": len(clean_bad),
                }
            )
        )
        return

    print(
        f"pilot assertions {matched}/{total}   files exact {files_exact}   "
        f"spurious {spurious_total}   unparsed {unparsed}"
        + (f"   unscorable {unscorable_count}" if unscorable_count else "")
    )
    for sev, (got, all_) in by_kind.items():
        print(f"  {sev:8} {got}/{all_}")
    if by_attribution:
        # The denominator stays `total` on purpose. These assertions are still misses; this line
        # only says why, so nobody reads the gap as unimplemented work and nobody is tempted to
        # shrink the corpus to make the fraction look better.
        detail = ", ".join(f"{why} {n}" for why, n in sorted(by_attribution.items()))
        print(f"  of the {total - matched} unmatched: {detail} (not gaps in this kernel)")
    print(
        f"  models the pilot calls clean: {clean_files - len(clean_bad)}/{clean_files} accepted"
    )
    for name, errs in clean_bad:
        print(f"    {name}: {len(errs)} error(s), first: {errs[0][2][:96]}")
    print("worst files:")
    for missing, name, got, all_ in sorted(worst, reverse=True)[:20]:
        print(f"  {missing:3} missing  {got:2}/{all_:2}  {name}")


if __name__ == "__main__":
    main()
