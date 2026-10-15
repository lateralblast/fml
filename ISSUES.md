# Issues

## Todo

- `--preserve` can leave a file with no default track, when the track that is kept was not flagged default before
- Rewrites of MKV (`--delete`, `--preserve`) and AVI files are not checked before the original is replaced, and free disk space is not checked first; only MP4 flag and language edits are verified
- An interrupted rewrite (Ctrl-C or a killed process) leaves the `.fml-tmp` temporary file behind, next to the original
- A dry run with several actions plans each one from the file as it is now, so `--set lang --default X --swap lang --dryrun` does not show the second action's effect on the first
- `--default none` is not supported for MP4 subtitles, because ffmpeg always enables the first track of the group
- AVI files carry no language or default flag, so `--get lang` on AVI reports the first audio track (usually `und`), and the AVI `--swap` reorder could not be observed in testing (the test tracks were identical)
- Only about 60 languages are built in; other names and two letter codes are rejected, and only three letter codes pass through, so a valid language outside the table needs its ISO 639-2 code
- Actions that find nothing to do (no track in that language, a format that doesn't support the action) print a warning but exit 0, so a script cannot tell nothing was changed
- `fml.py` has only been run on Python 3.14; the 3.9 target is checked by `ruff` only, and there is no test suite
- `--install` only knows apt on Ubuntu and brew on macOS, and warns on other systems

## In Progress

## Done
