"""Stopping a run with a signal, tested on a real process.

These start the tool as a separate process (``fml.py``, or the Rust build when ``FML_BIN`` is
set), so they cover the signal handling itself and not only what a handler would do. A stub
``mkvpropedit`` that sleeps gives time to send the signal while the copy exists.
"""

from __future__ import annotations

import signal
import subprocess
import sys
import time
from pathlib import Path

import pytest
from conftest import FML_BIN, Stubs, leftovers, mkv_stub_tracks

import fml

SLOW_MKVPROPEDIT = """#!{python}
import time
time.sleep(30)
"""


def command() -> list[str]:
    return [FML_BIN] if FML_BIN else [sys.executable, str(Path(fml.__file__))]


@pytest.mark.parametrize("sent", [signal.SIGTERM, signal.SIGINT], ids=["SIGTERM", "SIGINT"])
def test_a_signal_during_an_edit_removes_the_copy(fake_mkv: Path, stubs: Stubs,
                                                  sent: signal.Signals) -> None:
    stubs.set(mkv=mkv_stub_tracks([("ita", True), ("eng", False)]))
    slow = stubs.folder / "mkvpropedit"
    slow.write_text(SLOW_MKVPROPEDIT.format(python=sys.executable))
    slow.chmod(0o755)
    before = fake_mkv.read_bytes()
    process = subprocess.Popen(
        [*command(), "--set", "lang", "--default", "English", "-i", str(fake_mkv)],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.time() + 10
        while not leftovers(fake_mkv.parent) and time.time() < deadline:
            time.sleep(0.05)
        assert leftovers(fake_mkv.parent), "the tool never started working on a copy"
        time.sleep(0.3)  # let it reach the sleeping mkvpropedit
        process.send_signal(sent)
        _, err = process.communicate(timeout=10)
    finally:
        if process.poll() is None:
            process.kill()
    assert process.returncode == 130 and "interrupted" in err
    assert fake_mkv.read_bytes() == before
    assert leftovers(fake_mkv.parent) == []
