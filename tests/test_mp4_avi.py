"""Tests on real MP4 and AVI files made with ffmpeg."""

from __future__ import annotations

import json
import os
import stat
import struct
import subprocess
from pathlib import Path
from typing import Callable

import pytest
from conftest import (
    Ran,
    audio_streams,
    leftovers,
    mkv_layout,
    mp4_layout,
    mp4_titles,
    requires_tools,
)

import fml

pytestmark = [pytest.mark.real, requires_tools]

FILM = ["a:ita*", "a:eng", "a:fre", "s:eng*", "s:ita"]


def top_level_boxes(path: Path) -> list[str]:
    data = path.read_bytes()
    boxes, position = [], 0
    while position + 8 <= len(data):
        size, kind = struct.unpack(">I4s", data[position:position + 8])
        boxes.append(kind.decode())
        position += size if size >= 8 else len(data)
    return boxes


def duration(path: Path) -> float:
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration",
                          "-of", "csv=p=0", str(path)], capture_output=True, text=True,
                         check=True).stdout
    return float(out)


def stream_kinds(path: Path) -> list[str]:
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=codec_type",
                          "-of", "csv=p=0", str(path)], capture_output=True, text=True,
                         check=True).stdout
    return out.split()


# MP4 -------------------------------------------------------------------------

def test_the_fixture_is_what_the_tests_expect(film_mp4: Path) -> None:
    assert mp4_layout(film_mp4) == FILM


def test_info(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--info", "-i", film_mp4)
    rows = [line.split() for line in ran.out.splitlines()]
    assert rows[0] == [str(film_mp4), "(MP4)"]
    assert [r[0] for r in rows[2:]] == ["video:1", "audio:1", "audio:2", "audio:3",
                                        "subtitles:1", "subtitles:2"]
    assert rows[3][3:5] == ["ita", "yes"]


def test_get(run: Callable[..., Ran], film_mp4: Path) -> None:
    assert run("--get", "lang", "-i", film_mp4).out == "ita\n"
    assert run("--get", "lang", "--kind", "sub", "-i", film_mp4).out == "eng\n"
    ran = run("--get", "lang", "--lang", "fr", "-i", film_mp4)
    assert ran.out.splitlines()[2].split()[0] == "audio:3"


def test_set_default_language(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--set", "lang", "--default", "English", "-i", film_mp4)
    assert ran.code == 0, ran.err
    assert ran.out.splitlines() == [f"{film_mp4}: audio track 1 (ita): default yes -> no",
                                    f"{film_mp4}: audio track 2 (eng): default no -> yes"]
    assert mp4_layout(film_mp4) == ["a:ita", "a:eng*", "a:fre", "s:eng*", "s:ita"]
    assert leftovers(film_mp4.parent) == []


def test_set_default_is_unchanged_the_second_time(run: Callable[..., Ran],
                                                  film_mp4: Path) -> None:
    run("--set", "lang", "--default", "English", "-i", film_mp4)
    before = film_mp4.read_bytes()
    ran = run("--set", "lang", "--default", "English", "-i", film_mp4)
    assert "is already the default" in ran.out and film_mp4.read_bytes() == before


def test_the_streams_and_duration_are_kept(run: Callable[..., Ran], film_mp4: Path) -> None:
    kinds, length = stream_kinds(film_mp4), duration(film_mp4)
    run("--set", "lang", "--default", "English", "--move", "-i", film_mp4)
    assert stream_kinds(film_mp4) == kinds and duration(film_mp4) == pytest.approx(length, abs=0.05)


def test_permissions_and_times_are_kept(run: Callable[..., Ran], film_mp4: Path) -> None:
    film_mp4.chmod(0o640)
    os.utime(film_mp4, (1_600_000_000, 1_600_000_000))
    run("--set", "lang", "--default", "English", "-i", film_mp4)
    info = film_mp4.stat()
    assert stat.S_IMODE(info.st_mode) == 0o640 and int(info.st_mtime) == 1_600_000_000


def test_the_index_is_moved_to_the_front(run: Callable[..., Ran], film_mp4: Path) -> None:
    assert top_level_boxes(film_mp4).index("moov") > top_level_boxes(film_mp4).index("mdat")
    run("--set", "lang", "--default", "English", "-i", film_mp4)
    boxes = top_level_boxes(film_mp4)
    assert boxes.index("moov") < boxes.index("mdat")


def test_default_subtitles(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--set", "lang", "--default", "Italian", "--kind", "sub", "-i", film_mp4)
    assert ran.code == 0, ran.err
    assert mp4_layout(film_mp4) == ["a:ita*", "a:eng", "a:fre", "s:eng", "s:ita*"]


def test_default_none_is_refused_for_subtitles(run: Callable[..., Ran], film_mp4: Path) -> None:
    before = film_mp4.read_bytes()
    ran = run("--set", "lang", "--default", "none", "--kind", "sub", "-i", film_mp4)
    assert "MP4 files always have a default subtitle track" in ran.err
    assert film_mp4.read_bytes() == before


def test_set_the_language_of_a_track(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--set", "lang", "--lang", "German", "--track", "3", "-i", film_mp4)
    assert ran.out.strip() == f"{film_mp4}: audio track 3: language fre -> ger"
    assert mp4_layout(film_mp4) == ["a:ita*", "a:eng", "a:ger", "s:eng*", "s:ita"]


def test_set_default_by_track_and_tag_it(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--set", "default", "--track", "3", "--lang", "Japanese", "-i", film_mp4)
    assert mp4_layout(film_mp4) == ["a:ita", "a:eng", "a:jpn*", "s:eng*", "s:ita"]


def test_swap_makes_the_second_track_the_only_default(run: Callable[..., Ran],
                                                      film_mp4: Path) -> None:
    run("--set", "default", "--track", "3", "-i", film_mp4)
    run("--swap", "lang", "-i", film_mp4)
    assert mp4_layout(film_mp4)[:3] == ["a:ita", "a:eng*", "a:fre"]


def test_move_the_audio_track_first(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--set", "lang", "--default", "English", "--move", "-i", film_mp4)
    assert ran.out.splitlines() == [
        f"{film_mp4}: audio track 1 (ita): default yes -> no",
        f"{film_mp4}: audio track 2 (eng): default no -> yes, moved to the first audio track"]
    assert mp4_layout(film_mp4) == ["a:eng*", "a:ita", "a:fre", "s:eng*", "s:ita"]


def test_move_a_subtitle_track_first(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--set", "lang", "--default", "Italian", "--kind", "sub", "--move", "-i", film_mp4)
    assert mp4_layout(film_mp4) == ["a:ita*", "a:eng", "a:fre", "s:ita*", "s:eng"]


def test_move_with_a_tag_in_one_rewrite(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--set", "default", "--track", "3", "--lang", "German", "--move", "-i", film_mp4)
    assert mp4_layout(film_mp4) == ["a:ger*", "a:ita", "a:eng", "s:eng*", "s:ita"]


def test_move_when_already_first_is_unchanged(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--set", "lang", "--default", "Italian", "--move", "-i", film_mp4)
    assert "is already the default and the first track" in ran.out


def test_delete_audio_by_language(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--delete", "lang", "--lang", "eng", "-i", film_mp4)
    assert mp4_layout(film_mp4) == ["a:ita*", "a:fre", "s:eng*", "s:ita"]


def test_delete_the_last_subtitle_track(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--delete", "track", "--track", "last", "--kind", "sub", "-i", film_mp4)
    assert mp4_layout(film_mp4) == ["a:ita*", "a:eng", "a:fre", "s:eng*"]


def test_preserve_one_language(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--preserve", "lang", "--lang", "ita", "-i", film_mp4)
    assert mp4_layout(film_mp4) == ["a:ita*", "s:eng*", "s:ita"]


def test_the_last_audio_track_is_protected(run: Callable[..., Ran],
                                           make: Callable[..., Path]) -> None:
    path = make("solo.mp4", [("eng", None, True)])
    before = path.read_bytes()
    ran = run("--delete", "track", "--track", "1", "-i", path)
    assert "would remove every audio track" in ran.err and path.read_bytes() == before


def test_dry_run_changes_nothing_and_shows_the_command(run: Callable[..., Ran],
                                                       film_mp4: Path) -> None:
    before = film_mp4.read_bytes()
    ran = run("--set", "lang", "--default", "English", "--move", "-i", film_mp4, "-T", "-v")
    assert ran.out.startswith("[dry-run]")
    assert "-map 0:a:1 -map 0:a:0 -map 0:a:2" in ran.err
    assert film_mp4.read_bytes() == before and leftovers(film_mp4.parent) == []


def test_json_lists_the_ffmpeg_command(run: Callable[..., Ran], film_mp4: Path) -> None:
    ran = run("--set", "lang", "--default", "English", "-i", film_mp4, "-T", "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    assert result["commands"][0][0] == "ffmpeg" and result["status"] == "changed"


# AVI -------------------------------------------------------------------------

def test_avi_info_and_get(run: Callable[..., Ran], three_avi: Path) -> None:
    ran = run("--info", "-i", three_avi)
    assert ran.out.splitlines()[0] == f"{three_avi} (AVI)"
    assert sum(1 for line in ran.out.splitlines() if line.startswith("audio:")) == 3
    assert run("--get", "lang", "-i", three_avi).out == "und\n"  # no flags: the first track


@pytest.mark.parametrize("args", [
    ["--set", "lang", "--default", "English"],
    ["--set", "lang", "--default", "English", "--move"],
    ["--set", "default", "--track", "2"],
])
def test_avi_set_is_not_supported(run: Callable[..., Ran], three_avi: Path,
                                  args: list[str]) -> None:
    before = three_avi.read_bytes()
    ran = run(*args, "-i", three_avi)
    assert ran.code == 0 and "set is only supported for MKV and MP4 files" in ran.err
    assert three_avi.read_bytes() == before


@pytest.mark.parametrize("args, message", [
    (["--delete", "lang", "--lang", "eng"], "deleting by language is only supported"),
    (["--preserve", "lang", "--lang", "eng"], "preserving by language is only supported"),
])
def test_avi_by_language_is_not_supported(run: Callable[..., Ran], three_avi: Path,
                                          args: list[str], message: str) -> None:
    assert message in run(*args, "-i", three_avi).err


def test_avi_swap_rewrites_the_file(run: Callable[..., Ran], three_avi: Path) -> None:
    three_avi.chmod(0o640)
    os.utime(three_avi, (1_600_000_000, 1_600_000_000))
    before = three_avi.read_bytes()
    ran = run("--swap", "lang", "-i", three_avi)
    assert ran.code == 0, ran.err
    assert ran.out.strip() == f"{three_avi}: audio tracks 1 and 2 swapped"
    assert three_avi.read_bytes() != before and audio_streams(three_avi) == 3
    info = three_avi.stat()
    assert stat.S_IMODE(info.st_mode) == 0o640 and int(info.st_mtime) == 1_600_000_000
    assert leftovers(three_avi.parent) == []


def test_avi_delete_a_track(run: Callable[..., Ran], three_avi: Path) -> None:
    run("--delete", "track", "--track", "last", "-i", three_avi)
    assert audio_streams(three_avi) == 2
    run("--delete", "track", "--track", "1", "-i", three_avi)
    assert audio_streams(three_avi) == 1
    assert leftovers(three_avi.parent) == []


def test_avi_last_audio_track_is_protected(run: Callable[..., Ran], three_avi: Path) -> None:
    run("--delete", "track", "--track", "1", "-i", three_avi)
    run("--delete", "track", "--track", "1", "-i", three_avi)
    before = three_avi.read_bytes()
    ran = run("--delete", "track", "--track", "1", "-i", three_avi)
    assert "would remove every audio track" in ran.err and three_avi.read_bytes() == before
    assert run("--delete", "track", "--track", "1", "--force", "-i", three_avi).code == 0
    assert audio_streams(three_avi) == 0


def test_avi_dry_run(run: Callable[..., Ran], three_avi: Path) -> None:
    before = three_avi.read_bytes()
    ran = run("--swap", "lang", "-i", three_avi, "-T", "-v")
    assert ran.out.startswith("[dry-run]") and "-map 0:a:1 -map 0:a:0" in ran.err
    assert three_avi.read_bytes() == before and leftovers(three_avi.parent) == []


def test_avi_swap_needs_two_audio_tracks(run: Callable[..., Ran],
                                         make: Callable[..., Path]) -> None:
    path = make("one.avi", [("und", None, False)])
    assert "needs at least two audio tracks" in run("--swap", "lang", "-i", path).err


# All three containers together ----------------------------------------------

def test_a_directory_with_every_kind_of_file(run: Callable[..., Ran], tmp_path: Path,
                                             make: Callable[..., Path]) -> None:
    folder = tmp_path / "mixed"
    mkv = make("a.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    mp4 = make("b.mp4", [("ita", None, True), ("eng", None, False)], where=folder)
    avi = make("c.avi", [("und", None, False), ("und", None, False)], where=folder)
    (folder / "d.txt").write_text("not media")
    ran = run("--set", "lang", "--default", "English", "--move", "-d", folder)
    assert ran.code == 0, ran.err
    assert mkv_layout(mkv) == ["a:eng*", "a:ita"]
    assert mp4_layout(mp4) == ["a:eng*", "a:ita"]
    assert audio_streams(avi) == 2  # untouched: AVI can't be set
    assert f"{avi}: set is only supported" in ran.err
    assert leftovers(folder) == []


# Track names ---------------------------------------------------------------------

NAMED_AUDIO = [("ita", "Italiano", True), ("eng", "English 5.1", False), ("fre", "Français", False)]


@pytest.fixture
def named_mp4(make: Callable[..., Path]) -> Path:
    """An MP4 whose video and audio tracks have names (ffmpeg loses these when it rewrites)."""
    return make("named.mp4", NAMED_AUDIO, video_title="Video Title")


def test_the_named_fixture_has_its_names(named_mp4: Path) -> None:
    assert mp4_titles(named_mp4) == ["Video Title", "Italiano", "English 5.1", "Français"]


def handler_names(path: Path) -> list[str]:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "stream_tags=handler_name", "-of", "csv=p=0",
         str(path)], capture_output=True, text=True, check=True).stdout
    return out.split("\n")[:-1]


@pytest.mark.parametrize("args, titles", [
    (["--set", "lang", "--default", "English"],
     ["Video Title", "Italiano", "English 5.1", "Français"]),
    (["--set", "lang", "--default", "French", "--move"],
     ["Video Title", "Français", "Italiano", "English 5.1"]),
    (["--set", "default", "--track", "3", "--lang", "German", "--move"],
     ["Video Title", "Français", "Italiano", "English 5.1"]),
    (["--swap", "lang"], ["Video Title", "Italiano", "English 5.1", "Français"]),
    (["--delete", "lang", "--lang", "ita"], ["Video Title", "English 5.1", "Français"]),
    (["--delete", "track", "--track", "last"], ["Video Title", "Italiano", "English 5.1"]),
    (["--preserve", "lang", "--lang", "eng"], ["Video Title", "English 5.1"]),
])
def test_names_survive_every_kind_of_rewrite(run: Callable[..., Ran], named_mp4: Path,
                                             args: list[str], titles: list[str]) -> None:
    ran = run(*args, "-i", named_mp4)
    assert ran.code == 0, ran.err
    assert mp4_titles(named_mp4) == titles


def test_handler_names_are_left_alone(run: Callable[..., Ran], make: Callable[..., Path]) -> None:
    path = make("handlers.mp4", NAMED_AUDIO)
    before = handler_names(path)
    run("--set", "lang", "--default", "French", "--move", "-i", path)
    assert sorted(handler_names(path)) == sorted(before)
    assert len(before) == 4


def test_set_and_clear_a_name(run: Callable[..., Ran], named_mp4: Path) -> None:
    ran = run("--set", "name", "--track", "2", "--name", "Commentary", "-i", named_mp4)
    expected = f'{named_mp4}: audio track 2 (eng): name "English 5.1" -> "Commentary"'
    assert ran.out.strip() == expected
    assert mp4_titles(named_mp4) == ["Video Title", "Italiano", "Commentary", "Français"]
    run("--set", "name", "--track", "2", "--name", "", "-i", named_mp4)
    assert mp4_titles(named_mp4) == ["Video Title", "Italiano", "", "Français"]


def test_the_forced_flag_cannot_be_set_for_an_mp4(run: Callable[..., Ran], film_mp4: Path) -> None:
    before = film_mp4.read_bytes()
    ran = run("--set", "forced", "--track", "1", "--kind", "sub", "-i", film_mp4)
    assert ran.code == 0 and "the forced flag can only be set for MKV files" in ran.err
    assert film_mp4.read_bytes() == before


# Removing the default track ------------------------------------------------------

def test_removing_the_default_audio_makes_the_next_one_default(run: Callable[..., Ran],
                                                               film_mp4: Path) -> None:
    ran = run("--delete", "lang", "--lang", "ita", "-i", film_mp4)
    assert ran.out.splitlines() == [f"{film_mp4}: removed audio track 1 (ita)",
                                    f"{film_mp4}: audio track 2 (eng): default no -> yes"]
    assert mp4_layout(film_mp4) == ["a:eng*", "a:fre", "s:eng*", "s:ita"]


def test_removing_the_default_subtitle_makes_the_next_one_default(run: Callable[..., Ran],
                                                                  film_mp4: Path) -> None:
    ran = run("--delete", "lang", "--lang", "eng", "--kind", "sub", "-i", film_mp4)
    assert ran.out.splitlines()[-1] == f"{film_mp4}: subtitle track 2 (ita): default no -> yes"
    assert mp4_layout(film_mp4) == ["a:ita*", "a:eng", "a:fre", "s:ita*"]


def test_preserve_leaves_the_kept_track_default(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--preserve", "lang", "--lang", "fre", "-i", film_mp4)
    assert mp4_layout(film_mp4)[0] == "a:fre*"


# Keeping the original and undoing ------------------------------------------------

def test_backup_of_an_mp4_rewrite(run: Callable[..., Ran], film_mp4: Path) -> None:
    original = film_mp4.read_bytes()
    run("--set", "lang", "--default", "English", "-i", film_mp4, "--backup")
    backup = Path(str(film_mp4) + fml.BACKUP_MARK)
    assert backup.read_bytes() == original and mp4_layout(backup) == FILM
    assert mp4_layout(film_mp4)[1] == "a:eng*"
    assert leftovers(film_mp4.parent) == []


def test_undo_stacked_mp4_changes(run: Callable[..., Ran], named_mp4: Path,
                                  tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    layout, titles = mp4_layout(named_mp4), mp4_titles(named_mp4)
    for args in (["--set", "lang", "--default", "French", "--move"],
                 ["--set", "name", "--track", "2", "--name", "Renamed"],
                 ["--set", "lang", "--lang", "German", "--track", "3"]):
        assert run(*args, "-i", named_mp4, "--log", log).code == 0
    assert mp4_layout(named_mp4) != layout
    ran = run("--undo", log)
    assert ran.code == 0, ran.err
    assert mp4_layout(named_mp4) == layout and mp4_titles(named_mp4) == titles


def test_undo_restores_an_mp4_removal_from_its_backup(run: Callable[..., Ran], film_mp4: Path,
                                                      tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    original = film_mp4.read_bytes()
    run("--delete", "lang", "--lang", "eng", "-i", film_mp4, "--backup", "--log", log)
    run("--undo", log)
    assert film_mp4.read_bytes() == original


def test_undo_a_removal_without_a_backup_is_refused(run: Callable[..., Ran], film_mp4: Path,
                                                    tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--delete", "lang", "--lang", "eng", "-i", film_mp4, "--log", log)
    assert "can't be undone without the backup" in run("--undo", log).err


# Several files at once and filters, across containers ---------------------------------

def test_jobs_across_every_kind_of_file(run: Callable[..., Ran], tmp_path: Path,
                                        make: Callable[..., Path]) -> None:
    folder = tmp_path / "mixed"
    mkv = make("a.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    mp4 = make("b.mp4", [("ita", None, True), ("eng", None, False)], where=folder)
    avi = make("c.avi", [("und", None, False), ("und", None, False)], where=folder)
    ran = run("--set", "lang", "--default", "English", "--move", "-d", folder, "-j", "3")
    assert ran.code == 0, ran.err
    assert mkv_layout(mkv) == ["a:eng*", "a:ita"] and mp4_layout(mp4) == ["a:eng*", "a:ita"]
    assert audio_streams(avi) == 2 and f"{avi}: set is only supported" in ran.err


def test_include_picks_a_container(run: Callable[..., Ran], tmp_path: Path,
                                   make: Callable[..., Path]) -> None:
    folder = tmp_path / "mixed"
    mkv = make("a.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    mp4 = make("b.mp4", [("ita", None, True), ("eng", None, False)], where=folder)
    run("--set", "lang", "--default", "English", "-d", folder, "--include", "*.mp4")
    assert mkv_layout(mkv)[0] == "a:ita*" and mp4_layout(mp4)[1] == "a:eng*"


# AVI -------------------------------------------------------------------------------

def test_avi_backup_and_undo_of_a_swap(run: Callable[..., Ran], three_avi: Path,
                                       tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    original = three_avi.read_bytes()
    run("--swap", "lang", "-i", three_avi, "--backup", "--log", log)
    assert three_avi.read_bytes() != original
    assert Path(str(three_avi) + fml.BACKUP_MARK).read_bytes() == original
    run("--undo", log)
    assert three_avi.read_bytes() == original


def test_avi_swap_undo_needs_a_backup(run: Callable[..., Ran], three_avi: Path,
                                      tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--swap", "lang", "-i", three_avi, "--log", log)
    swapped = three_avi.read_bytes()
    ran = run("--undo", log)
    assert "can't be undone without the backup" in ran.err and three_avi.read_bytes() == swapped
