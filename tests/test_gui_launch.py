"""``fml-cli --gui`` hands over to the desktop app. Only the Rust build has the flag, so these run
when ``FML_BIN`` is set. The app is the program called ``fml``. The stand-in app is another copy of
the CLI (scripts called ``fml`` are ignored), which prints its usage when it is started."""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path

import pytest
from conftest import FML_BIN

pytestmark = pytest.mark.skipif(not FML_BIN, reason="--gui is only in the Rust build")

STARTED = "usage: fml"  # what the stand-in prints when it is started


def stand_in(path: Path) -> Path:
    """A program that is not a script: a copy of the CLI, which prints its usage."""
    shutil.copy2(FML_BIN, path)
    return path


@pytest.fixture
def program(tmp_path: Path) -> Path:
    """A copy of the CLI in a folder of its own, so the real app is not beside it."""
    folder = tmp_path / "bin"
    folder.mkdir()
    target = folder / "fml-cli"
    shutil.copy2(FML_BIN, target)
    return target


def launch(program: Path, *args: str, **env: str) -> subprocess.CompletedProcess[str]:
    # on a Mac the app may also be installed (fml.app in /Applications): look nowhere but where
    # a test says, or a real installation would be started in place of the stand-in
    clean = {"PATH": "/usr/bin:/bin", "FML_APP_DIRS": str(program.parent / "no-apps"), **env}
    return subprocess.run([str(program), *args], capture_output=True, text=True, env=clean,
                          check=False)


def test_gui_starts_the_app_beside_the_program(program: Path) -> None:
    stand_in(program.parent / "fml")
    done = launch(program, "--gui")
    assert done.returncode == 0 and done.stdout.startswith(STARTED)


def test_gui_can_be_named_with_fml_gui(program: Path, tmp_path: Path) -> None:
    gui = stand_in(tmp_path / "elsewhere")
    assert launch(program, "--gui", FML_GUI=str(gui)).stdout.startswith(STARTED)


def test_a_script_called_fml_is_not_the_app(program: Path) -> None:
    script = program.parent / "fml"
    script.write_text("#!/bin/sh\necho the wrong program\n")
    script.chmod(script.stat().st_mode | stat.S_IXUSR)
    done = launch(program, "--gui", PATH=str(program.parent) + ":/usr/bin:/bin")
    assert done.returncode == 1 and "was not found" in done.stderr
    assert "wrong program" not in done.stdout


def test_gui_says_so_when_the_app_is_missing(program: Path) -> None:
    done = launch(program, "--gui")
    assert done.returncode == 1 and "desktop app (fml)" in done.stderr
    assert "was not found" in done.stderr


@pytest.mark.skipif(sys.platform != "darwin", reason="fml.app is a Mac thing")
def test_gui_finds_an_installed_app_bundle(program: Path, tmp_path: Path) -> None:
    apps = tmp_path / "apps"
    inside = apps / "fml.app" / "Contents" / "MacOS"
    inside.mkdir(parents=True)
    stand_in(inside / "fml")
    done = launch(program, "--gui", FML_APP_DIRS=str(apps))
    assert done.returncode == 0 and done.stdout.startswith(STARTED)
    # a bundle that is not there is the same as no app
    empty = launch(program, "--gui", FML_APP_DIRS=str(tmp_path / "elsewhere"))
    assert empty.returncode == 1 and "was not found" in empty.stderr


def test_gui_takes_no_other_options(program: Path) -> None:
    done = launch(program, "--gui", "--dryrun")
    assert done.returncode == 2 and "takes no other options" in done.stderr
    assert not os.path.exists(program.parent / "fml")
