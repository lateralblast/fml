//! The fml desktop app: a window over the same engine the command line uses.
//!
//! The page (in `ui/`) sends a [`Request`], the backend turns it into a `Config`, checks it
//! with the same rules as the command line, runs the engine and sends back what happened.
//! While a run is going each file's result is also sent as an event, for the progress list.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use fml_core::languages::resolve_language;
use fml_core::util::which;
use fml_core::{
    request_cancel, reset_cancel, validate, Config, Event, Fml, Kind, Outcome, Sink, SubSpec, Task,
    REQUIRED_TOOLS, VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// Only one run at a time: two would fight over the same files and the stop request.
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize)]
struct TaskRequest {
    name: String,
    #[serde(default)]
    what: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SubtitleRequest {
    file: String,
    lang: String,
    name: String,
    forced: bool,
}

/// What the page asks for. Languages may be names or codes; they are resolved here.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Request {
    paths: Vec<String>,
    dir: String,
    recursive: bool,
    include: Vec<String>,
    exclude: Vec<String>,
    tasks: Vec<TaskRequest>,
    lang: String,
    default: String,
    prefer: Vec<String>,
    track: String,
    kind: String,
    forced: bool,
    force: bool,
    in_place: bool,
    backup: bool,
    log: String,
    jobs: usize,
    move_first: bool,
    name: Option<String>,
    /// For `set order`: the current number of each track of the kind, in the new order.
    order: Vec<usize>,
    /// For `add subtitle`: the subtitle file.
    sub_file: String,
    /// For `add subtitle`, several files each with its language, name and forced flag.
    subtitles: Vec<SubtitleRequest>,
    /// For `set track`: set or clear the forced flag.
    forced_flag: Option<bool>,
    /// For `extract subtitle`: the file to write.
    output: String,
    /// For `set order`: also make the track that ends up first the default.
    default_first: bool,
    /// For `set order`: languages in the order their tracks should come, for many files at once.
    order_lang: Vec<String>,
    dryrun: bool,
}

fn language(text: &str) -> Result<String, String> {
    if text.is_empty() {
        Ok(String::new())
    } else {
        resolve_language(text)
    }
}

impl Request {
    fn config(&self) -> Result<Config, String> {
        let default = if self.default.trim().eq_ignore_ascii_case("none") {
            "none".to_string()
        } else {
            language(&self.default)?
        };
        let prefer = self.prefer.iter().map(|l| resolve_language(l)).collect::<Result<_, _>>()?;
        Ok(Config {
            paths: self.paths.clone(),
            dir: self.dir.clone(),
            recursive: self.recursive,
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            lang: language(&self.lang)?,
            default,
            prefer,
            track: self.track.clone(),
            kind: Kind::parse(&self.kind).unwrap_or(Kind::Audio),
            forced: self.forced,
            force: self.force,
            in_place: self.in_place,
            backup: self.backup,
            dryrun: self.dryrun,
            log: self.log.clone(),
            jobs: self.jobs.max(1),
            move_first: self.move_first,
            name: self.name.clone(),
            order: self.order.clone(),
            sub_file: self.sub_file.clone(),
            subtitles: self
                .subtitles
                .iter()
                .map(|s| {
                    Ok(SubSpec {
                        file: s.file.clone(),
                        lang: language(&s.lang)?,
                        name: s.name.clone(),
                        forced: s.forced,
                    })
                })
                .collect::<Result<_, String>>()?,
            forced_flag: self.forced_flag,
            output: self.output.clone(),
            default_first: self.default_first,
            order_lang: self.order_lang.iter().map(|l| resolve_language(l)).collect::<Result<_, _>>()?,
            tasks: self.tasks.iter().map(|t| Task::new(&t.name, &t.what)).collect(),
            ..Config::default()
        })
    }
}

#[derive(Serialize)]
struct Summary {
    code: i32,
    results: Vec<Value>,
}

/// Releases the "a run is going" flag however the run ends.
struct RunGuard;

impl RunGuard {
    fn take() -> Result<RunGuard, String> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            Err("another run is still going".into())
        } else {
            Ok(RunGuard)
        }
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Run the engine for a request. With `announce` each result is also sent to the page as
/// it is ready.
fn execute(app: &AppHandle, request: Request, announce: bool) -> Result<Summary, String> {
    let mut config = request.config()?;
    config.progress = announce; // a run that is shown has a progress bar
    validate(&config)?;
    let _guard = RunGuard::take()?;
    reset_cancel();
    let handle = app.clone();
    let sink: Sink = Arc::new(move |event| {
        if !announce {
            return;
        }
        let _ = match event {
            Event::Result(result) => handle.emit("fml-result", result.to_json()),
            Event::Warn(message) => handle.emit("fml-warn", message),
            Event::Note(message) => handle.emit("fml-note", message),
            Event::Progress { file, stage, done, total } => handle.emit(
                "fml-progress",
                json!({"file": file, "stage": stage, "done": done, "total": total}),
            ),
        };
    });
    let ended = Fml::new(config, sink).run();
    Ok(Summary { code: ended.code, results: ended.results.iter().map(Outcome::to_json).collect() })
}

/// Read the tracks of the files the page has chosen.
#[tauri::command]
async fn scan(app: AppHandle, mut request: Request) -> Result<Summary, String> {
    request.tasks = vec![TaskRequest { name: "info".into(), what: String::new() }];
    request.dryrun = false;
    tauri::async_runtime::spawn_blocking(move || execute(&app, request, false))
        .await
        .map_err(|e| e.to_string())?
}

/// Preview (`dryrun`) or make the changes the request asks for.
#[tauri::command]
async fn run(app: AppHandle, request: Request) -> Result<Summary, String> {
    tauri::async_runtime::spawn_blocking(move || execute(&app, request, true))
        .await
        .map_err(|e| e.to_string())?
}

/// Stop the run that is going: the running tool is ended and temporary files removed.
#[tauri::command]
fn cancel() {
    request_cancel();
}

/// The version, and which of the commands fml needs are installed.
#[tauri::command]
fn app_info() -> Value {
    let tools: Vec<Value> = REQUIRED_TOOLS
        .iter()
        .map(|(tool, package)| {
            json!({"tool": tool, "package": package,
                   "path": which(tool).map(|p| p.to_string_lossy().into_owned())})
        })
        .collect();
    json!({"version": VERSION, "tools": tools, "home": std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default()})
}

/// The languages the page offers, by name.
#[tauri::command]
fn languages() -> Vec<Value> {
    let mut all: Vec<(&str, &str)> =
        fml_core::languages::all().iter().map(|&(code, _, _, name)| (code, name)).collect();
    all.sort_by_key(|&(_, name)| name);
    all.into_iter().map(|(code, name)| json!({"code": code, "name": name})).collect()
}

/// The extensions of the media files the app lists (as in the file picker).
const MEDIA_EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v", "mov", "avi", "webm"];

fn is_media_file(path: &std::path::Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    !name.contains(".fml-tmp")
        && !name.contains(".fml-bak")
        && path
            .extension()
            .map(|e| MEDIA_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
            .unwrap_or(false)
}

fn media_in(folder: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut paths: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            media_in(&path, out);
        } else if is_media_file(&path) {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// What was dropped on the window: one folder alone is worked on as a folder (with its filters
/// and subfolders); anything else becomes a list of the media files among the files dropped and
/// inside the folders dropped. `ignored` counts dropped files that are not media files.
#[tauri::command]
fn classify_dropped(paths: Vec<String>) -> Value {
    let (mut folders, mut files, mut ignored) = (Vec::new(), Vec::new(), 0usize);
    for text in &paths {
        let path = std::path::Path::new(text);
        if path.is_dir() {
            folders.push(text.clone());
        } else if is_media_file(path) {
            files.push(text.clone());
        } else {
            ignored += 1;
        }
    }
    if folders.len() == 1 && files.is_empty() {
        return json!({"folder": folders[0], "files": [], "ignored": ignored});
    }
    for folder in &folders {
        media_in(std::path::Path::new(folder), &mut files);
    }
    // a folder and one inside it, or a file and its folder, name a file twice
    let mut seen = std::collections::HashSet::new();
    files.retain(|f| seen.insert(f.clone()));
    json!({"folder": null, "files": files, "ignored": ignored})
}

/// Match subtitle files to media files by name, for the "Add subtitles" dialog: which media
/// file each belongs to and what its name says about its language, plus the files that match
/// nothing.
#[tauri::command]
fn match_subtitle_files(media: Vec<String>, subs: Vec<String>) -> Value {
    let found = fml_core::subtitles::match_subtitles(&media, &subs);
    json!({
        "matches": found.matches.iter().map(|m| json!({
            "media": m.media, "file": m.file, "lang": m.lang, "name": m.name, "forced": m.forced,
        })).collect::<Vec<_>>(),
        "unmatched": found.unmatched,
    })
}

/// The path a file gets when it is renamed within its folder, or why it can't be.
fn renamed_path(path: &std::path::Path, name: &str) -> Result<std::path::PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("the name is empty".into());
    }
    if name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err("a file name can't contain / or \\ (the file stays in its folder)".into());
    }
    let target = path.with_file_name(name);
    if target == path {
        return Err("that is already the name".into());
    }
    // a name that differs only in case is the same file on a case-insensitive disk (macOS, Windows)
    let same_file = std::fs::canonicalize(&target).ok() == std::fs::canonicalize(path).ok();
    if std::fs::symlink_metadata(&target).is_ok() && !same_file {
        return Err(format!("\"{name}\" already exists in that folder"));
    }
    Ok(target)
}

/// Rename a file within its folder. Nothing is overwritten. Not while a run is going, which
/// may be working on the file. The entries about the file in the log (the one the page is
/// using, if any) are changed to the new name, so a later undo still finds it.
#[tauri::command]
fn rename_file(path: String, name: String, log: Option<String>) -> Result<Value, String> {
    if RUNNING.load(Ordering::SeqCst) {
        return Err("a run is going: wait for it, or stop it".into());
    }
    let from = std::path::Path::new(&path);
    if !from.is_file() {
        return Err(format!("{path} is not a file"));
    }
    let to = renamed_path(from, &name)?;
    std::fs::rename(from, &to).map_err(|e| format!("could not rename: {e}"))?;
    let to = to.to_string_lossy().into_owned();
    // keep `--undo` working: the log names the file by its path
    let (entries, log_error) = match log.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        Some(log) => match fml_core::rename_in_log(log, &path, &to) {
            Ok(count) => (count, None),
            Err(error) => (0, Some(format!("could not update the log {log}: {error}"))),
        },
        None => (0, None),
    };
    Ok(json!({"path": to, "log_entries": entries, "log_error": log_error}))
}

/// Everything about one file for the Info dialog: its size and date, and the full technical
/// report from `mediainfo` (duration, bitrate, resolution, every track's properties). The track
/// list itself comes from the scan. A missing `mediainfo` leaves the report out, with the reason.
#[tauri::command]
async fn file_details(path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let meta = std::fs::metadata(&path).map_err(|e| format!("{path}: {e}"))?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let (report, report_error) = match which("mediainfo") {
            None => (None, Some("mediainfo is not installed".to_string())),
            Some(tool) => match std::process::Command::new(tool).arg(&path).output() {
                Ok(out) if out.status.success() => {
                    (Some(String::from_utf8_lossy(&out.stdout).into_owned()), None)
                }
                Ok(out) => (None, Some(String::from_utf8_lossy(&out.stderr).trim().to_string())),
                Err(e) => (None, Some(e.to_string())),
            },
        };
        Ok(json!({"path": path, "size": meta.len(), "modified": modified,
                  "report": report, "report_error": report_error}))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Debug builds only: let a script that drives the page send back what it saw.
#[cfg(debug_assertions)]
#[tauri::command]
fn dev_report(text: String) {
    if let Ok(path) = std::env::var("FML_GUI_REPORT") {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(not(debug_assertions))]
#[tauri::command]
fn dev_report(_text: String) {}

fn main() {
    // an app started from Finder has a bare PATH on macOS: find Homebrew's commands anyway
    fml_core::util::add_tool_folders();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan, run, cancel, app_info, languages, file_details, rename_file, match_subtitle_files, classify_dropped, dev_report
        ])
        .setup(|app| {
            // Debug builds only: FML_GUI_SCRIPT names a script to run in the window, so the
            // page can be driven and checked without clicking (see rust/README.md).
            #[cfg(debug_assertions)]
            if let Ok(path) = std::env::var("FML_GUI_SCRIPT") {
                use tauri::Manager;
                if let (Some(window), Ok(script)) =
                    (app.get_webview_window("main"), std::fs::read_to_string(path))
                {
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let _ = window.eval(&script);
                    });
                }
            }
            // Debug builds only: FML_GUI_THEME=dark (or light) shows the window in that theme whatever
            // the system is using, to check how the page looks in each.
            #[cfg(debug_assertions)]
            if let Ok(theme) = std::env::var("FML_GUI_THEME") {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let theme = if theme == "dark" { tauri::Theme::Dark } else { tauri::Theme::Light };
                    let _ = window.set_theme(Some(theme));
                }
            }
            // Debug builds only: FML_GUI_SIZE=1120x1000 gives the window that size (in points), so
            // the page can be captured whole.
            #[cfg(debug_assertions)]
            if let Ok(size) = std::env::var("FML_GUI_SIZE") {
                use tauri::Manager;
                if let (Some(window), Some((w, h))) = (
                    app.get_webview_window("main"),
                    size.split_once('x').and_then(|(w, h)| Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?))),
                ) {
                    let _ = window.set_size(tauri::LogicalSize::new(w, h));
                    let _ = window.center();
                }
            }
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the fml app");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(json: Value) -> Request {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn dropped_paths_become_a_folder_or_a_list_of_media_files() {
        let dir = std::env::temp_dir().join(format!("fml-drop-{}", std::process::id()));
        let (season, extras) = (dir.join("Season 1"), dir.join("Season 1").join("Extras"));
        std::fs::create_dir_all(&extras).unwrap();
        for name in ["b.mkv", "a.MP4", "notes.txt", "c.mkv.fml-bak", "d.fml-tmp.mp4"] {
            std::fs::write(season.join(name), b"x").unwrap();
        }
        std::fs::write(extras.join("e.avi"), b"x").unwrap();
        let text = |p: &std::path::Path| p.to_string_lossy().into_owned();
        // one folder alone is a folder
        let one = classify_dropped(vec![text(&season)]);
        assert_eq!(one["folder"], text(&season));
        assert_eq!(one["files"], json!([]));
        // files, or a folder with files, are a list: media only, folders searched, no temp or backup files
        let mixed = classify_dropped(vec![text(&season.join("b.mkv")), text(&season.join("notes.txt")), text(&extras)]);
        assert_eq!(mixed["folder"], Value::Null);
        assert_eq!(mixed["files"], json!([text(&season.join("b.mkv")), text(&extras.join("e.avi"))]));
        assert_eq!(mixed["ignored"], 1);
        let two = classify_dropped(vec![text(&season), text(&extras)]);
        let names: Vec<String> = two["files"].as_array().unwrap().iter()
            .map(|f| std::path::Path::new(f.as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["e.avi", "a.MP4", "b.mkv"]); // each file once
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_is_renamed_within_its_folder_and_nothing_is_overwritten() {
        let dir = std::env::temp_dir().join(format!("fml-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.mkv"), dir.join("b.mkv"));
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let a_str = a.to_string_lossy().into_owned();
        for (name, why) in [
            ("", "empty"), ("  ", "empty"), ("x/y.mkv", "can't contain"), ("..", "can't contain"),
            ("a.mkv", "already the name"), ("b.mkv", "already exists"),
        ] {
            let error = rename_file(a_str.clone(), name.to_string(), None).unwrap_err();
            assert!(error.contains(why), "{name}: {error}");
        }
        assert_eq!(std::fs::read(&b).unwrap(), b"b");
        let renamed = rename_file(a_str.clone(), " c d.mkv ".to_string(), None).unwrap();
        let new = renamed["path"].as_str().unwrap().to_string();
        assert_eq!(std::path::Path::new(&new), dir.join("c d.mkv"));
        assert_eq!(renamed["log_entries"], 0);
        assert!(!a.exists() && dir.join("c d.mkv").is_file());
        // the entries about the file in a log follow it
        let log = dir.join("log.jsonl");
        std::fs::write(&log, format!("{{\"file\":{}}}\n", serde_json::json!(new))).unwrap();
        let moved = rename_file(new.clone(), "d.mkv".to_string(), Some(log.to_string_lossy().into_owned())).unwrap();
        assert_eq!(moved["log_entries"], 1);
        assert!(std::fs::read_to_string(&log).unwrap().contains("d.mkv"));
        let new = moved["path"].as_str().unwrap().to_string();
        // a change of case only is allowed even where the disk ignores case
        let upper = rename_file(new, "C D.mkv".to_string(), None).unwrap();
        assert!(dir.read_dir().unwrap().any(|e| e.unwrap().file_name() == "C D.mkv"), "{upper}");
        assert!(rename_file(dir.join("gone.mkv").to_string_lossy().into_owned(), "z".into(), None).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_details_give_the_size_and_say_why_a_file_cannot_be_read() {
        let path = std::env::temp_dir().join(format!("fml-details-{}.bin", std::process::id()));
        std::fs::write(&path, b"12345").unwrap();
        let details = tauri::async_runtime::block_on(file_details(path.to_string_lossy().into_owned())).unwrap();
        assert_eq!(details["size"], 5);
        assert!(details["modified"].is_u64());
        // the report is either there or comes with a reason
        assert!(details["report"].is_string() || details["report_error"].is_string());
        std::fs::remove_file(&path).unwrap();
        let missing = tauri::async_runtime::block_on(file_details("/no/such/file".into()));
        assert!(missing.unwrap_err().contains("/no/such/file"));
    }

    #[test]
    fn languages_are_resolved_and_defaults_filled_in() {
        let config = request(json!({
            "paths": ["a.mkv"], "default": "English", "prefer": ["it", "Japanese"],
            "kind": "sub", "jobs": 0,
            "tasks": [{"name": "set", "what": "lang"}],
        }))
        .config()
        .unwrap();
        assert_eq!(config.default, "eng");
        assert_eq!(config.prefer, ["ita", "jpn"]);
        assert_eq!(config.kind, Kind::Subtitles);
        assert_eq!(config.jobs, 1);
        assert_eq!(config.tasks, [Task::new("set", "lang")]);
    }

    #[test]
    fn no_default_subtitles_is_kept_as_none() {
        let config = request(json!({"default": " None "})).config().unwrap();
        assert_eq!(config.default, "none");
    }

    #[test]
    fn an_unknown_language_is_an_error_the_page_can_show() {
        let error = request(json!({"lang": "Klingonese"})).config().unwrap_err();
        assert!(error.contains("unknown language"), "{error}");
    }

    #[test]
    fn the_same_rules_as_the_command_line_apply() {
        let config = request(json!({
            "paths": ["a.mkv"], "move_first": true, "in_place": true, "default": "eng",
            "tasks": [{"name": "set", "what": "lang"}],
        }))
        .config()
        .unwrap();
        assert!(validate(&config).unwrap_err().contains("can't be used with --in-place"));
    }

    #[test]
    fn the_language_list_is_sorted_by_name() {
        let names: Vec<String> =
            languages().iter().map(|l| l["name"].as_str().unwrap().to_string()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert!(names.contains(&"English".to_string()));
    }
}
