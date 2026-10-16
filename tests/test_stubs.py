"""Tests with stub tools: failure paths, safety behaviour and messages that real tools won't
produce on demand (a tool that fails, or exits 0 without doing the work)."""

from __future__ import annotations

import json
import signal
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable

import pytest
from conftest import MP4_HEADER, Ran, Stubs, leftovers, mkv_stub_tracks

import fml

AUDIO3 = [("ita", True), ("eng", False), ("fre", False)]
SUBS2 = [("eng", False), ("ita", False)]


def mi_track(kind: str, order: int, language: str, default: bool) -> dict[str, str]:
    return {"@type": kind, "@typeorder": str(order), "ID": str(order), "Format": "AAC",
            "Language": language, "Default": "Yes" if default else "No"}


@pytest.fixture
def mkv_stubs(stubs: Stubs) -> Stubs:
    stubs.set(mkv=mkv_stub_tracks(AUDIO3, SUBS2))
    return stubs


# A dry run shows the plan and runs nothing that changes ---------------------

def test_dry_run_shows_the_changes_and_the_command(run: Callable[..., Ran], fake_mkv: Path,
                                                   mkv_stubs: Stubs) -> None:
    before = fake_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "-T", "-v")
    assert ran.code == 0
    assert ran.out.splitlines() == [
        f"[dry-run] {fake_mkv}: audio track 1 (ita): default yes -> no",
        f"[dry-run] {fake_mkv}: audio track 2 (eng): default no -> yes"]
    assert "mkvpropedit" in ran.err and "Copying:" in ran.err
    assert fake_mkv.read_bytes() == before
    assert leftovers(fake_mkv.parent) == []


def test_dry_run_json_records_the_plan(run: Callable[..., Ran], fake_mkv: Path,
                                       mkv_stubs: Stubs) -> None:
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "-T", "-F", "json")
    document = json.loads(ran.out)
    assert document["dry_run"] is True and document["version"] == fml.__version__
    (result,) = document["results"]
    assert result["status"] == "changed" and result["dry_run"] is True
    assert result["changes"] == ["audio track 1 (ita): default yes -> no",
                                 "audio track 2 (eng): default no -> yes"]
    assert result["commands"][0][0] == "mkvpropedit"
    assert result["commands"][0][-1] == "flag-default=1"


def test_json_keeps_stdout_clean(run: Callable[..., Ran], fake_mkv: Path,
                                 mkv_stubs: Stubs) -> None:
    ran = run("--set", "lang", "--default", "Spanish", "-i", fake_mkv, "-F", "json", "-v")
    assert ran.code == 0
    (result,) = json.loads(ran.out)["results"]  # only the document is on stdout
    assert result["status"] == "skipped"
    assert result["message"] == 'has no audio track with language "spa"'
    assert ran.err.startswith("Executing:")  # --verbose output goes to stderr


# Failures leave the original alone ------------------------------------------

def test_a_failing_tool_leaves_the_original_and_exits_1(run: Callable[..., Ran],
                                                        fake_mkv: Path, mkv_stubs: Stubs) -> None:
    before = fake_mkv.read_bytes()
    mkv_stubs.set(mkvpropedit_exit=2)
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv)
    assert ran.code == 1
    assert "mkvpropedit failed with exit code 2: mkvpropedit: Error: cannot open" in ran.err
    assert fake_mkv.read_bytes() == before and leftovers(fake_mkv.parent) == []


def test_a_tool_that_does_nothing_is_caught(run: Callable[..., Ran], fake_mkv: Path,
                                            mkv_stubs: Stubs) -> None:
    """mkvpropedit exits 0 but the copy still has the old flags: not applied."""
    before = fake_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv)
    assert ran.code == 1
    assert "mkvpropedit did not make the change to the file" in ran.err
    assert fake_mkv.read_bytes() == before and leftovers(fake_mkv.parent) == []


def test_a_good_edit_replaces_the_file_with_the_checked_copy(
        run: Callable[..., Ran], fake_mkv: Path, mkv_stubs: Stubs) -> None:
    after = mkv_stub_tracks([("ita", False), ("eng", True), ("fre", False)], SUBS2)
    mkv_stubs.set(mkv_after=after)
    fake_mkv.write_bytes(fml_header() + b"original")
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv)
    assert ran.code == 0, ran.err
    assert fake_mkv.read_bytes().endswith(b"original")  # the stub left the copy as it was
    assert leftovers(fake_mkv.parent) == []


def fml_header() -> bytes:
    from conftest import MKV_HEADER
    return MKV_HEADER


def test_a_move_that_keeps_the_old_order_is_caught(run: Callable[..., Ran], fake_mkv: Path,
                                                   mkv_stubs: Stubs) -> None:
    """The flags in the new file are right for the new order, but the tracks were not moved:
    only the order (languages by position) shows it."""
    before = fake_mkv.read_bytes()
    mkv_stubs.set(mkv_after=mkv_stub_tracks([("ita", True), ("eng", False), ("fre", False)],
                                            SUBS2))
    ran = run("--set", "lang", "--default", "English", "--move", "-i", fake_mkv)
    assert ran.code == 1 and "mkvmerge did not make the change to the file" in ran.err
    assert fake_mkv.read_bytes() == before and leftovers(fake_mkv.parent) == []


def test_an_edit_that_leaves_the_language_alone_is_caught(run: Callable[..., Ran],
                                                          fake_mkv: Path,
                                                          mkv_stubs: Stubs) -> None:
    """Asked to retag track 2 as Spanish, the tool changes nothing else, so every default
    flag is as expected and only the language is wrong."""
    before = fake_mkv.read_bytes()
    ran = run("--set", "lang", "--lang", "Spanish", "--track", "2", "-i", fake_mkv)
    assert ran.code == 1 and "mkvpropedit did not make the change to the file" in ran.err
    assert fake_mkv.read_bytes() == before


def test_an_edit_that_sets_the_language_is_accepted(run: Callable[..., Ran], fake_mkv: Path,
                                                    mkv_stubs: Stubs) -> None:
    mkv_stubs.set(mkv_after=mkv_stub_tracks([("ita", True), ("spa", False), ("fre", False)],
                                            SUBS2))
    ran = run("--set", "lang", "--lang", "Spanish", "--track", "2", "-i", fake_mkv)
    assert ran.code == 0, ran.err
    assert ran.out.strip() == f"{fake_mkv}: audio track 2: language eng -> spa"


def test_removal_with_the_wrong_tracks_is_rejected(run: Callable[..., Ran], fake_mkv: Path,
                                                   mkv_stubs: Stubs) -> None:
    """mkvmerge exits 0, but the new file still has the English track."""
    before = fake_mkv.read_bytes()
    ran = run("--delete", "lang", "--lang", "eng", "-i", fake_mkv)
    assert ran.code == 1
    assert "mkvmerge did not make the change to the file" in ran.err
    assert fake_mkv.read_bytes() == before and leftovers(fake_mkv.parent) == []


def test_removal_with_the_right_tracks_is_accepted(run: Callable[..., Ran], fake_mkv: Path,
                                                   mkv_stubs: Stubs) -> None:
    mkv_stubs.set(mkv_after=mkv_stub_tracks([("ita", True), ("fre", False)], SUBS2))
    ran = run("--delete", "lang", "--lang", "eng", "-i", fake_mkv)
    assert ran.code == 0, ran.err
    assert ran.out.strip() == f"{fake_mkv}: removed audio track 2 (eng)"
    assert leftovers(fake_mkv.parent) == []


def test_mkvmerge_warnings_count_as_success(run: Callable[..., Ran], fake_mkv: Path,
                                            mkv_stubs: Stubs) -> None:
    mkv_stubs.set(mkv_after=mkv_stub_tracks([("ita", True), ("fre", False)], SUBS2),
                  mkvmerge_exit=1)
    assert run("--delete", "lang", "--lang", "eng", "-i", fake_mkv).code == 0


def test_mkvmerge_errors_fail_the_file(run: Callable[..., Ran], fake_mkv: Path,
                                       mkv_stubs: Stubs) -> None:
    mkv_stubs.set(mkvmerge_exit=2)
    ran = run("--delete", "lang", "--lang", "eng", "-i", fake_mkv)
    assert ran.code == 1 and "mkvmerge failed with exit code 2" in ran.err
    assert leftovers(fake_mkv.parent) == []


def test_an_mp4_flag_that_ffmpeg_ignores_is_caught(run: Callable[..., Ran], fake_mp4: Path,
                                                   stubs: Stubs) -> None:
    stubs.set(mi=[mi_track("Audio", 1, "it", True), mi_track("Audio", 2, "en", False)])
    ran = run("--set", "lang", "--default", "English", "-i", fake_mp4)
    assert ran.code == 1 and "ffmpeg did not make the change" in ran.err
    assert fake_mp4.read_bytes() == MP4_HEADER and leftovers(fake_mp4.parent) == []


def test_an_mp4_edit_that_ffmpeg_applies_is_accepted(run: Callable[..., Ran], fake_mp4: Path,
                                                     stubs: Stubs) -> None:
    stubs.set(mi=[mi_track("Audio", 1, "it", True), mi_track("Audio", 2, "en", False)],
              mi_after=[mi_track("Audio", 1, "it", False), mi_track("Audio", 2, "en", True)])
    ran = run("--set", "lang", "--default", "English", "-i", fake_mp4)
    assert ran.code == 0, ran.err
    assert leftovers(fake_mp4.parent) == []


def test_a_failing_ffmpeg_is_reported(run: Callable[..., Ran], fake_mp4: Path,
                                      stubs: Stubs) -> None:
    stubs.set(mi=[mi_track("Audio", 1, "it", True), mi_track("Audio", 2, "en", False)],
              ffmpeg_exit=1)
    ran = run("--set", "lang", "--default", "English", "-i", fake_mp4)
    assert ran.code == 1 and "ffmpeg failed with exit code 1: ffmpeg: Invalid data" in ran.err
    assert fake_mp4.read_bytes() == MP4_HEADER and leftovers(fake_mp4.parent) == []


def test_one_bad_file_does_not_stop_the_rest(run: Callable[..., Ran], tmp_path: Path,
                                             mkv_stubs: Stubs) -> None:
    mkv_stubs.set(mkvpropedit_exit=2)
    for name in ("a.mkv", "b.mkv", "c.mkv"):
        (tmp_path / name).write_bytes(fml_header())
    ran = run("--set", "lang", "--default", "English", "-d", tmp_path, "-F", "json")
    results = json.loads(ran.out)["results"]
    assert [r["status"] for r in results] == ["error"] * 3  # each was tried
    assert ran.code == 1


# Safeguards ------------------------------------------------------------------

def test_refuses_to_remove_every_audio_track(run: Callable[..., Ran], fake_mkv: Path,
                                             stubs: Stubs) -> None:
    stubs.set(mkv=mkv_stub_tracks([("eng", True)]))
    ran = run("--delete", "lang", "--lang", "eng", "-i", fake_mkv)
    assert ran.code == 0 and "would remove every audio track (use --force" in ran.err
    assert leftovers(fake_mkv.parent) == []


def test_force_allows_removing_every_audio_track(run: Callable[..., Ran], fake_mkv: Path,
                                                 stubs: Stubs) -> None:
    stubs.set(mkv=mkv_stub_tracks([("eng", True)]), mkv_after=mkv_stub_tracks([]))
    ran = run("--delete", "lang", "--lang", "eng", "--force", "-i", fake_mkv)
    assert ran.code == 0, ran.err
    assert ran.out.strip() == f"{fake_mkv}: removed audio track 1 (eng)"


def test_removing_every_subtitle_track_needs_no_force(run: Callable[..., Ran], fake_mkv: Path,
                                                      stubs: Stubs) -> None:
    stubs.set(mkv=mkv_stub_tracks([("eng", True)], [("eng", False)]),
              mkv_after=mkv_stub_tracks([("eng", True)]))
    ran = run("--delete", "lang", "--lang", "eng", "--kind", "sub", "-i", fake_mkv)
    assert ran.code == 0, ran.err


def test_preserve_refuses_when_nothing_would_be_left(run: Callable[..., Ran], fake_mkv: Path,
                                                     mkv_stubs: Stubs) -> None:
    ran = run("--preserve", "lang", "--lang", "spa", "-i", fake_mkv)
    assert ran.code == 0 and "so nothing would be left (use --force" in ran.err


def test_not_enough_free_space(run: Callable[..., Ran], fake_mkv: Path, mkv_stubs: Stubs,
                               monkeypatch: pytest.MonkeyPatch) -> None:
    class Usage:
        free = 3

    monkeypatch.setattr(fml.shutil, "disk_usage", lambda path: Usage())
    for args in (["--set", "lang", "--default", "English"],
                 ["--delete", "lang", "--lang", "eng"],
                 ["--set", "lang", "--default", "English", "--move"]):
        ran = run(*args, "-i", fake_mkv)
        assert ran.code == 1 and "not enough free space" in ran.err, args
        assert f"needs {fml.human_size(fake_mkv.stat().st_size)}" in ran.err
        assert "3 bytes free" in ran.err
    assert "--in-place" in run("--set", "lang", "--default", "English", "-i", fake_mkv).err
    assert leftovers(fake_mkv.parent) == []


def test_in_place_needs_no_free_space(run: Callable[..., Ran], fake_mkv: Path, mkv_stubs: Stubs,
                                      monkeypatch: pytest.MonkeyPatch) -> None:
    mkv_stubs.set(mkv_after=mkv_stub_tracks([("ita", False), ("eng", True), ("fre", False)],
                                            SUBS2))
    monkeypatch.setattr(fml.shutil, "disk_usage", lambda path: (_ for _ in ()).throw(OSError))
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "--in-place", "-v")
    assert ran.code == 0, ran.err
    assert "Copying:" not in ran.err
    assert f"mkvpropedit {fake_mkv} --edit" in ran.err  # edited the file itself


def test_a_dry_run_also_checks_the_space(run: Callable[..., Ran], fake_mkv: Path,
                                         mkv_stubs: Stubs, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(fml.shutil, "disk_usage",
                        lambda path: type("U", (), {"free": 0})())
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "-T")
    assert ran.code == 1 and "not enough free space" in ran.err


# Stopping part way ----------------------------------------------------------

def stop_during(monkeypatch: pytest.MonkeyPatch, tool: str, exception: BaseException) -> None:
    """Make the named tool raise as if the user pressed Ctrl-C while it ran."""
    real = subprocess.run

    def run_or_stop(cmd: Any, *args: Any, **kwargs: Any) -> Any:
        if cmd and Path(str(cmd[0])).name == tool:
            raise exception
        return real(cmd, *args, **kwargs)

    monkeypatch.setattr(fml.subprocess, "run", run_or_stop)


@pytest.mark.parametrize("tool, args", [
    ("mkvpropedit", ["--set", "lang", "--default", "English"]),
    ("mkvmerge", ["--delete", "lang", "--lang", "eng"]),
    ("mkvmerge", ["--set", "lang", "--default", "English", "--move"]),
])
def test_interrupting_removes_the_temporary_copy(run: Callable[..., Ran], fake_mkv: Path,
                                                 mkv_stubs: Stubs, monkeypatch: pytest.MonkeyPatch,
                                                 tool: str, args: list[str]) -> None:
    before = fake_mkv.read_bytes()
    real = subprocess.run

    def stop(cmd: Any, *a: Any, **kw: Any) -> Any:
        if cmd and Path(str(cmd[0])).name == tool and ("-o" in cmd or tool == "mkvpropedit"):
            # the copy or the new file exists when the tool is stopped
            Path(cmd[1] if tool == "mkvpropedit" else cmd[2]).write_bytes(b"partial")
            raise KeyboardInterrupt
        return real(cmd, *a, **kw)

    monkeypatch.setattr(fml.subprocess, "run", stop)
    ran = run(*args, "-i", fake_mkv)
    assert ran.code == 130 and "interrupted" in ran.err
    assert fake_mkv.read_bytes() == before
    assert leftovers(fake_mkv.parent) == []


def test_interrupting_still_prints_json(run: Callable[..., Ran], fake_mkv: Path,
                                        mkv_stubs: Stubs, monkeypatch: pytest.MonkeyPatch) -> None:
    stop_during(monkeypatch, "mkvpropedit", KeyboardInterrupt())
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "-F", "json")
    assert ran.code == 130
    assert json.loads(ran.out)["results"] == []


def test_sigterm_is_handled_like_ctrl_c(run: Callable[..., Ran], fake_mkv: Path, mkv_stubs: Stubs,
                                        monkeypatch: pytest.MonkeyPatch) -> None:
    def terminate_during_edit(cmd: Any, *a: Any, **kw: Any) -> Any:
        signal.getsignal(signal.SIGTERM)(signal.SIGTERM, None)  # as if `kill` arrived
        raise AssertionError("not reached")

    stop_during(monkeypatch, "mkvpropedit", KeyboardInterrupt())
    monkeypatch.setattr(fml.subprocess, "run", terminate_during_edit)
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv)
    assert ran.code == 130
    assert leftovers(fake_mkv.parent) == []


def test_main_installs_a_sigterm_handler(run: Callable[..., Ran]) -> None:
    run("--version")
    assert signal.getsignal(signal.SIGTERM) is fml.terminate


# Directories ----------------------------------------------------------------

def test_leftover_temporary_files_are_not_treated_as_media(run: Callable[..., Ran],
                                                           tmp_path: Path,
                                                           mkv_stubs: Stubs) -> None:
    (tmp_path / "a.mkv").write_bytes(fml_header())
    (tmp_path / "a.mkv.fml-tmp").write_bytes(fml_header())
    (tmp_path / "b.fml-tmp.mp4").write_bytes(MP4_HEADER)
    ran = run("--info", "-d", tmp_path)
    assert ran.out.count("(Matroska)") == 1 and "fml-tmp" not in ran.out


def test_directory_mode_skips_other_files_quietly(run: Callable[..., Ran], tmp_path: Path,
                                                  mkv_stubs: Stubs) -> None:
    (tmp_path / "a.mkv").write_bytes(fml_header())
    (tmp_path / "notes.txt").write_text("not media")
    (tmp_path / "sub").mkdir()
    (tmp_path / "sub" / "b.mkv").write_bytes(fml_header())
    flat = run("--get", "lang", "-d", tmp_path)
    assert flat.out.splitlines() == [f"{tmp_path / 'a.mkv'}: ita"] and flat.err == ""
    deep = run("--get", "lang", "-d", tmp_path, "-r")
    assert deep.out.splitlines() == [f"{tmp_path / 'a.mkv'}: ita",
                                     f"{tmp_path / 'sub' / 'b.mkv'}: ita"]


def test_directory_mode_is_in_sorted_order(run: Callable[..., Ran], tmp_path: Path,
                                           mkv_stubs: Stubs) -> None:
    for name in ("c.mkv", "a.mkv", "b.mkv"):
        (tmp_path / name).write_bytes(fml_header())
    names = [line.split(":")[0] for line in run("--get", "lang", "-d", tmp_path).out.splitlines()]
    assert names == [str(tmp_path / n) for n in ("a.mkv", "b.mkv", "c.mkv")]


def test_a_missing_directory_is_an_error(run: Callable[..., Ran], tmp_path: Path,
                                         mkv_stubs: Stubs) -> None:
    ran = run("--info", "-d", tmp_path / "nowhere")
    assert ran.code == 1 and "does not exist" in ran.err


@pytest.mark.parametrize("name, content, message", [
    ("missing.mkv", None, "does not exist"),
    ("notes.txt", b"hello", "is not a supported media file"),
    ("empty.mkv", b"", "is not a supported media file"),
])
def test_a_bad_file_is_an_error(run: Callable[..., Ran], tmp_path: Path, mkv_stubs: Stubs,
                                name: str, content: bytes | None, message: str) -> None:
    path = tmp_path / name
    if content is not None:
        path.write_bytes(content)
    ran = run("--info", "-i", path)
    assert ran.code == 1 and message in ran.err


# Tools that are not installed -----------------------------------------------

@pytest.fixture
def no_tools(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    empty = tmp_path / "empty"
    empty.mkdir()
    monkeypatch.setenv("PATH", str(empty))
    return empty


def test_check_lists_what_is_missing(run: Callable[..., Ran], no_tools: Path) -> None:
    ran = run("--check")
    assert ran.code == 1
    assert ran.out.splitlines() == [f"Missing:\t{tool}" for tool in fml.REQUIRED_TOOLS]
    assert "missing: mkvmerge, mkvpropedit, mediainfo, ffmpeg (use --install" in ran.err


def test_check_json_lists_the_packages(run: Callable[..., Ran], no_tools: Path) -> None:
    ran = run("--check", "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    assert result["status"] == "error"
    assert {entry["tool"]: entry["package"] for entry in result["data"]} == fml.REQUIRED_TOOLS
    assert all(entry["path"] is None for entry in result["data"])


def test_install_dry_run_installs_each_package_once(run: Callable[..., Ran],
                                                    no_tools: Path) -> None:
    ran = run("--check", "--install", "-T", "-v")
    assert ran.code == 0
    commands = [line.split("\t")[1] for line in ran.err.splitlines()]
    assert commands == ["sudo apt install -y mkvtoolnix", "sudo apt install -y mediainfo",
                        "sudo apt install -y ffmpeg"]


def test_install_on_macos_uses_brew(run: Callable[..., Ran], no_tools: Path,
                                    monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(fml.platform, "system", lambda: "Darwin")
    ran = run("--check", "--install", "-T", "-v")
    assert "brew install mkvtoolnix" in ran.err and "sudo" not in ran.err


def test_install_on_another_system_says_so(run: Callable[..., Ran], no_tools: Path,
                                           monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(fml, "detect_distro", lambda: "Arch")
    ran = run("--check", "--install", "-T")
    assert "don't know how to install \"mkvtoolnix\" on this OS" in ran.err


def test_an_action_names_the_commands_it_needs(run: Callable[..., Ran], fake_mkv: Path,
                                               fake_mp4: Path, no_tools: Path) -> None:
    mkv = run("--set", "lang", "--default", "English", "-i", fake_mkv)
    assert mkv.code == 1 and "command not found: mkvmerge, mkvpropedit" in mkv.err
    mp4 = run("--get", "lang", "-i", fake_mp4)
    assert mp4.code == 1 and "command not found: mediainfo" in mp4.err
    assert "mkvmerge" not in mp4.err  # only what an MP4 read needs


def test_each_action_only_needs_its_own_tools(run: Callable[..., Ran], fake_mkv: Path,
                                              stubs: Stubs, tmp_path: Path,
                                              monkeypatch: pytest.MonkeyPatch) -> None:
    (stubs.folder / "ffmpeg").unlink()
    (stubs.folder / "mediainfo").unlink()
    stubs.set(mkv=mkv_stub_tracks(AUDIO3))
    assert run("--get", "lang", "-i", fake_mkv).code == 0  # no ffmpeg or mediainfo needed


# Names are checked in the new file --------------------------------------------

def mi_named(kind: str, order: int, language: str, default: bool, title: str) -> dict[str, str]:
    track = mi_track(kind, order, language, default)
    if title:
        track["Title"] = title
    return track


def test_an_mp4_that_loses_a_track_name_is_caught(run: Callable[..., Ran], fake_mp4: Path,
                                                  stubs: Stubs) -> None:
    """The flags are right in the new file, but the name has gone: not replaced."""
    stubs.set(mi=[mi_named("Audio", 1, "it", True, "Italiano"),
                  mi_named("Audio", 2, "en", False, "English")],
              mi_after=[mi_named("Audio", 1, "it", False, "Italiano"),
                        mi_named("Audio", 2, "en", True, "")])
    ran = run("--set", "lang", "--default", "English", "-i", fake_mp4)
    assert ran.code == 1 and "ffmpeg did not make the change to the file" in ran.err
    assert fake_mp4.read_bytes() == MP4_HEADER


def test_an_mp4_that_keeps_its_names_is_accepted(run: Callable[..., Ran], fake_mp4: Path,
                                                 stubs: Stubs) -> None:
    stubs.set(mi=[mi_named("Audio", 1, "it", True, "Italiano"),
                  mi_named("Audio", 2, "en", False, "English")],
              mi_after=[mi_named("Audio", 1, "it", False, "Italiano"),
                        mi_named("Audio", 2, "en", True, "English")])
    assert run("--set", "lang", "--default", "English", "-i", fake_mp4).code == 0


def test_the_ffmpeg_command_writes_the_names_back(run: Callable[..., Ran], fake_mp4: Path,
                                                  stubs: Stubs) -> None:
    stubs.set(mi=[mi_named("Video", 1, "", False, "Film"),
                  mi_named("Audio", 1, "it", True, "Italiano"),
                  mi_named("Audio", 2, "en", False, "English")])
    ran = run("--set", "lang", "--default", "English", "--move", "-i", fake_mp4, "-T", "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    command = result["commands"][0]
    assert command[command.index("-metadata:s:v:0") + 1] == "title=Film"
    assert command[command.index("-metadata:s:a:0") + 1] == "title=English"  # moved first
    assert command[command.index("-metadata:s:a:1") + 1] == "title=Italiano"


def test_a_removal_that_loses_names_is_caught(run: Callable[..., Ran], fake_mkv: Path,
                                              stubs: Stubs) -> None:
    named = mkv_stub_tracks(AUDIO3)
    named[1]["properties"]["track_name"] = "Italiano"
    after = mkv_stub_tracks([("ita", True), ("fre", False)])  # the name went missing
    stubs.set(mkv=named, mkv_after=after)
    ran = run("--delete", "lang", "--lang", "eng", "-i", fake_mkv)
    assert ran.code == 1 and "did not make the change" in ran.err


# Removing the default track ----------------------------------------------------

def test_removing_the_default_audio_makes_the_next_one_default(
        run: Callable[..., Ran], fake_mkv: Path, stubs: Stubs) -> None:
    stubs.set(mkv=mkv_stub_tracks([("ita", True), ("eng", False), ("fre", False)]),
              mkv_after=mkv_stub_tracks([("eng", True), ("fre", False)]))
    ran = run("--delete", "lang", "--lang", "ita", "-i", fake_mkv, "-T", "-v")
    assert ran.out.splitlines() == [f"[dry-run] {fake_mkv}: removed audio track 1 (ita)",
                                    f"[dry-run] {fake_mkv}: audio track 2 (eng): default no -> yes"]
    assert "--default-track-flag 2:1" in ran.err
    assert run("--delete", "lang", "--lang", "ita", "-i", fake_mkv).code == 0


def test_removing_a_track_that_is_not_the_default_promotes_nothing(
        run: Callable[..., Ran], fake_mkv: Path, stubs: Stubs) -> None:
    stubs.set(mkv=mkv_stub_tracks(AUDIO3))
    ran = run("--delete", "lang", "--lang", "eng", "-i", fake_mkv, "-T", "-v")
    assert ran.out.splitlines() == [f"[dry-run] {fake_mkv}: removed audio track 2 (eng)"]
    assert "--default-track-flag" not in ran.err


def test_an_mkv_may_be_left_with_no_default_subtitle(run: Callable[..., Ran], fake_mkv: Path,
                                                     stubs: Stubs) -> None:
    stubs.set(mkv=mkv_stub_tracks(AUDIO3, [("eng", True), ("ita", False)]))
    ran = run("--delete", "lang", "--lang", "eng", "--kind", "sub", "-i", fake_mkv, "-T")
    assert ran.out.splitlines() == [f"[dry-run] {fake_mkv}: removed subtitle track 1 (eng)"]


def test_an_mp4_always_gets_a_default_subtitle(run: Callable[..., Ran], fake_mp4: Path,
                                               stubs: Stubs) -> None:
    stubs.set(mi=[mi_track("Audio", 1, "it", True), mi_track("Text", 1, "en", True),
                  mi_track("Text", 2, "it", False)])
    ran = run("--delete", "lang", "--lang", "eng", "--kind", "sub", "-i", fake_mp4, "-T")
    assert ran.out.splitlines() == [f"[dry-run] {fake_mp4}: removed subtitle track 1 (eng)",
                                    f"[dry-run] {fake_mp4}: subtitle track 2 (ita): "
                                    "default no -> yes"]


# Backups -----------------------------------------------------------------------

def good_edit(stubs: Stubs) -> None:
    stubs.set(mkv_after=mkv_stub_tracks([("ita", False), ("eng", True), ("fre", False)], SUBS2))


def test_backup_keeps_the_original_as_a_hard_link(run: Callable[..., Ran], fake_mkv: Path,
                                                  mkv_stubs: Stubs) -> None:
    good_edit(mkv_stubs)
    fake_mkv.write_bytes(fake_mkv.read_bytes() + b"original")
    inode = fake_mkv.stat().st_ino
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "--backup")
    backup = Path(str(fake_mkv) + fml.BACKUP_MARK)
    assert ran.code == 0, ran.err
    assert ran.out.splitlines()[-1] == f"{fake_mkv}: the original is kept as {backup}"
    assert backup.exists() and backup.stat().st_ino == inode and backup.stat().st_nlink == 1
    assert leftovers(fake_mkv.parent) == []


def test_backup_json_names_the_backup(run: Callable[..., Ran], fake_mkv: Path,
                                      mkv_stubs: Stubs) -> None:
    good_edit(mkv_stubs)
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "--backup", "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    assert result["backup"] == str(fake_mkv) + fml.BACKUP_MARK
    plain = json.loads(run("--set", "lang", "--default", "Spanish", "-i", fake_mkv, "-F",
                           "json").out)["results"][0]
    assert plain["backup"] is None


def test_backup_dry_run_only_plans_it(run: Callable[..., Ran], fake_mkv: Path,
                                      mkv_stubs: Stubs) -> None:
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "--backup", "-T")
    assert ran.out.splitlines()[-1] == (
        f"[dry-run] {fake_mkv}: the original would be kept as {fake_mkv}{fml.BACKUP_MARK}")
    assert not Path(str(fake_mkv) + fml.BACKUP_MARK).exists()


def test_backup_falls_back_to_a_copy_when_links_are_not_possible(
        run: Callable[..., Ran], fake_mkv: Path, mkv_stubs: Stubs,
        monkeypatch: pytest.MonkeyPatch) -> None:
    good_edit(mkv_stubs)
    monkeypatch.setattr(fml.os, "link", lambda *a, **k: (_ for _ in ()).throw(OSError("no links")))
    before = fake_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "--backup")
    backup = Path(str(fake_mkv) + fml.BACKUP_MARK)
    assert ran.code == 0, ran.err
    assert backup.read_bytes() == before and backup.stat().st_ino != fake_mkv.stat().st_ino


def test_a_backup_that_cannot_be_made_stops_the_replacement(
        run: Callable[..., Ran], fake_mkv: Path, mkv_stubs: Stubs,
        monkeypatch: pytest.MonkeyPatch) -> None:
    good_edit(mkv_stubs)
    before = fake_mkv.read_bytes()

    real_copy = fml.shutil.copy2
    copies = []

    def refuse(*args: Any, **kwargs: Any) -> None:
        raise OSError(28, "No space left on device")

    def copy_once(*args: Any, **kwargs: Any) -> Any:
        copies.append(args)
        if len(copies) > 1:  # the first copy is the one that is edited; the second is the backup
            refuse()
        return real_copy(*args, **kwargs)

    monkeypatch.setattr(fml.os, "link", refuse)
    monkeypatch.setattr(fml.shutil, "copy2", copy_once)
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv, "--backup")
    assert ran.code == 1 and "could not keep a backup: No space left on device" in ran.err
    assert fake_mkv.read_bytes() == before and leftovers(fake_mkv.parent) == []
    assert not Path(str(fake_mkv) + fml.BACKUP_MARK).exists()


def test_backup_is_not_made_when_nothing_changes(run: Callable[..., Ran], fake_mkv: Path,
                                                 mkv_stubs: Stubs) -> None:
    ran = run("--set", "lang", "--default", "Italian", "-i", fake_mkv, "--backup")
    assert "is already the default" in ran.out
    assert not Path(str(fake_mkv) + fml.BACKUP_MARK).exists()


def test_backups_are_not_treated_as_media(run: Callable[..., Ran], tmp_path: Path,
                                          mkv_stubs: Stubs) -> None:
    (tmp_path / "a.mkv").write_bytes(fml_header())
    (tmp_path / f"a.mkv{fml.BACKUP_MARK}").write_bytes(fml_header())
    (tmp_path / f"a.mkv{fml.BACKUP_MARK}.1").write_bytes(fml_header())
    assert run("--info", "-d", tmp_path).out.count("(Matroska)") == 1


# The log -----------------------------------------------------------------------

def test_log_lines_describe_the_change(run: Callable[..., Ran], fake_mkv: Path,
                                       mkv_stubs: Stubs, tmp_path: Path) -> None:
    good_edit(mkv_stubs)
    mkv_stubs.set(mark=True)
    log = tmp_path / "changes.jsonl"
    run("--set", "lang", "--default", "English", "-i", fake_mkv, "--log", log)
    (line,) = log.read_text().splitlines()
    entry = json.loads(line)
    assert entry["file"] == str(fake_mkv.resolve()) and entry["action"] == "set"
    assert entry["kind"] == "audio" and entry["version"] == fml.__version__
    assert entry["changes"] == ["audio track 1 (ita): default yes -> no",
                                "audio track 2 (eng): default no -> yes"]
    assert [t["default"] for t in entry["before"] if t["type"] == "audio"] == [True, False, False]
    assert [t["default"] for t in entry["after"] if t["type"] == "audio"] == [False, True, False]
    assert entry["backup"] is None and entry["moved_from"] is None and "time" in entry


def test_log_is_added_to_not_replaced(run: Callable[..., Ran], fake_mkv: Path, tmp_path: Path,
                                      mkv_stubs: Stubs) -> None:
    log = tmp_path / "changes.jsonl"
    log.write_text('{"earlier": "entry"}\n')
    good_edit(mkv_stubs)
    run("--set", "lang", "--default", "English", "-i", fake_mkv, "--log", log)
    lines = log.read_text().splitlines()
    assert lines[0] == '{"earlier": "entry"}' and len(lines) == 2


@pytest.mark.parametrize("args", [
    ["--set", "lang", "--default", "Spanish"],   # skipped
    ["--get", "lang"], ["--info"],                # nothing changes
    ["--set", "lang", "--default", "Italian"],    # already so
])
def test_nothing_is_logged_when_nothing_changes(run: Callable[..., Ran], fake_mkv: Path,
                                                tmp_path: Path, mkv_stubs: Stubs,
                                                args: list[str]) -> None:
    log = tmp_path / "changes.jsonl"
    run(*args, "-i", fake_mkv, "--log", log)
    assert not log.exists()


def test_a_dry_run_logs_nothing(run: Callable[..., Ran], fake_mkv: Path, tmp_path: Path,
                                mkv_stubs: Stubs) -> None:
    log = tmp_path / "changes.jsonl"
    run("--set", "lang", "--default", "English", "-i", fake_mkv, "--log", log, "-T")
    assert not log.exists()


def test_a_log_that_cannot_be_written_is_a_warning(run: Callable[..., Ran], fake_mkv: Path,
                                                   tmp_path: Path, mkv_stubs: Stubs) -> None:
    good_edit(mkv_stubs)
    ran = run("--set", "lang", "--default", "English", "-i", fake_mkv,
              "--log", tmp_path / "no-such-folder" / "l.jsonl")
    assert ran.code == 0 and "could not write the log" in ran.err  # the change was still made


# --include and --exclude -------------------------------------------------------

@pytest.fixture
def library(tmp_path: Path, mkv_stubs: Stubs) -> Path:
    for name in ("Season 1/e1.mkv", "Season 1/e2.MKV", "Season 1/Extras/bonus.mkv",
                 "Season 2/e1.mkv", "Extras/x.mkv", "top.mkv", "Season 2/clip.mp4"):
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(MP4_HEADER if name.endswith(".mp4") else fml_header())
    mkv_stubs.set(mi=[mi_track("Audio", 1, "it", True)])
    return tmp_path


def listed(run: Callable[..., Ran], folder: Path, *args: object) -> list[str]:
    ran = run("--get", "lang", "-d", folder, "-r", *args)
    return [line.split(":")[0].replace(str(folder) + "/", "") for line in ran.out.splitlines()]


def test_no_filters_finds_everything(run: Callable[..., Ran], library: Path) -> None:
    assert listed(run, library) == [
        "Extras/x.mkv", "Season 1/Extras/bonus.mkv", "Season 1/e1.mkv", "Season 1/e2.MKV",
        "Season 2/clip.mp4", "Season 2/e1.mkv", "top.mkv"]


@pytest.mark.parametrize("filters, expected", [
    (["--include", "*.mkv"], ["Extras/x.mkv", "Season 1/Extras/bonus.mkv", "Season 1/e1.mkv",
                              "Season 1/e2.MKV", "Season 2/e1.mkv", "top.mkv"]),
    (["--include", "*.mp4"], ["Season 2/clip.mp4"]),
    (["--include", "*.mp4", "--include", "top*"], ["Season 2/clip.mp4", "top.mkv"]),
    (["--exclude", "Extras"], ["Season 1/e1.mkv", "Season 1/e2.MKV", "Season 2/clip.mp4",
                               "Season 2/e1.mkv", "top.mkv"]),
    (["--exclude", "Season 1/*"], ["Extras/x.mkv", "Season 2/clip.mp4", "Season 2/e1.mkv",
                                   "top.mkv"]),
    (["--exclude", "*/e1.mkv"], ["Extras/x.mkv", "Season 1/Extras/bonus.mkv", "Season 1/e2.MKV",
                                 "Season 2/clip.mp4", "top.mkv"]),
    (["--include", "Season 1/*", "--exclude", "extras"], ["Season 1/e1.mkv", "Season 1/e2.MKV"]),
    (["--include", "nothing*"], []),
    (["--exclude", "*"], []),
])
def test_filters(run: Callable[..., Ran], library: Path, filters: list[str],
                 expected: list[str]) -> None:
    assert listed(run, library, *filters) == expected


def test_filters_apply_to_changes_too(run: Callable[..., Ran], library: Path,
                                      mkv_stubs: Stubs) -> None:
    ran = run("--set", "lang", "--default", "English", "-d", library, "-r", "--exclude", "Extras",
              "--exclude", "Season 2", "-T", "-F", "json")
    names = [Path(r["file"]).name for r in json.loads(ran.out)["results"]]
    assert names == ["e1.mkv", "e2.MKV", "top.mkv"]


# Several files at once --------------------------------------------------------

@pytest.fixture
def many(tmp_path: Path, mkv_stubs: Stubs) -> list[Path]:
    paths = []
    for number in range(8):
        path = tmp_path / f"e{number}.mkv"
        path.write_bytes(fml_header() + bytes([number]))
        paths.append(path)
    good_edit(mkv_stubs)
    return paths


def test_jobs_give_the_same_output_in_the_same_order(run: Callable[..., Ran], tmp_path: Path,
                                                     many: list[Path]) -> None:
    sequential = run("--set", "lang", "--default", "English", "-d", tmp_path, "-T")
    parallel = run("--set", "lang", "--default", "English", "-d", tmp_path, "-T", "-j", "4")
    assert parallel.out == sequential.out and parallel.code == sequential.code == 0
    assert [line.split(":")[0].split("/")[-1] for line in parallel.out.splitlines()][::2] == [
        f"e{n}.mkv" for n in range(8)]


def test_jobs_make_every_change(run: Callable[..., Ran], tmp_path: Path, many: list[Path]) -> None:
    ran = run("--set", "lang", "--default", "English", "-d", tmp_path, "-j", "3", "-F", "json")
    results = json.loads(ran.out)["results"]
    assert [r["status"] for r in results] == ["changed"] * 8
    assert [Path(r["file"]).name for r in results] == [f"e{n}.mkv" for n in range(8)]
    assert leftovers(tmp_path) == []


def test_jobs_log_every_change_as_valid_json(run: Callable[..., Ran], tmp_path: Path,
                                             many: list[Path]) -> None:
    log = tmp_path.parent / f"{tmp_path.name}.jsonl"
    run("--set", "lang", "--default", "English", "-d", tmp_path, "-j", "4", "--log", log)
    entries = [json.loads(line) for line in log.read_text().splitlines()]
    assert sorted(Path(e["file"]).name for e in entries) == [f"e{n}.mkv" for n in range(8)]


def test_jobs_with_failures_report_each_in_order(run: Callable[..., Ran], tmp_path: Path,
                                                 many: list[Path], mkv_stubs: Stubs) -> None:
    mkv_stubs.set(mkvpropedit_exit=2)
    ran = run("--set", "lang", "--default", "English", "-d", tmp_path, "-j", "4", "-F", "json")
    results = json.loads(ran.out)["results"]
    assert [r["status"] for r in results] == ["error"] * 8 and ran.code == 1
    assert leftovers(tmp_path) == []


def test_jobs_do_not_use_up_the_free_space_between_them(
        run: Callable[..., Ran], tmp_path: Path, many: list[Path], mkv_stubs: Stubs,
        monkeypatch: pytest.MonkeyPatch) -> None:
    """Room for one copy at a time: with the space set aside for the first, the others
    must not each find it free."""
    size = many[0].stat().st_size
    mkv_stubs.set(slow=0.6)
    (mkv_stubs.folder / "mkvpropedit").write_text(
        f"#!{sys.executable}\nimport time\ntime.sleep(0.6)\n")
    monkeypatch.setattr(fml.shutil, "disk_usage",
                        lambda path: type("U", (), {"free": int(size * 1.5)})())
    ran = run("--set", "lang", "--default", "English", "-d", tmp_path, "-j", "4", "-F", "json")
    statuses = [r["status"] for r in json.loads(ran.out)["results"]]
    assert statuses.count("changed") == 1
    assert statuses.count("error") == 7
    messages = {r["message"] for r in json.loads(ran.out)["results"] if r["status"] == "error"}
    assert all("not enough free space" in message for message in messages)


def test_space_is_given_back_after_each_file(run: Callable[..., Ran], tmp_path: Path,
                                             many: list[Path],
                                             monkeypatch: pytest.MonkeyPatch) -> None:
    """One at a time, the same room is enough for every file."""
    size = many[0].stat().st_size
    monkeypatch.setattr(fml.shutil, "disk_usage",
                        lambda path: type("U", (), {"free": int(size * 1.5)})())
    ran = run("--set", "lang", "--default", "English", "-d", tmp_path, "-F", "json")
    assert [r["status"] for r in json.loads(ran.out)["results"]] == ["changed"] * 8


def test_an_interrupt_during_parallel_work_stops_cleanly(
        run: Callable[..., Ran], tmp_path: Path, many: list[Path],
        monkeypatch: pytest.MonkeyPatch) -> None:
    real = fml.Fml.process

    def process(self: fml.Fml, task: fml.Task, path: str) -> fml.Result | None:
        if path.endswith("e0.mkv"):
            raise KeyboardInterrupt
        return real(self, task, path)

    monkeypatch.setattr(fml.Fml, "process", process)
    ran = run("--set", "lang", "--default", "English", "-d", tmp_path, "-j", "3")
    assert ran.code == 130 and "interrupted" in ran.err
    assert leftovers(tmp_path) == []


def test_one_job_for_one_file_is_run_directly(run: Callable[..., Ran], fake_mkv: Path,
                                              mkv_stubs: Stubs) -> None:
    assert run("--get", "lang", "-i", fake_mkv, "-j", "8").out == "ita\n"
