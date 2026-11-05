"""Stopping a run with a signal, tested on a real process.

These start the tool as a separate process (``fml.py``, or the Rust build when ``FML_BIN`` is
set), so they cover the signal handling itself and not only what a handler would do. A stub
``mkvpropedit`` that sleeps gives time to send the signal while the copy exists.
"""

from __future__ import annotations

import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import pytest
from conftest import FML_BIN, NO_MKVTOOLNIX, Stubs, leftovers, mkv_stub_tracks

import fml

# The stand-in writes its marker once it is running: the test sends its signal only after that.
SLOW_MKVPROPEDIT = """#!{python}
import pathlib, time
pathlib.Path({marker!r}).write_text("started")
time.sleep(30)
"""


def default_interrupt() -> None:
    """Ctrl-C works as usual in the tool. A background job of a non-interactive shell starts with
    SIGINT ignored, and a child inherits that, so the test resets it (the tool then installs its own
    handler)."""
    signal.signal(signal.SIGINT, signal.SIG_DFL)


def command() -> list[str]:
    return [FML_BIN] if FML_BIN else [sys.executable, str(Path(fml.__file__))]


@pytest.mark.parametrize("sent", [signal.SIGTERM, signal.SIGINT], ids=["SIGTERM", "SIGINT"])
def test_a_signal_during_an_edit_removes_the_copy(fake_mkv: Path, stubs: Stubs,
                                                  sent: signal.Signals) -> None:
    stubs.set(mkv=mkv_stub_tracks([("ita", True), ("eng", False)]))
    slow = stubs.folder / ("ffmpeg" if NO_MKVTOOLNIX else "mkvpropedit")
    marker = stubs.folder / "slow-started"
    slow.write_text(SLOW_MKVPROPEDIT.format(python=sys.executable, marker=str(marker)))
    slow.chmod(0o755)
    before = fake_mkv.read_bytes()
    process = subprocess.Popen(
        [*command(), "--set", "lang", "--default", "English", "-i", str(fake_mkv)],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        preexec_fn=default_interrupt if os.name == "posix" else None)
    try:
        deadline = time.time() + 10
        while not leftovers(fake_mkv.parent) and time.time() < deadline:
            time.sleep(0.05)
        assert leftovers(fake_mkv.parent), "the tool never started working on a copy"
        deadline = time.time() + 10
        while not marker.exists() and time.time() < deadline:
            time.sleep(0.05)
        assert marker.exists(), "the stand-in tool never started"
        process.send_signal(sent)
        _, err = process.communicate(timeout=10)
    finally:
        if process.poll() is None:
            process.kill()
    assert process.returncode == 130 and "interrupted" in err
    assert fake_mkv.read_bytes() == before
    assert leftovers(fake_mkv.parent) == []
