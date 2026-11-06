#!/usr/bin/env python3
"""Copy the licence files of the packages that the bundled tools come from, and write NOTICE.txt.

  usage: collect_notices.py LICENCES SOURCES

SOURCES is the sources.txt that bundle_tools.py writes: the real path of each file it copied.
A file from a Homebrew package (a path through .../Cellar/<name>/<version>/) has the package's
licence files copied to LICENCES/<name>/: the files named LICENSE, COPYING, COPYRIGHT or NOTICE
(any extension) at the top of the version folder, or in share/doc or share/licenses.
NOTICE.txt in LICENCES lists every package with its version, the source it was built from (the
URL and checksum of the stable release, from `brew info`), its homepage, the licence files copied
and the files it supplies. Files from anywhere else (such as the LGPL ffmpeg that build.sh --lgpl
builds) are listed as such.
A package with no licence file of its own gets the standard text of its licence
(packaging/notices/standard, named by the `license` Homebrew gives it). A licence with no text
there is a warning on stderr, and NOTICE.txt says what is not in the app: the build should not be
shipped with one.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

CELLAR = re.compile(r"/Cellar/([^/]+)/([^/]+)/")
LICENCE_NAME = re.compile(r"^(licen[cs]e|copying|copyright|notice)", re.IGNORECASE)
LICENCE_PLACES = ["*", "share/doc/*/*", "share/licenses/*/*"]
STANDARD = Path(__file__).resolve().parent.parent / "notices" / "standard"
STANDARD_TEXTS = {
    "BSL-1.0": "BSL-1.0.txt",
    "BSD-3-Clause": "BSD-3-Clause.txt",
    "LGPL-2.1": "LGPL-2.1.txt",
    "LGPL-3.0": "LGPL-3.0.txt",
    "GPL-2.0": "GPL-2.0.txt",
    "GPL-3.0": "GPL-3.0.txt",
    # Qt's own exception and the documentation licence, as Qt names them (Qt's LICENSES folder)
    "Qt-GPL-exception-1.0": "Qt-GPL-exception-1.0.txt",
    "GFDL-1.3-no-invariants": "GFDL-1.3-no-invariants-only.txt",
}
LICENCE_OPERATORS = {"AND", "OR", "WITH"}


def standard_texts(licence: str) -> tuple[list[Path], list[str]]:
    """The standard texts for the licences a Homebrew `license` names, and those with no text."""
    found: list[Path] = []
    uncovered: list[str] = []
    for token in re.findall(r"[A-Za-z0-9.+-]+", licence):
        if token in LICENCE_OPERATORS:
            continue
        # the full name first (Qt-GPL-exception-1.0, GFDL-1.3-no-invariants-only), then the base
        base = re.sub(r"(-only|-or-later)$|\+$", "", token)
        key = token if token in STANDARD_TEXTS else base
        if key in STANDARD_TEXTS:
            if STANDARD / STANDARD_TEXTS[key] not in found:
                found.append(STANDARD / STANDARD_TEXTS[key])
        elif token not in uncovered:
            uncovered.append(token)
    return found, uncovered


def licence_files(version_dir: Path) -> list[Path]:
    """The licence files of a package, found in the places Homebrew puts them."""
    found: set[Path] = set()
    for place in LICENCE_PLACES:
        for path in version_dir.glob(place):
            if path.is_file() and LICENCE_NAME.match(path.name):
                found.add(path)
    return sorted(found)


def brew_info(names: list[str]) -> dict[str, dict]:
    """The `brew info` record of each package, by name (empty if Homebrew cannot say)."""
    try:
        text = subprocess.run(["brew", "info", "--json=v2", *names], capture_output=True,
                              text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError) as err:
        print(f"warning: brew info failed, so NOTICE.txt has no source addresses: {err}",
              file=sys.stderr)
        return {}
    return {formula["name"]: formula for formula in json.loads(text).get("formulae", [])}


def main(argv: list[str]) -> None:
    if len(argv) != 3:
        raise SystemExit(__doc__)
    licences = Path(argv[1])
    sources = Path(argv[2]).read_text().split()
    licences.mkdir(parents=True, exist_ok=True)

    packages: dict[tuple[str, str], tuple[Path, list[str]]] = {}
    other: list[str] = []
    for source in sources:
        match = CELLAR.search(source)
        if not match:
            other.append(source)
            continue
        key = (match.group(1), match.group(2))
        version_dir, names = packages.get(key, (Path(source[: match.end() - 1]), []))
        names.append(Path(source).name)
        packages[key] = (version_dir, names)

    info = brew_info(sorted({name for name, _ in packages}))
    lines = [
        "Notices for the command-line tools and libraries bundled in this app.",
        "Each package's licence files are in the folder named for it, beside this file.",
        "The source of each package is the stable release at the address given (from Homebrew),",
        "with the checksum Homebrew records for it.",
        "",
    ]
    missing = []
    for (name, version), (version_dir, bundled) in sorted(packages.items()):
        files = licence_files(version_dir)
        formula = info.get(name, {})
        lines.append(f"{name} {version}")
        stable = formula.get("urls", {}).get("stable", {})
        if stable.get("url"):
            lines.append(f"  source:   {stable['url']}")
            if stable.get("checksum"):
                lines.append(f"  sha-256:  {stable['checksum']}")
        if formula.get("homepage"):
            lines.append(f"  homepage: {formula['homepage']}")
        lines.append(f"  supplies: {', '.join(sorted(set(bundled)))}")
        if files:
            copied = files
            lines.append(f"  licence:  {name}/" + ", ".join(path.name for path in files))
        else:
            copied, uncovered_here = standard_texts(formula.get("license") or "")
            if copied:
                names_here = ", ".join(path.name for path in copied)
                lines.append("  licence:  the standard text of its licence, as the package "
                             f"ships none: {names_here}")
            else:
                lines.append("  licence:  no licence file was found in the package")
                uncovered_here = [formula.get("license") or "unknown licence"]
            if uncovered_here:
                missing.append(f"{name} {version} ({', '.join(uncovered_here)})")
                lines.append(f"  not in the app: {', '.join(uncovered_here)}")
        if copied:
            folder = licences / name
            folder.mkdir(exist_ok=True)
            for path in copied:
                shutil.copy2(path, folder / path.name)
        lines.append("")
    if other:
        lines.append("Not from Homebrew:")
        lines += [f"  {path}" for path in sorted(set(other))]
        lines.append("")
    (licences / "NOTICE.txt").write_text("\n".join(lines))
    print(f"  {len(packages)} packages' licences in {licences} (NOTICE.txt lists them)")
    for package in missing:
        print(f"warning: the licence of {package} is not in the app (see NOTICE.txt)",
              file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv)
