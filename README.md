![FML](fml.jpg)

FML
---

Fix Media Language (etc)

Version
-------

Version 0.1.4

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

Examples
--------

Set default language to English:

```
./fml.sh --set lang --default English --file video.mkv
```

Get default language:

```
./fml.sh --get lang --lang default --file video.mkv
```

Help
----

When the --help, or --usage switch is used.

```
 ./fml.sh --help

Usage: fml.sh --action(s) [action(,action)] --option(s) [option(,option)]

switch(s):
---------
--action*)
    Action to perform
--debug)
    Enable debug mode
--default*)
    Set output format
--dryrun)
    Enable debug mode
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
    Action to perform
--set)
    Get information about file
--strict)
    Enable strict mode
--swap)
    Get information about file
--usage)
    Action to perform
--verbose)
    Enable verbos e mode
--version|-V)
    Print version information
```

Get help on options:

```
./fml.sh --usage options

Usage: fml.sh --action(s) [action(,action)] --option(s) [option(,option)]

option(s):
---------
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
version)
    Print version
printenv*)
    Print environment
printdefaults)
    Print defaults
set)
    Set file information
shellcheck)
    Shellcheck script
swap)
    Set file information
```