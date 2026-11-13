"""Compare `core/stdlib` with the reference libraries vendored as `.kpar` archives.

The pilot measures this kernel against the OMG reference implementation, and one class of
difference cannot be fixed by a rule: the standard library itself. This script answers whether the
two are the same library, file by file, by content.

    python tools/library_diff.py [--kpar DIR] [--stdlib DIR]

Exit code is 0 when every file matches, 1 when anything differs, so it can be a gate step. Run it
after unzipping the archives into a temporary directory yourself, or let this script do it — the
archives are plain zip files.
"""

import argparse
import hashlib
import pathlib
import sys
import tempfile
import zipfile


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def collect(root: pathlib.Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for path in root.rglob("*"):
        if path.is_file() and path.suffix in (".kerml", ".sysml"):
            out[path.name] = digest(path)
    return out


def main() -> int:
    here = pathlib.Path(__file__).resolve().parent.parent
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--kpar",
        default=str(here.parent / "research" / "upstream" / "kpar"),
        help="directory holding the reference .kpar archives",
    )
    ap.add_argument("--stdlib", default=str(here / "stdlib"), help="this repository's library")
    args = ap.parse_args()

    archives = sorted(pathlib.Path(args.kpar).glob("*.kpar"))
    if not archives:
        print(f"no .kpar archives under {args.kpar}", file=sys.stderr)
        return 2
    reference: dict[str, str] = {}
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        for archive in archives:
            with zipfile.ZipFile(archive) as zf:
                zf.extractall(root)
        reference = collect(root)
    ours = collect(pathlib.Path(args.stdlib))

    only_reference = sorted(set(reference) - set(ours))
    only_ours = sorted(set(ours) - set(reference))
    differing = sorted(n for n in set(reference) & set(ours) if reference[n] != ours[n])

    print(f"reference {len(reference)} files, stdlib {len(ours)} files")
    for label, names in (
        ("only in the reference", only_reference),
        ("only in stdlib", only_ours),
        ("differing", differing),
    ):
        print(f"  {label}: {len(names)}")
        for name in names[:10]:
            print(f"    {name}")
    return 0 if not (only_reference or only_ours or differing) else 1


if __name__ == "__main__":
    raise SystemExit(main())
