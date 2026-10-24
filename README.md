![FML](fml.png)

FML
===

Fix Media Language (etc)

Version
-------

Version 1.0.2

License
-------

CC BY-NC-SA: https://creativecommons.org/licenses/by-nc-sa/4.0/

Fund me here: https://ko-fi.com/richardatlateralblast

Contents
--------

- [Introduction](#introduction)
- [Requirements](#requirements)
- [MKV examples](#mkv-examples)
- [Moving a track first](#moving-a-track-first)
- [Taking a subtitle track out to a file](#taking-a-subtitle-track-out-to-a-file)
- [Changing a track's language, name and forced flag together](#changing-a-tracks-language-name-and-forced-flag-together)
- [Put the tracks in any order](#put-the-tracks-in-any-order)
- [Adding a subtitle file](#adding-a-subtitle-file)
- [Example: a whole season](#example-a-whole-season)
- [Subtitles](#subtitles)
- [Choosing a track by number](#choosing-a-track-by-number)
- [Language preferences](#language-preferences)
- [MP4 examples](#mp4-examples)
- [AVI examples](#avi-examples)
- [Directories](#directories)
- [Languages](#languages)
- [Track names and the forced flag](#track-names-and-the-forced-flag)
- [Choosing files](#choosing-files)
- [Several files at once](#several-files-at-once)
- [Dry runs](#dry-runs)
- [Safe changes](#safe-changes)
- [Keeping the original](#keeping-the-original)
- [Undoing changes](#undoing-changes)
- [Output formats](#output-formats)
- [Exit status](#exit-status)
- [Installing](#installing)
- [Development](#development)
- [Help](#help)
- [Moving from fml.sh](#moving-from-fmlsh)

Introduction
------------

> **The Python script, `archived/fml.py`, is archived.** The maintained version is the Rust one in
> [`rust/`](rust/README.md): the `fml` command and a desktop app, at the same version (0.7.2), with
> the same switches and output as this document describes. It is kept in the repository as
> the reference the Rust version is tested against (`FML_BIN=rust/target/release/fml-cli pytest`) and
> gets no fixes or new features. The Rust command is `fml-cli`; `./fml` in this directory is a wrapper that runs
> the Rust build, and `./fml --gui` opens the desktop app.

> **The shell script, `archived/fml.sh`, is no longer maintained.** It is kept in the repository for
> reference only and receives no fixes or new features. Use the Rust `fml`, which replaces it;
> see [Moving from fml.sh](#moving-from-fmlsh) for what changed.

`fml` was initially designed to change the default (first) language track on media files.
For example, if the video file has two audio tracks, the first being Italian and the
second being English, and you want English to be the default. It works on audio and subtitle
tracks of MKV, MP4 and AVI files.

Changes are made on a copy that is checked and then swapped in (see [Safe changes](#safe-changes)). MKV
metadata edits only touch the metadata, so the audio and video are untouched. MP4 files, and tracks that
have to be removed, are rewritten without re-encoding anything.

Requirements
------------

`fml` is built with Rust (`cargo` 1.80 or newer; see [Installing](#installing)) and has no other
dependencies of its own. It needs these commands:

| Command                   | Package    | Used for                      |
|---------------------------|------------|-------------------------------|
| `mkvmerge`, `mkvpropedit` | mkvtoolnix | reading and editing MKV files |
| `mediainfo`               | mediainfo  | reading MP4 and AVI files     |
| `ffmpeg`                  | ffmpeg     | rewriting MP4 and AVI files   |

Check they are installed with `./fml --check`, and install any that are missing
with `./fml --check --install`, which uses the package manager of the system:

| System                                      | Command                                        |
|---------------------------------------------|------------------------------------------------|
| Debian, Ubuntu and what is built on them    | `sudo apt install -y`                          |
| Fedora, RHEL and what is built on them      | `sudo dnf install -y` (`ffmpeg-free`)          |
| Arch Linux and what is built on it          | `sudo pacman -S --noconfirm --needed` (`mkvtoolnix-cli`) |
| macOS                                       | `brew install`                                 |
| Windows                                     | `winget install --id ... --exact` (`MoritzBunkus.MKVToolNix`, `MediaArea.MediaInfo`, `Gyan.FFmpeg`), else `choco install -y` (`mediainfo-cli`) |

A Linux distribution is recognised by the `ID` and `ID_LIKE` in `/etc/os-release`, so a distribution built on one of
these is covered too. Any other system is told it is not known. The package names were checked against the
package repositories, but the installs on Fedora, Arch and Windows have not been run on those systems (only
the commands that would be run are tested; `--dryrun --verbose` shows them). Fedora's `ffmpeg-free` is enough for
fml, which copies streams; the full `ffmpeg` needs RPM Fusion.
Each action only needs the commands it uses, so an MKV-only setup works without `ffmpeg`.

The desktop app (`./fml --gui`) is optional and also needs the system webview (see [Installing](#installing)).
In the app you can drop a folder, or files, on the window instead of using the buttons: a folder on its own is
worked on as a folder (with its filters and subfolders), and anything else becomes a list of the media files among
the files and inside the folders dropped. In the app, click a file's row for its track list, or its **Edit** button for the file's details, the track table
with IDs and the full `mediainfo` report. The Edit dialog also renames the file (in its folder; nothing is overwritten); the entries about it in the log
named in the form are changed to the new name, so **Undo from a log…** and `--undo` still find it (a kept original
`.fml-bak` keeps its own name, which the log records, so it is still found too). **Clear selected** takes the ticked files off the list (the files on
disk are not touched); a rescan of the same folder keeps them off.

MKV examples
------------

Show the tracks in a file:

```
./fml --info --file video.mkv
```

Set the default audio language to English. Only the tracks whose flag has to change are edited:

```
./fml --set lang --default English --file video.mkv
```

Get the default audio language:

```
./fml --get lang --file video.mkv
```

Show the audio tracks that are in a language:

```
./fml --get lang --lang English --file video.mkv
```

If the metadata is wrong, set the language of an audio track (the first unless `--track` says otherwise):

```
./fml --set lang --lang Italian --track 2 --file video.mkv
```

If the metadata is poor quality, you can use the swap action to make the second audio track the default:

```
./fml --swap lang --file video.mkv
```

Tracks can also be removed, which rewrites the file (checked first, with permissions and timestamps kept).

Remove all the audio that is not English:

```
./fml --preserve lang --lang eng --file video.mkv
```

Remove all the English audio, or one audio track by number (`last` is the last one):

```
./fml --delete lang --lang eng --file video.mkv
./fml --delete track --track 2 --file video.mkv
```

`--track` can also name a track of any kind, as the `--info` table shows it, and `--delete track` then removes
that track whatever `--kind` says: `subtitles:18` (or `sub:18`, `audio:2`), `id:20` (the ID column) or `name:Forced`
(the track name, ignoring case; it must match one track, else use the ID):

```
./fml --delete track --track subtitles:18 --file video.mkv
./fml --delete track --track id:20 --file video.mkv
./fml --delete track --track name:Traditional --file video.mkv
```

Video tracks cannot be removed. The other `--set` actions accept the same forms for a track of the kind they work on.

As a safeguard, `--preserve` and `--delete` refuse to leave a file with no audio at all, and
`--preserve` refuses if the file has none of the language you asked to keep.
`--force` overrides both.

If the track that is removed was the default, the first track left becomes the default, so the file always
has a default audio track (an MKV may still have no default subtitle track; an MP4 always has one).

Moving a track first
--------------------

Some players ignore the default flag and just play the first track. `--move` makes the chosen track the
first track of its kind as well as the default, so it works for those players too:

```
./fml --set lang --default English --move --file video.mkv
./fml --set lang --prefer eng,jpn --move --dir /media/films
./fml --set default --track 3 --lang English --move --file video.mkv
```

The tracks of other kinds stay where they are (video first, then the audio, then the subtitles), and the
other audio tracks keep their order. It works with `--kind sub` for subtitles too. An MKV is rewritten once
with `mkvmerge`, which reorders the tracks and sets their flags and languages together, and keeps the
chapters, attachments, tags and track names. If the track is already first, only the flag is edited, as
without `--move`. It cannot be used with `--in-place` or `--default none`, and `--swap` only changes the
default flag, never the order.

Taking a subtitle track out to a file
-------------------------------------

`--extract subtitle --track T [--output FILE|FOLDER]` writes a subtitle track to a file of its own and leaves the media
file as it is. An MKV is read with `mkvextract` (from `mkvtoolnix`) and the file gets the extension of the track
(`.srt` for SubRip, `.ass` for SubStation, `.vtt` for WebVTT, `.sup` for PGS); a text subtitle in an MP4 is written
as `.srt` with `ffmpeg`. VobSub and other formats that are not one file are refused. The name is made from the media
file (`Film.ita.srt`, `Film.eng.forced.srt`) and goes beside it, or in `--output` if that is a folder; give a file
name to choose it. The track is chosen as with `--delete track` (`--track 2`, `--track subtitles:2`, `--track id:5`,
`--track name:Forced`), and a plain number counts subtitle tracks. A file that exists is never replaced unless you
give `--force` with a file name.

```
./fml --extract subtitle --track 2 --file Film.mkv
./fml --extract subtitle --track name:Forced --output ~/Subs/ --file Film.mkv
```

In the desktop app, picking a subtitle track in the Edit dialog shows **Extract…**, which asks where to save it.

Changing a track's language, name and forced flag together
---------------------------------------------------------

`--set track --track T [--lang L] [--name TEXT] [--forced-flag yes|no]` changes several properties of one track in
one edit of the file, where `--set lang`, `--set name` and `--set forced` each make a change of their own. Only
what is given is changed; an empty `--name ""` removes the name. The track is chosen as with `--delete track`
(`--track 2`, `--track subtitles:3`, `--track id:5`, `--track name:Forced`). The forced flag is MKV only.

```
./fml --set track --track 3 --kind sub --lang Spanish --name "Latino" --forced-flag yes --file video.mkv
```

In the desktop app, pick a track in the Edit dialog and an editor appears above the table with its language, name
and forced flag, and **Save changes…**.

Put the tracks in any order
---------------------------

`--set order --order N,N,...` puts the tracks of `--kind` in a new order in one rewrite. Each number is the
current number of a track (as `--info` shows it), listed in the order the tracks should have: with three audio
tracks, `3,1,2` makes the third first, then the first, then the second. Flags, languages and names go with their
tracks, so the default flag does not move to the new first track unless you add `--default-first`, which makes
the track that ends up first the only default of its kind (it also fixes the flag when the order is already right). List every track of the kind once. MKV and MP4 only; it cannot be used with `--in-place`.

```
./fml --set order --order 3,1,2 --file video.mkv
./fml --set order --order 2,1 --kind sub --file video.mp4
```

`--order-lang ita,eng` does the same by language instead of by number: the tracks in the first language come
first, then the second, then the rest in their old order. It suits a whole season whose files have different
tracks (`./fml --set order --order-lang eng,ita --default-first --dir /media/Show.S01`).

`--log` records it and `--undo` puts the old order back. In the desktop app the **Edit** button of a file opens a dialog that shows
its audio and subtitle tracks, which can be picked and moved up or down, and the new order applied, or deleted
(**Delete…**; the last audio track is only removed if "Allow removing every audio track" is ticked). Each section also has
**Keep only** a language: choosing one marks, in the table, the tracks that would be removed (and which track
becomes the default) before anything is changed, and **Remove the others…** makes the change (`--preserve`). The same
section can **preview the arrangement, or the removal of the picked track's language, for all the ticked
files**: tracks are matched by language, and the preview in the results can then be applied.

Adding a subtitle file
----------------------

`--add subtitle --sub-file FILE --lang LANGUAGE [--name TEXT]` adds a subtitle file (`.srt`, `.ass`, `.ssa`,
`.vtt` or `.sup`) to an MKV or MP4 as a new subtitle track, the last one, in the language you give. It is not default
and not forced (change that afterwards with `--set default` or `--set forced`). The file is rewritten with
`mkvmerge` on a copy and checked, like every change; chapters, attachments and the other tracks are kept.

```
./fml --add subtitle --sub-file Film.it.srt --lang Italian --name "Italiano" --file Film.mkv
```

It works on MKV and MP4 files, not AVI. In an MP4 the subtitle is converted to `mov_text` text subtitles with `ffmpeg`
(`.srt`, `.ass`, `.ssa` and `.vtt` only, not a `.sup` picture subtitle, and no forced flag); the MP4 muxer makes the
first subtitle track the default if there was none, so the first subtitle added to a file without any is default.
It cannot be used with `--in-place`. `--undo` can
take the track out again only if the change was made with `--backup`, because it changes the number of tracks.
In the desktop app, **Add subtitles…** (in "1. Choose files") adds many at once: choose the subtitle files and each is
matched to a ticked MKV file by name (`Show.S01E03.ita.srt` goes with `Show.S01E03.mkv`, else the same season and
episode), with the language, the name (`SDH`, `Forced`) and the forced flag guessed from what follows the name. Check
the table, choose any language that could not be guessed, then **Preview** or add; each file is rewritten once with
all its new subtitles. The Edit dialog has an **Add a subtitle file** section: choose the file, the language (guessed
from the file name when it holds one, as in `Film.ita.srt`) and an optional name.

Example: a whole season
-----------------------

A season of episodes with the Italian audio first and the English second, played by something that always plays
the first track. The same four steps work for any folder of MKV files.

1. Look at the tracks of one episode:

   ```
   $ ./fml --info --file Show.1x01.mkv
   Show.1x01.mkv (Matroska)
   Track        ID  Codec              Language  Default  Forced  Name
   video:1      0   HEVC/H.265/MPEG-H  und       yes      -       HEVC
   audio:1      1   AAC                ita       yes      -       By Pir8
   audio:2      2   AAC                eng       -        -       Original Audio
   subtitles:1  3   SubRip/SRT         ita       yes      yes     Sub Ita Forced
   subtitles:2  4   SubRip/SRT         eng       -        -       Sub Eng Full
   subtitles:3  5   SubRip/SRT         ita       -        -       Sub Ita Full
   ```

2. Preview the change for the whole folder. Nothing is changed:

   ```
   $ ./fml --set lang --default English --move --dir /media/Show.S01 --dryrun
   [dry-run] Show.1x01.mkv: audio track 1 (ita): default yes -> no
   [dry-run] Show.1x01.mkv: audio track 2 (eng): default no -> yes, moved to the first audio track
   ...
   ```

3. Make the change. Each episode is rewritten once, as a checked copy that replaces the original only if it
   worked, so a failure on one episode leaves it as it was and the rest are still done:

   ```
   ./fml --set lang --default English --move --dir /media/Show.S01
   ```

4. Check the result: English is now the first audio track and the default.

   ```
   $ ./fml --get lang --dir /media/Show.S01
   Show.1x01.mkv: eng
   Show.1x02.mkv: eng
   ...
   ```

The forced Italian subtitle track in this example is still the default, so a player may show Italian subtitles
over the English audio. Turn the default subtitles off with:

```
./fml --set lang --default none --kind sub --dir /media/Show.S01
```

Each episode needs free space for a second copy while it is being rewritten, and the files are replaced, so
stop a torrent client seeding them first, or it will see them as changed.

Subtitles
---------

Add `--kind sub` (`-k sub`) to make an action work on subtitle tracks instead of audio.
The track numbers (`--track`) then count the subtitle tracks, as the `--info` table shows.

Make English the default subtitles. A track that is not forced is preferred, and `--forced`
chooses the forced track instead (the one that only subtitles foreign speech):

```
./fml --set lang --default English --kind sub --file video.mkv
./fml --set lang --default English --kind sub --forced --file video.mkv
```

Turn the default subtitles off, so that none show unless a viewer picks them (MKV only):

```
./fml --set lang --default none --kind sub --file video.mkv
```

The other actions work the same way:

```
./fml --get lang --kind sub --file video.mkv
./fml --delete lang --lang ita --kind sub --file video.mkv
./fml --preserve lang --lang eng --kind sub --file video.mkv
./fml --set lang --lang French --track 3 --kind sub --file video.mkv
```

Removing every subtitle track does not need `--force`.

Choosing a track by number
--------------------------

Files often have tracks tagged `und` (undetermined), so there is no language to choose by.
`--set default` makes a track the default by number, and `--lang` also tags it in the same edit:

```
./fml --set default --track 2 --lang English --file video.mkv
```

`--track last` is the last track. This also works with `--kind sub`.

Language preferences
--------------------

`--prefer` takes languages in order, and uses the first one that each file has. One command then
suits a library where the files differ:

```
./fml --set lang --prefer eng,jpn,fre --dir /media/films --recursive
```

A file with English audio gets English, a file with only Japanese and French gets Japanese, and a file
with none of them is skipped with a warning. With `--preserve` it keeps only the first preferred language
that a file has, and removes the rest:

```
./fml --preserve lang --prefer eng,jpn --dir /media/films
```

MP4 examples
------------

MP4 (and M4V and MOV) files work with the same actions, including subtitles:

```
./fml --set lang --default English --file video.mp4
./fml --set lang --default Italian --kind sub --file video.mp4
./fml --delete track --track last --file video.mp4
```

MP4 has no way to edit the tracks in place, so each change rewrites the file with `ffmpeg`, copying the
streams (nothing is re-encoded, so it is quick and lossless). The new file is read back and checked before
it replaces the original, and if the check fails the original is left alone. The index (`moov`) is moved
to the front of the file, as for streaming, and permissions and timestamps are kept.
`--default none` for subtitles is the exception: `ffmpeg` would enable the first subtitle track again, so it clears
the "enabled" bit in the track headers of a copy of the file instead (nothing is rewritten, so it is as quick as a
copy), and the copy is read back before it replaces the original.

Track names are kept: `ffmpeg` drops the names it can't read back, so they are written again, and the new file
is checked to have them.

AVI examples
------------

AVI has no track metadata to edit, so these rewrite the file, which takes longer.

Swap the first two audio tracks of an AVI file:

```
./fml --swap lang --file video.avi
```

Delete the last audio track from an AVI file:

```
./fml --delete track --track last --file video.avi
```

Directories
-----------

Use `--dir` instead of `--file` to process every file in a directory, and `--recursive` to include
subdirectories. Files that are not MKV or AVI are skipped. A failure on one file is reported
and the rest are still processed.

```
./fml --set lang --default English --dir /media/films --recursive
```

Several actions can be given. They run in the order given, each across all the files.

Each action has its own settings: the settings that follow an action on the command line belong to it, so two
actions can need different values. Settings given before the first action are shared by every action, and an
action's own settings are added on top of them (a setting given twice for one action uses the last):

```
./fml --set lang --default English --delete lang --lang ita --file video.mkv     # English default, then no Italian
./fml --kind sub --get lang --info --file video.mkv                              # --kind sub is for both
./fml --set lang --default French --move --set lang --default Italian -i a.mkv   # --move only for the first
```

The settings that go with an action are `--lang`, `--default`, `--prefer`, `--track`, `--kind`, `--forced`, `--move`,
`--name`, `--order`, `--order-lang`, `--default-first`, `--forced-flag`, `--sub-file` and `--output`. The rest
(`--file`, `--dir`, `--recursive`, `--include`, `--exclude`, `--dryrun`, `--backup`, `--log`, `--jobs`, `--force`,
`--in-place`, `--format` and `--verbose`) are for the whole command, wherever they are given. Each action is checked
with its own settings, so a mistake is reported for the action it is in.

Choosing files
--------------

`--include` and `--exclude` choose which files `--dir` processes. Each can be given more than once, and
matching ignores case. A pattern with no `/` matches a file name, or the name of any folder above it, so
`Extras` skips every folder of that name. A pattern with a `/` is matched against the whole path below the
directory, as in `Season 1/*`:

```
./fml --set lang --default English --dir /media/tv -r --include '*.mkv' --exclude Extras
./fml --get lang --dir /media/tv -r --include 'Season 1/*'
```

Several files at once
---------------------

`--jobs N` (`-j N`) processes N files at once. Rewriting is limited by the disk, so a few jobs help and many
rarely do, but when most of the time goes in the tools it is much quicker (24 files took 4.2 seconds one at a
time and 1.4 with `-j 4`). The output is the same, and in the same order, as a run one at a time. The free
space each file needs is set aside while it runs, so jobs can't use it up between them.

```
./fml --set lang --default English --move --dir /media/tv -r -j 4
```

Track names and the forced flag
-------------------------------

`--set name` sets the name of a track, and an empty name removes it. `--set forced` and `--set unforced` set
and clear the forced flag. They all take the track with `--track`, and `--kind sub` works on subtitles:

```
./fml --set name --track 2 --name "English 5.1" --file video.mkv
./fml --set name --track 1 --name "" --file video.mkv
./fml --set forced --track 2 --kind sub --file video.mkv
./fml --set unforced --track 2 --kind sub --file video.mkv
```

A name that starts with a dash has to be written with an equals sign, `--name=-text`, or it is taken for an
option. The forced flag can only be set for MKV files, because `mediainfo` can't read it from an MP4 to check
the change; names work for both.

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
$ ./fml --set lang --default English --file video.mkv --dryrun
[dry-run] video.mkv: audio track 1 (ita): default yes -> no
[dry-run] video.mkv: audio track 2 (eng): default no -> yes
```

Add `--verbose` to also show the commands that would be run. Verbose output goes to stderr, as do warnings,
so stdout only has the results.

Safe changes
------------

`fml` never changes your file until the new version is known to be good. Whatever it does, it works
on a separate copy and swaps it in at the end:

| Change                                              | How it is made                                                     |
|-----------------------------------------------------|--------------------------------------------------------------------|
| MKV default flags and languages (`--set`, `--swap`) | copy the file, edit the copy with `mkvpropedit`, check it, replace |
| MKV tracks removed (`--delete`, `--preserve`)       | `mkvmerge` writes a new file, which is checked, then replaces it   |
| MP4 and AVI                                         | `ffmpeg` writes a new file, which is checked, then replaces it     |

The check reads the new file back and confirms it has the expected tracks, in order, with their languages, names
and default flags (and, for MKV, forced flags), because a tool can exit successfully without applying a change
or can lose something. If the check or the tool fails, the original is left as it was,
the copy is removed, and the file is reported as an error. The new file replaces the original in a single atomic
step (there is never a moment with no file), and keeps its permissions and timestamps.

This needs free space beside the file for a second copy of it, and `fml` checks that first. If you stop it
with Ctrl-C or `kill`, the copy is removed.

Copying a large file takes time. For MKV metadata edits only, `--in-place` skips the copy and edits the file
directly, which is quick and needs no free space, but an interruption part way can leave the file damaged:

```
./fml --set lang --default English --file video.mkv --in-place
```

Keeping the original
--------------------

`--backup` keeps the original beside the changed file, named `NAME.fml-bak` (or `NAME.fml-bak.1` and so on if
that name is taken):

```
./fml --delete lang --lang eng --file video.mkv --backup
```

The backup is a hard link to the original, made just before the new file replaces it, so it costs no space
and is instant. Where links are not possible (some network shares and FAT file systems), or with `--in-place`
(which edits the file itself), it is a real copy, which needs the space. If the backup can't be made, nothing
is replaced. `--dir` ignores the backups, and so does everything else that looks for media by name, because
they end in `.fml-bak`. The JSON output has a `backup` field with the path.

Undoing changes
---------------

`--log FILE` records each change, one JSON line per changed file, with the tracks before and after:

```
./fml --set lang --default English --move --dir /media/tv -r --log changes.jsonl
```

`--undo FILE` (`-u`) then reverses them, newest first. A change is only undone if the file is still exactly as
that change left it, so a file that has been changed since is left alone and reported. `--file` or `--dir`
limits it to some of the files, and `--dryrun` shows what would be undone:

```
./fml --undo changes.jsonl --dryrun
./fml --undo changes.jsonl
./fml --undo changes.jsonl --dir "/media/tv/Season 1"
```

Changes to flags, languages, names and the order of tracks are undone by editing the file back. A removal
(`--delete`, `--preserve`) can only be undone from a `--backup` copy, which is checked against the log before it
is used and then moved back into place, so use `--backup` with `--log` for those. Without one the log says
the change can't be undone. An AVI swap also needs the backup. In a dry run only the newest change to a file is
checked, as the older ones depend on it.

Output formats
--------------

`--format json` prints a single JSON document instead of text, for use in scripts:

```
./fml --set lang --default English --dir /media/films --dryrun --format json
```

```json
{
  "version": "0.7.2",
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
      "backup": null,
      "data": {"layout": {"before": [...], "after": [...]}}
    }
  ]
}
```

`data.layout` (from every change that rewrites or edits the tracks of a file: `--set`, `--delete`, `--preserve`,
`--add` and `--swap`) holds the tracks of the file before and after the change, each as in `--info` (`type`,
`number`, `id`, `codec`, `language`, `default`, `forced`, `name`). The desktop app shows them side by side in its
preview. In a dry run it is what the file is expected to be.

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
./fml --help
```

```
usage: fml-cli [OPTIONS]

Fix the default audio and subtitle languages, and the tracks, of media files.

Options:
  -V, --version  print version
  -h, --help     Print help

files (MKV (and WebM), MP4 (M4V, MOV) and AVI files):
  -i, --file <FILE>        file to process
  -d, --dir <DIR>          directory to process
  -r, --recursive          include subdirectories of --dir
      --include <PATTERN>  with --dir, only process files that match PATTERN (a file name or folder name such as '*.mkv', or a path such as 'Season 1/*'); can be given more than once
      --exclude <PATTERN>  with --dir, skip files that match PATTERN (for example Extras skips every folder of that name); can be given more than once

actions (may be combined; they run in the order given):
  -C, --check
          check the required commands are installed
  -I, --info
          list the tracks of the file
  -u, --undo <LOG>
          undo the changes recorded in a --log file, newest first, for the files that are still as they were left (limited to --file or --dir if given)
  -g, --get <{lang}>
          get the default language, or with --lang the tracks in a language
  -c, --set <{lang,default,name,forced,unforced,order,track}>
          name: set the name of track --track to --name; forced, unforced: set or clear its forced flag (MKV); track: change the language (--lang), name (--name) and forced flag (--forced-flag) of track --track together; order: put the tracks of --kind in the order of --order; lang: make --default (or the first of --prefer) the default, or with --lang set the language of track --track (1); default: make track --track the default, and with --lang set its language
  -a, --add <{subtitle}>
          subtitle: add the subtitle file --sub-file to an MKV or MP4 as a new subtitle track in the language --lang (and the name --name); not default, not forced
  -x, --extract <{subtitle}>
          subtitle: write the subtitle track --track to a file (.srt, .ass, .vtt or .sup; an MP4's text subtitle as .srt), named after the media file unless --output says; the media file is not changed
  -s, --swap <{lang}>
          make the second audio track the default (the order is unchanged: see --move)
  -z, --delete <{lang,track}>
          remove the tracks in --lang, or track --track
  -p, --preserve <{lang}>
          remove all tracks that are not in --lang (or the first of --prefer that is present)

settings:
  -k, --kind <{audio,sub}>      the kind of track the actions work on (default audio) [default: audio]
  -l, --lang <LANG>             language, by name or ISO 639 code (English, en, eng)
  -D, --default <LANG>          language to make the default (none turns the default subtitles off)
      --prefer <LANGS>          languages in order of preference, e.g. eng,jpn,fre: the first that a file has is used, so one command suits a mixed library
      --forced...               with --kind sub, pick the forced subtitle track
  -t, --track <N>               a track number from 1 (among tracks of the kind), last, or any track by audio:N, subtitles:N, id:N (the ID column of --info) or name:TEXT
  -f, --force                   allow removing every audio track
      --output <FILE>           for --extract subtitle: the file to write, or a folder to put the generated name (FILM.ita.srt) in; an existing file needs --force
      --sub-file <FILE>         for --add subtitle: the subtitle file (.srt, .ass, .ssa, .vtt or .sup) to add; the file is rewritten
      --forced-flag <{yes,no}>  for --set track: set (yes) or clear (no) the forced flag of the track (MKV only)
      --order-lang <LANGS>      for --set order, instead of --order: languages in the order their tracks should come, e.g. ita,eng; tracks in other languages follow. Works on files whose tracks differ, so one command suits a whole season
      --default-first...        with --set order, also make the track that ends up first the only default of its kind, for players that follow the flag and players that play the first track
      --order <N,N,...>         for --set order: the current number of each track of the kind, in the order they should have, e.g. 2,1,3 swaps the first two of three; the file is rewritten
      --name <TEXT>             the track name for --set name (an empty name removes it)
      --move...                 with --set, also move the chosen track to be the first track of its kind, for players that play the first track whatever its flag; the file is rewritten
  -j, --jobs <N>                process N files at once (default 1; rewriting is limited by the disk, so more than a few rarely helps) [default: 1]
      --log <FILE>              record each change in FILE (one JSON line per file), so that --undo can reverse it
      --backup                  keep the original beside the changed file, as NAME.fml-bak (a hard link where possible, so it costs no space)
      --in-place                edit MKV files directly instead of a copy that replaces them if it worked: quicker and needs no free space, but an interruption can leave the file damaged
  -T, --dryrun                  show what would change without changing anything
  -v, --verbose                 show the commands that are run
  -F, --format <format>         output format (default text) [default: text]
      --gui                     open the desktop app (fml) instead; takes no other options
      --install                 install missing commands (apt, dnf, pacman, brew, winget or choco, by system)



examples:
  fml --set lang --default English --file video.mkv     make English the default audio
  fml --set lang --default English --move -i video.mkv  ... and move it to be the first audio track
  fml --set lang --prefer eng,jpn,fre --dir films       the best of those languages in each file
  fml --set default --track 2 --lang English -i a.mkv   make audio track 2 the default and English
  fml --set lang --default English --kind sub -i a.mkv  make English the default subtitles
  fml --set lang --default English --kind sub --forced  ... using the forced subtitle track
  fml --set lang --default none --kind sub -i a.mkv     turn the default subtitles off
  fml --get lang --dir /media/films --recursive         default audio language of each film
  fml --preserve lang --lang eng --file video.mkv       remove the audio that isn't English
  fml --delete track --track last --file video.mp4      remove the last audio track
  fml --set lang --default French --dir films --dryrun  show what would change
  fml --set lang --default eng --delete lang --lang ita -i a.mkv   each action has its own settings
```

Installing
----------

Build the command line tool, and the desktop app if you want it (it needs `cargo` 1.80 or newer; the app also
needs the system webview, on Ubuntu `libwebkit2gtk-4.1-dev` and `libgtk-3-dev`):

```
cd rust
cargo build --release                # the command line tool: rust/target/release/fml-cli
cargo build -p fml-gui --release     # the desktop app: rust/target/release/fml
```

`./fml` in this directory runs the release build (or the debug build if there is no release one), and
`./fml --gui` opens the desktop app. To get an `fml-cli` command on your `PATH` instead, copy the built files
into a directory on it, the app beside the command (`fml-cli --gui` looks for the app, the program called `fml`, next to itself, then
on `PATH` (skipping scripts, such as the archived Python `fml`), or at the path in `FML_GUI`):

```
install -m 755 rust/target/release/fml-cli rust/target/release/fml ~/.local/bin/
fml-cli --version
```

For a package of the app, `cd rust/fml-gui && cargo tauri build --bundles deb` leaves a `.deb` in
`rust/target/release/bundle/deb` (it needs `cargo install tauri-cli` first). The commands in
[Requirements](#requirements) are still needed, and `fml-cli --check --install` installs them.

On macOS, `brew install mkvtoolnix mediainfo ffmpeg` installs the commands (or `fml-cli --check --install`), and
`./build.sh --target mac [--arch arm64|x64|universal]` builds `fml_<version>_<arch>.dmg` and the `fml-cli` archive
into `dist/` (it needs `cargo install tauri-cli --version '^2'`; the system web view is built in). Drag `fml.app` to
`/Applications`, and `fml-cli --gui` will find it. The app adds Homebrew's folders (`/opt/homebrew/bin`,
`/usr/local/bin`) to its `PATH`, so it finds the commands even when opened from Finder. The `.dmg` is not signed or
notarised, so macOS may ask you to allow it in System Settings, Privacy & Security.

`./build.sh --target windows` on Linux or a Mac cross-compiles the Windows installer (`fml_<version>_x64-setup.exe`)
and `fml-cli-<version>-windows.zip` in a Docker container (MinGW, `x86_64-pc-windows-gnu`; needs `docker`, nothing
else), and leaves them in `dist/`. `docker/windows/smoke.sh` then runs the built `fml-cli.exe` under Wine against
stand-in tools as a smoke test. On Windows itself the installer's tools (Rust, tauri-cli) are needed and
`./build.sh --target windows` builds there. The Windows build has not been run on a real Windows machine yet; the
`mkvtoolnix`, `mediainfo` and `ffmpeg` commands must be on `PATH` there too.

Development
-----------

```
cd rust
cargo build --release     # the command line tool
cargo test                # unit tests of the library, the command line and the app backend
cd ..
FML_BIN=rust/target/release/fml-cli pytest     # the Python test suite, run against the Rust binary
```

The tests in `tests/` need nothing but `pytest` for most of them (they use stand-ins for the media tools).
The ones marked `real` generate media files with `ffmpeg` and check the results with `mkvmerge`, `mediainfo` and
`ffprobe`, and are skipped if those are not installed (`pytest -m "not real"` leaves them out). They only
write to temporary directories. With `FML_BIN` set, the tests that call the archived `fml.py`'s own functions
are skipped (see [`rust/README.md`](rust/README.md)). `rust/README.md` also says what the suite does not cover.

Moving from fml.sh
------------------

`fml.sh` is the original bash version. It is archived and **no longer maintained**: it is kept in the repository,
and still has its own `./fml.sh --help`, but it gets no fixes or new features, and `fml` replaces it.
The main changes are:

| fml.sh                                         | fml                                                     |
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

Attributions
------------

<a href="https://www.flaticon.com/free-icons/cameraman" title="cameraman icons">Cameraman icons created by MEDZ - Flaticon</a>
