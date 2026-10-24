# TODO

Status: `[ ]` open, `[~]` in progress, `[x]` done.

## Open

- [ ] Run `--install` for real on Fedora, Arch and Windows (winget and Chocolatey): the package names were checked against the repositories and the commands are tested, but none of these installs has been run.
- [ ] Match subtitle files to media by name from the command line too (`--add subtitle --sub-dir DIR`), not only in the app.
- [ ] Saved presets for bulk changes in the app, and a watch-folder mode that applies a preset to new files (careful with files still being written).
- [ ] Edit video track names and chapters (probably out of scope unless fml becomes a general MKV editor).
- [ ] Run the `.dmg` build (`./build.sh --target mac`) and the app from Finder on a Mac with `mkvtoolnix` and `mediainfo` installed, and sign and notarise it if it is to be shared. The CLI and app build and `cargo test` passes on macOS 27 (Apple silicon), but the real-file tests were not run there.
- [ ] Run the Windows build on a real Windows machine or VM (`~/code/pampas/windows/` has a working `dockurr/windows` VM setup to adapt): the real `mkvtoolnix`, `mediainfo` and `ffmpeg`, the app (WebView2), Ctrl-C, and the NSIS installer have never been run. It has only been cross-compiled, and `fml-cli.exe` smoke-tested under Wine with stand-in tools.
- [ ] Rust port: cover the paths the pytest suite cannot reach through the binary (free-space check, `--install`, `--check` with missing tools) with Rust tests, and add CI that builds it and runs the suite with `FML_BIN`.
- [ ] Run the checks automatically on every push (a CI workflow that runs `ruff`, `mypy` and `pytest` on Python 3.9 to 3.14, with `ffmpeg`, `mkvtoolnix` and `mediainfo` installed).

## Done

- [x] `--install` for more systems: apt (Debian family), dnf (Fedora), pacman (Arch), brew, winget and choco, from `/etc/os-release` or what is installed (1.0.2). Run the installs on Fedora, Arch and Windows themselves still to do (see the Windows item).
- [x] Support `--default none` for MP4 subtitles, by clearing the "enabled" bit in the track header of a copy of the file instead of going through `ffmpeg` (1.0.1).
- [x] Let each action in one command have its own settings: the settings that follow an action belong to it, those before the first are shared (1.0.0).
- [x] Desktop app: check the dark theme on real hardware (a Mac, by capturing the window in both themes): only the placeholder colour needed fixing (0.9.9). The Windows build is covered by the Windows item above.
- [x] Desktop app: drag and drop of a folder or files onto the window (0.9.8); a drop on a real window was not tried, only the code behind it.
- [x] Desktop app: per-file progress inside a long rewrite, from the size of the new file (0.9.7).
- [x] Keep `--undo` working after a file is renamed from the Edit dialog: the entries in the form's log follow the new name (0.9.6).
- [x] Extract a subtitle track to a file (`--extract subtitle`: `mkvextract` for an MKV, `ffmpeg` to `.srt` for an MP4), and an Extract… button in the Edit dialog (0.9.5).
- [x] A before/after track table in the preview (dry run) and results of the desktop app, from `data.layout` in the JSON output (0.9.4).
- [x] Keep only chosen languages (`--preserve`) with a live preview in the Edit dialog of what would be left: tracks marked in the table, and the default that results (0.9.3).
- [x] `--add subtitle` for MP4: the subtitle is converted to `mov_text` with `ffmpeg`, names are written back, and the first subtitle of a file without any is default (0.9.2).
- [x] Edit the language, name and forced flag of a track in the Edit dialog (one change to the file, not one per property) (0.9.1).
- [x] Add subtitle files in bulk: match `Show.S01E03.ita.srt` to `Show.S01E03.mkv` by name, guess the language, show the matches, and add them all (0.9.0).
- [x] Apply a reorder or removal from the Edit dialog to every selected file, matching tracks by language, so one arrangement fixes a whole season (0.8.9).
- [x] Make the first track the default when reordering (`--default-first`, and a checkbox in the Edit dialog), so a player that plays the first track also gets the default flag on it (0.8.8).
- [x] Port `fml` to Rust (`fml-core`, `fml-cli`) with a Tauri desktop app (`fml-gui`), at the same version, 0.7.2; archive `fml.py`.
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
