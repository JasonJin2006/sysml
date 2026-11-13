# Clause ledger: 3GPP TS 23.501

> Chinese translation: [docs/zh/research/clauses/README.md](../../docs/zh/research/clauses/README.md)

First step of the main line (see `docs/next-steps.md`). Generate the clause ledger from `research/corpus/3gpp/23501-i80.docx`.

The output goes to `research/corpus/3gpp/`. It is kept out of version control because the files contain the normative text of the specification. The scripts, the scoring script, the label files, and this README are kept in version control. The label file contains only line numbers and labels, with no normative text.

## Running

```
python research/clauses/build_clauses.py                    # build the ledger, mechanical checks, samples to label
```

## Output

- `23501-i80.clauses.jsonl`: one entry per clause. The fields are id, title, level, kind, normative and informative (paragraph counts), line (paragraph number), refs, and text. The kind is normative for paragraphs with shall or must, informative for paragraphs with should or may, descriptive, or empty (subheading only).
- `23501-i80.check.json`: all results of the mechanical checks.
- `23501-i80.review.csv`: 280 rows of samples awaiting labels. 50 rows check numbering against headings. 200 normative paragraphs have the machine judgment in the machine column, and the manual label goes in the human column. 30 reference rows check that each reference points to the right target.

## What is not in this repository

The labelled samples and the script that scored them are not published: they are research
output, not part of the product. The ledger, the builder and the mechanical checks are what the
tool needs, and they stay.

## Method

- The structure comes from paragraph styles. Heading1 through Heading9, along with H6, are headings. The number and the title are separated by a real tab character (the Word template writes `<w:tab/>`), so numbers are read directly, not guessed.
- The table of contents is TOC1 through TOC5, covering only the first five levels. Level-6 headings are checked for structure only.
- Numbers can include letters, such as 5.35A, 5.20b.1, 4.2.8.1A, and 6.3.12a.1. The first version did not recognize letters and missed 28 clauses. Level-6 headings use the H6 style, and another 22 were missed.

## Results of the mechanical checks

| Item | Result |
|---|---|
| Numbered clauses | 1011 (including 22 at level 6) |
| Table of contents matches | 989 entries at levels 1 to 5, matched one to one with the headings, with no missing, duplicate, or orphan parent clauses |
| Numbering gap | 1 case: under `6.3.6.3` there is only `.3`, with no `.1` or `.2`. The table of contents shows the same thing, so it is the numbering in the original |
| Internal references | 1449. 7 are unresolved, covering 6 clause numbers. The paragraphs containing them do not mention another standard, so they may be dangling references in the original. Another 58 are in paragraphs that mention another standard, and a person must judge where they point |
| Comparison with `docs/next-steps.md` | 1096 paragraphs contain shall (previously 1100), and 1075 are longer than 60 characters (previously 1078) |

## Assistant labels (not a manual review)

The assistant produced these labels after reading each paragraph. **They are not a manual review.** The method for manual review is described under "Not done yet" below.

Rules (based on meaning, not on modal verbs):

- **normative**: the sentence constrains an entity or a procedure step (what it must do). This includes obligations with shall and must, and also procedure steps, responsibility assignments, and rules that use no modal verb.
- **other**: NOTE and EXAMPLE blocks, definitions and field descriptions, descriptions of capabilities or services, permissions (may, can), recommendations (should), list titles, and sentences that cite other specifications.

Results:

| Item | Result |
| --- | --- |
| normative, 200 paragraphs (machine rule: paragraph contains shall or must) | Precision 0.980, recall 0.817, F1 0.891 (TP 98, FP 2, FN 22, TN 78) |
| Misjudgments (machine normative, assistant other) | 5307 (a parameter definition containing "shall stop"), 9569 (NOTE 2) |
| Misses (machine other, assistant normative) | 22 paragraphs. All are procedure or rule sentences that do not use shall or must, for example "The SMF configures ...", "... only apply to ...", or "The UE and AMF start the timer ..." |
| Headings, 50 entries | All ok. The numbers match the titles. Titles are taken from the heading paragraphs, and each was checked one by one. |
| References, 30 entries | 29 ok, 1 external: 5.28.3.2 points to TS 23.502, but the parser treated it as an internal reference |

These F1 figures show that looking only at shall and must misses about one fifth of the obligation sentences. The normative count in the ledger should rest on another basis, not on counting modal verbs alone.

## Findings in the original

- Under `6.3.6.3`, there is only `6.3.6.3.3`, with no `.1` or `.2`. The table of contents shows the same, so this is the numbering in the original.
- Seven internal references have no matching clause (for example, 5.12.3 refers to 5.12.12, and 5.8.5.6 refers to 5.8.3.2). They may be dangling references in the original.

## Not done yet

- **Sentence-level parsing**: coordinated forms such as `clauses A and B of TS x` are currently counted by the parser as internal references (one case found in the sample). Fixing this would change the sample, so the ledger must be regenerated and relabeled.
- **Second step**: two-way tracing between clauses and the model (this needs the kernel's `clause` field and `sysml trace`). Not started.
