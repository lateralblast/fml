"""Tests of --search subtitle and --add subtitle --online, on a stand-in service (Rust only)."""

from __future__ import annotations

import json
import threading
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any, Callable

import pytest
from conftest import FML_BIN, Ran, leftovers, mkv_layout, mkv_names, requires_tools

pytestmark = [pytest.mark.real, requires_tools,
              pytest.mark.skipif(not FML_BIN, reason="online subtitles are Rust only")]

SUBTITLE = "1\n00:00:00,000 --> 00:00:01,000\nCiao\n"


def hit(file_id: int, language: str, downloads: int, **flags: bool) -> dict[str, Any]:
    return {"attributes": {"language": language, "download_count": downloads,
                           "release": f"Show.S01E01.release{file_id}",
                           "files": [{"file_id": file_id, "file_name": f"{file_id}.srt"}], **flags}}


class Service:
    """A stand-in for the OpenSubtitles API: it keeps the requests it was sent."""

    def __init__(self) -> None:
        self.hits: list[dict[str, Any]] = []
        self.requests: list[str] = []
        self.bodies: list[str] = []

    def count(self, text: str) -> int:
        return sum(1 for r in self.requests if text in r)


@pytest.fixture
def service(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Iterator[Service]:
    state = Service()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args: object) -> None:
            pass

        def reply(self, body: str) -> None:
            data = body.encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self) -> None:
            state.requests.append(f"GET {self.path}")
            if self.path.startswith("/api/v1/subtitles"):
                self.reply(json.dumps({"data": state.hits}))
            else:
                self.reply(SUBTITLE)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", 0))
            state.bodies.append(self.rfile.read(length).decode())
            state.requests.append(f"POST {self.path}")
            if self.path.endswith("/login"):
                self.reply(json.dumps({"token": "tok"}))
            else:
                link = f"http://127.0.0.1:{self.server.server_port}/file/x.srt"
                self.reply(json.dumps({"link": link, "file_name": "x.srt", "remaining": 9}))

    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    monkeypatch.setenv("FML_OPENSUBTITLES_URL", f"http://127.0.0.1:{server.server_port}/api/v1")
    monkeypatch.setenv("FML_OPENSUBTITLES_KEY", "key")
    monkeypatch.setenv("FML_OPENSUBTITLES_USER", "me")
    monkeypatch.setenv("FML_OPENSUBTITLES_PASSWORD", "pw")
    monkeypatch.setenv("TMPDIR", str(tmp_path / "tmp"))
    (tmp_path / "tmp").mkdir()
    yield state
    server.shutdown()


@pytest.fixture
def episode(make: Callable[..., Path]) -> Path:
    return make("Show.S01E01.1080p.mkv", [("eng", None, True)], [("eng", False, True)])


def test_search_lists_the_subtitles_best_first(run: Callable[..., Ran], service: Service,
                                               episode: Path) -> None:
    service.hits = [hit(11, "it", 50), hit(12, "en", 900, moviehash_match=True),
                    hit(13, "it", 700, hearing_impaired=True)]
    ran = run("--search", "subtitle", "-i", episode)
    assert ran.code == 0
    rows = [line.split() for line in ran.out.splitlines()]
    assert rows[0] == [f"{episode}:"]
    assert [row[0] for row in rows[2:]] == ["12", "13", "11"]
    assert "made for this file" in ran.out and "hearing impaired" in ran.out
    assert "season_number=1" in service.requests[0] and "episode_number=1" in service.requests[0]
    assert "query=show" in service.requests[0]
    assert service.count("POST") == 0  # a search needs no login


def test_search_with_a_language_and_a_title(run: Callable[..., Ran], service: Service,
                                            episode: Path) -> None:
    service.hits = [hit(11, "it", 50)]
    ran = run("--search", "subtitle", "--lang", "Italian", "--query", "Other Show",
              "--format", "json", "-i", episode)
    assert "languages=it" in service.requests[0] and "query=other%20show" in service.requests[0]
    assert "season_number" not in service.requests[0]  # those belong to the guessed title
    data = json.loads(ran.out)["results"][0]["data"]
    assert [h["file_id"] for h in data["hits"]] == [11]
    assert data["hits"][0]["language"] == "ita"


def test_search_that_finds_nothing_says_so(run: Callable[..., Ran], service: Service,
                                           episode: Path) -> None:
    ran = run("--search", "subtitle", "-i", episode)
    assert ran.code == 0 and "no subtitles found" in ran.out


def test_search_without_a_key_is_an_error(run: Callable[..., Ran], service: Service, episode: Path,
                                          monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.setenv("FML_OPENSUBTITLES_KEY", "")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "nodata"))
    monkeypatch.setenv("HOME", str(tmp_path / "nohome"))
    ran = run("--search", "subtitle", "-i", episode)
    assert ran.code == 1 and "no OpenSubtitles API key" in ran.err
    assert service.requests == []


def test_add_online_takes_the_best_subtitle_in_the_language(run: Callable[..., Ran],
                                                            service: Service, episode: Path,
                                                            tmp_path: Path) -> None:
    service.hits = [hit(21, "it", 10, machine_translated=True),
                    hit(22, "it", 30, hearing_impaired=True),
                    hit(23, "it", 20), hit(24, "en", 999)]
    ran = run("--add", "subtitle", "--online", "--lang", "ita", "-i", episode)
    assert ran.code == 0 and "ID 23" in ran.out
    assert mkv_layout(episode) == ["a:eng*", "s:eng*", "s:ita"]
    assert mkv_names(episode)[-1] == ""
    assert service.count("POST /api/v1/login") == 1 and service.count("POST /api/v1/download") == 1
    assert '"file_id":23' in service.bodies[-1].replace(" ", "")
    assert not list((tmp_path / "tmp").glob("fml-*")), "the downloaded file is removed"
    assert not leftovers(episode)


def test_add_online_by_id_names_a_hearing_impaired_one(run: Callable[..., Ran], service: Service,
                                                       episode: Path) -> None:
    service.hits = [hit(31, "fr", 5, hearing_impaired=True), hit(32, "fr", 50)]
    run("--add", "subtitle", "--online", "--sub-id", "31", "-i", episode)
    assert mkv_layout(episode) == ["a:eng*", "s:eng*", "s:fre"]
    assert mkv_names(episode)[-1] == "SDH"


def test_add_online_dry_run_downloads_nothing(run: Callable[..., Ran], service: Service,
                                              episode: Path) -> None:
    service.hits = [hit(41, "it", 5)]
    before = mkv_layout(episode)
    ran = run("--add", "subtitle", "--online", "--lang", "ita", "--dryrun", "-i", episode)
    assert ran.code == 0 and "[dry-run]" in ran.out and "ID 41" in ran.out
    assert mkv_layout(episode) == before
    assert service.count("POST") == 0 and service.count("/file/") == 0


@pytest.mark.parametrize("hits,id_args,message", [
    ([hit(51, "it", 5, machine_translated=True), hit(52, "it", 5, foreign_parts_only=True)], [],
     "no suitable subtitle"),
    ([hit(53, "en", 5)], [], "no suitable subtitle"),
    ([hit(54, "it", 5)], ["--sub-id", "999"], "no subtitle with ID 999"),
])
def test_add_online_with_nothing_suitable_leaves_the_file(run: Callable[..., Ran], service: Service,
                                                          episode: Path, hits: list[dict[str, Any]],
                                                          id_args: list[str], message: str) -> None:
    service.hits = hits
    before = mkv_layout(episode)
    ran = run("--add", "subtitle", "--online", "--lang", "ita", *id_args, "-i", episode)
    assert ran.code == 0 and message in ran.err
    assert mkv_layout(episode) == before and service.count("POST") == 0


def test_add_online_logs_in_once_for_a_folder_and_can_be_undone(
        run: Callable[..., Ran], service: Service, make: Callable[..., Path],
        tmp_path: Path) -> None:
    folder = tmp_path / "season"
    files = [make(f"Show.S01E0{n}.mkv", [("eng", None, True)], [], where=folder) for n in (1, 2)]
    service.hits = [hit(61, "it", 5)]
    log = tmp_path / "log.jsonl"
    ran = run("--add", "subtitle", "--online", "--lang", "ita", "--backup", "--log", log,
              "-d", folder)
    assert ran.code == 0
    assert [mkv_layout(f) for f in files] == [["a:eng*", "s:ita"]] * 2
    assert service.count("POST /api/v1/login") == 1 and service.count("POST /api/v1/download") == 2
    run("--undo", log)
    assert [mkv_layout(f) for f in files] == [["a:eng*"]] * 2


def test_add_online_a_download_failure_changes_nothing(run: Callable[..., Ran], service: Service,
                                                       episode: Path,
                                                       monkeypatch: pytest.MonkeyPatch) -> None:
    service.hits = [hit(71, "it", 5)]
    monkeypatch.setenv("FML_OPENSUBTITLES_PASSWORD", "")
    monkeypatch.setenv("XDG_DATA_HOME", "/nonexistent")
    monkeypatch.setenv("HOME", "/nonexistent")
    before = mkv_layout(episode)
    ran = run("--add", "subtitle", "--online", "--lang", "ita", "-i", episode)
    assert ran.code == 1 and "needs an OpenSubtitles login" in ran.err
    assert mkv_layout(episode) == before and not leftovers(episode)


@pytest.mark.parametrize("args,message", [
    (["--add", "subtitle", "--online"], "needs --lang"),
    (["--add", "subtitle", "--online", "--lang", "ita", "--sub-file", "x.srt"],
     "takes no --sub-file"),
    (["--add", "subtitle", "--online", "--lang", "ita", "--track", "1"], "not --track"),
    (["--add", "subtitle", "--online", "--lang", "ita", "--in-place"],
     "can't be used with --in-place"),
    (["--set", "lang", "--default", "eng", "--online"], "--online only works with --add subtitle"),
    (["--info", "--sub-id", "3"], "--sub-id only works with --add subtitle --online"),
    (["--info", "--query", "x"], "--query only works with --search subtitle"),
    (["--add", "subtitle", "--online", "--lang", "ita", "--sub-id", "0"], "is not a subtitle ID"),
    (["--search", "movie"], "invalid choice"),
])
def test_online_argument_rules(run: Callable[..., Ran], episode: Path, args: list[str],
                               message: str) -> None:
    ran = run(*args, "-i", episode)
    assert ran.code == 2 and message in ran.err
