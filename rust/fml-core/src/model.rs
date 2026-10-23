//! The types shared by the engine and its front ends.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use serde_json::{json, Value};

use crate::track::{yes_no, Kind, Track};

/// The commands the tool runs and the package that provides each.
pub const REQUIRED_TOOLS: &[(&str, &str)] = &[
    ("mkvmerge", "mkvtoolnix"),
    ("mkvpropedit", "mkvtoolnix"),
    ("mediainfo", "mediainfo"),
    ("ffmpeg", "ffmpeg"),
];

/// Temporary files are named after the file they replace, with this in the name.
pub const TEMP_MARK: &str = ".fml-tmp";

/// With a backup the original is kept beside the result, with this added to its name.
pub const BACKUP_MARK: &str = ".fml-bak";

/// What `mkvmerge` exit codes mean: 0 success, 1 success with warnings, 2 error.
pub const MKVMERGE_OK: &[i32] = &[0, 1];

pub const FFMPEG_QUIET: &[&str] = &["-hide_banner", "-loglevel", "error", "-y"];

/// The kind of container a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mkv,
    Mp4,
    Avi,
}

impl Container {
    pub fn name(self) -> &'static str {
        match self {
            Container::Mkv => "Matroska",
            Container::Mp4 => "MP4",
            Container::Avi => "AVI",
        }
    }

    /// The commands needed to read a file of this kind.
    pub fn read_tools(self) -> &'static [&'static str] {
        match self {
            Container::Mkv => &["mkvmerge"],
            _ => &["mediainfo"],
        }
    }

    /// The commands needed to change a file of this kind.
    pub fn write_tools(self) -> &'static [&'static str] {
        match self {
            Container::Mkv => &["mkvmerge", "mkvpropedit"],
            _ => &["mediainfo", "ffmpeg"],
        }
    }
}

/// The outcome of an action on a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Changed,
    Unchanged,
    /// A warning: the file was left alone, and that is not a failure.
    Skipped,
    Error,
}

/// The result of an action on a file.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub file: String,
    pub action: String,
    pub status: Status,
    pub message: String,
    pub changes: Vec<String>,
    pub commands: Vec<Vec<String>>,
    pub data: Value,
    pub dry_run: bool,
    /// Where the original was kept, with a backup.
    pub backup: String,
    /// The place the track that was moved first had (for the log).
    pub moved_from: usize,
}

impl Outcome {
    pub fn new(file: &str, action: &str, dry_run: bool) -> Outcome {
        Outcome {
            file: file.to_string(),
            action: action.to_string(),
            status: Status::Ok,
            message: String::new(),
            changes: Vec::new(),
            commands: Vec::new(),
            data: Value::Null,
            dry_run,
            backup: String::new(),
            moved_from: 0,
        }
    }

    pub fn with_status(mut self, status: Status, message: &str) -> Outcome {
        self.status = status;
        self.message = message.to_string();
        self
    }

    pub fn error(&mut self, message: String) {
        self.status = Status::Error;
        self.message = message;
    }

    /// Mark a result that made changes as changed, unless it failed.
    pub fn finish(mut self) -> Outcome {
        if self.status == Status::Ok && !self.changes.is_empty() {
            self.status = Status::Changed;
        }
        self
    }

    pub fn to_json(&self) -> Value {
        json!({
            "file": self.file, "action": self.action, "status": self.status,
            "message": self.message, "changes": self.changes, "commands": self.commands,
            "dry_run": self.dry_run,
            "backup": if self.backup.is_empty() { Value::Null } else { json!(self.backup) },
            "data": self.data,
        })
    }
}

/// An action to run, with its argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub name: String,
    pub what: String,
}

impl Task {
    pub fn new(name: &str, what: &str) -> Task {
        Task { name: name.to_string(), what: what.to_string() }
    }
}

/// Settings from the command line or the GUI.
#[derive(Debug, Clone)]
pub struct Config {
    pub file: String,
    pub dir: String,
    /// An explicit list of files (from a front end that lets the user pick them); used
    /// instead of `file` and `dir` when it is not empty.
    pub paths: Vec<String>,
    pub recursive: bool,
    pub lang: String,
    /// A language, or "none" for no default subtitle track.
    pub default: String,
    pub prefer: Vec<String>,
    pub track: String,
    /// The kind of track the actions work on: audio or subtitles.
    pub kind: Kind,
    pub forced: bool,
    pub verbose: bool,
    pub dryrun: bool,
    pub install: bool,
    pub force: bool,
    /// Edit MKV files directly rather than a copy of them.
    pub in_place: bool,
    /// Keep the original beside the changed file.
    pub backup: bool,
    /// A file to record each change in, so that undo can reverse it.
    pub log: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// How many files to process at once.
    pub jobs: usize,
    /// Also make the chosen track the first of its kind.
    pub move_first: bool,
    pub name: Option<String>,
    pub tasks: Vec<Task>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            file: String::new(),
            dir: String::new(),
            paths: Vec::new(),
            recursive: false,
            lang: String::new(),
            default: String::new(),
            prefer: Vec::new(),
            track: String::new(),
            kind: Kind::Audio,
            forced: false,
            verbose: false,
            dryrun: false,
            install: false,
            force: false,
            in_place: false,
            backup: false,
            log: String::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            jobs: 1,
            move_first: false,
            name: None,
            tasks: Vec::new(),
        }
    }
}

/// A media file and the kind of container it is.
#[derive(Debug, Clone)]
pub struct Media {
    pub path: String,
    pub container: Container,
    pub tracks: Option<Vec<Track>>,
}

impl Media {
    pub fn new(path: &str, container: Container) -> Media {
        Media { path: path.to_string(), container, tracks: None }
    }
}

/// A change to a track: its default flag, language, name or forced flag, or its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub track: Track,
    pub default: Option<bool>,
    pub language: Option<String>,
    /// The place to move it to among tracks of its kind (1 is first).
    pub move_to: Option<usize>,
    pub name: Option<String>,
    pub forced: Option<bool>,
}

impl Edit {
    pub fn new(track: &Track) -> Edit {
        Edit {
            track: track.clone(),
            default: None,
            language: None,
            move_to: None,
            name: None,
            forced: None,
        }
    }

    pub fn describe(&self) -> String {
        let track = &self.track;
        let mut parts = Vec::new();
        if let Some(default) = self.default {
            parts.push(format!("default {} -> {}", yes_no(track.default), yes_no(default)));
        }
        if let Some(language) = &self.language {
            parts.push(format!("language {} -> {}", track.language, language));
        }
        if let Some(name) = &self.name {
            parts.push(format!("name \"{}\" -> \"{}\"", track.name, name));
        }
        if let Some(forced) = self.forced {
            parts.push(format!("forced {} -> {}", yes_no(track.forced), yes_no(forced)));
        }
        if let Some(place) = self.move_to {
            parts.push(if place == 1 {
                format!("moved to the first {} track", track.kind.word())
            } else {
                format!("moved to {} track {}", track.kind.word(), place)
            });
        }
        let named = self.default.is_some()
            || self.move_to.is_some()
            || self.name.is_some()
            || self.forced.is_some();
        let label = if named {
            format!("{} ({})", track.label(), track.language)
        } else {
            track.label()
        };
        format!("{label}: {}", parts.join(", "))
    }
}

/// Why everything stops: a problem such as a missing tool, or being asked to stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    Fatal(String),
    Interrupted,
}

pub type Res<T> = Result<T, Stop>;

/// What the engine tells its front end while it works.
#[derive(Debug, Clone)]
pub enum Event {
    Warn(String),
    /// A message for --verbose.
    Note(String),
    Result(Box<Outcome>),
}

pub type Sink = std::sync::Arc<dyn Fn(Event) + Send + Sync>;

static CANCEL: AtomicBool = AtomicBool::new(false);

/// Ask the engine to stop: running commands are ended and temporary files removed.
pub fn request_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

pub fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

/// Forget an earlier request to stop, before a new run.
pub fn reset_cancel() {
    CANCEL.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(number: usize, language: &str, default: bool) -> Track {
        Track {
            id: number as i64,
            kind: Kind::Audio,
            number,
            codec: "AAC".into(),
            language: language.into(),
            default,
            forced: false,
            name: String::new(),
        }
    }

    #[test]
    fn edits_describe_themselves() {
        let mut edit = Edit::new(&track(2, "eng", false));
        edit.default = Some(true);
        edit.move_to = Some(1);
        assert_eq!(
            edit.describe(),
            "audio track 2 (eng): default no -> yes, moved to the first audio track"
        );
        let mut edit = Edit::new(&track(1, "ita", true));
        edit.language = Some("eng".into());
        assert_eq!(edit.describe(), "audio track 1: language ita -> eng");
    }

    #[test]
    fn finished_results_with_changes_are_changed() {
        let mut outcome = Outcome::new("a.mkv", "set", false);
        outcome.changes.push("x".into());
        assert_eq!(outcome.finish().status, Status::Changed);
        let mut failed = Outcome::new("a.mkv", "set", false);
        failed.changes.push("x".into());
        failed.error("boom".into());
        assert_eq!(failed.finish().status, Status::Error);
    }
}
