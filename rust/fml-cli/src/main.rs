//! fml: fix the default audio and subtitle languages, and the tracks, of media files.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use clap::{Arg, ArgAction, ArgMatches, Command};
use fml_core::languages::resolve_language;
use fml_core::{
    format_tracks, info_text, Config, Event, Fml, Kind, Outcome, Status, Task, Track, VERSION,
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
  fml --set lang --default French --dir films --dryrun  show what would change";

/// The words each action takes.
const TASK_CHOICES: &[(&str, &[&str])] = &[
    ("get", &["lang"]),
    ("set", &["lang", "default", "name", "forced", "unforced"]),
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
    let number = text.chars().all(|c| c.is_ascii_digit()) && text.parse::<usize>().is_ok_and(|n| n >= 1);
    if text == "last" || number {
        Ok(text.to_string())
    } else {
        Err(format!("\"{text}\" is not a track number (from 1) or \"last\""))
    }
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
             forced flag (MKV); lang: make --default (or the first of --prefer) the default, or \
             with --lang set the language of track --track (1); default: make track --track the \
             default, and with --lang set its language",
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
                .value_name("LANG")
                .value_parser(language_arg)
                .help("language, by name or ISO 639 code (English, en, eng)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("default")
                .short('D')
                .long("default")
                .value_name("LANG")
                .value_parser(default_arg)
                .help("language to make the default (none turns the default subtitles off)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("prefer")
                .long("prefer")
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
                .action(ArgAction::SetTrue)
                .help("with --kind sub, pick the forced subtitle track")
                .help_heading(settings),
        )
        .arg(
            Arg::new("track")
                .short('t')
                .long("track")
                .value_name("N")
                .allow_negative_numbers(true)
                .value_parser(track_arg)
                .help("track number from 1 (among tracks of the kind), or last")
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
            Arg::new("name")
                .long("name")
                .value_name("TEXT")
                .help("the track name for --set name (an empty name removes it)")
                .help_heading(settings),
        )
        .arg(
            Arg::new("move")
                .long("move")
                .action(ArgAction::SetTrue)
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
                .help("install missing commands (apt on Ubuntu, brew on macOS)")
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

/// The actions, in the order they were given on the command line.
fn queued_tasks(matches: &ArgMatches) -> Vec<Task> {
    let mut queue: Vec<(usize, Task)> = Vec::new();
    for name in ["check", "info"] {
        if matches.get_flag(name) {
            for index in matches.indices_of(name).into_iter().flatten() {
                queue.push((index, Task::new(name, "")));
            }
        }
    }
    let valued = ["undo", "get", "set", "swap", "delete", "preserve"];
    for name in valued {
        let values: Vec<String> = matches.get_many::<String>(name).into_iter().flatten().cloned().collect();
        let indices: Vec<usize> = matches.indices_of(name).into_iter().flatten().collect();
        for (value, index) in values.into_iter().zip(indices) {
            queue.push((index, Task::new(name, &value)));
        }
    }
    queue.sort_by_key(|(index, _)| *index);
    queue.into_iter().map(|(_, task)| task).collect()
}

fn text(matches: &ArgMatches, name: &str) -> String {
    matches.get_one::<String>(name).cloned().unwrap_or_default()
}

fn many(matches: &ArgMatches, name: &str) -> Vec<String> {
    matches.get_many::<String>(name).into_iter().flatten().cloned().collect()
}

/// Reject combinations of arguments that make no sense, and build the settings.
fn config_from(matches: &ArgMatches) -> Config {
    let tasks = queued_tasks(matches);
    let (file, dir) = (text(matches, "file"), text(matches, "dir"));
    let kind = Kind::parse(&text(matches, "kind")).unwrap_or(Kind::Audio);
    let prefer: Vec<String> = text(matches, "prefer")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let config = Config {
        paths: Vec::new(),
        file,
        dir,
        recursive: matches.get_flag("recursive"),
        lang: text(matches, "lang"),
        default: text(matches, "default"),
        prefer,
        track: text(matches, "track"),
        kind,
        forced: matches.get_flag("forced"),
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
        move_first: matches.get_flag("move"),
        name: matches.get_one::<String>("name").cloned(),
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
/// `~/Applications` and `/Applications`, for the program inside the bundle.
fn find_app_bundle(own_dir: Option<&Path>) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
    own_dir
        .map(Path::to_path_buf)
        .into_iter()
        .chain(home)
        .chain([PathBuf::from("/Applications")])
        .map(|dir| dir.join("fml.app/Contents/MacOS/fml"))
        .find(|p| is_app(p))
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
