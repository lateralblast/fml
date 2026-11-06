# Archived scripts

**The Python script, `archived/fml.py`, is archived.** The maintained version is the Rust one in
[`rust/`](rust/README.md): the `mau` command and a desktop app, at the same version (it is kept at the same version number as the Rust one, so the tests can compare them), with
the same switches and output as this document describes. It is kept in the repository as
the reference the Rust version is tested against (`MAU_BIN=rust/target/release/mau-cli pytest`) and
gets no fixes or new features. The Rust command is `mau-cli`; `./mau` in this directory is a wrapper that runs
the Rust build, and `./mau --gui` opens the desktop app.

**The shell script, `archived/fml.sh`, is no longer maintained.** It is kept in the repository for
reference only and receives no fixes or new features. Use the Rust `mau`, which replaces it;
see [Moving from fml.sh](#moving-from-fmlsh) for what changed.

## Moving from fml.sh

`fml.sh` is the original bash version. It is archived and **no longer maintained**: it is kept in the repository,
and still has its own `./fml.sh --help`, but it gets no fixes or new features, and `mau` replaces it.
The main changes are:

| fml.sh                                         | mau                                                     |
|------------------------------------------------|---------------------------------------------------------|
| `--options verbose,dryrun`                     | `--verbose --dryrun`                                    |
| `--options install`                            | `--install`                                             |
| `--action NAME`, `--usage`                     | removed (use the action switches, and `--help`)         |
| `--debug`, `--strict`, `--shellcheck`, `--yes` | removed                                                 |
| (not in fml.sh)                                | `--backup`, `--log`/`--undo`, `--include`/`--exclude`, `--jobs`, `--set name`/`forced` |
| `--get lang` (the first audio track)           | `--get lang` gives the default audio language           |
| `--get lang --lang default`                    | `--get lang`                                            |
| `--format TEXT`, `JSON`, `XML`                 | `--format text` or `json`; `--info` shows a track table |
| `--delete track` (AVI only)                    | also works on MKV                                       |
| `--set lang --lang L` (first track only)       | any audio track, with `--track`                         |
| needs `jq`, `file`, `ffprobe`, `mkvinfo`       | not needed                                              |
| any failure aborts the run                     | per file errors are reported and the run continues      |
