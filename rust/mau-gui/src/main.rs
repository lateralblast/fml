//! The mau desktop app: a window over the same engine the command line uses.
//!
//! The page (in `ui/`) sends a [`Request`], the backend turns it into a `Config`, checks it
//! with the same rules as the command line, runs the engine and sends back what happened.
//! While a run is going each file's result is also sent as an event, for the progress list.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mau_core::languages::resolve_language;
use mau_core::util::which;
use mau_core::{
    request_cancel, reset_cancel, validate, Config, Event, Mau, Kind, Outcome, Sink, SubSpec, Task,
    whisper_path, OPTIONAL_TOOLS, VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// Only one run at a time: two would fight over the same files and the stop request.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Whether the online subtitle search is on: only when the app was started with `--enable search`.
static SEARCH_ENABLED: AtomicBool = AtomicBool::new(false);

const SEARCH_OFF: &str = "the online subtitle search is off: start mau with --enable search to use it";

/// Refuses the online search unless it was turned on when the app started.
fn search_allowed() -> Result<(), String> {
    if SEARCH_ENABLED.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(SEARCH_OFF.into())
    }
}

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
    /// For `preserve` with several languages: every language to keep (the tracks in all of them stay).
    keep: Vec<String>,
    /// For `delete match`: part of the codec name, and the number of channels (0: any).
    codec: String,
    channels: usize,
    track: String,
    kind: String,
    forced: bool,
    force: bool,
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
        let keep = self.keep.iter().map(|l| resolve_language(l)).collect::<Result<_, _>>()?;
        Ok(Config {
            paths: self.paths.clone(),
            dir: self.dir.clone(),
            recursive: self.recursive,
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            lang: language(&self.lang)?,
            default,
            prefer,
            keep,
            codec: self.codec.clone(),
            channels: self.channels,
            track: self.track.clone(),
            kind: match self.kind.trim() {
                "" => Kind::Audio,
                text => Kind::parse(text).ok_or_else(|| format!("\"{text}\" is not audio or sub (subtitles)"))?,
            },
            forced: self.forced,
            force: self.force,
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
            Event::Result(result) => handle.emit("mau-result", result.to_json()),
            Event::Warn(message) => handle.emit("mau-warn", message),
            Event::Note(message) => handle.emit("mau-note", message),
            Event::Progress { file, stage, done, total } => handle.emit(
                "mau-progress",
                json!({"file": file, "stage": stage, "done": done, "total": total}),
            ),
        };
    });
    let ended = Mau::new(config, sink).run();
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

/// The version, and which of the commands mau needs are installed. Looks along `PATH` on the
/// disk, so it runs off the window's thread like the other commands that touch files.
#[tauri::command]
async fn app_info() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(app_info_now).await.map_err(|e| e.to_string())
}

fn app_info_now() -> Value {
    // the optional whisper-cli (for Detect languages…) is listed too, found as --check finds it
    let mut tools: Vec<Value> = mau_core::model::required_tools()
        .iter()
        .map(|(tool, package)| {
            json!({"tool": tool, "package": package, "optional": false,
                   "path": which(tool).map(|p| p.to_string_lossy().into_owned())})
        })
        .collect();
    for (tool, package) in OPTIONAL_TOOLS {
        let path = whisper_path().map(|p| p.to_string_lossy().into_owned());
        tools.push(json!({"tool": tool, "package": package, "optional": true, "path": path}));
    }
    json!({"version": VERSION, "tools": tools, "search_enabled": SEARCH_ENABLED.load(Ordering::SeqCst),
           "home": std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default()})
}

/// The languages the page offers, by name.
#[tauri::command]
fn languages() -> Vec<Value> {
    let mut all: Vec<(&str, &str)> =
        mau_core::languages::all().iter().map(|&(code, _, _, name)| (code, name)).collect();
    all.sort_by_key(|&(_, name)| name);
    all.into_iter().map(|(code, name)| json!({"code": code, "name": name})).collect()
}

/// The About text: ABOUT.md, compiled into the app so that it is the same file as in the repository.
#[tauri::command]
fn about_text() -> &'static str {
    include_str!("../../../ABOUT.md")
}

/// Show the About window (the page about.html, with the app icon and ABOUT.md), or bring it to the front. The
/// header's About button and the app menu's About item both use it.
#[tauri::command]
fn show_about(app: AppHandle) -> Result<(), String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    if let Some(window) = app.get_webview_window("about") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "about", WebviewUrl::App("about.html".into()))
        .title("About mau")
        .inner_size(460.0, 600.0)
        .resizable(false)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The menu bar on macOS: the app menu (its About item opens the About window, not the system's plain panel), plus
/// the Edit and Window menus that a Mac app expects. Other systems keep the window's own menu.
#[cfg(target_os = "macos")]
fn app_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem as Item, Submenu};
    let about = MenuItem::with_id(app, "about", "About mau", true, None::<&str>)?;
    let app_menu = Submenu::with_items(app, "mau", true, &[
        &about,
        &Item::separator(app)?,
        &Item::services(app, None)?,
        &Item::separator(app)?,
        &Item::hide(app, None)?,
        &Item::hide_others(app, None)?,
        &Item::separator(app)?,
        &Item::quit(app, None)?,
    ])?;
    let edit = Submenu::with_items(app, "Edit", true, &[
        &Item::undo(app, None)?,
        &Item::redo(app, None)?,
        &Item::separator(app)?,
        &Item::cut(app, None)?,
        &Item::copy(app, None)?,
        &Item::paste(app, None)?,
        &Item::select_all(app, None)?,
    ])?;
    let window = Submenu::with_items(app, "Window", true, &[
        &Item::minimize(app, None)?,
        &Item::maximize(app, None)?,
        &Item::close_window(app, None)?,
    ])?;
    Menu::with_items(app, &[&app_menu, &edit, &window])
}

/// Open a web link in the system browser (never in the app's own window). Only plain http and https
/// addresses are passed on: no shell is given the address, and Windows gets only letters, digits and
/// the few characters a web address needs.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    let plain = url.chars().all(|c| c.is_ascii_alphanumeric() || ":/.-_?=&%#+~".contains(c));
    if !(url.starts_with("https://") || url.starts_with("http://")) || !plain {
        return Err(format!("not a web address: {url}"));
    }
    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&url).status()
    } else if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "start", "", &url]).status()
    } else {
        std::process::Command::new("xdg-open").arg(&url).status()
    };
    match opened {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("could not open {url}")),
        Err(error) => Err(format!("could not open {url}: {error}")),
    }
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

/// The media files in a folder and below it. Linked folders are followed, but a folder or file
/// reached twice (through a link) is only listed once.
fn media_in(folder: &std::path::Path, out: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    media_in_seen(folder, out, &mut seen);
}

fn media_in_seen(folder: &std::path::Path, out: &mut Vec<String>, seen: &mut std::collections::HashSet<std::path::PathBuf>) {
    let Ok(real) = std::fs::canonicalize(folder) else { return };
    if !seen.insert(real) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut paths: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            media_in_seen(&path, out, seen);
        } else if is_media_file(&path) {
            let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(real) {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
}

/// What was dropped on the window: one folder alone is worked on as a folder (with its filters
/// and subfolders); anything else becomes a list of the media files among the files dropped and
/// inside the folders dropped. `ignored` counts dropped files that are not media files.
/// Walks the folders dropped, so it runs off the window's thread (see `classify_dropped_now`).
#[tauri::command]
async fn classify_dropped(paths: Vec<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || classify_dropped_now(paths))
        .await
        .map_err(|e| e.to_string())
}

fn classify_dropped_now(paths: Vec<String>) -> Value {
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
/// Reads the folders of the subtitle files, so it runs off the window's thread.
#[tauri::command]
async fn match_subtitle_files(media: Vec<String>, subs: Vec<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || match_subtitle_files_now(media, subs))
        .await
        .map_err(|e| e.to_string())
}

fn match_subtitle_files_now(media: Vec<String>, subs: Vec<String>) -> Value {
    let found = mau_core::subtitles::match_subtitles(&media, &subs);
    json!({
        "matches": found.matches.iter().map(|m| json!({
            "media": m.media, "file": m.file, "lang": m.lang, "name": m.name, "forced": m.forced,
        })).collect::<Vec<_>>(),
        "unmatched": found.unmatched,
    })
}

/// What is saved for searching OpenSubtitles, without the secrets: whether there is a key and a
/// password, and the user name.
#[tauri::command]
async fn subtitle_settings() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let saved = mau_core::online::load_credentials();
        json!({
            "has_key": !saved.api_key.trim().is_empty(),
            "username": saved.username,
            "has_password": !saved.password.is_empty(),
        })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Save the key, user name and password. A value that is not given (null) is kept as it was;
/// an empty one clears it.
#[tauri::command]
async fn save_subtitle_settings(
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut saved = mau_core::online::load_credentials();
        if let Some(v) = api_key { saved.api_key = v.trim().to_string(); }
        if let Some(v) = username { saved.username = v.trim().to_string(); }
        if let Some(v) = password { saved.password = v; }
        mau_core::online::save_credentials(&saved)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The title, season and episode (or year) that a media file's name says, to start a search from.
#[tauri::command]
fn subtitle_guess(path: String) -> Value {
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let guess = mau_core::online::guess_query(&name);
    json!({"title": guess.title, "year": guess.year, "season": guess.season, "episode": guess.episode})
}

/// Search OpenSubtitles for a media file: reads 128 KB of it for the hash and asks the service,
/// so it runs off the window's thread.
#[tauri::command]
async fn search_subtitles(path: String, query: String, languages: Vec<String>) -> Result<Value, String> {
    search_allowed()?;
    tauri::async_runtime::spawn_blocking(move || {
        let credentials = mau_core::online::load_credentials();
        let languages = languages.iter().map(|l| resolve_language(l)).collect::<Result<Vec<_>, _>>()?;
        let hits = mau_core::online::search(&credentials, std::path::Path::new(&path), &query, &languages)?;
        Ok(json!(hits))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download a subtitle found by `search_subtitles` to the temporary folder; its path comes back.
#[tauri::command]
async fn fetch_subtitle(file_id: i64) -> Result<String, String> {
    search_allowed()?;
    tauri::async_runtime::spawn_blocking(move || {
        let credentials = mau_core::online::load_credentials();
        mau_core::online::download(&credentials, file_id).map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remove a subtitle that `fetch_subtitle` downloaded (anything else is left alone).
#[tauri::command]
fn forget_subtitle(path: String) {
    mau_core::online::forget_download(std::path::Path::new(&path));
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
/// Renames on the disk and rewrites the log, so it runs off the window's thread.
#[tauri::command]
async fn rename_file(path: String, name: String, log: Option<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || rename_file_now(path, name, log))
        .await
        .map_err(|e| e.to_string())?
}

fn rename_file_now(path: String, name: String, log: Option<String>) -> Result<Value, String> {
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
        Some(log) => match mau_core::rename_in_log(log, &path, &to) {
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
    if let Ok(path) = std::env::var("MAU_GUI_REPORT") {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(not(debug_assertions))]
#[tauri::command]
fn dev_report(_text: String) {}

fn main() {
    // an app started from Finder has a bare PATH on macOS: find Homebrew's commands anyway
    mau_core::util::add_tool_folders();
    // the online subtitle search is off unless started with `--enable search` (`mau-cli --gui` passes it on)
    let args: Vec<String> = std::env::args().collect();
    let enabled = args.windows(2).any(|pair| pair[0] == "--enable" && pair[1] == "search")
        || args.iter().any(|arg| arg == "--enable=search");
    SEARCH_ENABLED.store(enabled, Ordering::SeqCst);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan, run, cancel, app_info, languages, about_text, show_about, open_link, file_details, rename_file, match_subtitle_files, classify_dropped,
            subtitle_settings, save_subtitle_settings, subtitle_guess, search_subtitles, fetch_subtitle,
            forget_subtitle, dev_report
        ])
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "about" {
                let _ = show_about(app.clone());
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_menu(app_menu(app.handle())?)?;
            // Debug builds only: MAU_GUI_SCRIPT names a script to run in the window, so the
            // page can be driven and checked without clicking (see rust/README.md).
            #[cfg(debug_assertions)]
            if let Ok(path) = std::env::var("MAU_GUI_SCRIPT") {
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
            // Debug builds only: MAU_GUI_THEME=dark (or light) shows the window in that theme whatever
            // the system is using, to check how the page looks in each.
            #[cfg(debug_assertions)]
            if let Ok(theme) = std::env::var("MAU_GUI_THEME") {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let theme = if theme == "dark" { tauri::Theme::Dark } else { tauri::Theme::Light };
                    let _ = window.set_theme(Some(theme));
                }
            }
            // Debug builds only: MAU_GUI_SIZE=1120x1000 gives the window that size (in points), so
            // the page can be captured whole.
            #[cfg(debug_assertions)]
            if let Ok(size) = std::env::var("MAU_GUI_SIZE") {
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
        .expect("error while running the mau app");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(json: Value) -> Request {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn dropped_paths_become_a_folder_or_a_list_of_media_files() {
        let dir = std::env::temp_dir().join(format!("mau-drop-{}", std::process::id()));
        let (season, extras) = (dir.join("Season 1"), dir.join("Season 1").join("Extras"));
        std::fs::create_dir_all(&extras).unwrap();
        for name in ["b.mkv", "a.MP4", "notes.txt", "c.mkv.fml-bak", "d.fml-tmp.mp4"] {
            std::fs::write(season.join(name), b"x").unwrap();
        }
        std::fs::write(extras.join("e.avi"), b"x").unwrap();
        let text = |p: &std::path::Path| p.to_string_lossy().into_owned();
        // one folder alone is a folder
        let one = classify_dropped_now(vec![text(&season)]);
        assert_eq!(one["folder"], text(&season));
        assert_eq!(one["files"], json!([]));
        // files, or a folder with files, are a list: media only, folders searched, no temp or backup files
        let mixed = classify_dropped_now(vec![text(&season.join("b.mkv")), text(&season.join("notes.txt")), text(&extras)]);
        assert_eq!(mixed["folder"], Value::Null);
        assert_eq!(mixed["files"], json!([text(&season.join("b.mkv")), text(&extras.join("e.avi"))]));
        assert_eq!(mixed["ignored"], 1);
        let two = classify_dropped_now(vec![text(&season), text(&extras)]);
        let names: Vec<String> = two["files"].as_array().unwrap().iter()
            .map(|f| std::path::Path::new(f.as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["e.avi", "a.MP4", "b.mkv"]); // each file once
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_is_renamed_within_its_folder_and_nothing_is_overwritten() {
        let dir = std::env::temp_dir().join(format!("mau-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.mkv"), dir.join("b.mkv"));
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let a_str = a.to_string_lossy().into_owned();
        for (name, why) in [
            ("", "empty"), ("  ", "empty"), ("x/y.mkv", "can't contain"), ("..", "can't contain"),
            ("a.mkv", "already the name"), ("b.mkv", "already exists"),
        ] {
            let error = rename_file_now(a_str.clone(), name.to_string(), None).unwrap_err();
            assert!(error.contains(why), "{name}: {error}");
        }
        assert_eq!(std::fs::read(&b).unwrap(), b"b");
        let renamed = rename_file_now(a_str.clone(), " c d.mkv ".to_string(), None).unwrap();
        let new = renamed["path"].as_str().unwrap().to_string();
        assert_eq!(std::path::Path::new(&new), dir.join("c d.mkv"));
        assert_eq!(renamed["log_entries"], 0);
        assert!(!a.exists() && dir.join("c d.mkv").is_file());
        // the entries about the file in a log follow it
        let log = dir.join("log.jsonl");
        std::fs::write(&log, format!("{{\"file\":{}}}\n", serde_json::json!(new))).unwrap();
        let moved = rename_file_now(new.clone(), "d.mkv".to_string(), Some(log.to_string_lossy().into_owned())).unwrap();
        assert_eq!(moved["log_entries"], 1);
        assert!(std::fs::read_to_string(&log).unwrap().contains("d.mkv"));
        let new = moved["path"].as_str().unwrap().to_string();
        // a change of case only is allowed even where the disk ignores case
        let upper = rename_file_now(new, "C D.mkv".to_string(), None).unwrap();
        assert!(dir.read_dir().unwrap().any(|e| e.unwrap().file_name() == "C D.mkv"), "{upper}");
        assert!(rename_file_now(dir.join("gone.mkv").to_string_lossy().into_owned(), "z".into(), None).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_details_give_the_size_and_say_why_a_file_cannot_be_read() {
        let path = std::env::temp_dir().join(format!("mau-details-{}.bin", std::process::id()));
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
    fn the_keep_languages_are_resolved_and_the_match_fields_passed_on() {
        let config = request(json!({
            "paths": ["a.mkv"], "keep": ["English", "it"], "codec": "ac-3", "channels": 6,
            "tasks": [{"name": "delete", "what": "match"}],
        }))
        .config()
        .unwrap();
        assert_eq!(config.keep, ["eng", "ita"]);
        assert_eq!(config.codec, "ac-3");
        assert_eq!(config.channels, 6);
        assert!(request(json!({"keep": ["Klingonese"]})).config().is_err());
    }

    #[test]
    fn an_unknown_kind_is_an_error_and_a_missing_one_is_audio() {
        assert_eq!(request(json!({})).config().unwrap().kind, Kind::Audio);
        assert_eq!(request(json!({"kind": " Sub "})).config().unwrap().kind, Kind::Subtitles);
        let error = request(json!({"kind": "video"})).config().unwrap_err();
        assert!(error.contains("\"video\" is not audio or sub"), "{error}");
    }

    #[test]
    fn the_about_text_is_abouts_md_and_links_are_only_web_addresses() {
        assert!(about_text().contains("Richard Spindler"));
        assert!(open_link("javascript:alert(1)".into()).unwrap_err().contains("not a web address"));
        assert!(open_link("file:///etc/passwd".into()).unwrap_err().contains("not a web address"));
        assert!(open_link("https://x.example/a & calc".into()).unwrap_err().contains("not a web address"));
    }

    #[test]
    fn the_online_search_is_refused_unless_it_was_enabled() {
        // the test binary starts without --enable search, so the search and the download are refused
        let search = tauri::async_runtime::block_on(search_subtitles("a.mkv".into(), String::new(), Vec::new()));
        assert_eq!(search.unwrap_err(), SEARCH_OFF);
        let fetch = tauri::async_runtime::block_on(fetch_subtitle(1));
        assert_eq!(fetch.unwrap_err(), SEARCH_OFF);
        assert_eq!(app_info_now()["search_enabled"], false);
    }

    #[test]
    fn an_unknown_language_is_an_error_the_page_can_show() {
        let error = request(json!({"lang": "Klingonese"})).config().unwrap_err();
        assert!(error.contains("unknown language"), "{error}");
    }

    #[test]
    fn the_same_rules_as_the_command_line_apply() {
        let config = request(json!({
            "paths": ["a.mkv"], "codec": "ac-3", "default": "eng",
            "tasks": [{"name": "set", "what": "lang"}],
        }))
        .config()
        .unwrap();
        assert!(validate(&config).unwrap_err().contains("--codec and --channels only work with --delete match"));
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
