"""Unit tests: pure functions and the track logic. No media tools are needed."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest

import fml


def track(number: int = 1, kind: str = "audio", language: str = "eng", default: bool = False,
          forced: bool = False, track_id: int | None = None, name: str = "") -> fml.Track:
    return fml.Track(id=number if track_id is None else track_id, kind=kind, number=number,
                     codec="AAC", language=language, default=default, forced=forced, name=name)


# Languages -------------------------------------------------------------------

@pytest.mark.parametrize("text, code", [
    ("English", "eng"), ("english", "eng"), ("ENGLISH", "eng"), ("en", "eng"), ("eng", "eng"),
    ("French", "fre"), ("fr", "fre"), ("fre", "fre"), ("fra", "fre"),
    ("German", "ger"), ("deu", "ger"), ("de", "ger"),
    ("Chinese", "chi"), ("zho", "chi"), ("zh", "chi"), ("Mandarin", "chi"),
    ("Dutch", "dut"), ("nld", "dut"), ("Flemish", "dut"),
    ("Greek", "gre"), ("ell", "gre"), ("Japanese", "jpn"), ("ja", "jpn"),
    ("Welsh", "wel"), ("cym", "wel"), ("Undetermined", "und"), ("und", "und"),
    ("  eng  ", "eng"),
])
def test_resolve_language_known(text: str, code: str) -> None:
    assert fml.resolve_language(text) == code


def test_resolve_language_passes_through_unknown_three_letter_codes() -> None:
    assert fml.resolve_language("yid") == "yid"
    assert fml.resolve_language("YID") == "yid"


@pytest.mark.parametrize("text", ["Klingonese", "xx", "e", "", "   ", "english1", "1234", "e n"])
def test_resolve_language_rejects(text: str) -> None:
    with pytest.raises(ValueError, match="unknown language"):
        fml.resolve_language(text)


@pytest.mark.parametrize("value, code", [
    (None, "und"), ("", "und"), ("   ", "und"), (5, "und"), ("fra", "fre"), ("  EN ", "eng"),
    ("x-odd", "x-odd"), ("Deu", "ger"),
])
def test_normalise_language(value: object, code: str) -> None:
    assert fml.normalise_language(value) == code


def test_every_language_in_the_table_resolves_by_each_of_its_names() -> None:
    for language in fml.LANGUAGES:
        for key in (language.code, language.terminology, language.alpha2, language.name):
            if key:
                assert fml.resolve_language(key) == language.code


def test_language_codes_are_unique() -> None:
    codes = [language.code for language in fml.LANGUAGES]
    assert len(codes) == len(set(codes))


# Parsing tracks --------------------------------------------------------------

MKV_JSON: dict[str, Any] = {"tracks": [
    {"id": 0, "type": "video", "codec": "HEVC", "properties": {"language": "und"}},
    {"id": 1, "type": "audio", "codec": "AAC", "properties": {
        "language": "ita", "default_track": True, "forced_track": False, "track_name": "Italian"}},
    {"id": 2, "type": "audio", "codec": "AAC", "properties": {"language": "fra"}},
    {"id": 5, "type": "subtitles", "codec": "SRT", "properties": {
        "language": "eng", "forced_track": True}},
]}


def test_parse_mkv_tracks() -> None:
    tracks = fml.parse_mkv_tracks(MKV_JSON)
    assert [(t.kind, t.number, t.id) for t in tracks] == [
        ("video", 1, 0), ("audio", 1, 1), ("audio", 2, 2), ("subtitles", 1, 5)]
    assert tracks[1].language == "ita" and tracks[1].default and tracks[1].name == "Italian"
    assert tracks[2].language == "fre"  # fra is normalised
    assert not tracks[2].default and not tracks[2].forced
    assert tracks[3].forced and not tracks[3].default


def test_parse_mkv_tracks_copes_with_missing_data() -> None:
    assert fml.parse_mkv_tracks({}) == []
    (only,) = fml.parse_mkv_tracks({"tracks": [{"type": "audio"}]})
    assert only.language == "und" and not only.default and only.codec == ""


MEDIAINFO_JSON: dict[str, Any] = {"media": {"track": [
    {"@type": "General"},
    {"@type": "Video", "ID": "1", "Format": "AVC"},
    {"@type": "Audio", "@typeorder": "1", "ID": "2", "Format": "AAC", "Language": "it",
     "Default": "Yes", "Title": "Italian"},
    {"@type": "Audio", "@typeorder": "2", "ID": "3", "Format": "AAC", "Language": "en",
     "Default": "No"},
    {"@type": "Text", "ID": "4", "Format": "Timed Text", "Language": "fr", "Forced": "Yes"},
    {"@type": "Menu"},
]}}


def test_parse_mediainfo_tracks() -> None:
    tracks = fml.parse_mediainfo_tracks(MEDIAINFO_JSON)
    assert [(t.kind, t.number) for t in tracks] == [
        ("video", 1), ("audio", 1), ("audio", 2), ("subtitles", 1)]
    assert [t.language for t in tracks] == ["und", "ita", "eng", "fre"]
    assert tracks[1].default and tracks[1].name == "Italian"
    assert not tracks[2].default
    assert tracks[3].forced


def test_parse_mediainfo_tracks_copes_with_missing_data() -> None:
    assert fml.parse_mediainfo_tracks({}) == []
    assert fml.parse_mediainfo_tracks({"media": {"track": [{"@type": "General"}]}}) == []


def test_track_to_dict_round_trips() -> None:
    original = [track(1, "audio", "eng", True, name="x"), track(1, "subtitles", "fre", forced=True)]
    assert fml.tracks_from_dicts([t.to_dict() for t in original]) == original


def test_track_label() -> None:
    assert track(2).label == "audio track 2"
    assert track(1, "subtitles").label == "subtitle track 1"


def test_format_tracks_aligns_columns() -> None:
    table = fml.format_tracks([track(1, language="ita", default=True, name="Italian"),
                               track(2, language="eng")])
    lines = table.splitlines()
    assert lines[0].split() == ["Track", "ID", "Codec", "Language", "Default", "Forced", "Name"]
    assert lines[1].split() == ["audio:1", "1", "AAC", "ita", "yes", "-", "Italian"]
    assert lines[2].split() == ["audio:2", "2", "AAC", "eng", "-", "-"]
    assert len({line.index("AAC") for line in lines[1:]}) == 1
    assert all(line == line.rstrip() for line in lines)


@pytest.mark.parametrize("size, text", [
    (0, "0 bytes"), (1023, "1023 bytes"), (1024, "1.0 KB"), (1536, "1.5 KB"),
    (1024 ** 2, "1.0 MB"), (730 * 1024 ** 2, "730.0 MB"), (1024 ** 3, "1.0 GB"),
    (5 * 1024 ** 4, "5.0 TB"), (1023 * 1024 ** 3, "1023.0 GB"),
])
def test_human_size(size: float, text: str) -> None:
    assert fml.human_size(size) == text


# Detecting files -------------------------------------------------------------

@pytest.mark.parametrize("header, kind", [
    (bytes.fromhex("1a45dfa39f428288") + b"matroska", "mkv"),
    (bytes.fromhex("1a45dfa39f428284") + b"webm", "mkv"),
    (b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00", "mp4"),
    (b"\x00\x00\x00\x20ftypM4V \x00\x00\x00\x00", "mp4"),
    (b"\x00\x00\x00\x14ftypqt  \x00\x00\x00\x00", "mp4"),
    (b"RIFF\x00\x00\x00\x00AVI LIST", "avi"),
    (b"RIFF\x00\x00\x00\x00WAVEfmt ", ""),
    (bytes.fromhex("1a45dfa3") + b"unknown doctype", ""),
    (b"hello world, not media", ""),
    (b"", ""),
])
def test_detect_kind(tmp_path: Path, header: bytes, kind: str) -> None:
    path = tmp_path / "file"
    path.write_bytes(header)
    assert fml.detect_kind(str(path)) == kind


def test_detect_kind_of_a_missing_file(tmp_path: Path) -> None:
    assert fml.detect_kind(str(tmp_path / "nothing")) == ""


# Choosing tracks and building edits -----------------------------------------

AUDIO = [track(1, language="ita", default=True), track(2, language="eng"),
         track(3, language="fre"), track(4, language="eng")]


def describe(edits: list[fml.Edit]) -> list[str]:
    return [edit.describe() for edit in edits]


def test_default_edits_change_only_what_differs() -> None:
    edits = fml.Fml.default_edits(AUDIO, AUDIO[1], "")
    assert describe(edits) == ["audio track 1 (ita): default yes -> no",
                               "audio track 2 (eng): default no -> yes"]


def test_default_edits_are_empty_when_already_right() -> None:
    assert fml.Fml.default_edits(AUDIO, AUDIO[0], "") == []


def test_default_edits_clear_every_other_default() -> None:
    tracks = [track(1, default=True), track(2, default=True), track(3)]
    edits = fml.Fml.default_edits(tracks, tracks[2], "")
    assert [(e.track.number, e.default) for e in edits] == [(1, False), (2, False), (3, True)]


def test_default_edits_with_no_chosen_track_clear_defaults() -> None:
    tracks = [track(1, default=True, kind="subtitles"), track(2, kind="subtitles")]
    edits = fml.Fml.default_edits(tracks, None, "")
    assert [(e.track.number, e.default) for e in edits] == [(1, False)]


def test_default_edits_can_set_a_language() -> None:
    tracks = [track(1, language="und", default=True), track(2, language="und")]
    edits = fml.Fml.default_edits(tracks, tracks[1], "eng")
    assert describe(edits) == ["audio track 1 (und): default yes -> no",
                               "audio track 2 (und): default no -> yes, language und -> eng"]


def test_default_edits_skip_a_language_that_is_already_right() -> None:
    tracks = [track(1, language="eng", default=True)]
    assert fml.Fml.default_edits(tracks, tracks[0], "eng") == []


def test_default_edits_move() -> None:
    edits = fml.Fml.default_edits(AUDIO, AUDIO[2], "", move=True)
    assert describe(edits) == ["audio track 1 (ita): default yes -> no",
                               "audio track 3 (fre): default no -> yes, "
                               "moved to the first audio track"]
    assert [e.move_to for e in edits] == [None, 1]


def test_default_edits_do_not_move_the_first_track() -> None:
    assert fml.Fml.default_edits(AUDIO, AUDIO[0], "", move=True) == []
    tracks = [track(1, language="eng"), track(2, language="ita", default=True)]
    edits = fml.Fml.default_edits(tracks, tracks[0], "", move=True)
    assert [e.move_to for e in edits] == [None, None]  # first already: flags only


def test_edit_describe_language_only() -> None:
    edit = fml.Edit(track(2, language="und"), language="eng")
    assert edit.describe() == "audio track 2: language und -> eng"


def test_edit_describe_subtitles() -> None:
    edit = fml.Edit(track(3, "subtitles", "ita"), default=True, move_to=1)
    assert edit.describe() == ("subtitle track 3 (ita): default no -> yes, "
                               "moved to the first subtitle track")


def fml_with(tracks: list[fml.Track], **config: Any) -> tuple[fml.Fml, fml.Media]:
    app = fml.Fml(fml.Config(**config))
    media = fml.Media("x.mkv", "mkv", list(tracks))
    return app, media


def test_pick_track_prefers_the_first_in_the_language() -> None:
    app, _ = fml_with(AUDIO)
    chosen = app.pick_track(AUDIO, "eng")
    assert chosen is not None and chosen.number == 2
    assert app.pick_track(AUDIO, "spa") is None


SUBS = [track(1, "subtitles", "eng", forced=True), track(2, "subtitles", "eng"),
        track(3, "subtitles", "ita")]


def test_pick_track_prefers_a_subtitle_that_is_not_forced() -> None:
    app, _ = fml_with(SUBS, kind="subtitles")
    chosen = app.pick_track(SUBS, "eng")
    assert chosen is not None and chosen.number == 2


def test_pick_track_with_forced_chooses_the_forced_subtitle() -> None:
    app, _ = fml_with(SUBS, kind="subtitles", forced=True)
    chosen = app.pick_track(SUBS, "eng")
    assert chosen is not None and chosen.number == 1
    assert app.pick_track(SUBS, "ita") is None  # there is no forced Italian track


def test_pick_track_falls_back_to_a_forced_subtitle_when_it_is_the_only_one() -> None:
    only = [track(1, "subtitles", "eng", forced=True)]
    app, _ = fml_with(only, kind="subtitles")
    chosen = app.pick_track(only, "eng")
    assert chosen is not None and chosen.number == 1


def test_choose_uses_the_first_language_that_is_present() -> None:
    app, _ = fml_with(AUDIO)
    assert app.choose(AUDIO, ["jpn", "fre", "eng"]) == ("fre", AUDIO[2])
    assert app.choose(AUDIO, ["jpn", "spa"]) is None
    assert app.choose(AUDIO, []) is None


def test_languages_prefers_the_list() -> None:
    assert fml_with([], prefer=["eng", "jpn"])[0].languages("fre") == ["eng", "jpn"]
    assert fml_with([])[0].languages("fre") == ["fre"]
    assert fml_with([])[0].languages("") == []


def test_missing_message() -> None:
    app, _ = fml_with([])
    assert app.missing_message(["eng"]) == 'has no audio track with language "eng"'
    assert app.missing_message(["eng", "jpn"]) == "has no audio track in any of: eng, jpn"
    assert fml_with([], kind="subtitles")[0].missing_message(["eng"]).startswith(
        'has no subtitle track')


def test_resolve_track_numbers_and_last() -> None:
    app, media = fml_with(AUDIO, track="2")
    result = fml.Result("x.mkv", "set")
    chosen = app.resolve_track(media, result)
    assert chosen is not None and chosen.number == 2
    app.config.track = "last"
    chosen = app.resolve_track(media, result)
    assert chosen is not None and chosen.number == 4
    app.config.track = ""
    chosen = app.resolve_track(media, result)
    assert chosen is not None and chosen.number == 1


def test_resolve_track_out_of_range_skips() -> None:
    app, media = fml_with(AUDIO, track="9")
    result = fml.Result("x.mkv", "set")
    assert app.resolve_track(media, result) is None
    assert result.status == "skipped"
    assert result.message == "has no audio track 9 (4 audio tracks)"


def test_expected_layout_applies_edits_and_moves() -> None:
    tracks = [track(1, "video", "und")] + AUDIO + [track(1, "subtitles", "eng")]
    app, media = fml_with(tracks)
    edits = fml.Fml.default_edits(AUDIO, AUDIO[2], "ger", move=True)
    layout = app.expected_layout(media, edits)
    assert [(t.kind, t.language, t.default) for t in layout] == [
        ("video", "und", False), ("audio", "ger", True), ("audio", "ita", False),
        ("audio", "eng", False), ("audio", "eng", False), ("subtitles", "eng", False)]


def test_temp_path_keeps_the_extension() -> None:
    assert fml.Fml.temp_path("/a/b/film.mp4") == "/a/b/film.fml-tmp.mp4"
    assert fml.Fml.temp_path("film.avi") == "film.fml-tmp.avi"
    assert fml.TEMP_MARK in fml.Fml.temp_path("x.mp4")


def test_ffmpeg_command_for_a_move_maps_the_streams_in_the_new_order() -> None:
    tracks = [track(1, "video", "und")] + AUDIO[:3] + [track(1, "subtitles", "eng")]
    app, media = fml_with(tracks)
    media.kind = "mp4"
    edits = fml.Fml.default_edits(AUDIO[:3], AUDIO[2], "", move=True)
    cmd = app.ffmpeg_command(media, edits, edits[-1].track, "out.fml-tmp.mp4")
    maps = [cmd[i + 1] for i, arg in enumerate(cmd) if arg == "-map"]
    assert maps == ["0:v?", "0:a:2", "0:a:0", "0:a:1", "0:s?", "0:d?", "0:t?"]
    # the dispositions refer to the new positions: ita was 0 and is now 1, fre was 2 and is now 0
    assert cmd[cmd.index("-disposition:a:1") + 1] == "-default"
    assert cmd[cmd.index("-disposition:a:0") + 1] == "+default"
    assert cmd[-3:] == ["-movflags", "+faststart", "out.fml-tmp.mp4"]


def test_mkvmerge_command_for_a_move() -> None:
    tracks = [track(1, "video", "und", track_id=0)] + [
        track(1, language="ita", default=True, track_id=1), track(2, language="eng", track_id=2),
        track(3, language="fre", track_id=3), track(1, "subtitles", "eng", track_id=4)]
    app, media = fml_with(tracks)
    edits = fml.Fml.default_edits(tracks[1:4], tracks[2], "", move=True)
    cmd = app.mkvmerge_command(media, edits, edits[-1].track, "out.fml-tmp")
    assert cmd[:5] == ["mkvmerge", "-o", "out.fml-tmp", "--track-order", "0:0,0:2,0:1,0:3,0:4"]
    assert cmd[5:] == ["--default-track-flag", "1:0", "--default-track-flag", "2:1", "x.mkv"]


# Misc ------------------------------------------------------------------------

def test_terminate_acts_like_ctrl_c() -> None:
    with pytest.raises(KeyboardInterrupt):
        fml.terminate(15, None)


def test_version_is_in_step_with_the_header() -> None:
    header = next(line for line in Path(fml.__file__).read_text().splitlines()
                  if line.startswith("# Version:"))
    assert header.split()[2] == fml.__version__


def test_version_matches_the_readme_and_changelog() -> None:
    root = Path(fml.__file__).parent.parent  # fml.py lives in archived/
    readme = (root / "README.md").read_text()
    assert f"\nVersion {fml.__version__}\n" in readme
    changelog = (root / "CHANGELOG.md").read_text()
    first = next(line for line in changelog.splitlines()
                 if line.startswith("## [") and "Unreleased" not in line)
    assert first.startswith(f"## [{fml.__version__}]")


def test_the_rust_version_is_in_step() -> None:
    root = Path(fml.__file__).parent.parent
    cargo = (root / "rust" / "Cargo.toml").read_text()
    assert f'version = "{fml.__version__}"' in cargo
    lib = (root / "rust" / "fml-core" / "src" / "lib.rs").read_text()
    assert f'VERSION: &str = "{fml.__version__}"' in lib
    tauri = (root / "rust" / "fml-gui" / "tauri.conf.json").read_text()
    assert f'"version": "{fml.__version__}"' in tauri


def test_every_switch_is_documented_in_the_readme() -> None:
    # argparse lays its help out differently on each Python version, so compare the switches,
    # not the text
    readme = (Path(fml.__file__).parent.parent / "README.md").read_text()
    start = readme.index("```\nusage: fml")
    pasted = readme[start:readme.index("```", start + 4)]
    for action in fml.build_parser()._actions:  # noqa: SLF001
        for option in action.option_strings:
            assert option in pasted, f"{option} is missing from the help in the README"
            assert option in readme


def test_the_package_version_is_a_plain_string_for_setuptools() -> None:
    # pyproject.toml reads __version__ by parsing the source, so it must be a literal
    source = Path(fml.__file__).read_text()
    assert f'__version__ = "{fml.__version__}"' in source


# Edits: names, forced flags and places ---------------------------------------

def test_edit_describe_name_and_forced() -> None:
    renamed = fml.Edit(track(2, language="eng", name="Original"), name="English")
    assert renamed.describe() == 'audio track 2 (eng): name "Original" -> "English"'
    cleared = fml.Edit(track(1, name="x"), name="")
    assert cleared.describe() == 'audio track 1 (eng): name "x" -> ""'
    forced = fml.Edit(track(1, "subtitles", "ita", forced=False), forced=True)
    assert forced.describe() == "subtitle track 1 (ita): forced no -> yes"


def test_edit_describe_a_move_to_a_later_place() -> None:
    edit = fml.Edit(track(1, language="eng"), default=False, move_to=3)
    assert edit.describe() == "audio track 1 (eng): default no -> no, moved to audio track 3" \
        or edit.describe().endswith("moved to audio track 3")


def test_reordered_moves_to_any_place() -> None:
    tracks = [track(1, "video", "und")] + AUDIO[:3] + [track(1, "subtitles", "eng")]
    app, media = fml_with(tracks)

    def order(moving: fml.Track, to: int) -> list[int]:
        return [t.number for t in app.reordered(media, moving, to) if t.kind == "audio"]

    assert order(AUDIO[2], 1) == [3, 1, 2]
    assert order(AUDIO[0], 2) == [2, 1, 3]
    assert order(AUDIO[0], 3) == [2, 3, 1]
    assert order(AUDIO[0], 9) == [2, 3, 1]  # past the end: last
    assert [t.kind for t in app.reordered(media, AUDIO[2], 1)] == [
        "video", "audio", "audio", "audio", "subtitles"]  # other kinds stay where they were
    assert app.reordered(media, None) == tracks


def test_expected_layout_applies_names_and_forced() -> None:
    tracks = [track(1, "subtitles", "eng", name="old"), track(2, "subtitles", "ita")]
    app, media = fml_with(tracks, kind="subtitles")
    layout = app.expected_layout(media, [fml.Edit(tracks[0], name="new", forced=True)])
    assert [(t.name, t.forced) for t in layout] == [("new", True), ("", False)]


def test_name_args_write_each_name_at_its_new_place() -> None:
    audio = [track(1, language="ita", name="Italiano"), track(2, language="eng", name=""),
             track(3, language="fre", name="Français")]
    video = [track(1, "video", "und", name="Video")]
    sub = [track(1, "subtitles", "eng", name="Forced")]
    args = fml.Fml.name_args({"video": video, "audio": [audio[2], audio[0], audio[1]],
                              "subtitles": sub})
    assert args == ["-metadata:s:v:0", "title=Video", "-metadata:s:a:0", "title=Français",
                    "-metadata:s:a:1", "title=Italiano", "-metadata:s:s:0", "title=Forced"]


def test_name_args_can_rename_and_remove() -> None:
    audio = [track(1, name="One"), track(2, name="Two")]
    args = fml.Fml.name_args({"audio": audio}, {("audio", 1): "Uno", ("audio", 2): ""})
    assert args == ["-metadata:s:a:0", "title=Uno"]  # an empty name is not written


def test_name_args_skip_kinds_ffmpeg_cannot_select() -> None:
    assert fml.Fml.name_args({"data": [track(1, "data", name="x")]}) == []


# Removing the default ----------------------------------------------------------

def test_promotion_of_the_first_track_when_no_default_is_left() -> None:
    left = [track(2, language="eng"), track(3, language="fre")]
    mkv, mp4, avi = (fml.Media("x", kind) for kind in ("mkv", "mp4", "avi"))
    assert fml.Fml.promotion(mkv, left) == left[0]
    assert fml.Fml.promotion(mp4, left) == left[0]
    assert fml.Fml.promotion(avi, left) is None  # AVI has no flags


def test_no_promotion_when_a_default_is_left_or_nothing_is_left() -> None:
    mkv = fml.Media("x", "mkv")
    assert fml.Fml.promotion(mkv, [track(2, default=True), track(3)]) is None
    assert fml.Fml.promotion(mkv, []) is None


def test_subtitles_are_only_promoted_in_an_mp4() -> None:
    left = [track(1, "subtitles", "eng")]
    assert fml.Fml.promotion(fml.Media("x", "mkv"), left) is None  # an MKV may have none
    assert fml.Fml.promotion(fml.Media("x", "mp4"), left) == left[0]  # an MP4 insists


# --include and --exclude ------------------------------------------------------

@pytest.mark.parametrize("relative, patterns, hit", [
    ("a.mkv", ["*.mkv"], True), ("a.MKV", ["*.mkv"], True), ("a.mp4", ["*.mkv"], False),
    ("Season 1/a.mkv", ["*.mkv"], True), ("Season 1/a.mkv", ["a.mkv"], True),
    ("Season 1/a.mkv", ["Season 1"], True), ("Season 1/a.mkv", ["season 2"], False),
    ("Extras/x.mkv", ["Extras"], True), ("Season 1/Extras/x.mkv", ["extras"], True),
    ("Season 1/a.mkv", ["Season 1/*"], True), ("Season 1/a.mkv", ["Season 2/*"], False),
    ("Season 1/Extras/x.mkv", ["Season 1/*"], True), ("Season 1/a.mkv", ["*/a.mkv"], True),
    ("a.mkv", ["*/a.mkv"], False), ("a.mkv", [], False),
    ("a.mkv", ["x*", "*.mkv"], True), ("Season 1/a.mkv", ["S?ason*"], True),
    ("Extras 2/a.mkv", ["Extras"], False),  # the whole name has to match
])
def test_pattern_hit(relative: str, patterns: list[str], hit: bool) -> None:
    assert fml.Fml.pattern_hit(Path(relative), patterns) is hit


# Undoing -----------------------------------------------------------------------

def test_same_state_compares_what_matters() -> None:
    first = [track(1, language="eng", default=True, name="x")]
    assert fml.Fml.same_state(first, [track(1, language="eng", default=True, name="x")])
    assert fml.Fml.same_state(first, [track(1, language="eng", default=True, name="x",
                                            track_id=9)])  # the ID is not compared
    for other in (track(1, language="ita", default=True, name="x"),
                  track(1, language="eng", default=False, name="x"),
                  track(1, language="eng", default=True, name="y"),
                  track(1, language="eng", default=True, name="x", forced=True),
                  track(1, "subtitles", "eng", default=True, name="x")):
        assert not fml.Fml.same_state(first, [other])
    assert not fml.Fml.same_state(first, first + first)


def test_restore_edits_for_a_move_and_a_flag() -> None:
    before = [track(1, language="ita", default=True), track(2, language="eng"),
              track(3, language="fre")]
    now = [replace_default(before[1], True), replace_default(before[0], False), before[2]]
    app, media = fml_with([replace_number(t, i + 1) for i, t in enumerate(now)])
    edits = app.restore_edits(media, before, 2)
    described = describe(edits)
    assert described == ["audio track 2 (ita): default no -> yes",
                         "audio track 1 (eng): default yes -> no, moved to audio track 2"]
    assert [e.move_to for e in edits] == [None, 2]  # listed in the order they will be in


def replace_default(item: fml.Track, default: bool) -> fml.Track:
    return fml.replace(item, default=default)


def replace_number(item: fml.Track, number: int) -> fml.Track:
    return fml.replace(item, number=number, id=number)


def test_restore_edits_for_names_and_languages() -> None:
    before = [track(1, language="eng", name="A"), track(2, language="eng")]
    now = [fml.replace(before[0], name="B"), fml.replace(before[1], language="ita")]
    app, media = fml_with(now)
    edits = app.restore_edits(media, before, None)
    assert [(e.name, e.language) for e in edits] == [("A", None), (None, "eng")]


def test_restore_edits_leave_the_forced_flag_of_an_mp4_alone() -> None:
    before = [track(1, "subtitles", "eng", forced=False)]
    now = [fml.replace(before[0], forced=True)]
    app, media = fml_with(now, kind="subtitles")
    assert len(app.restore_edits(media, before, None)) == 1  # an MKV: restored
    media.kind = "mp4"
    assert app.restore_edits(media, before, None) == []  # an MP4: can't be checked


# Backups -----------------------------------------------------------------------

def test_backup_path_is_numbered_when_taken(tmp_path: Path) -> None:
    path = str(tmp_path / "film.mkv")
    assert fml.Fml.backup_path(path) == path + fml.BACKUP_MARK
    Path(path + fml.BACKUP_MARK).write_text("x")
    assert fml.Fml.backup_path(path) == path + fml.BACKUP_MARK + ".1"
    Path(path + fml.BACKUP_MARK + ".1").write_text("x")
    assert fml.Fml.backup_path(path) == path + fml.BACKUP_MARK + ".2"


def test_jobs_argument() -> None:
    import argparse
    assert fml.jobs_arg("1") == 1 and fml.jobs_arg("8") == 8
    for bad in ("0", "-1", "x", "", "2.5"):
        with pytest.raises(argparse.ArgumentTypeError):
            fml.jobs_arg(bad)
