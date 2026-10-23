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
    request_cancel, reset_cancel, validate, Config, Event, Fml, Kind, Outcome, Sink, Task,
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
    let config = request.config()?;
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan, run, cancel, app_info, languages, dev_report
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
