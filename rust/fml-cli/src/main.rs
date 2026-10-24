//! fml: fix the default audio and subtitle languages, and the tracks, of media files.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use clap::{Arg, ArgAction, ArgMatches, Command};
use fml_core::languages::resolve_language;
use fml_core::{
    format_tracks, info_text, Config, Event, Fml, Kind, Outcome, Status, Task, TaskSettings, Track, VERSION,
};
use serde_json::{json, Value};

const EXAMPLES: &str = "examples:
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
  fml --set lang --default eng --delete lang --lang ita -i a.mkv   each action has its own settings";

/// The words each action takes.
const TASK_CHOICES: &[(&str, &[&str])] = &[
    ("get", &["lang"]),
    ("set", &["lang", "default", "name", "forced", "unforced", "order", "track"]),
    ("add", &["subtitle"]),
    ("extract", &["subtitle"]),
    ("swap", &["lang"]),
    ("delete", &["lang", "track"]),
    ("preserve", &["lang"]),
];

fn language_arg(text: &str) -> Result<String, String> {
    resolve_language(text)
}

fn default_arg(text: &str) -> Result<String, String> {
    if text.trim().eq_ignore_ascii_case("none") {
        Ok("none".into())
    } else {
        language_arg(text)
    }
}

/// A comma separated list of languages, without repeats.
fn prefer_arg(text: &str) -> Result<String, String> {
    let mut languages: Vec<String> = Vec::new();
    for item in text.split(',').filter(|item| !item.trim().is_empty()) {
        let language = language_arg(item)?;
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    if languages.is_empty() {
        return Err("give at least one language".into());
    }
    Ok(languages.join(","))
}

fn kind_arg(text: &str) -> Result<String, String> {
    Kind::parse(text)
        .map(|kind| kind.name().to_string())
        .ok_or_else(|| format!("\"{text}\" is not audio or sub (subtitles)"))
}

/// A value that must be one of a list, worded as `argparse` words it.
fn choice_arg(text: &str, choices: &[&str]) -> Result<String, String> {
    if choices.contains(&text) {
        Ok(text.to_string())
    } else {
        let list = choices.iter().map(|c| format!("'{c}'")).collect::<Vec<_>>().join(", ");
        Err(format!("invalid choice: '{text}' (choose from {list})"))
    }
}

fn jobs_arg(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(jobs) if jobs >= 1 && text.chars().all(|c| c.is_ascii_digit()) => Ok(jobs),
        _ => Err(format!("\"{text}\" is not a number of jobs (1 or more)")),
    }
}

fn track_arg(text: &str) -> Result<String, String> {
    fml_core::TrackSpec::parse(text).map(|_| text.to_string())
}

fn build_command() -> Command {
    let mut command = Command::new("fml-cli")
        .version(VERSION)
        .about("Fix the default audio and subtitle languages, and the tracks, of media files.")
        .after_help(EXAMPLES)
        .help_template("usage: {usage}\n\n{about}\n\n{all-args}\n\n{after-help}")
        .infer_long_args(true)
        .disable_version_flag(true)
        .arg(
            Arg::new("version")
                .short('V')
                .long("version")
                .action(ArgAction::Version)
                .help("print version"),
        );
    let files = "files (MKV (and WebM), MP4 (M4V, MOV) and AVI files)";
    command = command
        .arg(
            Arg::new("file")
                .short('i')
                .long("file")
                .value_name("FILE")
                .conflicts_with("dir")
                .help("file to process")
                .help_heading(files),
        )
        .arg(
            Arg::new("dir")
                .short('d')
                .long("dir")
                .value_name("DIR")
                .help("directory to process")
                .help_heading(files),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .action(ArgAction::SetTrue)
                .help("include subdirectories of --dir")
                .help_heading(files),
        )
        .arg(
            Arg::new("include")
                .long("include")
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help(
                    "with --dir, only process files that match PATTERN (a file name or folder \
                     name such as '*.mkv', or a path such as 'Season 1/*'); can be given more \
                     than once",
                )
                .help_heading(files),
        )
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help(
                    "with --dir, skip files that match PATTERN (for example Extras skips every \
                     folder of that name); can be given more than once",
                )
                .help_heading(files),
        );
    let actions = "actions (may be combined; they run in the order given)";
    command = command
        .arg(
            Arg::new("check")
                .short('C')
                .long("check")
                .action(ArgAction::SetTrue)
                .help("check the required commands are installed")
                .help_heading(actions),
        )
        .arg(
            Arg::new("info")
                .short('I')
                .long("info")
                .action(ArgAction::SetTrue)
                .help("list the tracks of the file")
                .help_heading(actions),
        )
        .arg(
            Arg::new("undo")
                .short('u')
                .long("undo")
                .value_name("LOG")
                .action(ArgAction::Append)
                .help(
                    "undo the changes recorded in a --log file, newest first, for the files \
                     that are still as they were left (limited to --file or --dir if given)",
                )
                .help_heading(actions),
        );
    let action_help = [
        ("get", 'g', "get the default language, or with --lang the tracks in a language"),
        (
            "set",
            'c',
            "name: set the name of track --track to --name; forced, unforced: set or clear its \
             forced flag (MKV); track: change the language (--lang), name (--name) and forced flag \
             (--forced-flag) of track --track together; order: put the tracks of --kind in the order of --order; lang: make --default (or the first of --prefer) the default, or \
             with --lang set the language of track --track (1); default: make track --track the \
             default, and with --lang set its language",
        ),
        (
            "add",
            'a',
            "subtitle: add the subtitle file --sub-file to an MKV or MP4 as a new subtitle track in \
             the language --lang (and the name --name); not default, not forced",
        ),
        (
            "extract",
            'x',
            "subtitle: write the subtitle track --track to a file (.srt, .ass, .vtt or .sup; an MP4's \
             text subtitle as .srt), named after the media file unless --output says; the media \
             file is not changed",
        ),
        ("swap", 's', "make the second audio track the default (the order is unchanged: see --move)"),
        ("delete", 'z', "remove the tracks in --lang, or track --track"),
        (
            "preserve",
            'p',
            "remove all tracks that are not in --lang (or the first of --prefer that is present)",
        ),
    ];
    for (name, short, help) in action_help {
        let choices = TASK_CHOICES.iter().find(|(n, _)| *n == name).map_or(&[][..], |(_, c)| *c);
        command = command.arg(
            Arg::new(name)
                .short(short)
                .long(name)
                .value_name(&*Box::leak(format!("{{{}}}", choices.join(",")).into_boxed_str()))
                .value_parser(move |text: &str| choice_arg(text, choices))
                .action(ArgAction::Append)
                .help(help)
                .help_heading(actions),
        );
    }
    let settings = "settings";
    command
        .arg(
            Arg::new("kind")
                .short('k')
                .long("kind")
                .action(ArgAction::Append)
                .value_name("{audio,sub}")
                .value_parser(kind_arg)
                .default_value("audio")
                .help("the kind of track the actions work on (default audio)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("lang")
                .short('l')
                .long("lang")
                .action(ArgAction::Append)
                .value_name("LANG")
                .value_parser(language_arg)
                .help("language, by name or ISO 639 code (English, en, eng)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("default")
                .short('D')
                .long("default")
                .action(ArgAction::Append)
                .value_name("LANG")
                .value_parser(default_arg)
                .help("language to make the default (none turns the default subtitles off)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("prefer")
                .long("prefer")
                .action(ArgAction::Append)
                .value_name("LANGS")
                .value_parser(prefer_arg)
                .help(
                    "languages in order of preference, e.g. eng,jpn,fre: the first that a file \
                     has is used, so one command suits a mixed library",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("forced")
                .long("forced")
                .action(ArgAction::Count)
                .help("with --kind sub, pick the forced subtitle track")
                .help_heading(settings),
        )
        .arg(
            Arg::new("track")
                .short('t')
                .long("track")
                .action(ArgAction::Append)
                .value_name("N")
                .allow_negative_numbers(true)
                .value_parser(track_arg)
                .help(
                    "a track number from 1 (among tracks of the kind), last, or any track by \
                     audio:N, subtitles:N, id:N (the ID column of --info) or name:TEXT",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("force")
                .short('f')
                .long("force")
                .action(ArgAction::SetTrue)
                .help("allow removing every audio track")
                .help_heading(settings),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .value_name("FILE")
                .help(
                    "for --extract subtitle: the file to write, or a folder to put the generated \
                     name (FILM.ita.srt) in; an existing file needs --force",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("sub-dir")
                .long("sub-dir")
                .action(ArgAction::Append)
                .value_name("DIR")
                .help(
                    "for --add subtitle, instead of --sub-file: add the subtitle files in DIR (and \
                     below it) to the files being processed, each matched by name (FILM.ita.srt \
                     goes with FILM.mkv, else the same season and episode); the language, \
                     Forced and SDH come from the name, --lang is used where it has none",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("sub-file")
                .long("sub-file")
                .action(ArgAction::Append)
                .value_name("FILE")
                .help(
                    "for --add subtitle: the subtitle file (.srt, .ass, .ssa, .vtt or .sup) to \
                     add; the file is rewritten",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("forced-flag")
                .long("forced-flag")
                .value_name("{yes,no}")
                .value_parser(|text: &str| choice_arg(text, &["yes", "no"]))
                .help("for --set track: set (yes) or clear (no) the forced flag of the track (MKV only)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("order-lang")
                .long("order-lang")
                .action(ArgAction::Append)
                .value_name("LANGS")
                .value_parser(prefer_arg)
                .help(
                    "for --set order, instead of --order: languages in the order their tracks \
                     should come, e.g. ita,eng; tracks in other languages follow. Works on \
                     files whose tracks differ, so one command suits a whole season",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("default-first")
                .long("default-first")
                .action(ArgAction::Count)
                .help(
                    "with --set order, also make the track that ends up first the only default \
                     of its kind, for players that follow the flag and players that play the \
                     first track",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("order")
                .long("order")
                .value_name("N,N,...")
                .value_parser(|text: &str| fml_core::parse_order(text).map(|_| text.to_string()))
                .help(
                    "for --set order: the current number of each track of the kind, in the order \
                     they should have, e.g. 2,1,3 swaps the first two of three; the file is \
                     rewritten",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .action(ArgAction::Append)
                .value_name("TEXT")
                .help("the track name for --set name (an empty name removes it)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("move")
                .long("move")
                .action(ArgAction::Count)
                .help(
                    "with --set, also move the chosen track to be the first track of its kind, \
                     for players that play the first track whatever its flag; the file is \
                     rewritten",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("jobs")
                .short('j')
                .long("jobs")
                .value_name("N")
                .value_parser(jobs_arg)
                .default_value("1")
                .help(
                    "process N files at once (default 1; rewriting is limited by the disk, so \
                     more than a few rarely helps)",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("log")
                .long("log")
                .value_name("FILE")
                .help(
                    "record each change in FILE (one JSON line per file), so that --undo can \
                     reverse it",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("backup")
                .long("backup")
                .action(ArgAction::SetTrue)
                .help(
                    "keep the original beside the changed file, as NAME.fml-bak (a hard link \
                     where possible, so it costs no space)",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("in-place")
                .long("in-place")
                .action(ArgAction::SetTrue)
                .help(
                    "edit MKV files directly instead of a copy that replaces them if it worked: \
                     quicker and needs no free space, but an interruption can leave the file \
                     damaged",
                )
                .help_heading(settings),
        )
        .arg(
            Arg::new("dryrun")
                .short('T')
                .long("dryrun")
                .action(ArgAction::SetTrue)
                .help("show what would change without changing anything")
                .help_heading(settings),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .help("show the commands that are run")
                .help_heading(settings),
        )
        .arg(
            Arg::new("format")
                .short('F')
                .long("format")
                .value_parser(|text: &str| choice_arg(text, &["text", "json"]))
                .default_value("text")
                .help("output format (default text)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("gui")
                .long("gui")
                .action(ArgAction::SetTrue)
                .help("open the desktop app (fml) instead; takes no other options")
                .help_heading(settings),
        )
        .arg(
            Arg::new("install")
                .long("install")
                .action(ArgAction::SetTrue)
                .help("install missing commands (apt, dnf, pacman, brew, winget or choco, by system)")
                .help_heading(settings),
        )
}

/// What `argparse` does for a combination it rejects: say so, and exit with status 2.
fn usage_error(message: &str) -> ! {
    eprintln!("usage: fml-cli [-h] [-V] [options]");
    eprintln!("fml-cli: error: {message}");
    std::process::exit(2);
}

/// Report a command line that could not be read, as `argparse` words it.
fn explain(error: clap::Error) -> ! {
    use clap::error::{ContextKind, ErrorKind};
    let value = |kind: ContextKind| {
        error.get(kind).map(|v| v.to_string().split(" <").next().unwrap_or("").to_string())
    };
    match error.kind() {
        ErrorKind::UnknownArgument => {
            usage_error(&format!("unrecognized arguments: {}", value(ContextKind::InvalidArg).unwrap_or_default()))
        }
        ErrorKind::ArgumentConflict => usage_error(&format!(
            "argument {}: not allowed with argument {}",
            value(ContextKind::InvalidArg).unwrap_or_default(),
            value(ContextKind::PriorArg).unwrap_or_default()
        )),
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => {
            let detail = error.get(ContextKind::InvalidValue).map(|v| v.to_string()).unwrap_or_default();
            let message = error.to_string();
            let reason = message.split("': ").nth(1).unwrap_or(&detail).lines().next().unwrap_or("");
            usage_error(&format!(
                "argument {}: {reason}",
                value(ContextKind::InvalidArg).unwrap_or_default()
            ))
        }
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => error.exit(),
        _ => {
            let text = error.to_string();
            usage_error(text.trim_start_matches("error: ").lines().next().unwrap_or(""))
        }
    }
}

/// The actions, in the order they were given on the command line, each with the place it was
/// given at (the settings that follow it belong to it).
fn queued_tasks(matches: &ArgMatches) -> Vec<(usize, Task)> {
    let mut queue: Vec<(usize, Task)> = Vec::new();
    for name in ["check", "info"] {
        if matches.get_flag(name) {
            for index in matches.indices_of(name).into_iter().flatten() {
                queue.push((index, Task::new(name, "")));
            }
        }
    }
    let valued = ["undo", "get", "set", "add", "extract", "swap", "delete", "preserve"];
    for name in valued {
        let values: Vec<String> = matches.get_many::<String>(name).into_iter().flatten().cloned().collect();
        let indices: Vec<usize> = matches.indices_of(name).into_iter().flatten().collect();
        for (value, index) in values.into_iter().zip(indices) {
            queue.push((index, Task::new(name, &value)));
        }
    }
    queue.sort_by_key(|(index, _)| *index);
    queue
}

fn text(matches: &ArgMatches, name: &str) -> String {
    matches.get_one::<String>(name).cloned().unwrap_or_default()
}

fn many(matches: &ArgMatches, name: &str) -> Vec<String> {
    matches.get_many::<String>(name).into_iter().flatten().cloned().collect()
}

/// The settings that belong to an action, each with where it was given on the command line (a
/// switch with no value has "true"). A setting that was not given, only defaulted, is left out.
fn setting_occurrences(matches: &ArgMatches) -> Vec<(usize, &'static str, String)> {
    let mut found = Vec::new();
    let valued = [
        "lang", "default", "prefer", "track", "kind", "name", "order", "order-lang", "forced-flag",
        "sub-file", "sub-dir", "output",
    ];
    for name in valued {
        let values = many(matches, name);
        let indices: Vec<usize> = matches.indices_of(name).into_iter().flatten().collect();
        if values.len() == indices.len() {
            found.extend(indices.into_iter().zip(values).map(|(index, value)| (index, name, value)));
        }
    }
    for name in ["forced", "move", "default-first"] {
        // a switch that was not given still has a count (0) and an index of its own
        if matches.get_count(name) > 0 {
            found.extend(matches.indices_of(name).into_iter().flatten().map(|i| (i, name, "true".to_string())));
        }
    }
    found.sort_by_key(|(index, _, _)| *index);
    found
}

fn set_setting(settings: &mut TaskSettings, name: &str, value: String) {
    let list = |text: &str| -> Vec<String> {
        text.split(',').filter(|s| !s.is_empty()).map(String::from).collect()
    };
    match name {
        "lang" => settings.lang = Some(value),
        "default" => settings.default = Some(value),
        "prefer" => settings.prefer = Some(list(&value)),
        "track" => settings.track = Some(value),
        "kind" => settings.kind = Some(Kind::parse(&value).unwrap_or(Kind::Audio)),
        "name" => settings.name = Some(value),
        "order" => settings.order = fml_core::parse_order(&value).ok(),
        "order-lang" => settings.order_lang = Some(list(&value)),
        "forced-flag" => settings.forced_flag = Some(value == "yes"),
        "sub-file" => settings.sub_file = Some(value),
        "sub-dir" => settings.sub_dir = Some(value),
        "output" => settings.output = Some(value),
        "forced" => settings.forced = Some(true),
        "move" => settings.move_first = Some(true),
        "default-first" => settings.default_first = Some(true),
        _ => {}
    }
}

/// Reject combinations of arguments that make no sense, and build the settings. A setting that
/// follows an action belongs to that action; one given before the first action is shared by all.
fn config_from(matches: &ArgMatches) -> Config {
    let mut queue = queued_tasks(matches);
    let mut base = TaskSettings::default();
    for (index, name, value) in setting_occurrences(matches) {
        match queue.iter_mut().rev().find(|(at, _)| *at < index) {
            Some((_, task)) => set_setting(&mut task.settings, name, value),
            None => set_setting(&mut base, name, value),
        }
    }
    let tasks: Vec<Task> = queue.into_iter().map(|(_, task)| task).collect();
    let (file, dir) = (text(matches, "file"), text(matches, "dir"));
    let config = Config {
        paths: Vec::new(),
        file,
        dir,
        recursive: matches.get_flag("recursive"),
        lang: base.lang.unwrap_or_default(),
        default: base.default.unwrap_or_default(),
        prefer: base.prefer.unwrap_or_default(),
        track: base.track.unwrap_or_default(),
        kind: base.kind.unwrap_or(Kind::Audio),
        forced: base.forced.unwrap_or(false),
        verbose: matches.get_flag("verbose"),
        dryrun: matches.get_flag("dryrun"),
        install: matches.get_flag("install"),
        force: matches.get_flag("force"),
        in_place: matches.get_flag("in-place"),
        backup: matches.get_flag("backup"),
        log: text(matches, "log"),
        include: many(matches, "include"),
        exclude: many(matches, "exclude"),
        jobs: matches.get_one::<usize>("jobs").copied().unwrap_or(1),
        move_first: base.move_first.unwrap_or(false),
        name: base.name,
        order: base.order.unwrap_or_default(),
        sub_file: base.sub_file.unwrap_or_default(),
        sub_dir: base.sub_dir.unwrap_or_default(),
        sub_matches: Default::default(),
        output: base.output.unwrap_or_default(),
        progress: false,
        subtitles: Vec::new(),
        default_first: base.default_first.unwrap_or(false),
        forced_flag: base.forced_flag,
        order_lang: base.order_lang.unwrap_or_default(),
        tasks,
    };
    check_arguments(&config);
    config
}

fn check_arguments(args: &Config) {
    if let Err(message) = fml_core::validate(args) {
        usage_error(&message);
    }
}

/// Print a result as text, as the Python version does.
fn print_result(result: &Outcome, config: &Config) {
    let name = if result.file.is_empty() { String::new() } else { format!("{}: ", result.file) };
    if result.action == "check" {
        for entry in result.data.as_array().into_iter().flatten() {
            let tool = entry["tool"].as_str().unwrap_or("");
            match entry["path"].as_str() {
                Some(path) => println!("Found:\t\t{tool} ({path})"),
                None => println!("Missing:\t{tool}"),
            }
        }
    }
    if matches!(result.status, Status::Error | Status::Skipped) {
        eprintln!("Warning: {name}{}", result.message);
    } else if result.action == "check" {
    } else if result.action == "info" {
        println!("{}", info_text(result));
    } else if result.action == "get" {
        print_get(result, config);
    } else if !result.changes.is_empty() {
        let prefix = if result.dry_run { "[dry-run] " } else { "" };
        for change in &result.changes {
            println!("{prefix}{name}{change}");
        }
    } else {
        println!("{name}{}", result.message);
    }
}

fn print_get(result: &Outcome, config: &Config) {
    if let Some(tracks) = result.data.get("tracks") {
        let tracks: Vec<Track> = serde_json::from_value(tracks.clone()).unwrap_or_default();
        println!("{}", result.file);
        println!("{}", format_tracks(&tracks));
        return;
    }
    let languages: Vec<String> = result
        .data
        .as_object()
        .and_then(|data| data.iter().find(|(key, _)| key.starts_with("default_")))
        .and_then(|(_, value)| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let languages = if languages.is_empty() { vec!["none".to_string()] } else { languages };
    for language in languages {
        if config.dir.is_empty() {
            println!("{language}");
        } else {
            println!("{}: {language}", result.file);
        }
    }
}

#[cfg(unix)]
extern "C" fn stop_running(_signal: libc::c_int) {
    fml_core::request_cancel();
}

/// Handle being asked to stop (Ctrl-C or `kill`) so that temporary files are removed.
#[cfg(unix)]
fn handle_signals() {
    // SAFETY: the handler only stores to an atomic, which is allowed in a signal handler.
    unsafe {
        libc::signal(libc::SIGTERM, stop_running as *const () as usize);
        // a background job in a non-interactive shell ignores Ctrl-C, and keeps doing so
        let previous = libc::signal(libc::SIGINT, stop_running as *const () as usize);
        if previous == libc::SIG_IGN {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
        }
    }
}

/// Handle Ctrl-C and the console being closed, so that temporary files are removed.
#[cfg(windows)]
fn handle_signals() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    unsafe extern "system" fn stop_running(_event: u32) -> i32 {
        fml_core::request_cancel();
        1 // handled: the run stops by itself, after it has cleaned up
    }
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        SetConsoleCtrlHandler(Some(stop_running), 1);
    }
}

/// The name of the desktop app program.
const APP: &str = if cfg!(windows) { "fml.exe" } else { "fml" };

/// The desktop app is the program called `fml` (`fml.exe` on Windows). A script of that name (the archived
/// Python `fml`, or the `./fml` wrapper in a checkout, which would start this program
/// again) is not it.
fn is_app(path: &Path) -> bool {
    use std::io::Read;
    let mut start = [0u8; 2];
    path.is_file()
        && std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut start)).is_ok()
        && &start != b"#!"
}

/// Where the desktop app is: `FML_GUI` if it names a file, else `fml` beside this
/// program, else the first `fml` on `PATH` that is a program and not a script.
fn find_gui(own_dir: Option<&Path>, override_path: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = override_path.map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(path);
    }
    let beside = own_dir.map(|dir| dir.join(APP)).filter(|p| is_app(p));
    beside.or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|dir| dir.join(APP)).find(|p| is_app(p))
    })
}

/// On macOS the app is installed as `fml.app` (from the .dmg): look beside this program, then in
/// `~/Applications` and `/Applications`, for the program inside the bundle. `FML_APP_DIRS` (folders
/// separated as in `PATH`) is searched instead of those, which is how the tests keep a real
/// installation out of the way.
fn find_app_bundle(own_dir: Option<&Path>) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let folders: Vec<PathBuf> = match std::env::var_os("FML_APP_DIRS") {
        Some(dirs) => std::env::split_paths(&dirs).collect(),
        None => {
            let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
            own_dir
                .map(Path::to_path_buf)
                .into_iter()
                .chain(home)
                .chain([PathBuf::from("/Applications")])
                .collect()
        }
    };
    folders.into_iter().map(|dir| dir.join("fml.app/Contents/MacOS/fml")).find(|p| is_app(p))
}

/// Start the app: on Unix this program becomes it; on Windows (no `exec`) it is started and
/// this program ends. Only returns if it could not be started.
fn start_app(gui: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(std::process::Command::new(gui).exec())
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new(gui).spawn().map(|_| ())
    }
}

/// `fml-cli --gui`: open the desktop app.
fn open_gui() -> ExitCode {
    if std::env::args().len() > 2 {
        usage_error("--gui opens the app and takes no other options");
    }
    let own_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    let found = find_gui(own_dir.as_deref(), std::env::var("FML_GUI").ok().as_deref())
        .or_else(|| find_app_bundle(own_dir.as_deref()));
    let Some(gui) = found else {
        eprintln!(
            "fml-cli: the desktop app (fml) was not found beside fml-cli or on PATH{} \
             (build it with: cd rust && cargo build -p fml-gui --release)",
            if cfg!(target_os = "macos") { " or as fml.app in /Applications" } else { "" }
        );
        return ExitCode::from(1);
    };
    match start_app(&gui) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fml-cli: could not start {}: {error}", gui.display());
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    handle_signals();
    if std::env::args().skip(1).any(|arg| arg == "--gui") {
        return open_gui();
    }
    let mut command = build_command();
    if std::env::args().len() <= 1 {
        let _ = command.print_help();
        println!();
        return ExitCode::SUCCESS;
    }
    let matches = match command.try_get_matches() {
        Ok(matches) => matches,
        Err(error) => explain(error),
    };
    let config = config_from(&matches);
    let json_output = text(&matches, "format") == "json";

    let output = Arc::new(Mutex::new(()));
    let printer_config = config.clone();
    let lock = Arc::clone(&output);
    let sink: fml_core::Sink = Arc::new(move |event| {
        // lines are printed under a lock, so that threads can't mix them up
        let _guard = lock.lock().unwrap();
        match event {
            Event::Warn(message) => eprintln!("Warning: {message}"),
            Event::Note(message) => eprintln!("{message}"),
            Event::Result(result) if !json_output => print_result(&result, &printer_config),
            Event::Result(_) => {}
            Event::Progress { .. } => {}
        }
        let _ = std::io::stdout().flush();
    });
    let dryrun = config.dryrun;
    let ended = Fml::new(config, sink).run();
    if json_output {
        let document: Value = json!({
            "version": VERSION, "dry_run": dryrun,
            "results": ended.results.iter().map(Outcome::to_json).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&document).unwrap_or_default());
    }
    ExitCode::from(ended.code as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_is_found_beside_the_program_or_by_override() {
        let dir = std::env::temp_dir().join(format!("fml-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let none = |p: Option<PathBuf>| p.filter(|p| p.starts_with(&dir));
        assert_eq!(none(find_gui(Some(&dir), None)), None);
        let gui = dir.join(APP);
        std::fs::write(&gui, b"\x7fELF").unwrap();
        assert_eq!(find_gui(Some(&dir), None), Some(gui.clone()));
        let other = dir.join("other");
        std::fs::write(&other, b"").unwrap();
        assert_eq!(find_gui(Some(&dir), other.to_str()), Some(other));
        assert_eq!(find_gui(Some(&dir), Some("/no/such/file")), Some(gui.clone()));
        // a script called fml is not the app
        std::fs::write(&gui, b"#!/bin/sh\n").unwrap();
        assert_eq!(none(find_gui(Some(&dir), None)), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
