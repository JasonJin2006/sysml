# System Modeling Conventions

This document is for a person or an agent doing SysML v2 modeling with this tool. It says nothing
about the implementation; reading it is enough to produce a model the tool accepts.

Before the first command, put the binary somewhere you can call it:

```bash
cargo build --release --manifest-path core/Cargo.toml     # or: cmd //c core\build.bat build --release
./core/target/release/sysml --version
```

The conventions below assume `sysml` is on `PATH`. A model directory holds the model and the
ledger the tool writes beside it; nothing else is required.

## Zero: Remember One Thing

Every number in a model will later be read by someone as "this is what we want." **Every number you pick, if nobody objects, will live on under someone else's name.** Every command in this workspace exists to keep that from happening quietly.

## Before Each Step: Ask What the Model Is Waiting For

```
sysml next <model>.sysml
```

It reports three things: which stage the model has reached, what it is stuck on, and the next command to run. The stages follow the order of systems engineering:

| Stage | Exit criterion |
|---|---|
| 1 requirements | There is a requirement, and its constraint is not a placeholder such as `{1 > 0}` |
| 2 questions | No questions are unanswered |
| 3 allocation | Every concern has a requirement derived from it, and every requirement has a subject or a satisfy |
| 4 values | Every value has a ledger entry, and every assumption has been approved by a person |
| 5 interfaces | Both ends of every connection exist |
| 6 verification | Every constraint has a verdict |

The criteria are computed from the model and the ledger, not from the tool's opinion. **Do not skip them.** Every step you bypass will come back, unchanged, in the `check` or `prov` output when you deliver.

It does not stop you, by design: real projects often sketch a structure first while requirements are still open. It simply stops explaining, on your behalf, why a step is not passing.

It also checks only what it can check. Behavior and timing, which it cannot judge, remain your judgment.

## Before Zero: Research, Then Model

**Your training knowledge is not enough, and it does not count as evidence.** Each value must come either from checkable material or from your judgment, and the ledger must keep the two separate. So research before you model:

```
sysml source <model> --title "electric skateboard datasheet" --kind datasheet   --locator "https://..." --quote "Deck length: 1800 mm, whole board 12 kg"   --accessed 2026-10-25 --by agent
```

`--quote` is required. **A link alone does not count as a source.** The reader does not need to know whether you looked at a web page. The reader needs to know whether you read the number correctly, and only a quotation can show that.

Research should find these things, in order of importance:

1. **Published specifications of comparable products or designs.** What others have achieved shows whether your numbers are unrealistic.
2. **Industry standards and regulations.** This matters most for crewed equipment: structural safety factors, braking distance, and electrical protection ratings.
3. **Design patterns and failure cases.** What went wrong with similar products is more useful than how well they sold.
4. **Real-world reference points for order-of-magnitude checks.** For each number you fill in, find a real-world value to compare it against.

Use these categories for `--kind`: `standard` / `datasheet` / `paper` / `product` / `regulation` / `expert`.

The category is not a formality. When judging a number, a reader first checks what kind of material it came from.

**If nothing is found:** record the value as `assumed`, state its falsifier, and list in the report what you searched for and what you did not find. Do not present a judgment as a specification because nothing could be found.

## One: Record What You Do Not Know Before Starting

When you receive requirements, do not model straight away. Do one thing first: **list every fork you cannot infer from the requirements and that a person must decide.**

```
cd <workspace>
sysml ask <model>.sysml --question "what is being inspected? that decides payload and route" --at "P::Platform" --blocks "P::CoverageReq" --by agent
```

`--blocks` can be repeated. Each value points to a requirement that this question blocks. **It must be a requirement that really exists in the model.** If you get it wrong, the command refuses and lists candidates.

Criterion: **a parameter that would force rework if set wrong, and that the requirements leave open, is a fork.** Platform form, operating radius, accuracy, environmental rating, service life, and autonomy level are almost always forks. Units, field names, and naming style are not.

After you record them, give the list of questions to the user, then **stop**. Do not fill in default values while you wait.

### When Nobody Answers

When someone is available, the rule above is the whole story. **When nobody is available (a one-time delivery or an unattended run), stopping there means doing nothing.** In a real test, one agent spent its whole run deciding whether to ask or to guess, and wrote not a single word.

When no one answers, follow this order:

1. **Record the questions anyway.** Do not skip recording because nobody answered. The record is the entry point for the next person.
2. For each requirement blocked by a question, fill in a value marked `assumed`, and state its falsifier.
3. The first section of the delivery report must be the list of these questions. For each one, state which assumption you used in its place.
4. **Never write `--by "human:..."` yourself.** If no person has spoken, no person has approved anything.

A blocked requirement is reported as `PROV010` by `prov`. This is not an error to fix. It is **a sentence that must be read**: the numbers under this requirement are held up by assumptions. The report must repeat it, and it must not stay only in the tool's output.

## Two: After the User Answers

```
sysml answer <model>.sysml --id Q001 --text "transmission line, tower spacing 300 m" --by "human:<name>"
```

If the user says "你看着办" (roughly "up to you"), that is not an answer. It is permission to assume. Record the value as `assumed`, and state the source of the permission in `--why`: `--why "用户在 Q003 授权自行决定"`.

**If an answer does not match its question, do not force it in.** The user may have quoted the wrong number. If the user answered Q005 with words meant for Q002, record the answer by what it actually says, and open a new question for it. Do not squeeze it into a mismatched question just to keep the ledger tidy.

## Three: Every Number Needs a Source

Every `attribute x = value` in the model must have a decision record. There are four kinds of origin:

| origin | When to use it | Required |
|---|---|---|
| `spec` | The requirements, a standard, or a supplier document states the value | `--why` gives the source |
| `derived` | Calculated from other values | `--from` lists the input elements, and each one must have a ledger entry |
| `assumed` | Your judgment | `--why` and `--falsifier` are both required |
| `asked` | A person has answered | `--question` points to an **answered** question |

```
sysml decide <model>.sysml --element "P::Drone::cruiseSpeed" --value "10 [m/s]" --origin assumed --why "common cruise range for industrial inspection" --falsifier "if the job needs <8 or >15 m/s, both the energy and time budget are recomputed" --by agent
```

**`--falsifier` is the most important field in this convention.** It does not ask for "this value might be wrong." It asks for "what observation would show that it is wrong." If you cannot write a falsifier, you do not know where the risk lies, and you should ask a person.

`derived` is not a refuge. `--from` must point to elements that really exist in the model and have values of their own. If even one input in the formula is one you picked, the entry is `assumed`, not `derived`.

## Four: A Person Approves Assumptions

A value marked `assumed` needs a person to accept the risk. **You cannot do this for a person:**

```
sysml approve <model>.sysml --id D003 --by "human:jason"
```

`--by` must start with `human:`, or the command is refused. One approval cannot override another. The `PROV006` warning from `prov` disappears only after approval.

## Five: Retract a Record That Was Wrong

```
sysml retract <model>.sysml --id D006
```

Do not edit `.ledger.json` by hand. Retracting a record does not let later records reuse its number.

## Before Five: The Model Must Be Big Enough

A one-sentence requirement should not produce a fifty-line model. **The size of a complex system's model should match the volume of decisions it carries.**

The criterion is **coverage**, not line count. The six stages of `next` can check structure, values, interfaces, requirements, and verification. They cannot check behavior or timing, so you must account for those two layers yourself:

| Layer | Minimum content |
| --- | --- |
| Structure | Subsystems decomposed to a grain that can be assigned independently; each subsystem has its own values and ports |
| Interfaces | Connections across subsystems; each one states what it carries and under what conditions |
| Behavior | At least one state machine (how the system switches between modes, including fault modes) |
| Timing | Key operating sequences (startup, mission, recovery), with an order among the steps |
| Requirements | Each requirement has a decidable constraint, not a prose description |
| Values | Key design parameters have a value, a source, and an order-of-magnitude check |

Rather than a fifty-line skeleton, the right output for a one-sentence requirement is **a skeleton that lists every pending fork**, plus **complete versions of the parts that are already determined**. Record forks with `ask`. Do not leave them blank, and do not guess.

If the task is "design a crewed aircraft," a fifty-line model shows that you thought the problem was smaller than it is.

## After Five: Probe Files Leave No Trace

It is fine to write small files to try out syntax. **But the first file must be the model.**

The first thing to do after receiving the requirements: write a complete skeleton that passes `check`, with packages, subsystems, requirements, and connections. All numbers may be `assumed`. Then revise it one item at a time. **Do not learn the whole syntax before starting to write.** To find out how the tool judges X, add one X to the model, run `check`, and read the result. Do not open a new file for each question.

The criterion is strict: **if there is no model after fifteen minutes, you are avoiding writing one.**

The reasoning comes from a test. The same one-sentence requirement was run three times:

| Run | Result |
|---|---|
| Run 1 | 3 hours 37 minutes, 40 probes, 0 deliverables |
| Run 2 | The contract did not say what to do when nobody answers, so the agent spent the whole run deciding and wrote nothing |
| Run 3 | Probes moved into `_scratch/`; 21 minutes and 47 probes, still 0 deliverables |

In all three runs, the agent kept **removing uncertainty**. Each probe answered one question, which felt like progress. The tool is most useful for reading what you have already written. A skeleton is the thing it can read.

```
models/_scratch/        for trying syntax, at most three; delete the whole directory before delivery
models/<model>.sysml      the deliverable
models/<model>.ledger.json
```

## Six: Three Gates Must Pass Before Delivery

```
sysml check  <model>.sysml
sysml prov   <model>.sysml
sysml status <model>.sysml
```

The exit codes of `check` and `prov` must both be 0. Do not count "most of it passed" as a pass.

### Retry Budget

If the same model still has not converged after **five** runs of `check`, stop.

The rule is to **stop**, not to try once more. In the report, state which check is stuck, which approaches you tried, and why none of them worked. Each run after the fifth tells you that you lack information (an unanswered question), not patience. In the real test, the agent that ran for three hours and forty minutes, creating 40 probe files, never reached delivery.

The same reasoning applies to probes: more than three files in `models/_scratch/` means you are using probes in place of a model.

Handle every category that `check` reports:

| Code | Meaning | What to do |
|---|---|---|
| `SEM001` | A connection endpoint does not exist | Fix the model |
| `SEM003` | A value expression refers to a name that does not exist | Fix the model |
| `SEM005` | A constraint does not hold | Fix the model or the value |
| `SEM006` | A constraint cannot be decided (missing value, mixed units) | Supply the value, or state why you do not |
| `SEM007` | A constraint consists only of literals (`{1>0}`) | It is a placeholder, not a requirement |
| `PROV001` | A value in the model has no ledger entry | Add it with `decide` |
| `PROV003` | Claims `derived` but names no inputs | Add `--from`, or change to `assumed` |
| `PROV005` | An assumption has no falsifier | Add one, or ask a person instead |
| `PROV006` | An assumption has no human approval | Ask a person to run `approve` |
| `PROV009` | Claims a person answered, but the question is still open | Run `answer` first |
| `PROV010` | A requirement is blocked by an open question | This is not an error. It is the current state, and the report must state it |

When a machine reads the report, use `--json`. Do not parse the human-readable output.

## Seven: What to State at Delivery

Write these in this order, without changing it:

1. **The model path**, and the exit codes of `check`, `prov`, and `status`, quoted exactly.
2. **Open questions**: how many are open, and which requirement each one blocks.
3. **Assumption list**: each `assumed` value, its falsifier, and whether a person approved it.
4. **Requirements that cannot be decided**: how many are `inconclusive`, and why.
5. **What was not done**: which analyses have not started, and why (usually because they depend on open items).

Do not write "all verification passed." Write "check exit code 0, prov exit code 0, 7 requirements: 5 pass, 2 inconclusive (reason: cross-unit comparison)."

## Eight: Three Prohibitions

**Do not describe an `assumed` value as `derived`.** If a number was picked by judgment, say it was picked.

**Do not mark a question `answered` when nobody has answered.** `answered_by` must be a real person. Writing `--by "human:xxx"` yourself, when no person has spoken, is a forged record.

**Do not treat "the user authorized me to decide" as a spec.** That is an `assumed` value. A person has only agreed to accept the risk.

## Nine: Command Reference

```
sysml.exe check  <model> [--json]     syntax / names / values / three-valued constraint verdicts
sysml.exe prov   <model> [--json]     provenance gate: every number needs an entry
sysml.exe next   <model> [--json]     how far the model got, where it is stuck, the next command
sysml.exe status <model> [--issues]   one-screen summary
sysml.exe review <model> [--out x.html]  generate a review page for a person

sysml.exe ask    <model> --question ... --blocks <requirement> --by agent
sysml.exe answer <model> --id Q001 --text ... --by "human:<name>"
sysml.exe decide <model> --element ... --origin ... --value ...
sysml.exe approve <model> --id D003 --by "human:<name>"
sysml.exe retract <model> --id D006
sysml.exe ledger <model> [--json]     show the whole ledger

sysml.exe outline | graph | render | resolve   structure and views
sysml.exe studio <model> [--port 7373]          editable canvas, opened in a browser
```

`studio` is for people. Drag boxes to change the layout, click an element to see its source and questions, and edit numbers and names in the panel. It still edits the `.sysml` text and the ledger, not a separate model. So what you see in the text and on the canvas is always the same thing. It writes two side files in the same directory: `<模型名>.sysml.layout.json` (box positions, pure geometry) and the existing `.ledger.json`. **The layout file is not part of the model, and it need not be delivered.** The ledger must be delivered.

The ledger is `<模型名>.ledger.json` in the same directory. **It must be delivered together with the model.** The reasons must travel with the model, or the next person sees only the numbers.
