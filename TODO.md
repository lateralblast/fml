# TODO

Status: `[ ]` open, `[~]` in progress, `[x]` done.

## Open

- [ ] Run the checks automatically on every push (a CI workflow that runs `ruff`, `mypy` and `pytest` on Python 3.9 to 3.14, with `ffmpeg`, `mkvtoolnix` and `mediainfo` installed).
- [ ] Let each action in one command have its own settings. `--default`, `--lang` and `--track` are shared by every action on the command line, so `--set lang --default X --delete lang --lang Y` is rejected.
- [ ] Support `--default none` for MP4 subtitles. The `ffmpeg` mov muxer always enables the first track of the group, so this needs another way to write the flag, or stays unsupported.
- [ ] Install support for other systems (`winget`/`choco` on Windows, and other Linux distributions) in `--install`; only apt on Ubuntu and brew on macOS now.

## Done

- [x] Add `--jobs N` to process files in parallel, with ordered output and space set aside per job (0.7.2).
- [x] Add `--include` and `--exclude` patterns for `--dir` (0.7.1).
- [x] Add `--log` and `--undo`, to reverse flag, language, name and order changes, and removals from a backup (0.7.0).
- [x] Add `--backup`, which keeps the original beside the changed file as a hard link (0.6.9).
- [x] Add `--set name`, `--set forced` and `--set unforced`, and check names and forced flags in the result (0.6.8).
- [x] Decide what `--preserve` and `--delete` do to the default flag: the first track left becomes the default (0.6.7).
- [x] Keep track names when an MP4 is rewritten (0.6.6).
- [x] Check `fml.py` on Python 3.9 to 3.14: the suite passes on each, and `mypy --strict` on 3.10 to 3.14 (0.6.5).
- [x] Add a `pyproject.toml` with an installable `fml` command, the `dev` extra, and `ruff`, `mypy` and `pytest` settings (0.6.4).
- [x] Add a `pytest` suite of 278 tests: unit, command line, stub tools and real MKV, MP4 and AVI files (0.6.3).
- [x] Add `--move`, which also moves the chosen track to be the first of its kind (MKV with `mkvmerge --track-order`, MP4 with `ffmpeg`), and check the order of the result (0.6.2).
- [x] Remove the temporary copy when stopped with Ctrl-C or `kill`, and skip leftover `.fml-tmp` files in `--dir` (0.6.1).
- [x] Check every rewrite (`--delete`, `--preserve`, AVI `--swap`) before it replaces the original, and check free disk space before copying or rewriting (0.6.0).
- [x] Make MKV flag and language edits work on a copy that is checked and then replaces the original, with `--in-place` for the old direct edit (0.5.9).
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
