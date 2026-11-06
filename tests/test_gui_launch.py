"""``mau-cli --gui`` hands over to the desktop app. Only the Rust build has the flag, so these run
when ``MAU_BIN`` is set. The app is the program called ``mau``. The stand-in app is another copy of
the CLI (scripts called ``mau`` are ignored), which prints its usage when it is started."""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path

import pytest
from conftest import MAU_BIN

pytestmark = pytest.mark.skipif(not MAU_BIN, reason="--gui is only in the Rust build")

STARTED = "usage: mau"  # what the stand-in prints when it is started


def stand_in(path: Path) -> Path:
    """A program that is not a script: a copy of the CLI, which prints its usage."""
    shutil.copy2(MAU_BIN, path)
    return path


@pytest.fixture
def program(tmp_path: Path) -> Path:
    """A copy of the CLI in a folder of its own, so the real app is not beside it."""
    folder = tmp_path / "bin"
    folder.mkdir()
    target = folder / "mau-cli"
    shutil.copy2(MAU_BIN, target)
    return target


def launch(program: Path, *args: str, **env: str) -> subprocess.CompletedProcess[str]:
    # on a Mac the app may also be installed (mau.app in /Applications): look nowhere but where
    # a test says, or a real installation would be started in place of the stand-in
    clean = {"PATH": "/usr/bin:/bin", "MAU_APP_DIRS": str(program.parent / "no-apps"), **env}
    return subprocess.run([str(program), *args], capture_output=True, text=True, env=clean,
                          check=False)


def test_gui_starts_the_app_beside_the_program(program: Path) -> None:
    stand_in(program.parent / "mau")
    done = launch(program, "--gui")
    assert done.returncode == 0 and done.stdout.startswith(STARTED)


def test_gui_can_be_named_with_mau_gui(program: Path, tmp_path: Path) -> None:
    gui = stand_in(tmp_path / "elsewhere")
    assert launch(program, "--gui", MAU_GUI=str(gui)).stdout.startswith(STARTED)


def test_a_script_called_mau_is_not_the_app(program: Path) -> None:
    script = program.parent / "mau"
    script.write_text("#!/bin/sh\necho the wrong program\n")
    script.chmod(script.stat().st_mode | stat.S_IXUSR)
    done = launch(program, "--gui", PATH=str(program.parent) + ":/usr/bin:/bin")
    assert done.returncode == 1 and "was not found" in done.stderr
    assert "wrong program" not in done.stdout


def test_gui_says_so_when_the_app_is_missing(program: Path) -> None:
    done = launch(program, "--gui")
    assert done.returncode == 1 and "desktop app (mau)" in done.stderr
    assert "was not found" in done.stderr


@pytest.mark.skipif(sys.platform != "darwin", reason="mau.app is a Mac thing")
def test_gui_finds_an_installed_app_bundle(program: Path, tmp_path: Path) -> None:
    apps = tmp_path / "apps"
    inside = apps / "mau.app" / "Contents" / "MacOS"
    inside.mkdir(parents=True)
    stand_in(inside / "mau")
    done = launch(program, "--gui", MAU_APP_DIRS=str(apps))
    assert done.returncode == 0 and done.stdout.startswith(STARTED)
    # a bundle that is not there is the same as no app
    empty = launch(program, "--gui", MAU_APP_DIRS=str(tmp_path / "elsewhere"))
    assert empty.returncode == 1 and "was not found" in empty.stderr


@pytest.mark.parametrize("args", [
    ["--gui", "--enable", "search"],
    ["--enable", "search", "--gui"],
    ["--gui", "--enable=search"],
])
def test_gui_passes_enable_search_on_to_the_app(program: Path, args: list[str]) -> None:
    # the stand-in is the CLI, which is given the same options: with no action it reports that
    # the option was read (and no action was asked for), rather than printing its usage
    stand_in(program.parent / "mau")
    done = launch(program, *args)
    assert done.returncode == 2 and "no action given" in done.stderr


@pytest.mark.parametrize("args", [["--gui", "--dryrun"], ["--gui", "--enable", "dupes"]])
def test_gui_takes_no_other_options(program: Path, args: list[str]) -> None:
    done = launch(program, *args)
    assert done.returncode == 2 and "takes no other options than --enable search" in done.stderr
    assert not os.path.exists(program.parent / "mau")
