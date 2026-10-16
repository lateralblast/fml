"""Tests on real MKV files made with ffmpeg and inspected with mkvmerge."""

from __future__ import annotations

import json
import os
import stat
from pathlib import Path
from typing import Callable

import pytest
from conftest import (
    Ran,
    leftovers,
    mkv_json,
    mkv_layout,
    mkv_names,
    requires_tools,
    same_file,
)

import fml

pytestmark = [pytest.mark.real, requires_tools]

FILM = ["a:ita*", "a:eng", "a:fre", "s:eng", "s:engF", "s:ita"]


def test_the_fixture_is_what_the_tests_expect(film_mkv: Path) -> None:
    assert mkv_layout(film_mkv) == FILM


# Reading ---------------------------------------------------------------------

def test_info_lists_the_tracks(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--info", "-i", film_mkv)
    rows = [line.split() for line in ran.out.splitlines()]
    assert rows[0] == [str(film_mkv), "(Matroska)"]
    assert rows[1][:2] == ["Track", "ID"]
    assert [r[0] for r in rows[2:]] == ["video:1", "audio:1", "audio:2", "audio:3",
                                        "subtitles:1", "subtitles:2", "subtitles:3"]
    assert rows[3][3:5] == ["ita", "yes"]
    assert rows[6][:5] == ["subtitles:1", "4", "SubRip/SRT", "eng", "-"]
    assert rows[7][5] == "yes"  # the forced subtitle track


def test_info_json_has_the_tracks(run: Callable[..., Ran], film_mkv: Path) -> None:
    (result,) = json.loads(run("--info", "-i", film_mkv, "-F", "json").out)["results"]
    assert result["data"]["container"] == "Matroska"
    audio = [t for t in result["data"]["tracks"] if t["type"] == "audio"]
    assert [(t["number"], t["language"], t["default"], t["name"]) for t in audio] == [
        (1, "ita", True, "Italian"), (2, "eng", False, "English"), (3, "fre", False, "French")]


def test_get_reports_the_default_language(run: Callable[..., Ran], film_mkv: Path) -> None:
    assert run("--get", "lang", "-i", film_mkv).out == "ita\n"


def test_get_reports_none_when_no_subtitle_is_default(run: Callable[..., Ran],
                                                      film_mkv: Path) -> None:
    assert run("--get", "lang", "--kind", "sub", "-i", film_mkv).out == "none\n"


def test_get_lists_the_tracks_in_a_language(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--get", "lang", "--lang", "English", "-i", film_mkv)
    rows = [line.split() for line in ran.out.splitlines()]
    assert rows[0] == [str(film_mkv)] and [r[0] for r in rows[2:]] == ["audio:2"]


def test_get_in_a_language_the_file_lacks(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--get", "lang", "--lang", "German", "-i", film_mkv)
    assert ran.code == 0 and 'has no audio track with language "ger"' in ran.err


# Setting the default ---------------------------------------------------------

def test_set_default_language(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--set", "lang", "--default", "English", "-i", film_mkv)
    assert ran.code == 0, ran.err
    assert ran.out.splitlines() == [f"{film_mkv}: audio track 1 (ita): default yes -> no",
                                    f"{film_mkv}: audio track 2 (eng): default no -> yes"]
    assert mkv_layout(film_mkv) == ["a:ita", "a:eng*", "a:fre", "s:eng", "s:engF", "s:ita"]


def test_set_default_is_unchanged_the_second_time(run: Callable[..., Ran],
                                                  film_mkv: Path) -> None:
    run("--set", "lang", "--default", "English", "-i", film_mkv)
    before = film_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "English", "-i", film_mkv, "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    assert result["status"] == "unchanged" and result["commands"] == []
    assert film_mkv.read_bytes() == before


@pytest.mark.parametrize("language", ["English", "en", "eng", "ENG", "English "])
def test_set_default_accepts_every_spelling_of_the_language(
        run: Callable[..., Ran], film_mkv: Path, language: str) -> None:
    assert run("--set", "lang", "--default", language, "-i", film_mkv).code == 0
    assert mkv_layout(film_mkv)[:3] == ["a:ita", "a:eng*", "a:fre"]


def test_set_default_for_a_language_that_is_missing(run: Callable[..., Ran],
                                                    film_mkv: Path) -> None:
    before = film_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "Japanese", "-i", film_mkv)
    assert ran.code == 0 and 'has no audio track with language "jpn"' in ran.err
    assert film_mkv.read_bytes() == before


def test_dry_run_changes_nothing(run: Callable[..., Ran], film_mkv: Path) -> None:
    before = film_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "English", "-i", film_mkv, "-T")
    assert ran.out.startswith("[dry-run]")
    assert film_mkv.read_bytes() == before and leftovers(film_mkv.parent) == []


def test_the_original_is_replaced_by_a_checked_copy(run: Callable[..., Ran],
                                                    film_mkv: Path) -> None:
    inode = film_mkv.stat().st_ino
    run("--set", "lang", "--default", "English", "-i", film_mkv)
    assert film_mkv.stat().st_ino != inode  # a new file took its place


def test_in_place_edits_the_same_file(run: Callable[..., Ran], film_mkv: Path) -> None:
    inode = film_mkv.stat().st_ino
    ran = run("--set", "lang", "--default", "English", "-i", film_mkv, "--in-place")
    assert ran.code == 0, ran.err
    assert film_mkv.stat().st_ino == inode
    assert mkv_layout(film_mkv)[:3] == ["a:ita", "a:eng*", "a:fre"]


def test_permissions_and_times_are_kept(run: Callable[..., Ran], film_mkv: Path) -> None:
    film_mkv.chmod(0o640)
    os.utime(film_mkv, (1_600_000_000, 1_600_000_000))
    run("--set", "lang", "--default", "English", "-i", film_mkv)
    info = film_mkv.stat()
    assert stat.S_IMODE(info.st_mode) == 0o640 and int(info.st_mtime) == 1_600_000_000
    assert leftovers(film_mkv.parent) == []


def test_set_the_language_of_a_track(run: Callable[..., Ran], und_mkv: Path) -> None:
    ran = run("--set", "lang", "--lang", "Italian", "--track", "2", "-i", und_mkv)
    assert ran.out.strip() == f"{und_mkv}: audio track 2: language und -> ita"
    assert mkv_layout(und_mkv) == ["a:und*", "a:ita", "a:und"]
    again = run("--set", "lang", "--lang", "Italian", "--track", "2", "-i", und_mkv)
    assert "is already ita" in again.out


def test_set_language_of_the_last_track_and_out_of_range(run: Callable[..., Ran],
                                                         und_mkv: Path) -> None:
    run("--set", "lang", "--lang", "fr", "--track", "last", "-i", und_mkv)
    assert mkv_layout(und_mkv) == ["a:und*", "a:und", "a:fre"]
    ran = run("--set", "lang", "--lang", "fr", "--track", "9", "-i", und_mkv)
    assert "has no audio track 9 (3 audio tracks)" in ran.err


def test_set_default_by_track_number_and_tag_it(run: Callable[..., Ran], und_mkv: Path) -> None:
    ran = run("--set", "default", "--track", "3", "--lang", "English", "-i", und_mkv)
    assert ran.out.splitlines() == [
        f"{und_mkv}: audio track 1 (und): default yes -> no",
        f"{und_mkv}: audio track 3 (und): default no -> yes, language und -> eng"]
    assert mkv_layout(und_mkv) == ["a:und", "a:und", "a:eng*"]


def test_swap_makes_the_second_track_the_only_default(run: Callable[..., Ran],
                                                      film_mkv: Path) -> None:
    run("--set", "default", "--track", "3", "-i", film_mkv)  # a third track is now default
    assert mkv_layout(film_mkv)[:3] == ["a:ita", "a:eng", "a:fre*"]
    ran = run("--swap", "lang", "-i", film_mkv)
    assert ran.code == 0, ran.err
    assert mkv_layout(film_mkv)[:3] == ["a:ita", "a:eng*", "a:fre"]
    assert "unchanged" not in ran.out and "is already the default" in run(
        "--swap", "lang", "-i", film_mkv).out


def test_swap_needs_two_audio_tracks(run: Callable[..., Ran],
                                     make: Callable[..., Path]) -> None:
    path = make("one.mkv", [("eng", None, True)])
    assert "needs at least two audio tracks" in run("--swap", "lang", "-i", path).err


def test_prefer_picks_the_first_language_each_file_has(run: Callable[..., Ran], tmp_path: Path,
                                                       make: Callable[..., Path]) -> None:
    folder = tmp_path / "lib"
    make("A.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    make("B.mkv", [("jpn", None, True), ("fre", None, False)], where=folder)
    make("C.mkv", [("spa", None, True)], where=folder)
    make("D.mkv", [("fre", None, True), ("jpn", None, False)], where=folder)
    ran = run("--set", "lang", "--prefer", "eng,jpn,fre", "-d", folder)
    assert ran.code == 0
    assert mkv_layout(folder / "A.mkv") == ["a:ita", "a:eng*"]
    assert mkv_layout(folder / "B.mkv") == ["a:jpn*", "a:fre"]  # already the default
    assert mkv_layout(folder / "C.mkv") == ["a:spa*"]  # none of them
    assert mkv_layout(folder / "D.mkv") == ["a:fre", "a:jpn*"]
    assert "has no audio track in any of: eng, jpn, fre" in ran.err


# Moving ----------------------------------------------------------------------

def test_move_makes_the_track_first_and_keeps_everything_else(run: Callable[..., Ran],
                                                              film_mkv: Path) -> None:
    before = mkv_json(film_mkv)
    ran = run("--set", "lang", "--default", "English", "--move", "-i", film_mkv)
    assert ran.out.splitlines() == [
        f"{film_mkv}: audio track 1 (ita): default yes -> no",
        f"{film_mkv}: audio track 2 (eng): default no -> yes, moved to the first audio track"]
    assert mkv_layout(film_mkv) == ["a:eng*", "a:ita", "a:fre", "s:eng", "s:engF", "s:ita"]
    after = mkv_json(film_mkv)
    names = [t["properties"].get("track_name") for t in after["tracks"] if t["type"] == "audio"]
    assert names == ["English", "Italian", "French"]  # names travel with their tracks
    assert [c["num_entries"] for c in after["chapters"]] == [c["num_entries"]
                                                             for c in before["chapters"]] == [2]
    assert len(after["attachments"]) == len(before["attachments"]) == 1
    assert after["container"]["properties"]["title"] == "Test Film"
    assert after["tracks"][0]["type"] == "video"  # video stays first
    assert leftovers(film_mkv.parent) == []


def test_move_when_the_track_is_already_first_only_edits_the_flag(run: Callable[..., Ran],
                                                                  film_mkv: Path) -> None:
    run("--set", "lang", "--default", "English", "-i", film_mkv)
    ran = run("--set", "lang", "--default", "Italian", "--move", "-i", film_mkv, "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    assert result["commands"][0][0] == "mkvpropedit"  # no remux
    assert mkv_layout(film_mkv)[:3] == ["a:ita*", "a:eng", "a:fre"]


def test_move_when_it_is_already_first_and_default_is_unchanged(run: Callable[..., Ran],
                                                                film_mkv: Path) -> None:
    ran = run("--set", "lang", "--default", "Italian", "--move", "-i", film_mkv)
    assert ran.out.strip() == (
        f"{film_mkv}: audio track 1 (ita) is already the default and the first track")


def test_move_a_subtitle_track(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--set", "lang", "--default", "Italian", "--kind", "sub", "--move", "-i", film_mkv)
    assert ran.code == 0, ran.err
    assert mkv_layout(film_mkv) == ["a:ita*", "a:eng", "a:fre", "s:ita*", "s:eng", "s:engF"]


def test_move_with_prefer_and_set_default_by_track(run: Callable[..., Ran], und_mkv: Path,
                                                   film_mkv: Path) -> None:
    run("--set", "lang", "--prefer", "jpn,fre", "--move", "-i", film_mkv)
    assert mkv_layout(film_mkv)[:3] == ["a:fre*", "a:ita", "a:eng"]
    ran = run("--set", "default", "--track", "3", "--lang", "English", "--move", "-i", und_mkv)
    assert ran.code == 0, ran.err
    assert mkv_layout(und_mkv) == ["a:eng*", "a:und", "a:und"]


def test_move_dry_run_shows_the_remux(run: Callable[..., Ran], film_mkv: Path) -> None:
    before = film_mkv.read_bytes()
    ran = run("--set", "lang", "--default", "English", "--move", "-i", film_mkv, "-T", "-v")
    assert "--track-order 0:0,0:2,0:1,0:3,0:4,0:5,0:6" in ran.err
    assert "moved to the first audio track" in ran.out
    assert film_mkv.read_bytes() == before


# Subtitles ------------------------------------------------------------------

def test_default_subtitles_prefer_the_one_that_is_not_forced(run: Callable[..., Ran],
                                                             film_mkv: Path) -> None:
    run("--set", "lang", "--default", "English", "--kind", "sub", "-i", film_mkv)
    assert mkv_layout(film_mkv) == ["a:ita*", "a:eng", "a:fre", "s:eng*", "s:engF", "s:ita"]


def test_forced_picks_the_forced_subtitle(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "lang", "--default", "English", "--kind", "sub", "--forced", "-i", film_mkv)
    assert mkv_layout(film_mkv) == ["a:ita*", "a:eng", "a:fre", "s:eng", "s:eng*F", "s:ita"]


def test_default_none_turns_the_subtitles_off(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "lang", "--default", "Italian", "--kind", "sub", "-i", film_mkv)
    assert "s:ita*" in mkv_layout(film_mkv)
    ran = run("--set", "lang", "--default", "none", "--kind", "sub", "-i", film_mkv)
    assert ran.out.strip() == f"{film_mkv}: subtitle track 3 (ita): default yes -> no"
    assert not any(item.endswith("*") for item in mkv_layout(film_mkv)[3:])
    assert mkv_layout(film_mkv)[0] == "a:ita*"  # audio untouched
    assert "there is already no default subtitle track" in run(
        "--set", "lang", "--default", "none", "--kind", "sub", "-i", film_mkv).out


def test_set_a_subtitle_language(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "lang", "--lang", "French", "--track", "3", "--kind", "sub", "-i", film_mkv)
    assert mkv_layout(film_mkv)[3:] == ["s:eng", "s:engF", "s:fre"]


# Removing tracks ------------------------------------------------------------

def test_delete_audio_by_language(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--delete", "lang", "--lang", "eng", "-i", film_mkv)
    assert ran.out.strip() == f"{film_mkv}: removed audio track 2 (eng)"
    assert mkv_layout(film_mkv) == ["a:ita*", "a:fre", "s:eng", "s:engF", "s:ita"]
    assert leftovers(film_mkv.parent) == []


def test_delete_a_track_by_number(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--delete", "track", "--track", "last", "-i", film_mkv)
    assert mkv_layout(film_mkv)[:2] == ["a:ita*", "a:eng"] and len(mkv_layout(film_mkv)) == 5
    run("--delete", "track", "--track", "2", "--kind", "sub", "-i", film_mkv)
    assert mkv_layout(film_mkv) == ["a:ita*", "a:eng", "s:eng", "s:ita"]


def test_preserve_keeps_only_one_language(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--preserve", "lang", "--lang", "fre", "-i", film_mkv)
    assert ran.out.splitlines() == [f"{film_mkv}: removed audio track 1 (ita)",
                                    f"{film_mkv}: removed audio track 2 (eng)",
                                    f"{film_mkv}: audio track 3 (fre): default no -> yes"]
    assert mkv_layout(film_mkv)[0] == "a:fre*" and len(mkv_layout(film_mkv)) == 4


def test_preserve_with_prefer_keeps_the_first_preferred_language(run: Callable[..., Ran],
                                                                 film_mkv: Path) -> None:
    run("--preserve", "lang", "--prefer", "jpn,eng,ita", "-i", film_mkv)
    assert mkv_layout(film_mkv)[0] == "a:eng*" and len(mkv_layout(film_mkv)) == 4


def test_removal_keeps_chapters_attachments_and_the_title(run: Callable[..., Ran],
                                                          film_mkv: Path) -> None:
    run("--delete", "lang", "--lang", "eng", "-i", film_mkv)
    data = mkv_json(film_mkv)
    assert data["chapters"][0]["num_entries"] == 2 and len(data["attachments"]) == 1
    assert data["container"]["properties"]["title"] == "Test Film"


def test_removal_keeps_permissions_and_times(run: Callable[..., Ran], film_mkv: Path) -> None:
    film_mkv.chmod(0o600)
    os.utime(film_mkv, (1_500_000_000, 1_500_000_000))
    run("--delete", "lang", "--lang", "eng", "-i", film_mkv)
    info = film_mkv.stat()
    assert stat.S_IMODE(info.st_mode) == 0o600 and int(info.st_mtime) == 1_500_000_000


def test_the_last_audio_track_is_protected(run: Callable[..., Ran],
                                           make: Callable[..., Path]) -> None:
    path = make("solo.mkv", [("eng", None, True)])
    before = path.read_bytes()
    ran = run("--delete", "lang", "--lang", "eng", "-i", path)
    assert "would remove every audio track" in ran.err and path.read_bytes() == before
    forced = run("--delete", "lang", "--lang", "eng", "--force", "-i", path)
    assert forced.code == 0 and mkv_layout(path) == []


def test_preserve_a_language_the_file_lacks_needs_force(run: Callable[..., Ran],
                                                        film_mkv: Path) -> None:
    before = film_mkv.read_bytes()
    ran = run("--preserve", "lang", "--lang", "spa", "--kind", "sub", "-i", film_mkv)
    assert "so nothing would be left" in ran.err and film_mkv.read_bytes() == before
    run("--preserve", "lang", "--lang", "spa", "--kind", "sub", "--force", "-i", film_mkv)
    assert mkv_layout(film_mkv) == ["a:ita*", "a:eng", "a:fre"]  # every subtitle removed


# Directories and actions together --------------------------------------------

def test_directory_mode_with_a_mix_of_files(run: Callable[..., Ran], tmp_path: Path,
                                            make: Callable[..., Path]) -> None:
    folder = tmp_path / "season"
    first = make("e1.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    second = make("e2.mkv", [("eng", None, True), ("ita", None, False)], where=folder)
    third = make("e3.mkv", [("ita", None, True), ("eng", None, False)], where=folder / "sub")
    (folder / "readme.txt").write_text("not media")
    flat = run("--set", "lang", "--default", "English", "--move", "-d", folder)
    assert flat.code == 0, flat.err
    assert mkv_layout(first) == ["a:eng*", "a:ita"] and mkv_layout(second) == ["a:eng*", "a:ita"]
    assert mkv_layout(third) == ["a:ita*", "a:eng"]  # not recursive
    run("--set", "lang", "--default", "English", "--move", "-d", folder, "-r")
    assert mkv_layout(third) == ["a:eng*", "a:ita"]
    assert (folder / "readme.txt").read_text() == "not media"
    assert leftovers(folder) == []


def test_several_actions_run_in_order(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--set", "lang", "--default", "English", "--delete", "track", "--track", "1",
              "--get", "lang", "-i", film_mkv)
    assert ran.code == 0, ran.err
    assert mkv_layout(film_mkv)[:2] == ["a:eng*", "a:fre"]  # set first, then ita (track 1) removed
    assert ran.out.splitlines()[-1] == "eng"


def test_settings_are_shared_by_every_action(run: Callable[..., Ran], film_mkv: Path) -> None:
    """--lang is one setting for the whole command, so it can't mean two different things."""
    ran = run("--set", "lang", "--default", "English", "--delete", "lang", "--lang", "ita",
              "-i", film_mkv)
    assert ran.code == 2 and "not --default and --lang" in ran.err


def test_json_output_covers_every_file(run: Callable[..., Ran], tmp_path: Path,
                                       make: Callable[..., Path]) -> None:
    folder = tmp_path / "j"
    make("one.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    make("two.mkv", [("jpn", None, True)], where=folder)
    document = json.loads(run("--set", "lang", "--default", "English", "-d", folder,
                              "-T", "-F", "json").out)
    assert document["version"] == fml.__version__
    assert [(Path(r["file"]).name, r["status"]) for r in document["results"]] == [
        ("one.mkv", "changed"), ("two.mkv", "skipped")]


def test_the_exit_status_is_0_when_files_are_only_skipped(run: Callable[..., Ran],
                                                          film_mkv: Path) -> None:
    before = film_mkv.read_bytes()
    assert run("--set", "lang", "--default", "Japanese", "-i", film_mkv).code == 0
    assert same_file(film_mkv, film_mkv) and film_mkv.read_bytes() == before


# Removing the default track ------------------------------------------------------

def test_removing_the_default_audio_track_makes_the_next_one_default(
        run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--delete", "lang", "--lang", "ita", "-i", film_mkv)
    assert ran.out.splitlines() == [f"{film_mkv}: removed audio track 1 (ita)",
                                    f"{film_mkv}: audio track 2 (eng): default no -> yes"]
    assert mkv_layout(film_mkv) == ["a:eng*", "a:fre", "s:eng", "s:engF", "s:ita"]


def test_removing_a_track_that_is_not_the_default_changes_no_flags(run: Callable[..., Ran],
                                                                   film_mkv: Path) -> None:
    ran = run("--delete", "track", "--track", "3", "-i", film_mkv)
    assert ran.out.splitlines() == [f"{film_mkv}: removed audio track 3 (fre)"]
    assert mkv_layout(film_mkv)[:2] == ["a:ita*", "a:eng"]


def test_an_mkv_keeps_having_no_default_subtitle(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "lang", "--default", "Italian", "--kind", "sub", "-i", film_mkv)  # s:ita*
    run("--delete", "lang", "--lang", "ita", "--kind", "sub", "-i", film_mkv)
    assert mkv_layout(film_mkv)[3:] == ["s:eng", "s:engF"]  # none was made default


# Names and the forced flag ------------------------------------------------------

def test_set_a_track_name(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--set", "name", "--track", "2", "--name", "English 5.1", "-i", film_mkv)
    assert ran.out.strip() == f'{film_mkv}: audio track 2 (eng): name "English" -> "English 5.1"'
    assert mkv_names(film_mkv)[:3] == ["Italian", "English 5.1", "French"]
    again = run("--set", "name", "--track", "2", "--name", "English 5.1", "-i", film_mkv)
    assert again.out.strip() == f'{film_mkv}: audio track 2 is already named "English 5.1"'


def test_remove_a_track_name(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "name", "--track", "1", "--name", "", "-i", film_mkv)
    assert mkv_names(film_mkv)[:3] == ["", "English", "French"]
    assert "has no name" in run("--set", "name", "--track", "1", "--name", "", "-i", film_mkv).out


def test_names_may_hold_any_text(run: Callable[..., Ran], film_mkv: Path) -> None:
    for name in ("Français", "日本語", "A \"quoted\" name", "-leading dash", "with  spaces"):
        # the = form lets a name start with a dash, which argparse would take for an option
        assert run("--set", "name", "--track", "3", f"--name={name}", "-i", film_mkv).code == 0
        assert mkv_names(film_mkv)[2] == name
    assert run("--set", "name", "--track", "3", "--name", "-x", "-i", film_mkv).code == 2


def test_set_a_subtitle_name(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "name", "--track", "3", "--kind", "sub", "--name", "Italiano", "-i", film_mkv)
    assert mkv_names(film_mkv)[3:] == ["", "", "Italiano"]


def test_set_and_clear_the_forced_flag(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--set", "forced", "--track", "3", "--kind", "sub", "-i", film_mkv)
    assert ran.out.strip() == f"{film_mkv}: subtitle track 3 (ita): forced no -> yes"
    assert mkv_layout(film_mkv)[3:] == ["s:eng", "s:engF", "s:itaF"]
    assert "is already forced" in run("--set", "forced", "--track", "3", "--kind", "sub",
                                      "-i", film_mkv).out
    run("--set", "unforced", "--track", "2", "--kind", "sub", "-i", film_mkv)
    assert mkv_layout(film_mkv)[3:] == ["s:eng", "s:eng", "s:itaF"]
    assert "is already not forced" in run("--set", "unforced", "--track", "2", "--kind", "sub",
                                          "-i", film_mkv).out


def test_the_forced_flag_of_an_audio_track(run: Callable[..., Ran], film_mkv: Path) -> None:
    run("--set", "forced", "--track", "2", "-i", film_mkv)
    assert mkv_layout(film_mkv)[:3] == ["a:ita*", "a:engF", "a:fre"]


def test_name_and_forced_edits_dry_run_and_json(run: Callable[..., Ran], film_mkv: Path) -> None:
    before = film_mkv.read_bytes()
    ran = run("--set", "name", "--track", "1", "--name", "X", "-i", film_mkv, "-T", "-F", "json")
    (result,) = json.loads(ran.out)["results"]
    assert result["changes"] == ['audio track 1 (ita): name "Italian" -> "X"']
    assert "name=X" in result["commands"][0] and film_mkv.read_bytes() == before


def test_a_name_edit_out_of_range(run: Callable[..., Ran], film_mkv: Path) -> None:
    ran = run("--set", "name", "--track", "9", "--name", "X", "-i", film_mkv)
    assert "has no audio track 9" in ran.err


# Keeping the original ----------------------------------------------------------

def test_backup_keeps_the_original(run: Callable[..., Ran], film_mkv: Path) -> None:
    original = film_mkv.read_bytes()
    inode = film_mkv.stat().st_ino
    ran = run("--set", "lang", "--default", "English", "-i", film_mkv, "--backup")
    backup = Path(str(film_mkv) + fml.BACKUP_MARK)
    assert ran.out.splitlines()[-1] == f"{film_mkv}: the original is kept as {backup}"
    assert backup.read_bytes() == original and backup.stat().st_ino == inode
    assert mkv_layout(backup)[0] == "a:ita*"  # the backup is the file as it was
    assert mkv_layout(film_mkv)[1] == "a:eng*"
    assert leftovers(film_mkv.parent) == []  # backups use their own mark


def test_backup_on_every_kind_of_change(run: Callable[..., Ran], film_mkv: Path) -> None:
    for args in (["--set", "name", "--track", "1", "--name", "X"],
                 ["--set", "lang", "--default", "English", "--move"],
                 ["--delete", "lang", "--lang", "fre"]):
        before = film_mkv.read_bytes()
        run(*args, "-i", film_mkv, "--backup")
        newest = sorted(film_mkv.parent.glob(f"film.mkv{fml.BACKUP_MARK}*"))[-1]
        assert newest.read_bytes() == before, args


def test_backups_are_numbered(run: Callable[..., Ran], film_mkv: Path) -> None:
    for default in ("English", "French", "Italian"):
        run("--set", "lang", "--default", default, "-i", film_mkv, "--backup")
    names = sorted(path.name for path in film_mkv.parent.glob("film.mkv*"))
    assert names == ["film.mkv", "film.mkv.fml-bak", "film.mkv.fml-bak.1", "film.mkv.fml-bak.2"]


def test_in_place_backup_is_a_real_copy(run: Callable[..., Ran], film_mkv: Path) -> None:
    original = film_mkv.read_bytes()
    run("--set", "lang", "--default", "English", "-i", film_mkv, "--in-place", "--backup")
    backup = Path(str(film_mkv) + fml.BACKUP_MARK)
    assert backup.read_bytes() == original
    assert backup.stat().st_ino != film_mkv.stat().st_ino and mkv_layout(film_mkv)[1] == "a:eng*"


def test_backup_dry_run_and_directory_mode(run: Callable[..., Ran], tmp_path: Path,
                                           make: Callable[..., Path]) -> None:
    folder = tmp_path / "d"
    make("a.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    make("b.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    run("--set", "lang", "--default", "English", "-d", folder, "--backup", "-T")
    assert sorted(path.name for path in folder.iterdir()) == ["a.mkv", "b.mkv"]
    run("--set", "lang", "--default", "English", "-d", folder, "--backup")
    assert sorted(path.name for path in folder.iterdir()) == [
        "a.mkv", "a.mkv.fml-bak", "b.mkv", "b.mkv.fml-bak"]
    # the backups are not processed as media files on the next run
    ran = run("--get", "lang", "-d", folder)
    assert [line.split(": ")[1] for line in ran.out.splitlines()] == ["eng", "eng"]


# The log and undoing ------------------------------------------------------------

def test_undo_reverses_stacked_changes_exactly(run: Callable[..., Ran], film_mkv: Path,
                                               tmp_path: Path) -> None:
    log = tmp_path / "changes.jsonl"
    before = (mkv_layout(film_mkv), mkv_names(film_mkv))
    for args in (["--set", "lang", "--default", "English", "--move"],
                 ["--set", "name", "--track", "1", "--name", "Original"],
                 ["--set", "forced", "--track", "1", "--kind", "sub"],
                 ["--set", "lang", "--lang", "Spanish", "--track", "3"]):
        assert run(*args, "-i", film_mkv, "--log", log).code == 0
    assert (mkv_layout(film_mkv), mkv_names(film_mkv)) != before
    assert len(log.read_text().splitlines()) == 4
    ran = run("--undo", log)
    assert ran.code == 0, ran.err
    assert (mkv_layout(film_mkv), mkv_names(film_mkv)) == before
    assert leftovers(film_mkv.parent) == []


def test_undo_a_move_to_a_later_place(run: Callable[..., Ran], film_mkv: Path,
                                      tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "French", "--move", "-i", film_mkv, "--log", log)
    assert mkv_layout(film_mkv)[:3] == ["a:fre*", "a:ita", "a:eng"]
    run("--undo", log)
    assert mkv_layout(film_mkv)[:3] == ["a:ita*", "a:eng", "a:fre"]


def test_undo_keeps_everything_else_in_the_file(run: Callable[..., Ran], film_mkv: Path,
                                                tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "English", "--move", "-i", film_mkv, "--log", log)
    run("--undo", log)
    data = mkv_json(film_mkv)
    assert data["chapters"][0]["num_entries"] == 2 and len(data["attachments"]) == 1
    assert data["container"]["properties"]["title"] == "Test Film"


def test_undo_dry_run_changes_nothing(run: Callable[..., Ran], film_mkv: Path,
                                      tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "English", "-i", film_mkv, "--log", log)
    before = film_mkv.read_bytes()
    ran = run("--undo", log, "-T")
    assert ran.out.startswith("[dry-run]") and film_mkv.read_bytes() == before


def test_undo_dry_run_of_stacked_changes_is_not_alarming(run: Callable[..., Ran],
                                                         film_mkv: Path, tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "English", "-i", film_mkv, "--log", log)
    run("--set", "name", "--track", "1", "--name", "X", "-i", film_mkv, "--log", log)
    ran = run("--undo", log, "-T")
    assert ran.code == 0 and ran.err == ""  # the older change is not reported as a problem
    assert 'name "X" -> "Italian"' in ran.out and "an older change" in ran.out


def test_undo_leaves_a_file_that_has_changed_since(run: Callable[..., Ran], film_mkv: Path,
                                                   tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "English", "-i", film_mkv, "--log", log)
    run("--set", "lang", "--default", "French", "-i", film_mkv)  # not logged
    before = film_mkv.read_bytes()
    ran = run("--undo", log)
    assert "has changed since it was logged, so it was left alone" in ran.err
    assert film_mkv.read_bytes() == before


def test_undo_twice_is_harmless(run: Callable[..., Ran], film_mkv: Path, tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "English", "-i", film_mkv, "--log", log)
    run("--undo", log)
    restored = film_mkv.read_bytes()
    ran = run("--undo", log)
    assert ran.code == 0 and "is already as it was before this change" in ran.out
    assert film_mkv.read_bytes() == restored


def test_undo_cannot_reverse_a_removal_without_a_backup(run: Callable[..., Ran],
                                                        film_mkv: Path, tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--delete", "lang", "--lang", "eng", "-i", film_mkv, "--log", log)
    before = film_mkv.read_bytes()
    ran = run("--undo", log)
    assert ran.code == 0 and "can't be undone without the backup it would need" in ran.err
    assert film_mkv.read_bytes() == before


def test_undo_restores_a_removal_from_its_backup_exactly(run: Callable[..., Ran],
                                                         film_mkv: Path, tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    original = film_mkv.read_bytes()
    run("--delete", "lang", "--lang", "eng", "-i", film_mkv, "--log", log, "--backup")
    assert film_mkv.read_bytes() != original
    ran = run("--undo", log)
    assert ran.out.strip().startswith(f"{film_mkv}: restored from ")
    assert film_mkv.read_bytes() == original
    assert not Path(str(film_mkv) + fml.BACKUP_MARK).exists()  # the backup was used up


def test_undo_refuses_a_backup_that_is_not_what_was_logged(run: Callable[..., Ran],
                                                           film_mkv: Path,
                                                           tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--delete", "lang", "--lang", "eng", "-i", film_mkv, "--log", log, "--backup")
    backup = Path(str(film_mkv) + fml.BACKUP_MARK)
    run("--set", "lang", "--default", "French", "-i", backup, "--in-place")  # the backup changed
    before = film_mkv.read_bytes()
    ran = run("--undo", log)
    assert "is not what was logged, so it was not used" in ran.err
    assert film_mkv.read_bytes() == before and backup.exists()


def test_undo_scope_is_limited_by_file_and_directory(run: Callable[..., Ran], tmp_path: Path,
                                                     make: Callable[..., Path]) -> None:
    folder = tmp_path / "d"
    first = make("a.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    second = make("b.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    other = make("c.mkv", [("ita", None, True), ("eng", None, False)], where=tmp_path / "e")
    log = tmp_path / "c.jsonl"
    for path in (first, second, other):
        run("--set", "lang", "--default", "English", "-i", path, "--log", log)
    run("--undo", log, "--file", first)
    assert mkv_layout(first)[0] == "a:ita*" and mkv_layout(second)[1] == "a:eng*"
    run("--undo", log, "--dir", folder)
    assert mkv_layout(second)[0] == "a:ita*" and mkv_layout(other)[1] == "a:eng*"
    run("--undo", log)
    assert mkv_layout(other)[0] == "a:ita*"


def test_undo_json_and_a_missing_file(run: Callable[..., Ran], film_mkv: Path,
                                      tmp_path: Path) -> None:
    log = tmp_path / "c.jsonl"
    run("--set", "lang", "--default", "English", "-i", film_mkv, "--log", log)
    document = json.loads(run("--undo", log, "-F", "json").out)
    (result,) = document["results"]
    assert result["action"] == "undo" and result["status"] == "changed"
    film_mkv.unlink()
    assert "no longer exists" in run("--undo", log).err


# Several files at once ------------------------------------------------------------

def test_jobs_on_real_files_match_a_sequential_run(run: Callable[..., Ran], tmp_path: Path,
                                                   make: Callable[..., Path]) -> None:
    sequential, parallel = tmp_path / "seq", tmp_path / "par"
    for folder in (sequential, parallel):
        for number in range(6):
            make(f"e{number}.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    one = run("--set", "lang", "--default", "English", "--move", "-d", sequential)
    four = run("--set", "lang", "--default", "English", "--move", "-d", parallel, "-j", "4")
    assert one.code == four.code == 0
    assert one.out.replace(str(sequential), "X") == four.out.replace(str(parallel), "X")
    for number in range(6):
        assert mkv_layout(sequential / f"e{number}.mkv") == mkv_layout(
            parallel / f"e{number}.mkv") == ["a:eng*", "a:ita"]
    assert leftovers(parallel) == []


def test_jobs_with_backups_and_a_log(run: Callable[..., Ran], tmp_path: Path,
                                     make: Callable[..., Path]) -> None:
    folder, log = tmp_path / "d", tmp_path / "c.jsonl"
    for number in range(5):
        make(f"e{number}.mkv", [("ita", None, True), ("eng", None, False)], where=folder)
    run("--set", "lang", "--default", "English", "--move", "-d", folder, "-j", "3",
        "--backup", "--log", log)
    assert len(list(folder.glob("*.fml-bak"))) == 5 and len(log.read_text().splitlines()) == 5
    run("--undo", log)
    assert all(mkv_layout(folder / f"e{n}.mkv") == ["a:ita*", "a:eng"] for n in range(5))
