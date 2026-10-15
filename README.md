![FML](fml.jpg)

FML
===

Fix Media Language (etc)

Version
-------

Version 0.5.8

License
-------

CC BY-NC-SA: https://creativecommons.org/licenses/by-nc-sa/4.0/

Fund me here: https://ko-fi.com/richardatlateralblast

Contents
--------

- [Introduction](#introduction)
- [Requirements](#requirements)
- [MKV examples](#mkv-examples)
- [Subtitles](#subtitles)
- [Choosing a track by number](#choosing-a-track-by-number)
- [Language preferences](#language-preferences)
- [MP4 examples](#mp4-examples)
- [AVI examples](#avi-examples)
- [Directories](#directories)
- [Languages](#languages)
- [Dry runs](#dry-runs)
- [Output formats](#output-formats)
- [Exit status](#exit-status)
- [Help](#help)
- [Moving from fml.sh](#moving-from-fmlsh)

Introduction
------------

> **The shell script, `fml.sh`, is no longer maintained.** It is kept in the repository for
> reference only and receives no fixes or new features. Use `fml.py`, which replaces it;
> see [Moving from fml.sh](#moving-from-fmlsh) for what changed.

`fml.py` was initially designed to change the default (first) language track on media files.
For example, if the video file has two audio tracks, the first being Italian and the
second being English, and you want English to be the default. It works on audio and subtitle
tracks of MKV, MP4 and AVI files.

It edits MKV files in place where it can (only the metadata changes, so it is fast and the
audio and video are untouched). MP4 files, and tracks that have to be removed, are rewritten
without re-encoding anything.

Requirements
------------

Python 3.9 or newer (tested on 3.14). There are no Python dependencies.
It needs these commands:

| Command                   | Package    | Used for                      |
|---------------------------|------------|-------------------------------|
| `mkvmerge`, `mkvpropedit` | mkvtoolnix | reading and editing MKV files |
| `mediainfo`               | mediainfo  | reading MP4 and AVI files     |
| `ffmpeg`                  | ffmpeg     | rewriting MP4 and AVI files   |

Check they are installed with `./fml.py --check`, and install any that are missing
(apt on Ubuntu, brew on macOS) with `./fml.py --check --install`.
Each action only needs the commands it uses, so an MKV-only setup works without `ffmpeg`.

To work on the script, `pip install -r requirements.txt` installs the linters it is checked with
(`ruff check fml.py` and `mypy --strict fml.py`). They are not needed to run it.

MKV examples
------------

Show the tracks in a file:

```
./fml.py --info --file video.mkv
```

Set the default audio language to English. Only the tracks whose flag has to change are edited:

```
./fml.py --set lang --default English --file video.mkv
```

Get the default audio language:

```
./fml.py --get lang --file video.mkv
```

Show the audio tracks that are in a language:

```
./fml.py --get lang --lang English --file video.mkv
```

If the metadata is wrong, set the language of an audio track (the first unless `--track` says otherwise):

```
./fml.py --set lang --lang Italian --track 2 --file video.mkv
```

If the metadata is poor quality, you can use the swap action to make the second audio track the default:

```
./fml.py --swap lang --file video.mkv
```

Tracks can also be removed, which rewrites the file (permissions and timestamps are kept).

Remove all the audio that is not English:

```
./fml.py --preserve lang --lang eng --file video.mkv
```

Remove all the English audio, or one audio track by number (`last` is the last one):

```
./fml.py --delete lang --lang eng --file video.mkv
./fml.py --delete track --track 2 --file video.mkv
```

As a safeguard, `--preserve` and `--delete` refuse to leave a file with no audio at all, and
`--preserve` refuses if the file has none of the language you asked to keep.
`--force` overrides both.

Subtitles
---------

Add `--kind sub` (`-k sub`) to make an action work on subtitle tracks instead of audio.
The track numbers (`--track`) then count the subtitle tracks, as the `--info` table shows.

Make English the default subtitles. A track that is not forced is preferred, and `--forced`
chooses the forced track instead (the one that only subtitles foreign speech):

```
./fml.py --set lang --default English --kind sub --file video.mkv
./fml.py --set lang --default English --kind sub --forced --file video.mkv
```

Turn the default subtitles off, so that none show unless a viewer picks them (MKV only):

```
./fml.py --set lang --default none --kind sub --file video.mkv
```

The other actions work the same way:

```
./fml.py --get lang --kind sub --file video.mkv
./fml.py --delete lang --lang ita --kind sub --file video.mkv
./fml.py --preserve lang --lang eng --kind sub --file video.mkv
./fml.py --set lang --lang French --track 3 --kind sub --file video.mkv
```

Removing every subtitle track does not need `--force`.

Choosing a track by number
--------------------------

Files often have tracks tagged `und` (undetermined), so there is no language to choose by.
`--set default` makes a track the default by number, and `--lang` also tags it in the same edit:

```
./fml.py --set default --track 2 --lang English --file video.mkv
```

`--track last` is the last track. This also works with `--kind sub`.

Language preferences
--------------------

`--prefer` takes languages in order, and uses the first one that each file has. One command then
suits a library where the files differ:

```
./fml.py --set lang --prefer eng,jpn,fre --dir /media/films --recursive
```

A file with English audio gets English, a file with only Japanese and French gets Japanese, and a file
with none of them is skipped with a warning. With `--preserve` it keeps only the first preferred language
that a file has, and removes the rest:

```
./fml.py --preserve lang --prefer eng,jpn --dir /media/films
```

MP4 examples
------------

MP4 (and M4V and MOV) files work with the same actions, including subtitles:

```
./fml.py --set lang --default English --file video.mp4
./fml.py --set lang --default Italian --kind sub --file video.mp4
./fml.py --delete track --track last --file video.mp4
```

MP4 has no way to edit the tracks in place, so each change rewrites the file with `ffmpeg`, copying the
streams (nothing is re-encoded, so it is quick and lossless). The new file is read back and checked before
it replaces the original, and if the check fails the original is left alone. The index (`moov`) is moved
to the front of the file, as for streaming, and permissions and timestamps are kept.
An MP4 file always has a default subtitle track, so `--default none` is refused.

AVI examples
------------

AVI has no track metadata to edit, so these rewrite the file, which takes longer.

Swap the first two audio tracks of an AVI file:

```
./fml.py --swap lang --file video.avi
```

Delete the last audio track from an AVI file:

```
./fml.py --delete track --track last --file video.avi
```

Directories
-----------

Use `--dir` instead of `--file` to process every file in a directory, and `--recursive` to include
subdirectories. Files that are not MKV or AVI are skipped. A failure on one file is reported
and the rest are still processed.

```
./fml.py --set lang --default English --dir /media/films --recursive
```

Several actions can be given. They run in the order given, each across all the files.

Languages
---------

`--lang` and `--default` take a language name, a two letter code or a three letter ISO 639-2 code:
`French`, `fr`, `fre` and `fra` all mean the same thing. Names and two letter codes must be known
(about 60 common languages are built in), and an unknown one is rejected. Any other three letter code
is passed through.

Languages read from files are converted the same way, so a file tagged `fra` matches `--lang French`.

Dry runs
--------

`--dryrun` shows what would change and changes nothing:

```
$ ./fml.py --set lang --default English --file video.mkv --dryrun
[dry-run] video.mkv: audio track 1 (ita): default yes -> no
[dry-run] video.mkv: audio track 2 (eng): default no -> yes
```

Add `--verbose` to also show the commands that would be run. Verbose output goes to stderr, as do warnings,
so stdout only has the results.

Output formats
--------------

`--format json` prints a single JSON document instead of text, for use in scripts:

```
./fml.py --set lang --default English --dir /media/films --dryrun --format json
```

```json
{
  "version": "0.5.8",
  "dry_run": true,
  "results": [
    {
      "file": "/media/films/a.mkv",
      "action": "set",
      "status": "changed",
      "message": "",
      "changes": ["audio track 1 (ita): default yes -> no", "audio track 2 (eng): default no -> yes"],
      "commands": [["mkvpropedit", "/media/films/a.mkv", "--edit", "track:a1", "--set", "flag-default=0", "--edit", "track:a2", "--set", "flag-default=1"]],
      "dry_run": true,
      "data": null
    }
  ]
}
```

`status` is one of `changed`, `unchanged`, `skipped` (nothing could be done, for example the file has no
track in that language) or `error`. For `--info` and `--get`, `data` holds the tracks or languages.

Exit status
-----------

| Status | Meaning                                                                    |
|--------|----------------------------------------------------------------------------|
| 0      | Everything worked (files skipped with a warning count as worked)           |
| 1      | A file could not be processed, a command failed, or a command is missing   |
| 2      | The command line was wrong                                                 |

Help
----

```
./fml.py --help
```

```
usage: fml [-h] [-V] [-i FILE | -d DIR] [-r] [-C] [-I] [-g {lang}]
           [-c {lang,default}] [-s {lang}] [-z {lang,track}] [-p {lang}]
           [-k {audio,sub}] [-l LANG] [-D LANG] [--prefer LANGS] [--forced]
           [-t N] [-f] [-T] [-v] [-F {text,json}] [--install]

Fix the default audio and subtitle languages, and the tracks, of media files.

options:
  -h, --help            show this help message and exit
  -V, --version         show program's version number and exit

files:
  MKV (and WebM), MP4 (M4V, MOV) and AVI files

  -i, --file FILE       file to process
  -d, --dir DIR         directory to process
  -r, --recursive       include subdirectories of --dir

actions:
  may be combined; they run in the order given

  -C, --check           check the required commands are installed
  -I, --info            list the tracks of the file
  -g, --get {lang}      get the default language, or with --lang the tracks in
                        a language
  -c, --set {lang,default}
                        lang: make --default (or the first of --prefer) the
                        default, or with --lang set the language of track
                        --track (1); default: make track --track the default,
                        and with --lang set its language
  -s, --swap {lang}     make the second audio track the default
  -z, --delete {lang,track}
                        remove the tracks in --lang, or track --track
  -p, --preserve {lang}
                        remove all tracks that are not in --lang (or the first
                        of --prefer that is present)

settings:
  -k, --kind {audio,sub}
                        the kind of track the actions work on (default audio)
  -l, --lang LANG       language, by name or ISO 639 code (English, en, eng)
  -D, --default LANG    language to make the default (none turns the default
                        subtitles off)
  --prefer LANGS        languages in order of preference, e.g. eng,jpn,fre:
                        the first that a file has is used, so one command
                        suits a mixed library
  --forced              with --kind sub, pick the forced subtitle track
  -t, --track N         track number from 1 (among tracks of the kind), or
                        last
  -f, --force           allow removing every audio track
  -T, --dryrun          show what would change without changing anything
  -v, --verbose         show the commands that are run
  -F, --format {text,json}
                        output format (default text)
  --install             install missing commands (apt on Ubuntu, brew on
                        macOS)

examples:
  fml --set lang --default English --file video.mkv     make English the default audio
  fml --set lang --prefer eng,jpn,fre --dir films       the best of those languages in each file
  fml --set default --track 2 --lang English -i a.mkv   make audio track 2 the default and English
  fml --set lang --default English --kind sub -i a.mkv  make English the default subtitles
  fml --set lang --default English --kind sub --forced  ... using the forced subtitle track
  fml --set lang --default none --kind sub -i a.mkv     turn the default subtitles off
  fml --get lang --dir /media/films --recursive         default audio language of each film
  fml --preserve lang --lang eng --file video.mkv       remove the audio that isn't English
  fml --delete track --track last --file video.mp4      remove the last audio track
  fml --set lang --default French --dir films --dryrun  show what would change
```

Moving from fml.sh
------------------

`fml.sh` is the original bash version. It is archived and **no longer maintained**: it is kept in the repository,
and still has its own `./fml.sh --help`, but it gets no fixes or new features, and `fml.py` replaces it.
The main changes are:

| fml.sh                                         | fml.py                                                  |
|------------------------------------------------|---------------------------------------------------------|
| `--options verbose,dryrun`                     | `--verbose --dryrun`                                    |
| `--options install`                            | `--install`                                             |
| `--action NAME`, `--usage`                     | removed (use the action switches, and `--help`)         |
| `--debug`, `--strict`, `--shellcheck`, `--yes` | removed                                                 |
| `--get lang` (the first audio track)           | `--get lang` gives the default audio language           |
| `--get lang --lang default`                    | `--get lang`                                            |
| `--format TEXT`, `JSON`, `XML`                 | `--format text` or `json`; `--info` shows a track table |
| `--delete track` (AVI only)                    | also works on MKV                                       |
| `--set lang --lang L` (first track only)       | any audio track, with `--track`                         |
| needs `jq`, `file`, `ffprobe`, `mkvinfo`       | not needed                                              |
| any failure aborts the run                     | per file errors are reported and the run continues      |
