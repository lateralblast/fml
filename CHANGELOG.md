# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
