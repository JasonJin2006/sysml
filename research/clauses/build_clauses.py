"""Build the clause ledger for 3GPP TS 23.501 from its Word source.

Structure comes from paragraph styles. A heading's number and title are separated by a real tab,
which the Word template writes as <w:tab/>, so the number is read rather than guessed. Every
numbered heading is checked against the table of contents (TOC1..TOC8), in full. Body paragraphs
belong to the nearest numbered heading above them.

Writes into research/corpus/3gpp/, next to the source. That folder is not committed, because the
files hold spec text. research/clauses/README.md says what each file is for.
"""

import csv
import json
import random
import re
import zipfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "research" / "corpus" / "3gpp" / "23501-i80.docx"

PARAGRAPH = re.compile(r"<w:p[ >].*?</w:p>|<w:p/>", re.S)
TOKEN = re.compile(r"<w:tab\s*/>|<w:noBreakHyphen\s*/>|<w:br\s*/>|<w:t(?: [^>]*)?>(.*?)</w:t>", re.S)
STYLE = re.compile(r'<w:pStyle w:val="([^"]+)"')
# Clause numbers can carry a letter in any segment: 5.35A, 5.20b.1, 4.2.8.1A, 6.3.12a.1.
NUMBER = re.compile(r"\d+[A-Za-z]?(?:\.\d+[A-Za-z]?)*$")
ANNEX_LETTER = re.compile(r"[A-Z](?:\.\d+)*$")
ANNEX_TEXT = re.compile(r"Annex\s+[A-Z]\b")
INTERNAL_REF = re.compile(r"\b(?:clauses?|subclauses?)\s+(\d+[A-Za-z]?(?:\.\d+[A-Za-z]?)+)")
OTHER_SPEC = re.compile(r"\b(?:TS|TR|Std|RFC|IEEE|IETF|ETSI|ITU-T|IEC|ISO)\b")
EXTERNAL_TAIL = re.compile(r"\s+(?:of|in)\s+[^;.()]{0,60}?" + OTHER_SPEC.pattern)
TS_REF = re.compile(r"\bTS\s+(\d{2}\.\d{3})\b")
NORMATIVE = re.compile(r"\b(?:shall|must)\b")
INFORMATIVE = re.compile(r"\b(?:should|may)\b")
SHALL = re.compile(r"\bshall\b")


def paragraph_text(block):
    parts = []
    for match in TOKEN.finditer(block):
        token = match.group(0)
        if token.startswith("<w:tab"):
            parts.append("\t")
        elif token.startswith("<w:noBreakHyphen"):
            parts.append("-")
        elif token.startswith("<w:br"):
            parts.append(" ")
        else:
            parts.append(match.group(1) or "")
    text = "".join(parts)
    for entity, char in (("&lt;", "<"), ("&gt;", ">"), ("&quot;", '"'), ("&apos;", "'"), ("&amp;", "&")):
        text = text.replace(entity, char)
    return text.strip()


def paragraphs():
    xml = zipfile.ZipFile(SOURCE).read("word/document.xml").decode("utf-8")
    for number, block in enumerate(PARAGRAPH.findall(xml), 1):
        style = STYLE.search(block)
        yield number, (style.group(1) if style else ""), paragraph_text(block)


def structure():
    clauses, toc, annexes, unnumbered = [], set(), [], []
    current, total, shall_all, shall_long = None, 0, 0, 0
    for number, style, text in paragraphs():
        total += 1
        if SHALL.search(text):
            shall_all += 1
            shall_long += len(text) > 60
        if not text:
            continue
        if re.fullmatch(r"TOC\d", style):
            head = text.split("\t", 1)[0].strip()
            if NUMBER.match(head):
                toc.add(head)
            continue
        # 3GPP writes level-6 clause headings with the style H6, not Heading6.
        if re.fullmatch(r"Heading\d|H6", style):
            head, _, title = text.partition("\t")
            head, title = head.strip(), " ".join(title.split())
            if NUMBER.match(head):
                current = {"id": head, "title": title, "level": int(style[-1]), "line": number, "body": []}
                clauses.append(current)
            else:
                if ANNEX_TEXT.match(text) or ANNEX_LETTER.match(head):
                    annexes.append(text)
                else:
                    unnumbered.append(text)
                current = None
            continue
        if current is not None:
            current["body"].append((number, text))
    return clauses, toc, annexes, unnumbered, total, shall_all, shall_long

def analyse(clauses, toc):
    ids = [clause["id"] for clause in clauses]
    known = set(ids)
    kinds, records, unresolved, ambiguous, external, internal = Counter(), [], [], [], 0, 0
    for clause in clauses:
        texts = [text for _, text in clause["body"]]
        normative = sum(bool(NORMATIVE.search(text)) for text in texts)
        informative = sum(bool(INFORMATIVE.search(text)) for text in texts)
        kind = ("empty" if not texts else "normative" if normative
                else "informative" if informative else "descriptive")
        kinds[kind] += 1
        refs = []
        for text in texts:
            for match in INTERNAL_REF.finditer(text):
                # "clause 4.2 of TS 23.502" points into another specification, not this one.
                if EXTERNAL_TAIL.match(text, match.end()):
                    external += 1
                    continue
                internal += 1
                refs.append(match.group(1))
                if match.group(1) not in known:
                    # A paragraph that also names another specification may point into it instead.
                    (ambiguous if OTHER_SPEC.search(text) else unresolved).append([clause["id"], match.group(1)])
            refs.extend(f"TS {number}" for number in TS_REF.findall(text))
        records.append({
            "id": clause["id"], "title": clause["title"], "level": clause["level"],
            "kind": kind, "normative": normative, "informative": informative,
            "line": clause["line"], "refs": sorted(set(refs)), "text": " ".join(texts),
        })
    numbers = Counter(ids)
    siblings = {}
    for number in ids:
        parent, _, last = number.rpartition(".")
        if last.isdigit():
            siblings.setdefault(parent, []).append(int(last))
    gaps = [[parent, sorted(set(range(1, max(seq) + 1)) - set(seq))] for parent, seq in siblings.items()]
    # The table of contents lists levels 1 to 5 only, so level-6 headings are checked for structure
    # alone; the cross-check against the contents is over the levels it can list.
    shallow = {clause["id"] for clause in clauses if clause["level"] <= 5}
    checks = {
        "numbered_clauses": len(ids),
        "level6_headings": sum(clause["level"] == 6 for clause in clauses),
        "duplicate_numbers": sorted(number for number, count in numbers.items() if count > 1),
        "toc_entries": len(toc),
        "headings_missing_from_toc": sorted(shallow - toc),
        "toc_entries_without_heading": sorted(toc - known),
        "clauses_whose_parent_is_missing": [i for i in ids if "." in i and i.rpartition(".")[0] not in known],
        "sibling_numbers_with_gaps": [gap for gap in gaps if gap[1]],
        "internal_references": internal,
        "internal_references_unresolved": unresolved,
        "internal_references_ambiguous": ambiguous,
        "external_clause_references": external,
        "kinds": dict(kinds),
    }
    return records, checks


def samples(clauses, records):
    """Three samples for a person to read: 50 headings, 200 paragraphs to label, 30 references."""
    rng = random.Random(23501)
    rows = [["numbering", c["id"], c["line"], "", c["title"], ""] for c in rng.sample(clauses, 50)]
    body = [(number, text, c["id"]) for c in clauses for number, text in c["body"] if len(text) >= 40]
    positive = [item for item in body if NORMATIVE.search(item[1])]
    negative = [item for item in body if not NORMATIVE.search(item[1])]
    for number, text, clause_id in rng.sample(positive, 100) + rng.sample(negative, 100):
        machine = "normative" if NORMATIVE.search(text) else "other"
        rows.append(["normative", clause_id, number, machine, text, ""])
    pairs = [(r["id"], ref, r["text"]) for r in records for ref in r["refs"] if not ref.startswith("TS ")]
    for clause_id, ref, text in rng.sample(pairs, 30):
        at = text.find(ref)
        rows.append(["reference", clause_id, "", ref, text[max(0, at - 100): at + 60], ""])
    return rows


def main():
    clauses, toc, annexes, unnumbered, total, shall_all, shall_long = structure()
    records, checks = analyse(clauses, toc)
    checks.update({
        "paragraphs": total, "annex_headings": len(annexes), "unnumbered_headings": len(unnumbered),
        "paragraphs_with_shall": shall_all, "paragraphs_with_shall_over_60_chars": shall_long,
    })
    base = SOURCE.parent / "23501-i80"
    with open(f"{base}.clauses.jsonl", "w", encoding="utf-8", newline="\n") as f:
        for record in records:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    with open(f"{base}.check.json", "w", encoding="utf-8", newline="\n") as f:
        json.dump(checks, f, ensure_ascii=False, indent=1)
    with open(f"{base}.review.csv", "w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["task", "clause", "line", "machine", "text", "human"])
        writer.writerows(samples(clauses, records))
    print(f"clauses {len(records)}  kinds {checks['kinds']}")
    print(f"shall paragraphs {shall_all} (over 60 chars {shall_long})  toc entries {len(toc)}")
    print(f"annex headings {len(annexes)}  unnumbered headings {len(unnumbered)}")
    for key in ("duplicate_numbers", "headings_missing_from_toc", "toc_entries_without_heading",
                "clauses_whose_parent_is_missing", "internal_references_unresolved"):
        print(f"{key}: {len(checks[key])}  first {checks[key][:4]}")
    print(f"sibling numbers with gaps: {len(checks['sibling_numbers_with_gaps'])}")
    print(f"internal references {checks['internal_references']}  external {checks['external_clause_references']}")


if __name__ == "__main__":
    main()
