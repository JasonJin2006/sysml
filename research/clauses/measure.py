"""How many normative clauses of TS 23.501 can be written as a constraint at all.

The plan in `docs/next-steps.md` said 18 clauses carry a decidable value, and that if fewer than
five of them could be closed, the number itself had to go into the conclusion. This measures it.

Usage:  python measure.py [--clauses PATH]
"""

import argparse
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT = os.path.join(HERE, "..", "corpus", "3gpp", "23501-i80.clauses.jsonl")

# A number with a physical unit. `5G` is excluded by requiring a unit word after the number and
# by not accepting a bare `G`; the text's most common number is the system's name.
UNIT = re.compile(
    r"(?<![\w.])\d+(?:\.\d+)?\s*"
    r"(?:ms|s|sec|min|h|hour|km|m|kg|g|MHz|GHz|Mbps|Gbps|kbps|bps|%|byte|kB|MB|GB|TB|dBm|dB)\b"
)

# A phrase that turns a number into a bound. Without one, a clause states a range in a table or
# describes a value, and no predicate can be written from the sentence alone.
COMPARATIVE = re.compile(
    r"\b(at most|no more than|maximum|shall not exceed|less than|greater than|"
    r"at least|no less than|minimum|shall be (?:greater|less))\b",
    re.I,
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--clauses", default=DEFAULT)
    args = parser.parse_args()

    clauses = []
    with open(args.clauses, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                clauses.append(json.loads(line))

    normative = [c for c in clauses if c.get("kind") == "normative"]
    with_unit = [c for c in normative if UNIT.search(c.get("text", ""))]
    with_comp = [c for c in normative if COMPARATIVE.search(c.get("text", ""))]
    both = [
        c
        for c in normative
        if UNIT.search(c.get("text", "")) and COMPARATIVE.search(c.get("text", ""))
    ]

    print(f"clauses            {len(clauses)}")
    print(f"normative          {len(normative)}")
    print(f"number and unit    {len(with_unit)}")
    print(f"comparative phrase {len(with_comp)}")
    print(f"both               {len(both)}   <- the shape a constraint can take")
    print()
    for c in both:
        print(f"  {c['id']:12s} {c['title'][:46]:46s} {UNIT.search(c['text']).group(0)}")


if __name__ == "__main__":
    main()
