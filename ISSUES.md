# Issues

## Todo

- `--preserve` can leave a file with no default track, when the track that is kept was not flagged default before
- `mediainfo` does not report the forced flag that `ffmpeg` writes into an MP4, so `--set forced` is MKV only and `--forced` can't pick a forced subtitle in an MP4 reliably
- A track name that starts with `-` must be written `--name=-text`, because `argparse` takes `--name -text` for an option
- `--undo` can only reverse a removal, or an AVI swap, from a `--backup` copy; without one the log can say what changed but not put it back
- An interrupt (or `kill`) during `--jobs` waits for the files already being processed to finish before it exits
- `--swap` only changes the default flag and never the order of the tracks (`--set ... --move` reorders), which the name does not suggest
- A dry run with several actions plans each one from the file as it is now, so `--set lang --default X --swap lang --dryrun` does not show the second action's effect on the first
- `--default none` is not supported for MP4 subtitles, because ffmpeg always enables the first track of the group
- AVI files carry no language or default flag, so `--get lang` on AVI reports the first audio track (usually `und`), and the AVI `--swap` reorder could not be observed in testing (the test tracks were identical)
- Only about 60 languages are built in; other names and two letter codes are rejected, and only three letter codes pass through, so a valid language outside the table needs its ISO 639-2 code
- Actions that find nothing to do (no track in that language, a format that doesn't support the action) print a warning but exit 0, so a script cannot tell nothing was changed
- `--default`, `--lang`, `--track` and the other settings are shared by every action in one command, so two actions that need different values (`--set lang --default X` and `--delete lang --lang Y`) must be run as separate commands
- `--install` only knows apt on Ubuntu and brew on macOS, and warns on other systems

## In Progress

## Done

- Rewriting an MP4 with `ffmpeg` lost track names that its demuxer cannot read, and the check before replacing the original did not look at names (fixed in 0.6.6 and 0.6.8)
- Removing the default track left a file with no default audio track (fixed in 0.6.7)

- `fml.py` had only been run on Python 3.14, the 3.9 target was checked by `ruff` only, and there was no test suite (fixed in 0.6.3 to 0.6.5: 278 tests pass on 3.9 to 3.14)

- There was no way to reorder tracks, so a player that always plays the first track kept playing the old first track (added in 0.6.2 as `--move`)

- An interrupted rewrite (Ctrl-C or `kill`) left the `.fml-tmp` temporary file behind, next to the original (fixed in 0.6.1; `kill -9` and a power cut can still leave one, and the next run ignores it)
- Rewrites of MKV (`--delete`, `--preserve`) and AVI files were not checked before the original was replaced, and free disk space was not checked first (fixed in 0.6.0)
