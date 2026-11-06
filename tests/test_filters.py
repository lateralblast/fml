"""Tests of --keep, --delete dupes and --delete match on real files (Rust only)."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Callable

import pytest
from conftest import MAU_BIN, Ran, leftovers, mkv_json, mkv_layout, mp4_layout, requires_tools

pytestmark = [pytest.mark.real, requires_tools,
              pytest.mark.skipif(not MAU_BIN, reason="these switches are Rust only")]

AUDIO = [("eng", "English 5.1", True), ("eng", "Stereo", False), ("jpn", None, False),
         ("ita", None, False)]
SUBS = [("eng", False, False), ("eng", True, False), ("eng", False, False), ("ita", False, False)]


@pytest.fixture
def dupes_mkv(make: Callable[..., Path]) -> Path:
    return make("dupes.mkv", AUDIO, SUBS)


# --keep ----------------------------------------------------------------------

def test_keep_several_languages(run: Callable[..., Ran], dupes_mkv: Path) -> None:
    ran = run("--preserve", "lang", "--keep", "eng,jpn", "-i", dupes_mkv)
    assert ran.code == 0
    assert mkv_layout(dupes_mkv) == ["a:eng*", "a:eng", "a:jpn",
                                     "s:eng", "s:engF", "s:eng", "s:ita"]
    assert not leftovers(dupes_mkv)


def test_keep_works_for_subtitles(run: Callable[..., Ran], dupes_mkv: Path) -> None:
    run("--preserve", "lang", "--keep", "ita", "--kind", "sub", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv) == ["a:eng*", "a:eng", "a:jpn", "a:ita", "s:ita"]


def test_keep_makes_the_next_track_default_when_the_default_goes(
        run: Callable[..., Ran], dupes_mkv: Path) -> None:
    run("--preserve", "lang", "--keep", "jpn,ita", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv)[:2] == ["a:jpn*", "a:ita"]


def test_keep_with_none_of_the_languages_leaves_the_file(run: Callable[..., Ran],
                                                         dupes_mkv: Path) -> None:
    before = mkv_layout(dupes_mkv)
    ran = run("--preserve", "lang", "--keep", "spa,fre", "-i", dupes_mkv)
    assert "nothing would be left" in ran.err
    assert mkv_layout(dupes_mkv) == before


def test_keep_on_an_mp4(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--preserve", "lang", "--keep", "eng,fre", "-i", film_mp4)
    assert mp4_layout(film_mp4)[:2] == ["a:eng*", "a:fre"]


@pytest.mark.parametrize("args,message", [
    (["--preserve", "lang", "--keep", "eng", "--lang", "eng"], "not more than one"),
    (["--preserve", "lang", "--keep", "eng", "--prefer", "eng"], "not more than one"),
    (["--delete", "lang", "--lang", "eng", "--keep", "eng"], "--keep only works with --preserve"),
    (["--preserve", "lang", "--keep", ""], "give at least one language"),
])
def test_keep_rules(run: Callable[..., Ran], dupes_mkv: Path, args: list[str],
                    message: str) -> None:
    ran = run(*args, "-i", dupes_mkv)
    assert ran.code == 2 and message in ran.err


# --delete dupes --------------------------------------------------------------

def test_dupes_keeps_the_first_track_of_each_language(run: Callable[..., Ran],
                                                      dupes_mkv: Path) -> None:
    ran = run("--delete", "dupes", "-i", dupes_mkv)
    assert ran.code == 0 and "removed audio track 2 (eng)" in ran.out
    assert mkv_layout(dupes_mkv) == ["a:eng*", "a:jpn", "a:ita",
                                     "s:eng", "s:engF", "s:eng", "s:ita"]


def test_dupes_on_subtitles_keeps_the_forced_track_apart(run: Callable[..., Ran],
                                                         dupes_mkv: Path) -> None:
    run("--delete", "dupes", "--kind", "sub", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv) == ["a:eng*", "a:eng", "a:jpn", "a:ita",
                                     "s:eng", "s:engF", "s:ita"]


def test_dupes_with_no_duplicates_changes_nothing(run: Callable[..., Ran], film_mkv: Path) -> None:
    before = mkv_layout(film_mkv)
    ran = run("--delete", "dupes", "-i", film_mkv)
    assert ran.code == 0
    assert mkv_layout(film_mkv) == before and not leftovers(film_mkv)


def test_dupes_dry_run_and_undo(run: Callable[..., Ran], dupes_mkv: Path, tmp_path: Path) -> None:
    before = mkv_layout(dupes_mkv)
    run("--delete", "dupes", "--dryrun", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv) == before
    log = tmp_path / "log.jsonl"
    run("--delete", "dupes", "--backup", "--log", log, "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv) != before
    run("--undo", log)
    assert mkv_layout(dupes_mkv) == before


def test_dupes_takes_no_language(run: Callable[..., Ran], dupes_mkv: Path) -> None:
    ran = run("--delete", "dupes", "--lang", "eng", "-i", dupes_mkv)
    assert ran.code == 2 and "takes no --lang" in ran.err


# --delete match --------------------------------------------------------------

def test_match_by_channels(run: Callable[..., Ran], dupes_mkv: Path) -> None:
    # the test tones are mono
    ran = run("--delete", "match", "--channels", "2", "-i", dupes_mkv)
    assert ran.code == 0 and mkv_layout(dupes_mkv)[:4] == ["a:eng*", "a:eng", "a:jpn", "a:ita"]
    run("--delete", "match", "--channels", "1", "--lang", "jpn", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv)[:3] == ["a:eng*", "a:eng", "a:ita"]


def test_match_by_codec_is_a_part_of_the_name_in_any_case(run: Callable[..., Ran],
                                                          dupes_mkv: Path) -> None:
    before = mkv_layout(dupes_mkv)
    run("--delete", "match", "--codec", "dts", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv) == before
    run("--delete", "match", "--codec", "aac", "--lang", "ita", "-i", dupes_mkv)
    assert mkv_layout(dupes_mkv)[:3] == ["a:eng*", "a:eng", "a:jpn"]


def test_match_will_not_remove_the_last_audio_track(run: Callable[..., Ran],
                                                    make: Callable[..., Path]) -> None:
    mkv = make("one.mkv", [("eng", None, True)])
    before = mkv_layout(mkv)
    ran = run("--delete", "match", "--codec", "aac", "-i", mkv)
    assert ran.code == 0 and mkv_layout(mkv) == before
    assert not leftovers(mkv)


def test_match_on_an_mp4(run: Callable[..., Ran], film_mp4: Path) -> None:
    run("--delete", "match", "--lang", "fre", "--codec", "aac", "-i", film_mp4)
    assert mp4_layout(film_mp4)[:2] == ["a:ita*", "a:eng"]


@pytest.mark.parametrize("args,message", [
    (["--delete", "match"], "needs at least one of"),
    (["--delete", "lang", "--lang", "eng", "--codec", "aac"], "only work with --delete match"),
    (["--delete", "match", "--channels", "0"], "is not a number of channels"),
    (["--delete", "match", "--channels", "2", "--kind", "sub"], "only applies to audio"),
])
def test_match_rules(run: Callable[..., Ran], dupes_mkv: Path, args: list[str],
                     message: str) -> None:
    ran = run(*args, "-i", dupes_mkv)
    assert ran.code == 2 and message in ran.err


# channels in the output ------------------------------------------------------

def test_info_shows_the_channels(run: Callable[..., Ran], dupes_mkv: Path) -> None:
    ran = run("--info", "-i", dupes_mkv)
    assert [line.split()[6] for line in ran.out.splitlines()[3:4]] == ["1"]
    data = json.loads(run("--info", "--format", "json", "-i", dupes_mkv).out)
    tracks = data["results"][0]["data"]["tracks"]
    assert [t.get("channels") for t in tracks] == [None, 1, 1, 1, 1, None, None, None, None]
    assert mkv_json(dupes_mkv)["tracks"][1]["properties"]["audio_channels"] == 1
