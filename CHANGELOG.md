# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.3] - 2026-10-15

### Added
- `--info` lists the tracks of an MKV or AVI file as a table (track, ID, codec, language, default, forced, name); `--format json` gives the same as data
- `--get lang` reports the default audio language of each file (the first audio track for AVI, which has no default flag)

## [0.5.2] - 2026-10-15

### Added
- `--format json` prints one JSON document of the results of every action, for scripts, including the changes and the commands that were run
- `--dryrun` lists each change that would be made (for example `audio track 1 (ita): default yes -> no`) rather than only the commands
- `--set lang --default` only edits the tracks whose default flag has to change, and reports `unchanged` when nothing does
- `--delete track` works on MKV files as well as AVI
- `--force` allows `--delete` to remove every audio track; without it `--delete` and `--preserve` refuse, so a file is never left without audio
- Files are rewritten through a temporary file that is only moved into place when the command succeeded, keeping the original permissions and timestamps, and `--delete` and `--preserve` remove all their tracks in one `mkvmerge` run
- A failure on one file is reported and the run continues with the rest; the exit status is 1 if any file failed
- Warnings and `--verbose` output go to stderr, so stdout only has the results

### Changed
- Each action only checks for the commands it uses (`mkvmerge`/`mkvpropedit` for MKV, `mediainfo`/`ffmpeg` for AVI), and `jq`, `file`, `ffprobe` and `mkvinfo` are no longer needed

## [0.5.1] - 2026-10-15

### Added
- `--lang` and `--default` take language names, two letter (ISO 639-1) and three letter (ISO 639-2) codes, so `French`, `fr`, `fre` and `fra` are the same language, and unknown names are rejected with an error instead of being cut to three letters
- Languages in files are converted the same way, so a file tagged `fra` or `deu` matches `--lang French` or `--lang German`

## [0.5.0] - 2026-10-15

### Changed
- `fml.py` now uses `argparse`, with standard `--help`, usage errors (exit status 2), abbreviated long options and a typed value for each option. It no longer mirrors `fml.sh`, which is archived and no longer maintained
- `--options verbose,dryrun` is now `--verbose --dryrun`, and `--options install` is `--install`
- `--get lang` reports the default audio language; `--get lang --lang <language>` lists the audio tracks in that language
- `--set lang --lang <language>` sets the language of any audio track, chosen with `--track` (the first by default); it takes `--default` or `--lang`, not both
- `--format` is `text` or `json`
- `--track` is an audio track number from 1, or `last`
- `fml.py` reads the version from `__version__`

### Removed
- `--action`, `--option`, `--usage`, `--debug`, `--strict`, `--shellcheck` and `--yes` from `fml.py`, along with the `printenv`, `printdefaults` and `shellcheck` actions
- `--format xml` and `--format html`, and `--lang default`

## [0.4.9] - 2026-10-15

### Fixed
- `--option` in `fml.sh` and `fml.py` rejects names that aren't switchable options (`recursive`, `verbose`, `strict`, `dryrun`, `debug`, `force`, `yes`, `install`) with `Unknown option` and exit 1, instead of silently accepting a typo or setting a value such as `file` to `true`
- A `no` prefix is only stripped when the rest is an option name, so it no longer mangles a name that merely starts with `no`

## [0.4.8] - 2026-10-15

### Fixed
- `fml.sh` now checks that `file` is installed (`--check`, and the startup check). It uses `file` to detect the format, and without it every MKV and AVI operation silently did nothing

## [0.4.7] - 2026-10-15

### Fixed
- An unknown switch now prints the help and exits 1 in `fml.sh` and `fml.py`; it exited 0, so scripts couldn't tell a mistyped switch from success

## [0.4.6] - 2026-10-15

### Changed
- `fml.sh` rewritten to current shell practice: no `eval` (commands are run from argument lists), `local` variables, `[[ ]]` tests, `jq --arg` instead of splicing values into filters, shared helpers in place of duplicated code, and a `main` function; the switches, actions and options are unchanged
- `fml.py` restructured as a typed `Fml` class driven by an `Options` dataclass instead of module-level globals; it passes `ruff` and `mypy --strict`
- `--action printenv` and `--action printdefaults` in `fml.sh` list the options in sorted order, matching `fml.py`
- `--dir` processes files in sorted order
- Rewritten files are replaced with a single `mv -f` instead of `rm` then `mv`, so a failed move no longer loses the original
- `install` is now a documented option (default `false`), shown in `--usage options`

### Fixed
- `--set lang --default` in `fml.sh` now clears other default audio tracks when the requested language is already default; the branch that did this could never run, so it warned that no track existed
- `--get lang --lang <language>` in `fml.sh` no longer warns that the language is missing when the file has more than one track in that language
- `--dir` in `fml.sh` no longer processes files again for each extra action (the file list grew with every action)
- `--delete lang` and `--preserve lang` in `fml.sh` no longer exit 1 after succeeding
- `--dryrun` no longer reports "Failed to process file" after `--delete`, `--preserve` and AVI `--swap`; it shows the `mv` that would run
- `--delete track` with a missing, non-numeric or zero `--track` reports an invalid track instead of passing a bad stream index to `ffmpeg` (or a traceback in `fml.py`)
- `--strict` no longer aborts with an unbound variable when a command is run
- `fml.py` flushes its output before running a command, so messages stay in order when output is redirected
- `fml.py` reads the Linux distribution from `/etc/os-release` when `lsb_release` is missing

## [0.4.5] - 2026-10-15

### Fixed
- `--check --option install` in `fml.sh` and `fml.py` now installs the missing packages (one `mkvtoolnix` install covers `mkvinfo`, `mkvmerge` and `mkvpropedit`) and rechecks, instead of only printing the install hint
- `--option install` in `fml.sh` now runs `apt install` / `brew install` rather than only printing the command; `--dryrun` still just shows it
- Both scripts warn when they don't know how to install a package on the current OS

## [0.4.4] - 2026-10-15

### Added
- `--check` (`-C`, or `--action check`) in `fml.sh` and `fml.py` reports whether each required command is installed and exits 1 if any are missing

### Changed
- The required command list now includes `mkvmerge` and `mkvpropedit`, so the startup check in both scripts fails if either is missing

## [0.4.3] - 2026-10-15

### Added
- `fml.py`, a Python port of `fml.sh` with the same switches, actions and options; it runs the media tools with argument lists and parses `mkvmerge` JSON itself instead of using `jq`

### Changed
- In `fml.py`, `--set lang --default` now clears other default audio tracks when the requested language is already default, which the bash version's unreachable branch never did

## [0.4.2] - 2026-09-29

### Fixed
- `--strict` no longer aborts on unset options; all option keys are initialised

## [0.4.1] - 2026-09-29

### Fixed
- Literal `\n` in `IFS`, `find` option order, stray file list output for `--get` with `--dir`, missing `ffmpeg` check, template leftover script name, and deprecated `let`/`expr`

## [0.4.0] - 2026-09-29

### Fixed
- Language names now map to the correct ISO 639-2 codes for Chinese, Dutch, French, German, Greek and Japanese instead of the first three letters

## [0.3.9] - 2026-09-29

### Fixed
- MKV default track changes used mkvmerge track IDs where mkvpropedit expects audio track numbers; AVI `--track last` and AVI swap no longer assume a fixed stream layout

## [0.3.8] - 2026-09-29

### Fixed
- Missing files, directories, tools and bad parameter values now exit with an error in `--dryrun` mode instead of carrying on

## [0.3.7] - 2026-09-29

### Fixed
- File names containing quotes, `$`, backticks or spaces broke, or could inject into, the commands run through `eval`; file names are now shell escaped

## [0.3.6] - 2026-09-29

### Fixed
- `get` on non-MKV files never used plain `mediainfo` output, and the file name was not quoted; `--format` values are now case insensitive and `text` gives plain output

## [0.3.5] - 2026-09-29

### Fixed
- `get` on MKV files without a language used a mistyped array name and a stray character in the file name, and its `json` check never matched the `JSON` default

## [0.3.4] - 2026-09-29

### Fixed
- The `no` prefix for options (e.g. `nodryrun`) now turns the option off instead of on

## [0.3.3] - 2026-09-29

### Fixed
- `-o` was claimed by both `--format` and `--opt`, so options could not be set with the short switch; `--format` now uses `-F`

## [0.3.2] - 2026-09-29

### Fixed
- `--delete lang` now removes the tracks matching the language instead of the non-matching ones

## [0.3.1] - 2026-09-29

### Fixed
- `--preserve` and `--delete` now read the language from `--lang` (previously only `--default`, so an empty value matched every track and could strip all audio tracks); they now refuse to run without a language

## [0.3.0] - 2025-09-26

### Fixed
- Parameter check

## [0.2.9] - 2025-09-05

### Added
- Support for short command line options

## [0.2.8] - 2025-08-17

### Changed
- Improved handling of deletion of multiple audio tracks

## [0.2.7] - 2025-08-17

### Changed
- Improvements and bug fixes

## [0.2.6] - 2025-08-17

### Changed
- Improved processing of multiple tracks

## [0.2.5] - 2025-08-17

### Added
- Delete a specific audio track from AVI files

## [0.2.4] - 2025-08-17

### Added
- Delete the last audio track from AVI files

## [0.2.3] - 2025-08-17

### Added
- Check for the ffmpeg package

## [0.2.2] - 2025-08-17

### Added
- Swapping audio tracks on AVI files

## [0.2.1] - 2025-08-17

### Added
- Additional check for other tracks set to default

## [0.2.0] - 2025-08-17

### Fixed
- Typo

## [0.1.9] - 2025-08-17

### Fixed
- Escaping of file names

## [0.1.8] - 2025-08-17

### Added
- File check for preserve/delete functions

## [0.1.7] - 2025-08-17

### Changed
- Updated documentation

## [0.1.6] - 2025-08-17

### Added
- Preserve and delete functions

## [0.1.5] - 2025-08-16

### Fixed
- Documentation for switches

## [0.1.4] - 2025-08-16

### Changed
- Improved handling of files without a requested language

## [0.1.3] - 2025-08-16

### Changed
- Improved get function

## [0.1.2] - 2025-08-16

### Fixed
- Bug fixes

## [0.1.1] - 2025-08-16

### Added
- Capability to process directories

## [0.1.0] - 2025-08-16

### Fixed
- Bug fixes

## [0.0.9] - 2025-08-16

### Changed
- Documentation update

## [0.0.8] - 2025-08-16

### Added
- Code to get default language

### Changed
- Updated documentation

## [0.0.7] - 2025-08-16

### Changed
- Updated documentation

## [0.0.6] - 2025-08-16

### Added
- First working version of default audio track change based on language

## [0.0.5] - 2025-08-16

### Added
- Basic default functionality

## [0.0.4] - 2025-08-16

### Added
- Basic swap functionality

## [0.0.3] - 2025-08-16

### Added
- Basic set functionality

## [0.0.2] - 2025-08-16

### Added
- Basic get functionality

## [0.0.1] - 2025-08-16

### Added
- Initial version

[0.4.2]: https://github.com/lateralblast/fml/compare/v0.3.0...v0.4.2
[0.3.0]: https://github.com/lateralblast/fml/compare/v0.2.9...v0.3.0
