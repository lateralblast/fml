"""The licence texts the sandboxed app's notices can supply (packaging/mac/collect_notices.py)."""

from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "packaging" / "mac" / "collect_notices.py"
_spec = importlib.util.spec_from_file_location("collect_notices", SCRIPT)
assert _spec and _spec.loader
notices = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(notices)

# the licence Homebrew gives qtbase (brew info --json=v2 qtbase)
QT_LICENCE = ("(LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only) AND "
              "(GPL-3.0-only WITH Qt-GPL-exception-1.0) AND BSD-3-Clause AND "
              "GFDL-1.3-no-invariants-only")


def test_the_qt_licence_has_a_text_for_every_part() -> None:
    found, uncovered = notices.standard_texts(QT_LICENCE)
    assert uncovered == []
    names = {path.name for path in found}
    assert {"Qt-GPL-exception-1.0.txt", "BSD-3-Clause.txt", "GFDL-1.3-no-invariants-only.txt",
            "GPL-2.0.txt", "GPL-3.0.txt", "LGPL-3.0.txt"} <= names


def test_each_listed_text_is_there_and_is_the_real_licence() -> None:
    for file in notices.STANDARD_TEXTS.values():
        path = notices.STANDARD / file
        assert path.is_file() and path.stat().st_size > 500, file
    files = ("Qt-GPL-exception-1.0.txt", "GFDL-1.3-no-invariants-only.txt", "BSD-3-Clause.txt")
    text = {file: (notices.STANDARD / file).read_text(errors="replace") for file in files}
    assert "The Qt Company GPL Exception 1.0" in text["Qt-GPL-exception-1.0.txt"]
    assert "GNU Free Documentation License" in text["GFDL-1.3-no-invariants-only.txt"]
    assert "Redistribution and use in source and binary forms" in text["BSD-3-Clause.txt"]


def test_a_licence_with_no_text_is_reported_not_guessed() -> None:
    found, uncovered = notices.standard_texts("Some-Licence-1.0 AND GPL-3.0-only")
    assert uncovered == ["Some-Licence-1.0"]
    assert [path.name for path in found] == ["GPL-3.0.txt"]
