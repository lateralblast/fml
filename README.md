![FML](fml.jpg)

FML
===

Fix Media Language (etc)

Version
-------

Version 0.2.5

License
-------

CC BY-SA: https://creativecommons.org/licenses/by-sa/4.0/

Fund me here: https://ko-fi.com/richardatlateralblast

Introduction
------------

This script was initially designed to change the default (first) language track on on media files.
For example if, if the video file has two tracks, the first being Italian and the
second being English and you want to swap those so that the first track is English.

Goals
-----

The main goal initially as described above was to change the default language track,
but I've tried to make the platform generic enough to add additional capabilities as needed.

Currently the first version supports MKV files, but the plan is to use mediainfo
to add support for other files.

MKV Examples
------------

As MKV is a container, you can generally just update the metadata to alter a file if you are not altering the content.

Set default language to English for a MKV file:

```
./fml.sh --set lang --default English --file video.mkv
```

Get default language from a MKV file:

```
./fml.sh --get lang --lang default --file video.mkv
```

If you need to delete a track, this can also be done.

Delete all non English tracks from a MKV file:

```
./fml.sh --preserve lang --lang eng --file video.mkv
```

Delete all English tracks from a MKV file:

```
./fml.sh --delete lang --lang eng --file video.mkv
```

If the metadata is poor quality, e.g. the language information is incorrect or not present,
you can use the swap option to swap the first two tracks.

AVI Examples
------------

With AVI files, you generally need to rewrite the file, so this will obviously take longer
than altering just the metadata.

Swap first two audio tracks of an AVI file:

```
./fml.sh --swap lang --file video.avi --options verbose
```

Delete last audio track from AVI file:

```
./fml.sh --delete track --track last --file video.avi --options verbose
```

Help
----

When the --help, or --usage switch is used.

```
./fml.sh --help

Usage: fml.sh --action(s) [action(,action)] --option(s) [option(,option)]

switch(es):
---------
--action*)
    Action to perform
--debug)
    Enable debug mode
--default*)
    Set default
--delete)
    Delete item from file (e.g. track)
--dir*)
    Directory to process
--dryrun)
    Enable dryrun mode
--file)
    File to process
--force)
    Enable force mode
--format*)
    Set output format
--get)
    Get information about file
--help|-h)
    Print help information
--info)
    Get information about file
--lang*)
    Set language
--option*)
    Options to set
--preserve*|--leave*)
    Preserve item from file (e.g. track)
--recursive)
    Enable recursive mode
--set)
    Set information about file
--shellcheck)
    Run shellcheck against script
--strict)
    Enable strict mode
--swap)
    Swap information about file
--track)
    Track to perform operation on
--usage)
    Display usage
--verbose)
    Enable verbose mode
--version|-V)
    Print version information
```

Get help on options:

```
./fml.sh --usage options

Usage: fml.sh --action(s) [action(,action)] --option(s) [option(,option)]

option(s):
---------
recursive (default = false)
   Recursively process directory
verbose (default = false)
   Verbose mode
strict (default = false)
   Strict mode
dryrun (default = false)
   Dryrun mode
debug (default = false)
   Debug mode
force (default = false)
   Force actions
yes (default = false)
   Answer yes to questions
format (default = JSON)
   Information format
```

Get help on actions:

```
./fml.sh --usage actions

Usage: fml.sh --action(s) [action(,action)] --option(s) [option(,option)]

action(s):
---------
get|info)
    Get file information
help)
    Print actions help
delete*)
    Delete file information
version)
    Print version
pres*|leave*)
    Preserve/leave file information
printenv*)
    Print environment
printdefaults)
    Print defaults
set)
    Set file information
shellcheck)
    Shellcheck script
swap)
    Swap file information
```
