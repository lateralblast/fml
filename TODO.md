# TODO

Status: `[ ]` open, `[~]` in progress, `[x]` done.

## Open

- [ ] Add a `pytest` suite: stub `mkvmerge`/`mkvpropedit`/`mediainfo`/`ffmpeg` tests for the command line and messages, plus tests on real files generated with `ffmpeg` (MKV with audio and subtitles, MP4, AVI) checked with `mkvmerge -J` / `mediainfo`. Then update `CLAUDE.md`, which currently says not to add a test suite.
- [ ] Add a `pyproject.toml` with an installable `fml` command, and lint (`ruff`, `mypy --strict`) and test config.
- [ ] Run the checks on Python 3.9 to 3.13. `fml.py` targets 3.9 but has only been run on 3.14 (current `mypy` no longer checks 3.9).
- [ ] Verify rewrites before replacing the original for MKV (`mkvmerge`) and AVI as well as MP4: re-read the temporary file and check the track count, and check there is enough free disk space first. Only the MP4 flag and language edits are verified now.
- [ ] Add `--backup` / `--keep` to keep the original beside the result for `--delete`, `--preserve`, the AVI swap and MP4 rewrites.
- [ ] Add an undo log: `--log changes.json` records each file's previous default flags and languages, and `--undo` puts them back.
- [ ] Add `--include` / `--exclude` glob patterns for `--dir`.
- [ ] Add `--jobs N` to process a directory in parallel, without interleaving the output (rewrites are disk bound, so do this last).
- [ ] Add editing of track names (`--name`) and the forced flag; `--info` already shows both.
- [ ] Decide whether `--preserve` should set the default flag on the track that is left when it had none, so the file isn't left with no default track.
- [ ] Support `--default none` for MP4 subtitles. The `ffmpeg` mov muxer always enables the first track of the group, so this needs another way to write the flag, or stays unsupported.
- [ ] Install support for other systems (`winget`/`choco` on Windows, and other Linux distributions) in `--install`; only apt on Ubuntu and brew on macOS now.

## Done

- [x] Make `--swap` leave the second audio track as the only default (0.5.8).
- [x] Add MP4, M4V and MOV support, rewritten with `ffmpeg` (stream copy) and checked before replacing the original (0.5.7).
- [x] Add `--prefer` language preference lists for `--set lang` and `--preserve` (0.5.6).
- [x] Add `--set default --track N [--lang L]` to choose the default by track number and tag its language (0.5.5).
- [x] Add subtitle support with `--kind sub`, `--forced` and `--default none` (0.5.4).
- [x] Add the `--info` track table and `--get lang` default language (0.5.3).
- [x] Add `--format json`, change lists in dry runs, edit only the tracks that need it, `--delete track` on MKV, `--force` safeguards, safe file replacement, per file errors and stderr diagnostics (0.5.2).
- [x] Add ISO 639 language names and codes, and reject unknown languages (0.5.1).
- [x] Switch `fml.py` to `argparse`, drop the options that mirrored `fml.sh`, and archive `fml.sh` (0.5.0).
- [x] Make `--option` reject unknown option names, and only strip a `no` prefix from real options (0.4.9).
- [x] Check that `file` is installed in `fml.sh` (0.4.8).
- [x] Exit 1 on an unknown switch (0.4.7).
- [x] Clean up both scripts (no `eval`, typed `Fml` class, shared helpers) and fix the bugs found, such as the unreachable already-default branch and `--dir` repeating files (0.4.6).
- [x] Make `--check --option install` install the missing packages and recheck (0.4.5).
- [x] Add `--check` to report the required commands (0.4.4).
- [x] Create `fml.py`, a Python port of `fml.sh` (0.4.3).
- [x] Mark `fml.sh` as no longer maintained in the README, ignore `__pycache__` in `.gitignore`, and make `requirements.txt` Python oriented.
