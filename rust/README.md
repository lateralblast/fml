# mau in Rust

A Rust version of `archived/fml.py`, as a command line tool and a desktop app, both built on one library.
`archived/fml.py` is the archived reference: the Rust version is meant to behave the same, and the same test
suite is used to check it.

```
rust/
  mau-core/   the library: languages, tracks, edits, safe rewriting, backup, log and undo
  mau-cli/    the `mau-cli` command (clap); same switches and output as fml.py
  mau-gui/    the desktop app (Tauri 2), built as the program `mau`: src/main.rs is the backend, ui/ is the page
```

`mau-core` prints nothing. A front end gives it a `Config` and a sink for `Event`s (warnings,
verbose notes, and one `Outcome` per file) and shows them its own way. `validate` holds the
rules about which switches go together, so the command line and the app enforce the same ones.

## Build and run

Needs `cargo` (1.85 or newer; 1.90 or newer for the app) and the same tools as `fml.py`: `mkvmerge`, `mkvpropedit`,
`mediainfo` and `ffmpeg`.

```
cd rust
cargo build --release            # the command line tool: target/release/mau-cli
target/release/mau-cli --set lang --default English --move --dryrun --dir films
cargo test                       # unit tests of the library, the command line and the app backend
```

A plain `cargo build` leaves the app out, because it needs the system webview. For the app,
on Ubuntu install `libwebkit2gtk-4.1-dev` and `libgtk-3-dev` (and `cargo install tauri-cli`
for bundling), then:

```
cargo build -p mau-gui --release     # target/release/mau (the app)
cd mau-gui && cargo tauri build --bundles deb    # a .deb in target/release/bundle/deb
```

`mau-cli --gui` opens the desktop app: the command finds the app, the program called `mau`, beside itself (or on `PATH`, skipping scripts,
or at the path in `MAU_GUI`; on a Mac also as `mau.app` beside it, in `~/Applications` or in `/Applications`, or only in
the folders of `MAU_APP_DIRS` if that is set) and replaces itself with it. It takes no other options. From a checkout,
`./mau` (a small script in the repository root) runs the release build, or the debug build if
there is no release one, so `./mau --gui` and `./mau --set lang ...` both work there once both
are built (`cargo build --release && cargo build -p mau-gui --release`).

## Windows

`../build.sh --target windows` cross-compiles for Windows from Linux or a Mac, in the container described by
`../docker/windows/Dockerfile` (Ubuntu, MinGW, NSIS, `x86_64-pc-windows-gnu`): the installer
(`mau_<version>_x64-setup.exe`) and `mau-cli.exe` (as `mau-cli-<version>-windows.zip`) end up in `../dist/`. The target
directory and cargo registry are Docker volumes (`mau-windows-target`, `mau-windows-cargo`), so only the first build
compiles everything.

The code that differs is small: `util.rs` (`which` through `PATHEXT`, `disk_free` through `GetDiskFreeSpaceExW`, no
signal numbers, running `.bat`/`.cmd` stand-ins by resolving the program first) and `mau-cli` (`SetConsoleCtrlHandler`
for Ctrl-C, and `--gui` starts the app with `mau.exe` instead of replacing itself). `libc` is only a dependency on
Unix. `docker/windows/smoke.sh` runs the built `mau-cli.exe` under Wine with `.bat` stand-ins for the media tools and
checks a real edit, the backup and log, and that a refused change leaves no temporary file.

Not covered: the real tools (`mkvtoolnix` and the rest for Windows), the app itself (it needs WebView2), Ctrl-C, and
running the installer. For that a Windows 11 virtual machine is the next step: `~/code/pampas/windows/` has a working
one (`dockurr/windows`, QEMU/KVM, a watcher that runs `build.cmd` from a shared folder) that could be adapted.

## Checking it against the Python suite

The pytest suite in `../tests` runs the command line through a `run` helper. With `MAU_BIN` set
it runs the Rust binary instead of `fml.py`, so every real-file and stub-tool test applies:

```
cargo build --release
MAU_BIN=$PWD/target/release/mau-cli pytest ../tests        # from the repository root
```

Tests that call `fml.py`'s own functions or patch Python internals are skipped then (the whole
of `test_unit.py` and about 16 in `test_stubs.py`); the Rust unit tests in `mau-core` cover the
pure parts instead (`engine_tests.rs`). `tests/test_signals.py` starts a real process, so it
checks Ctrl-C and `kill` handling of either version.

What the suite does **not** cover for the Rust build, because those tests patch Python: the
free-space check, `--install`, and `--check` with missing tools. They are the same logic, but
check them by hand before relying on them.

## Differences from `fml.py`

- `--help` and the wording of option errors come from `clap`, not `argparse`. Errors that the
  tests look for (`unrecognized arguments`, `invalid choice`, `not allowed with argument`) are
  worded the same way; the help text layout is different.
- A track of a kind other than video, audio or subtitles (such as DVD buttons) is ignored
  instead of being listed, so `--track-order` does not mention it.
- The log is written as compact JSON lines; `fml.py` writes `", "` and `": "` separators. Each
  version reads the other's logs.
- A symbolic link to a media file is followed: the file it points to is the one changed, and the output
  names that file. `fml.py` replaces the link with the changed copy.
- A symbolic link to a folder is followed by `--recursive`, and a folder reached twice is read once. The
  archived `fml.py` uses `pathlib`'s `rglob`, which may not follow links, depending on the Python version.
- With no media files, `--dir` says so on stderr. `fml.py` says nothing.
- `--info` on an MKV shows a subtitle codec as ffprobe names it (`subrip`); `fml.py` shows mkvmerge's
  name (`SubRip/SRT`). See `TODO.md`.
- The project was renamed from `fml` to `mau` in 1.5.7, so the engine type that mirrors `fml.py`'s
  `Fml` class is `Mau`, and the command is `mau-cli`, not `fml.py`. The `.fml-tmp` and `.fml-bak`
  suffixes are the same in both.

## The app

Add a folder (or files), pick a change, **Preview** it, then **Apply**. A preview is a dry run;
applying asks first, and the log (on by default, `~/mau-log.jsonl`) lets **Undo from a log** put
changes back, from the kept original when `Keep the original` was ticked. The page is plain
HTML, CSS and JavaScript in `mau-gui/ui` (no build step); the Rust side exposes `scan`, `run`,
`cancel`, `app_info`, `languages` and `file_details` (the size, date and `mediainfo` report for the Edit dialog), `rename_file` and `classify_dropped` (what was dropped on the window: a folder, or a list of media files).

In a debug build, `MAU_GUI_SCRIPT=script.js MAU_GUI_REPORT=out.txt target/debug/mau` runs
the script in the window after it loads, and the script can send text back with
`invoke('dev_report', {text})`. That is how the app was driven and checked without a person
clicking (`xvfb-run` gives it a screen on a machine with no display). `MAU_GUI_THEME=dark` (or `light`)
shows the window in that theme whatever the system uses, which is how the page was checked in both, and
`MAU_GUI_SIZE=1120x1000` gives it a size, which is how the screenshots in `docs/images/` were taken. Both are
compiled out of release builds.
