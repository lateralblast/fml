//! The engine: runs the requested actions against media files.
//!
//! Every change is made on a copy of the file (or, for an MKV, on a rewritten file),
//! read back and checked, and only then moved over the original.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use serde_json::{json, Value};

use crate::model::*;
use crate::install::{detect_installer, Installer};
use crate::mp4;
use crate::track::{
    format_tracks, human_size, parse_mediainfo_tracks, parse_mkv_tracks, Kind, Track, TrackSpec,
};
use crate::util::{
    copy_file, copy_stat, detect_kind, disk_free, fnmatch, run_command,
    shell_join, strerror, which, Completed, Ran,
};
use crate::VERSION;

/// Room on disk that running jobs have set aside, so several at once can't each find
/// enough and then use it up between them.
#[derive(Default)]
struct Space {
    reserved: HashMap<u64, i64>,
    holds: HashMap<String, Vec<(u64, i64)>>,
}

#[derive(Default)]
struct Shared {
    /// Temporary files to remove if we are stopped.
    pending: Mutex<Vec<String>>,
    space: Mutex<Space>,
    install: Mutex<()>,
    log: Mutex<()>,
}

/// What a run did.
pub struct RunEnd {
    pub results: Vec<Outcome>,
    /// The exit status.
    pub code: i32,
}

pub struct Fml {
    pub config: Config,
    shared: Arc<Shared>,
    sink: Sink,
    /// What `--install` uses on this system, if it is known.
    installer: Option<Installer>,
}

fn skip(mut result: Outcome, message: &str) -> Outcome {
    result.status = Status::Skipped;
    result.message = message.to_string();
    result
}

fn mark_skipped(result: &mut Outcome, message: &str) {
    result.status = Status::Skipped;
    result.message = message.to_string();
}

/// An absolute, tidied path (without following links), like Python's `os.path.abspath`.
pub fn abspath(path: &str) -> String {
    let full = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for part in full.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

fn temp_path(path: &str) -> String {
    // ffmpeg picks the output format from the extension, so the real one is kept
    let p = Path::new(path);
    match p.extension() {
        Some(ext) => format!(
            "{}{}.{}",
            p.with_extension("").to_string_lossy(),
            TEMP_MARK,
            ext.to_string_lossy()
        ),
        None => format!("{path}{TEMP_MARK}"),
    }
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The tracks, with the one being moved put at a place among its kind.
pub fn reordered(all: &[Track], moving: Option<&Track>, to: usize) -> Vec<Track> {
    match moving {
        Some(moving) => reordered_by(all, &[(moving.clone(), to)]),
        None => all.to_vec(),
    }
}

/// The tracks, with each track in `moves` put at its place (from 1) among its kind, and the
/// kind's other tracks filling the places left, in their old order. A place past the end is
/// the last; two tracks wanting one place get the next free one.
pub fn reordered_by(all: &[Track], moves: &[(Track, usize)]) -> Vec<Track> {
    let mut tracks = all.to_vec();
    let mut kinds: Vec<Kind> = Vec::new();
    for (track, _) in moves {
        if !kinds.contains(&track.kind) {
            kinds.push(track.kind);
        }
    }
    for kind in kinds {
        let slots: Vec<usize> =
            tracks.iter().enumerate().filter(|(_, t)| t.kind == kind).map(|(i, _)| i).collect();
        let count = slots.len();
        let mut placed: Vec<Option<Track>> = vec![None; count];
        let mut moving: Vec<(Kind, usize)> = Vec::new();
        for (track, to) in moves.iter().filter(|(t, _)| t.kind == kind) {
            let want = (*to).clamp(1, count.max(1)) - 1;
            if let Some(at) = (want..count).chain(0..want).find(|i| placed[*i].is_none()) {
                placed[at] = Some(track.clone());
                moving.push(track.key());
            }
        }
        let mut rest = tracks
            .iter()
            .filter(|t| t.kind == kind && !moving.contains(&t.key()))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter();
        for (slot, track) in slots.iter().zip(placed) {
            if let Some(track) = track.or_else(|| rest.next()) {
                tracks[*slot] = track;
            }
        }
    }
    tracks
}

/// The track numbers of a kind put in the order of a list of languages: the tracks in the
/// first language, then those in the second, and so on, then the rest; each group in its
/// old order.
pub fn order_by_languages(tracks: &[Track], languages: &[String]) -> Vec<usize> {
    let rank = |t: &Track| languages.iter().position(|l| *l == t.language).unwrap_or(languages.len());
    let mut sorted: Vec<&Track> = tracks.iter().collect();
    sorted.sort_by_key(|t| rank(t)); // stable
    sorted.iter().map(|t| t.number).collect()
}

/// The edits that make one track the only default (none if `chosen` is `None`), give it
/// a language if one is given, and move it first if asked. Tracks that are right are
/// left alone.
pub fn default_edits(
    tracks: &[Track],
    chosen: Option<&Track>,
    language: &str,
    move_first: bool,
) -> Vec<Edit> {
    let mut edits = Vec::new();
    for track in tracks {
        let is_chosen = chosen.is_some_and(|c| c.number == track.number);
        let default = if track.default == is_chosen { None } else { Some(is_chosen) };
        let new_language = (is_chosen && !language.is_empty() && language != track.language)
            .then(|| language.to_string());
        let moving = move_first && is_chosen && track.number != tracks[0].number;
        if default.is_some() || new_language.is_some() || moving {
            let mut edit = Edit::new(track);
            edit.default = default;
            edit.language = new_language;
            edit.move_to = moving.then_some(1);
            edits.push(edit);
        }
    }
    edits
}

/// The tracks the edits move, with the place each goes to.
fn moves_of(edits: &[Edit]) -> Vec<(Track, usize)> {
    edits.iter().filter_map(|e| e.move_to.map(|to| (e.track.clone(), to))).collect()
}

/// The tracks of each kind, in the order they will have in the new file; `replaced`
/// gives the new list for a kind that changes.
fn kept_tracks(all: &[Track], replaced: Option<(Kind, Vec<Track>)>) -> Vec<(Kind, Vec<Track>)> {
    let mut kept: Vec<(Kind, Vec<Track>)> = Vec::new();
    for track in all {
        match kept.iter_mut().find(|(kind, _)| *kind == track.kind) {
            Some((_, list)) => list.push(track.clone()),
            None => kept.push((track.kind, vec![track.clone()])),
        }
    }
    if let Some((kind, tracks)) = replaced {
        match kept.iter_mut().find(|(k, _)| *k == kind) {
            Some(slot) => slot.1 = tracks,
            None => kept.push((kind, tracks)),
        }
    }
    kept
}

/// ffmpeg options that write the track names back into a rewritten MP4.
///
/// ffmpeg drops the names it cannot read from an MP4 (the ones mediainfo reports as the
/// title), so each is written again, at the track's place in the new file. `renamed`
/// gives new names for tracks being renamed (an empty name removes it).
fn name_args(
    kept: &[(Kind, Vec<Track>)],
    renamed: &HashMap<(Kind, usize), String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (kind, tracks) in kept {
        for (index, track) in tracks.iter().enumerate() {
            let name = renamed.get(&track.key()).unwrap_or(&track.name);
            if !name.is_empty() {
                out.push(format!("-metadata:s:{}:{}", kind.selector(), index));
                out.push(format!("title={name}"));
            }
        }
    }
    out
}

/// Point the entries of a `--log` file that are about `from` at `to`, after the file was renamed,
/// so that `--undo` still finds it. Other lines are kept as they are. The log is rewritten
/// through a temporary file, so it is never left half written. Returns how many entries were
/// changed; a log that does not exist has none.
pub fn rename_in_log(log: &str, from: &str, to: &str) -> io::Result<usize> {
    let text = match fs::read_to_string(log) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let (from, to) = (abspath(from), abspath(to));
    let mut changed = 0;
    let lines: Vec<String> = text
        .lines()
        .map(|line| match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(mut entry)) if entry.get("file").and_then(Value::as_str) == Some(&from) => {
                entry.insert("file".to_string(), json!(to));
                changed += 1;
                Value::Object(entry).to_string()
            }
            _ => line.to_string(),
        })
        .collect();
    if changed > 0 {
        let temp_file = format!("{log}{TEMP_MARK}");
        let mut out = lines.join("\n");
        if text.ends_with('\n') {
            out.push('\n');
        }
        let written = fs::write(&temp_file, out)
            .and_then(|()| copy_stat(Path::new(log), Path::new(&temp_file)))
            .and_then(|()| fs::rename(&temp_file, log));
        if let Err(error) = written {
            let _ = fs::remove_file(&temp_file);
            return Err(error);
        }
    }
    Ok(changed)
}

/// The tracks before and after a change, for a front end that shows what it does. It goes in the
/// result's `data` as `{"layout": {"before": [...], "after": [...]}}`.
fn layout_data(before: &[Track], after: &[Track]) -> Value {
    json!({"layout": {"before": before, "after": after}})
}

/// The extension a subtitle track of this codec is extracted with, or `None` if it can't be
/// extracted to a single file (VobSub is two files). A text subtitle in an MP4 is written as `.srt`.
pub fn subtitle_extension(container: Container, codec: &str) -> Option<&'static str> {
    let codec = codec.to_lowercase();
    if container == Container::Mp4 {
        return (codec.contains("timed text") || codec.contains("tx3g") || codec.contains("mov_text"))
            .then_some("srt");
    }
    if codec.contains("subrip") || codec.contains("srt") {
        Some("srt")
    } else if codec.contains("substation") {
        Some("ass")
    } else if codec.contains("webvtt") {
        Some("vtt")
    } else if codec.contains("pgs") {
        Some("sup")
    } else {
        None
    }
}

/// The track that becomes the default when the default is removed.
///
/// If none of the tracks left has the default flag, the first one gets it. That is
/// always done for audio, so a file isn't left without a default track, and for an MP4,
/// where the muxer insists on a default track for every kind. An MKV may have no default
/// subtitle track, so none is made. AVI has no flags.
fn promotion(container: Container, survivors: &[Track]) -> Option<Track> {
    if container == Container::Avi || survivors.is_empty() || survivors.iter().any(|t| t.default) {
        return None;
    }
    let first = &survivors[0];
    (first.kind == Kind::Audio || container == Container::Mp4).then(|| first.clone())
}

/// A track name as comparable text: mkvtoolnix on macOS stores text in decomposed form
/// (`c` and a combining cedilla for `ç`), so the name read back is not byte for byte the name given.
fn same_name(a: &str, b: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    a.trim().nfc().eq(b.trim().nfc())
}

fn same_state(first: &[Track], second: &[Track]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(a, b)| {
            (a.kind, &a.language, a.default, a.forced) == (b.kind, &b.language, b.default, b.forced)
                && same_name(&a.name, &b.name)
        })
}

/// Whether a path (below the directory) matches one of the patterns, ignoring case.
///
/// A pattern with no `/` matches the file name or the name of any folder above it, so
/// `Extras` skips everything in a folder of that name. A pattern with a `/` is matched
/// against the whole path, as in `Season 1/*`.
pub fn pattern_hit(relative: &Path, patterns: &[String]) -> bool {
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let text = parts.join("/");
    patterns.iter().map(|p| p.to_lowercase()).any(|pattern| {
        if pattern.contains('/') {
            fnmatch(&pattern, &text)
        } else {
            parts.iter().any(|part| fnmatch(&pattern, part))
        }
    })
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        out.push(path.clone());
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if recursive && is_dir {
            walk(&path, recursive, out);
        }
    }
}

impl Fml {
    pub fn new(config: Config, sink: Sink) -> Fml {
        Fml { config, shared: Arc::new(Shared::default()), sink, installer: detect_installer() }
    }

    /// The same engine, with the settings of one action on top of its own.
    fn for_task(&self, task: &Task) -> Fml {
        Fml {
            config: self.config.for_task(task),
            shared: Arc::clone(&self.shared),
            sink: Arc::clone(&self.sink),
            installer: self.installer,
        }
    }

    /// The same engine, working on another kind of track.
    fn with_kind(&self, kind: Kind) -> Fml {
        let mut config = self.config.clone();
        config.kind = kind;
        Fml {
            config,
            shared: Arc::clone(&self.shared),
            sink: Arc::clone(&self.sink),
            installer: self.installer,
        }
    }

    fn kind(&self) -> Kind {
        self.config.kind
    }

    fn word(&self) -> &'static str {
        self.config.kind.word()
    }

    // Messages and commands ----------------------------------------------------

    pub fn warn(&self, message: &str) {
        (self.sink)(Event::Warn(message.to_string()));
    }

    /// A message shown with --verbose.
    fn note(&self, message: &str) {
        if self.config.verbose {
            (self.sink)(Event::Note(message.to_string()));
        }
    }

    /// Run a read-only command (also during a dry run). `None` if it can't be run.
    fn query(&self, cmd: &[String]) -> Res<Option<Completed>> {
        self.note(&format!("Executing:\t{}", shell_join(cmd)));
        match run_command(cmd) {
            Ok(Ran::Done(done)) => Ok(Some(done)),
            Ok(Ran::Cancelled) => Err(Stop::Interrupted),
            Err(_) => Ok(None),
        }
    }

    /// Run a command that changes things, unless this is a dry run. The command is
    /// recorded in the result. On failure the result becomes an error.
    fn apply(&self, result: &mut Outcome, cmd: Vec<String>, ok_codes: &[i32]) -> Res<bool> {
        self.note(&format!("Executing:\t{}", shell_join(&cmd)));
        result.commands.push(cmd.clone());
        if self.config.dryrun {
            return Ok(true);
        }
        match run_command(&cmd) {
            Err(error) => {
                result.error(format!("{} failed: {}", cmd[0], strerror(&error)));
                Ok(false)
            }
            Ok(Ran::Cancelled) => Err(Stop::Interrupted),
            Ok(Ran::Done(done)) if !ok_codes.contains(&done.code) => {
                let text = if done.stderr.trim().is_empty() { &done.stdout } else { &done.stderr };
                let last = text.trim().lines().last().map(str::to_string);
                result.error(format!(
                    "{} failed with exit code {}{}",
                    cmd[0],
                    done.code,
                    last.map(|l| format!(": {l}")).unwrap_or_default()
                ));
                Ok(false)
            }
            Ok(Ran::Done(_)) => Ok(true),
        }
    }

    // Environment --------------------------------------------------------------

    fn install_package(&self, package: &str) {
        let Some(installer) = self.installer else {
            let hint = if cfg!(windows) { " (install winget or Chocolatey)" } else { "" };
            self.warn(&format!("don't know how to install \"{package}\" on this OS{hint}"));
            return;
        };
        let cmd = installer.command(package);
        self.note(&format!("Executing:\t{}", shell_join(&cmd)));
        if !self.config.dryrun {
            // found through PATHEXT on Windows (winget is winget.exe)
            let program = which(&cmd[0]).unwrap_or_else(|| PathBuf::from(&cmd[0]));
            let _ = Command::new(program).args(&cmd[1..]).status();
        }
    }

    fn install_missing(&self, missing: &[String]) {
        let mut done: Vec<&str> = Vec::new();
        for tool in missing {
            if let Some(&(_, package)) = REQUIRED_TOOLS.iter().find(|(t, _)| t == tool) {
                if !done.contains(&package) {
                    done.push(package);
                    self.install_package(package);
                }
            }
        }
        // the installers change PATH for programs started later, not for this one
        if !self.config.dryrun && !done.is_empty() {
            crate::util::add_tool_folders();
        }
    }

    /// Report which of the required commands are installed.
    pub fn check_tools(&self) -> Outcome {
        let find = || -> Vec<(String, Option<PathBuf>)> {
            REQUIRED_TOOLS.iter().map(|(t, _)| (t.to_string(), which(t))).collect()
        };
        let mut found = find();
        let missing_of = |found: &[(String, Option<PathBuf>)]| -> Vec<String> {
            found.iter().filter(|(_, p)| p.is_none()).map(|(t, _)| t.clone()).collect()
        };
        let mut missing = missing_of(&found);
        if !missing.is_empty() && self.config.install {
            self.install_missing(&missing);
            if !self.config.dryrun {
                found = find();
                missing = missing_of(&found);
            }
        }
        let mut result = Outcome::new("", "check", self.config.dryrun);
        result.data = Value::Array(
            found
                .iter()
                .map(|(tool, path)| {
                    let package = REQUIRED_TOOLS.iter().find(|(t, _)| t == tool).map(|(_, p)| *p);
                    json!({"tool": tool, "path": path.as_ref().map(|p| p.to_string_lossy()),
                           "package": package})
                })
                .collect(),
        );
        if !missing.is_empty() && !(self.config.install && self.config.dryrun) {
            let mut message = format!("missing: {}", missing.join(", "));
            if !self.config.install {
                message.push_str(" (use --install to install them)");
            }
            result.error(message);
        }
        result
    }

    /// Make sure commands are installed, installing them if install was asked for.
    fn require(&self, tools: &[&str]) -> Res<()> {
        let missing_now = || -> Vec<String> {
            tools.iter().filter(|t| which(t).is_none()).map(|t| t.to_string()).collect()
        };
        let mut missing = missing_now();
        if missing.is_empty() {
            return Ok(());
        }
        if self.config.install {
            {
                // the first thread installs, the others then find them
                let _guard = self.shared.install.lock().unwrap();
                missing = missing_now();
                if !missing.is_empty() {
                    self.install_missing(&missing);
                }
            }
            if self.config.dryrun {
                return Ok(());
            }
            missing = missing_now();
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Stop::Fatal(format!(
                "command not found: {} (use --install to install, or --check to see what is missing)",
                missing.join(", ")
            )))
        }
    }

    // Files --------------------------------------------------------------------

    /// The file, or the files in the directory (that include and exclude allow).
    pub fn files(&self) -> Res<Vec<String>> {
        if !self.config.paths.is_empty() {
            return Ok(self.config.paths.clone());
        }
        if !self.config.file.is_empty() {
            return Ok(vec![self.config.file.clone()]);
        }
        let base = Path::new(&self.config.dir);
        if !base.is_dir() {
            return Err(Stop::Fatal(format!("directory \"{}\" does not exist", self.config.dir)));
        }
        let mut paths = Vec::new();
        walk(base, self.config.recursive, &mut paths);
        paths.sort();
        let mut files = Vec::new();
        for path in paths {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !path.is_file() || name.contains(TEMP_MARK) || name.contains(BACKUP_MARK) {
                continue;
            }
            let relative = path.strip_prefix(base).unwrap_or(&path);
            if !self.config.include.is_empty() && !pattern_hit(relative, &self.config.include) {
                continue;
            }
            if pattern_hit(relative, &self.config.exclude) {
                continue;
            }
            files.push(path.to_string_lossy().into_owned());
        }
        Ok(files)
    }

    /// Read the tracks of a file.
    pub fn read_tracks(&self, media: &Media) -> Res<Vec<Track>> {
        let cmd = if media.container == Container::Mkv {
            args(&["mkvmerge", "-F", "json", "-i", &media.path])
        } else {
            args(&["mediainfo", "--Output=JSON", &media.path])
        };
        let Some(done) = self.query(&cmd)? else { return Ok(Vec::new()) };
        let parsed = serde_json::from_str::<Value>(&done.stdout)
            .map_err(|e| e.to_string())
            .and_then(|data| {
                if media.container == Container::Mkv {
                    parse_mkv_tracks(&data)
                } else {
                    parse_mediainfo_tracks(&data)
                }
            });
        match parsed {
            Ok(tracks) => Ok(tracks),
            Err(_) => {
                self.warn(&format!("could not read the tracks of {}", media.path));
                Ok(Vec::new())
            }
        }
    }

    /// The tracks of a file (read once, then remembered).
    fn tracks(&self, media: &mut Media) -> Res<Vec<Track>> {
        if media.tracks.is_none() {
            media.tracks = Some(self.read_tracks(media)?);
        }
        Ok(media.tracks.clone().unwrap_or_default())
    }

    /// The tracks of the kind being worked on.
    fn selected(&self, media: &mut Media) -> Res<Vec<Track>> {
        Ok(self.tracks(media)?.into_iter().filter(|t| t.kind == self.kind()).collect())
    }

    fn result(&self, media: &Media, action: &str) -> Outcome {
        Outcome::new(&media.path, action, self.config.dryrun)
    }

    /// The track chosen with --track (the first if none was).
    fn resolve_track(&self, media: &mut Media, result: &mut Outcome) -> Res<Option<Track>> {
        let Some(track) = self.resolve_any_track(media, result)? else { return Ok(None) };
        if track.kind != self.kind() {
            mark_skipped(
                result,
                &format!(
                    "{} is a {} track, not {} (use --kind {})",
                    self.config.track,
                    track.kind.word(),
                    self.word(),
                    track.kind.name()
                ),
            );
            return Ok(None);
        }
        Ok(Some(track))
    }

    /// The track chosen with --track, of whatever kind: a position among the tracks of the kind
    /// being worked on, or `audio:N`/`subtitles:N`, `id:N` or `name:TEXT`.
    fn resolve_any_track(&self, media: &mut Media, result: &mut Outcome) -> Res<Option<Track>> {
        let all = self.tracks(media)?;
        let text = if self.config.track.is_empty() { "1" } else { self.config.track.as_str() };
        let spec = match TrackSpec::parse(text) {
            Ok(spec) => spec,
            Err(message) => {
                mark_skipped(result, &message);
                return Ok(None);
            }
        };
        let of_kind = |kind: Kind| -> Vec<Track> { all.iter().filter(|t| t.kind == kind).cloned().collect() };
        let by_position = |kind: Kind, wanted: Option<usize>| -> Result<Track, String> {
            let tracks = of_kind(kind);
            let number = wanted.unwrap_or(tracks.len());
            if number < 1 || number > tracks.len() {
                return Err(format!(
                    "has no {w} track {text} ({n} {w} tracks)",
                    w = kind.word(),
                    n = tracks.len()
                ));
            }
            Ok(tracks[number - 1].clone())
        };
        let found = match &spec {
            TrackSpec::Number(n) => by_position(self.kind(), Some(*n)),
            TrackSpec::Last => by_position(self.kind(), None),
            TrackSpec::Of(kind, n) => by_position(*kind, *n),
            TrackSpec::Id(id) => all
                .iter()
                .find(|t| t.id == *id)
                .cloned()
                .ok_or_else(|| format!("has no track with ID {id}")),
            TrackSpec::Name(name) => {
                let hits: Vec<&Track> =
                    all.iter().filter(|t| t.name.to_lowercase() == name.to_lowercase()).collect();
                match hits.len() {
                    0 => Err(format!("has no track named \"{name}\"")),
                    1 => Ok(hits[0].clone()),
                    n => Err(format!(
                        "has {n} tracks named \"{name}\" (choose one with id:N, for ID {})",
                        hits.iter().map(|t| t.id.to_string()).collect::<Vec<_>>().join(", ")
                    )),
                }
            }
        };
        match found {
            Ok(track) => Ok(Some(track)),
            Err(message) => {
                mark_skipped(result, &message);
                Ok(None)
            }
        }
    }

    /// Check there is room beside a file to work on a copy of it as big as the file.
    ///
    /// The room is set aside until the file is done (`release_space`), so that several
    /// jobs at once can't each find enough and then use it up between them.
    fn check_space(&self, result: &mut Outcome, path: &str, what: &str) -> bool {
        let checked = (|| -> io::Result<(i64, u64, i64)> {
            let need = fs::metadata(path)?.len() as i64;
            let folder = fs::canonicalize(path)?.parent().map(Path::to_path_buf).unwrap_or_default();
            let (free, device) = disk_free(&folder)?;
            Ok((need, device, free as i64))
        })();
        let (need, device, free_now) = match checked {
            Ok(values) => values,
            Err(error) => {
                result.error(format!("could not check the free space: {}", strerror(&error)));
                return false;
            }
        };
        let mut space = self.shared.space.lock().unwrap();
        let free = free_now - space.reserved.get(&device).copied().unwrap_or(0);
        if free >= need {
            *space.reserved.entry(device).or_insert(0) += need;
            space.holds.entry(path.to_string()).or_default().push((device, need));
            return true;
        }
        result.changes.clear();
        result.error(format!(
            "not enough free space to {what}: needs {}, {} free",
            human_size(need as f64),
            human_size(free.max(0) as f64)
        ));
        false
    }

    /// Give back the room a file set aside.
    fn release_space(&self, path: &str) {
        let mut space = self.shared.space.lock().unwrap();
        if let Some(holds) = space.holds.remove(path) {
            for (device, need) in holds {
                *space.reserved.entry(device).or_insert(0) -= need;
            }
        }
    }

    fn pend(&self, temp_file: &str) {
        self.shared.pending.lock().unwrap().push(temp_file.to_string());
    }

    /// Copy a file, so the copy can be changed and swapped in if that works.
    /// Run `work`, which writes `temp_file`, and while it does tell the front end how big the
    /// file has got compared with `source` (a copy, rewrite or removal makes a file about that
    /// size). It tells nothing unless asked (`Config::progress`) or in a dry run.
    fn track_progress<T>(&self, file: &str, source: &str, temp_file: &str, stage: &str, work: impl FnOnce() -> T) -> T {
        if !self.config.progress || self.config.dryrun {
            return work();
        }
        let total = fs::metadata(source).map(|m| m.len()).unwrap_or(0);
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !stop.load(Ordering::SeqCst) {
                    let done = fs::metadata(temp_file).map(|m| m.len()).unwrap_or(0);
                    (self.sink)(Event::Progress {
                        file: file.to_string(),
                        stage: stage.to_string(),
                        done: done.min(total),
                        total,
                    });
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            });
            let outcome = work();
            stop.store(true, Ordering::SeqCst);
            outcome
        })
    }

    /// `apply` for a command that writes `temp_file` from `source`, with progress.
    fn apply_tracked(
        &self,
        result: &mut Outcome,
        cmd: Vec<String>,
        ok_codes: &[i32],
        source: &str,
        temp_file: &str,
        stage: &str,
    ) -> Res<bool> {
        self.track_progress(source, source, temp_file, stage, || self.apply(result, cmd, ok_codes))
    }

    fn start_copy(&self, result: &mut Outcome, path: &str, temp_file: &str) -> Res<bool> {
        if !self.check_space(result, path, "copy the file (use --in-place to edit it directly)") {
            return Ok(false);
        }
        self.pend(temp_file);
        self.note(&format!("Copying:\t{path} -> {temp_file}"));
        if self.config.dryrun {
            return Ok(true);
        }
        let copied = self.track_progress(path, path, temp_file, "Copying", || {
            copy_file(Path::new(path), Path::new(temp_file))
        });
        match copied {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(Stop::Interrupted),
            Err(error) => {
                let _ = fs::remove_file(temp_file);
                result.error(format!("could not copy the file: {}", strerror(&error)));
                Ok(false)
            }
        }
    }

    /// Where the original is kept: the file's name and `.fml-bak`, or a numbered one if
    /// that is taken.
    fn backup_path(path: &str) -> String {
        let mut backup = format!("{path}{BACKUP_MARK}");
        let mut number = 0;
        while fs::symlink_metadata(&backup).is_ok() {
            number += 1;
            backup = format!("{path}{BACKUP_MARK}.{number}");
        }
        backup
    }

    /// Keep the original as a backup. A hard link costs nothing, and is only possible
    /// while the file is going to be replaced, not edited; if it fails, or a copy is
    /// needed, the file is copied. Returns false (and an error result) if it can't be.
    fn keep_original(&self, result: &mut Outcome, path: &str, link: bool) -> Res<bool> {
        let backup = Self::backup_path(path);
        result.backup = backup.clone();
        result.changes.push(format!(
            "the original {} kept as {backup}",
            if self.config.dryrun { "would be" } else { "is" }
        ));
        if self.config.dryrun {
            return Ok(true);
        }
        if !link || fs::hard_link(path, &backup).is_err() {
            if !self.check_space(result, path, "keep a backup") {
                return Ok(false);
            }
            match copy_file(Path::new(path), Path::new(&backup)) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    let _ = fs::remove_file(&backup);
                    return Err(Stop::Interrupted);
                }
                Err(error) => {
                    let _ = fs::remove_file(&backup);
                    result.error(format!("could not keep a backup: {}", strerror(&error)));
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Replace a file with its changed copy, keeping its permissions and times.
    ///
    /// On an error the copy is removed and the original is left as it was. With a backup
    /// the original is kept first, and if that fails nothing is replaced.
    fn replace_file(&self, result: &mut Outcome, temp_file: &str, path: &str) -> Res<()> {
        if self.config.backup && result.status != Status::Error {
            self.keep_original(result, path, true)?;
        }
        if self.config.dryrun {
            return Ok(());
        }
        if result.status == Status::Error {
            let _ = fs::remove_file(temp_file);
        } else if let Err(error) = copy_stat(Path::new(path), Path::new(temp_file))
            .and_then(|()| fs::rename(temp_file, path))
        {
            let _ = fs::remove_file(temp_file);
            result.error(format!("could not replace the file: {}", strerror(&error)));
        }
        self.shared.pending.lock().unwrap().retain(|p| p != temp_file);
        Ok(())
    }

    /// Remove any temporary file left when we are stopped part way.
    pub fn cleanup(&self) {
        let mut pending = self.shared.pending.lock().unwrap();
        for temp_file in pending.iter() {
            let _ = fs::remove_file(temp_file);
        }
        pending.clear();
    }

    // Choosing tracks ----------------------------------------------------------

    /// The track to use for a language: with --forced the first forced track, otherwise
    /// the first one (preferring a track that is not forced, for subtitles).
    fn pick_track(&self, tracks: &[Track], lang: &str) -> Option<Track> {
        let mut candidates: Vec<&Track> = tracks.iter().filter(|t| t.language == lang).collect();
        if self.config.forced {
            candidates.retain(|t| t.forced);
        } else if self.kind() == Kind::Subtitles {
            let unforced: Vec<&Track> = candidates.iter().copied().filter(|t| !t.forced).collect();
            if !unforced.is_empty() {
                candidates = unforced;
            }
        }
        candidates.first().map(|t| (*t).clone())
    }

    /// The languages to look for, in order: --prefer, or the single language.
    fn languages(&self, single: &str) -> Vec<String> {
        if !self.config.prefer.is_empty() {
            self.config.prefer.clone()
        } else if !single.is_empty() {
            vec![single.to_string()]
        } else {
            Vec::new()
        }
    }

    /// The first language that has a track, with that track.
    fn choose(&self, tracks: &[Track], languages: &[String]) -> Option<(String, Track)> {
        languages
            .iter()
            .find_map(|lang| self.pick_track(tracks, lang).map(|track| (lang.clone(), track)))
    }

    fn missing_message(&self, languages: &[String]) -> String {
        if languages.len() == 1 {
            format!("has no {} track with language \"{}\"", self.word(), languages[0])
        } else {
            format!("has no {} track in any of: {}", self.word(), languages.join(", "))
        }
    }

    // Editing tracks -----------------------------------------------------------

    /// The tracks the file should have after the edits.
    fn expected_layout(&self, all: &[Track], edits: &[Edit]) -> Vec<Track> {
        let by_track: HashMap<(Kind, usize), &Edit> =
            edits.iter().map(|e| (e.track.key(), e)).collect();
        reordered_by(all, &moves_of(edits))
            .into_iter()
            .map(|mut track| {
                if let Some(edit) = by_track.get(&track.key()) {
                    if let Some(default) = edit.default {
                        track.default = default;
                    }
                    if let Some(language) = edit.language.as_ref().filter(|l| !l.is_empty()) {
                        track.language = language.clone();
                    }
                    if let Some(name) = &edit.name {
                        track.name = name.clone();
                    }
                    if let Some(forced) = edit.forced {
                        track.forced = forced;
                    }
                }
                track
            })
            .collect()
    }

    /// Check a changed copy has the expected tracks, in order, with the expected
    /// languages and names and (for the kind being edited) default flags. A tool can exit
    /// successfully without applying a change, or can lose something, so this is checked
    /// before the copy replaces the original.
    fn verify_layout(
        &self,
        result: &mut Outcome,
        rewritten: &Media,
        expected: &[Track],
        tool: &str,
    ) -> Res<()> {
        let actual = self.read_tracks(rewritten)?;
        let wrong = actual.len() != expected.len()
            || actual.iter().zip(expected).any(|(got, want)| {
                got.kind != want.kind
                    || got.language != want.language
                    || !same_name(&got.name, &want.name)
                    || (rewritten.container == Container::Mkv && got.forced != want.forced)
                    || (want.kind == self.kind() && got.default != want.default)
            });
        if wrong {
            result.error(format!("{tool} did not make the change to the file"));
        }
        Ok(())
    }

    /// Apply edits to a file.
    ///
    /// An MKV is edited with mkvpropedit, on a copy that replaces the file if it worked
    /// (or in place with --in-place). If a track has to be moved, or the file is an MP4,
    /// the file is rewritten instead, with mkvmerge or ffmpeg. Either way the result is
    /// read back and checked before it replaces the original.
    fn edit_tracks(
        &self,
        media: &mut Media,
        mut result: Outcome,
        edits: Vec<Edit>,
        unchanged: &str,
    ) -> Res<Outcome> {
        if edits.is_empty() {
            result.status = Status::Unchanged;
            result.message = unchanged.to_string();
            return Ok(result);
        }
        result.changes.extend(edits.iter().map(Edit::describe));
        let moves = moves_of(&edits);
        if let [(track, _)] = moves.as_slice() {
            result.moved_from = track.number;
        } else if !moves.is_empty() {
            // several tracks move: the log keeps the old number of each track, in the new order
            let all = self.tracks(media)?;
            result.order = reordered_by(&all, &moves)
                .iter()
                .filter(|t| t.kind == self.kind())
                .map(|t| t.number)
                .collect();
        }
        if media.container == Container::Mkv && moves.is_empty() {
            return self.edit_mkv(media, result, &edits);
        }
        if !self.check_space(&mut result, &media.path, "rewrite the file") {
            return Ok(result);
        }
        let all = self.tracks(media)?;
        let expected = self.expected_layout(&all, &edits);
        result.data = layout_data(&all, &expected);
        let (temp_file, cmd, tool, ok): (String, Vec<String>, &str, &[i32]) =
            if media.container == Container::Mkv {
                let temp_file = format!("{}{TEMP_MARK}", media.path);
                let cmd = self.mkvmerge_command(media, &all, &edits, &temp_file);
                (temp_file, cmd, "mkvmerge", MKVMERGE_OK)
            } else {
                let temp_file = temp_path(&media.path);
                let cmd = self.ffmpeg_command(media, &all, &edits, &temp_file);
                (temp_file, cmd, "ffmpeg", &[0])
            };
        self.pend(&temp_file);
        let done = self.apply_tracked(&mut result, cmd, ok, &media.path, &temp_file, "Rewriting")?;
        if done && !self.config.dryrun {
            let rewritten = Media::new(&temp_file, media.container);
            self.verify_layout(&mut result, &rewritten, &expected, tool)?;
        }
        self.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    /// Change the default flags of an MP4 by changing the "enabled" bit in the track headers of a copy
    /// of the file, which `ffmpeg` can't do (it enables the first track of each kind again). Nothing
    /// is rewritten, so it is as quick as a copy; the copy is read back before it replaces the file.
    fn edit_mp4_flags(
        &self,
        media: &mut Media,
        mut result: Outcome,
        edits: Vec<Edit>,
        unchanged: &str,
    ) -> Res<Outcome> {
        if edits.is_empty() {
            result.status = Status::Unchanged;
            result.message = unchanged.to_string();
            return Ok(result);
        }
        result.changes.extend(edits.iter().map(Edit::describe));
        let all = self.tracks(media)?;
        let expected = self.expected_layout(&all, &edits);
        result.data = layout_data(&all, &expected);
        let temp_file = temp_path(&media.path);
        if !self.start_copy(&mut result, &media.path, &temp_file)? {
            return Ok(result);
        }
        self.note(&format!("Patching:\tthe enabled bit of {} track header(s) in {temp_file}", edits.len()));
        if !self.config.dryrun {
            let patched = mp4::read_track_flags(Path::new(&temp_file)).and_then(|flags| {
                let mut changes = Vec::new();
                for edit in &edits {
                    let Some(flag) = flags.iter().find(|f| f.kind == edit.track.kind && f.number == edit.track.number)
                    else {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "a track was not found in the file"));
                    };
                    changes.push((flag.offset, edit.default.unwrap_or(flag.enabled)));
                }
                mp4::set_enabled(Path::new(&temp_file), &changes)
            });
            match patched {
                Ok(()) => {
                    let rewritten = Media::new(&temp_file, media.container);
                    self.verify_layout(&mut result, &rewritten, &expected, "the flag change")?;
                }
                Err(error) => result.error(format!("could not change the track flags: {}", strerror(&error))),
            }
        }
        self.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    /// Change flags and languages of an MKV with mkvpropedit, on a copy.
    fn edit_mkv(&self, media: &mut Media, mut result: Outcome, edits: &[Edit]) -> Res<Outcome> {
        let selector = self.kind().selector();
        let mut options: Vec<String> = Vec::new();
        for edit in edits {
            options.extend(args(&["--edit", &format!("track:{selector}{}", edit.track.number)]));
            if let Some(default) = edit.default {
                options.extend(args(&["--set", &format!("flag-default={}", default as i32)]));
            }
            if let Some(language) = &edit.language {
                options.extend(args(&["--set", &format!("language={language}")]));
            }
            if let Some(name) = &edit.name {
                if name.is_empty() {
                    options.extend(args(&["--delete", "name"]));
                } else {
                    options.extend(args(&["--set", &format!("name={name}")]));
                }
            }
            if let Some(forced) = edit.forced {
                options.extend(args(&["--set", &format!("flag-forced={}", forced as i32)]));
            }
        }
        let command = |file: &str| -> Vec<String> {
            let mut cmd = args(&["mkvpropedit", file]);
            cmd.extend(options.iter().cloned());
            cmd
        };
        if self.config.in_place {
            if self.config.backup && !self.keep_original(&mut result, &media.path, false)? {
                return Ok(result);
            }
            self.apply(&mut result, command(&media.path), &[0])?;
            return Ok(result.finish());
        }
        let temp_file = format!("{}{TEMP_MARK}", media.path);
        if !self.start_copy(&mut result, &media.path, &temp_file)? {
            return Ok(result);
        }
        let all = self.tracks(media)?;
        let expected = self.expected_layout(&all, edits);
        result.data = layout_data(&all, &expected);
        if self.apply(&mut result, command(&temp_file), &[0])? && !self.config.dryrun {
            let rewritten = Media::new(&temp_file, media.container);
            self.verify_layout(&mut result, &rewritten, &expected, "mkvpropedit")?;
        }
        self.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    /// Rewrite an MKV with the tracks in a new order and new flags and languages.
    fn mkvmerge_command(
        &self,
        media: &Media,
        all: &[Track],
        edits: &[Edit],
        temp_file: &str,
    ) -> Vec<String> {
        let order = reordered_by(all, &moves_of(edits));
        let order_text =
            order.iter().map(|t| format!("0:{}", t.id)).collect::<Vec<_>>().join(",");
        let mut cmd = args(&["mkvmerge", "-o", temp_file, "--track-order", &order_text]);
        for edit in edits {
            let id = edit.track.id;
            if let Some(default) = edit.default {
                cmd.extend(args(&["--default-track-flag", &format!("{id}:{}", default as i32)]));
            }
            if let Some(language) = &edit.language {
                cmd.extend(args(&["--language", &format!("{id}:{language}")]));
            }
            if let Some(name) = &edit.name {
                cmd.extend(args(&["--track-name", &format!("{id}:{name}")]));
            }
            if let Some(forced) = edit.forced {
                cmd.extend(args(&["--forced-display-flag", &format!("{id}:{}", forced as i32)]));
            }
        }
        cmd.push(media.path.clone());
        cmd
    }

    /// Rewrite a file with ffmpeg, copying the streams, with new flags and languages and,
    /// if a track is being moved, the streams of its kind in the new order.
    fn ffmpeg_command(
        &self,
        media: &Media,
        all: &[Track],
        edits: &[Edit],
        temp_file: &str,
    ) -> Vec<String> {
        let selector = self.kind().selector();
        let tracks: Vec<Track> = all.iter().filter(|t| t.kind == self.kind()).cloned().collect();
        let mut order = tracks.clone();
        let mut maps = args(&["-map", "0"]);
        let moves = moves_of(edits);
        if !moves.is_empty() {
            order = reordered_by(all, &moves)
                .into_iter()
                .filter(|t| t.kind == self.kind())
                .collect();
            let group = |kind: Kind| -> Vec<String> {
                if kind == self.kind() {
                    order
                        .iter()
                        .flat_map(|t| args(&["-map", &format!("0:{selector}:{}", t.number - 1)]))
                        .collect()
                } else {
                    args(&["-map", &format!("0:{}?", kind.selector())])
                }
            };
            maps = [Kind::Video, Kind::Audio, Kind::Subtitles]
                .into_iter()
                .flat_map(group)
                .chain(args(&["-map", "0:d?", "-map", "0:t?"]))
                .collect();
        }
        let renamed: HashMap<(Kind, usize), String> = edits
            .iter()
            .filter_map(|e| e.name.as_ref().map(|n| (e.track.key(), n.clone())))
            .collect();
        let mut cmd = vec!["ffmpeg".to_string()];
        cmd.extend(args(crate::model::FFMPEG_QUIET));
        cmd.extend(args(&["-i", &media.path]));
        cmd.extend(maps);
        cmd.extend(args(&["-c", "copy"]));
        cmd.extend(name_args(&kept_tracks(all, Some((self.kind(), order.clone()))), &renamed));
        let position: HashMap<usize, usize> =
            order.iter().enumerate().map(|(i, t)| (t.number, i)).collect();
        for edit in edits {
            let index = position[&edit.track.number];
            if let Some(default) = edit.default {
                cmd.push(format!("-disposition:{selector}:{index}"));
                cmd.push(if default { "+default" } else { "-default" }.to_string());
            }
            if let Some(language) = &edit.language {
                cmd.push(format!("-metadata:s:{selector}:{index}"));
                cmd.push(format!("language={language}"));
            }
        }
        cmd.extend(Self::ffmpeg_output(media, temp_file));
        cmd
    }

    /// The output options of an ffmpeg rewrite.
    fn ffmpeg_output(media: &Media, temp_file: &str) -> Vec<String> {
        if media.container == Container::Mp4 {
            args(&["-movflags", "+faststart", temp_file])
        } else {
            args(&[temp_file])
        }
    }

    /// Rewrite a file without the given tracks.
    fn remove_tracks(
        &self,
        media: &mut Media,
        mut result: Outcome,
        remove: Vec<Track>,
    ) -> Res<Outcome> {
        let tracks = self.selected(media)?;
        if remove.is_empty() {
            return Ok(skip(result, &format!("has no matching {} tracks to remove", self.word())));
        }
        if self.kind() == Kind::Audio && remove.len() == tracks.len() && !self.config.force {
            return Ok(skip(result, "this would remove every audio track (use --force to allow)"));
        }
        if !self.check_space(&mut result, &media.path, "rewrite the file") {
            return Ok(result);
        }
        let removed: Vec<(Kind, usize)> = remove.iter().map(Track::key).collect();
        let survivors: Vec<Track> =
            tracks.iter().filter(|t| !removed.contains(&t.key())).cloned().collect();
        let promote = promotion(media.container, &survivors);
        for track in &remove {
            result.changes.push(format!("removed {} ({})", track.label(), track.language));
        }
        if let Some(promote) = &promote {
            result
                .changes
                .push(format!("{} ({}): default no -> yes", promote.label(), promote.language));
        }
        let all = self.tracks(media)?;
        let expected: Vec<Track> = all
            .iter()
            .filter(|t| !removed.contains(&t.key()))
            .map(|t| {
                let mut t = t.clone();
                if promote.as_ref().is_some_and(|p| p.key() == t.key()) {
                    t.default = true;
                }
                t
            })
            .collect();
        result.data = layout_data(&all, &expected);
        let (temp_file, cmd, tool, ok): (String, Vec<String>, &str, &[i32]) =
            if media.container == Container::Mkv {
                let ids = remove.iter().map(|t| t.id.to_string()).collect::<Vec<_>>().join(",");
                let temp_file = format!("{}{TEMP_MARK}", media.path);
                let select = if self.kind() == Kind::Audio { "--audio-tracks" } else { "--subtitle-tracks" };
                let mut cmd = args(&["mkvmerge", "-o", &temp_file, select, &format!("!{ids}")]);
                if let Some(promote) = &promote {
                    cmd.extend(args(&["--default-track-flag", &format!("{}:1", promote.id)]));
                }
                cmd.push(media.path.clone());
                (temp_file, cmd, "mkvmerge", MKVMERGE_OK)
            } else {
                let temp_file = temp_path(&media.path);
                let selector = self.kind().selector();
                let mut cmd = vec!["ffmpeg".to_string()];
                cmd.extend(args(crate::model::FFMPEG_QUIET));
                cmd.extend(args(&["-i", &media.path, "-map", "0"]));
                for track in &remove {
                    cmd.extend(args(&["-map", &format!("-0:{selector}:{}", track.number - 1)]));
                }
                cmd.extend(args(&["-c", "copy"]));
                cmd.extend(name_args(
                    &kept_tracks(&all, Some((self.kind(), survivors.clone()))),
                    &HashMap::new(),
                ));
                if promote.is_some() {
                    cmd.extend(args(&[&format!("-disposition:{selector}:0"), "+default"]));
                }
                cmd.extend(Self::ffmpeg_output(media, &temp_file));
                (temp_file, cmd, "ffmpeg", &[0])
            };
        self.pend(&temp_file);
        let done = self.apply_tracked(&mut result, cmd, ok, &media.path, &temp_file, "Rewriting")?;
        if done && !self.config.dryrun {
            let rewritten = Media::new(&temp_file, media.container);
            self.verify_layout(&mut result, &rewritten, &expected, tool)?;
        }
        self.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    // Operations ---------------------------------------------------------------

    fn do_info(&self, media: &mut Media) -> Res<Outcome> {
        let mut result = self.result(media, "info");
        let tracks = self.tracks(media)?;
        result.data = json!({"container": media.container.name(), "tracks": tracks});
        Ok(result)
    }

    fn do_get(&self, media: &mut Media) -> Res<Outcome> {
        let mut result = self.result(media, "get");
        let tracks = self.selected(media)?;
        if !self.config.lang.is_empty() {
            let matches: Vec<&Track> =
                tracks.iter().filter(|t| t.language == self.config.lang).collect();
            if matches.is_empty() {
                return Ok(skip(result, &self.missing_message(&[self.config.lang.clone()])));
            }
            result.data = json!({"tracks": matches});
        } else {
            let mut chosen: Vec<&Track> = tracks.iter().filter(|t| t.default).collect();
            if chosen.is_empty() && self.kind() == Kind::Audio {
                chosen = tracks.iter().take(1).collect(); // AVI has no default flag
            }
            let languages: Vec<&String> = chosen.iter().map(|t| &t.language).collect();
            let mut data = serde_json::Map::new();
            data.insert(format!("default_{}", self.kind().name()), json!(languages));
            result.data = Value::Object(data);
        }
        Ok(result)
    }

    fn do_set(&self, media: &mut Media, what: &str) -> Res<Outcome> {
        let mut result = self.result(media, "set");
        if media.container == Container::Avi {
            return Ok(skip(result, "set is only supported for MKV and MP4 files"));
        }
        let tracks = self.selected(media)?;
        if matches!(what, "name" | "forced" | "unforced") {
            return self.set_track_property(media, result, what);
        }
        if what == "track" {
            return self.set_track(media, result);
        }
        if what == "order" {
            let wanted = if self.config.order_lang.is_empty() {
                self.config.order.clone()
            } else {
                order_by_languages(&tracks, &self.config.order_lang)
            };
            let mut sorted = wanted.clone();
            sorted.sort_unstable();
            if sorted != (1..=tracks.len()).collect::<Vec<_>>() {
                let message = format!(
                    "--order must list each of the {} {} tracks once",
                    tracks.len(),
                    self.word()
                );
                return Ok(skip(result, &message));
            }
            let mut edits: Vec<Edit> = Vec::new();
            for (place, number) in wanted.iter().enumerate() {
                let track = &tracks[number - 1];
                let mut edit = Edit::new(track);
                if *number != place + 1 {
                    edit.move_to = Some(place + 1);
                }
                if self.config.default_first && track.default != (place == 0) {
                    edit.default = Some(place == 0);
                }
                if edit.move_to.is_some() || edit.default.is_some() {
                    edits.push(edit);
                }
            }
            let unchanged = format!(
                "{} tracks are already in that order{}",
                self.word(),
                if self.config.default_first { " and the first is the default" } else { "" }
            );
            return self.edit_tracks(media, result, edits, &unchanged);
        }
        if what == "default" {
            let Some(track) = self.resolve_track(media, &mut result)? else { return Ok(result) };
            let lang = self.config.lang.clone();
            let unchanged = format!("{} is already the default", track.label())
                + &if lang.is_empty() { String::new() } else { format!(" and {lang}") }
                + if self.config.move_first { " and the first track" } else { "" };
            let edits = default_edits(&tracks, Some(&track), &lang, self.config.move_first);
            return self.edit_tracks(media, result, edits, &unchanged);
        }
        if !self.config.default.is_empty() || !self.config.prefer.is_empty() {
            return self.set_default_language(media, result, &tracks);
        }
        let Some(track) = self.resolve_track(media, &mut result)? else { return Ok(result) };
        let mut edits = Vec::new();
        if track.language != self.config.lang {
            let mut edit = Edit::new(&track);
            edit.language = Some(self.config.lang.clone());
            edits.push(edit);
        }
        let unchanged = format!("{} is already {}", track.label(), track.language);
        self.edit_tracks(media, result, edits, &unchanged)
    }

    /// Set the name or the forced flag of the track chosen with --track.
    fn set_track_property(&self, media: &mut Media, mut result: Outcome, what: &str) -> Res<Outcome> {
        if what != "name" && media.container != Container::Mkv {
            return Ok(skip(
                result,
                "the forced flag can only be set for MKV files (mediainfo cannot read it from \
                 an MP4 to check the change)",
            ));
        }
        let Some(track) = self.resolve_track(media, &mut result)? else { return Ok(result) };
        let mut edit = Edit::new(&track);
        let (changed, unchanged);
        if what == "name" {
            let wanted = self.config.name.clone().unwrap_or_default();
            changed = !same_name(&track.name, &wanted);
            unchanged = if wanted.is_empty() {
                format!("{} has no name", track.label())
            } else {
                format!("{} is already named \"{wanted}\"", track.label())
            };
            edit.name = Some(wanted);
        } else {
            let forced = what == "forced";
            changed = track.forced != forced;
            unchanged = format!(
                "{} is already {}",
                track.label(),
                if forced { "forced" } else { "not forced" }
            );
            edit.forced = Some(forced);
        }
        let edits = if changed { vec![edit] } else { Vec::new() };
        self.edit_tracks(media, result, edits, &unchanged)
    }

    /// Change the language, name and forced flag of one track (`--track`) together, in one
    /// edit of the file; only what is given (`--lang`, `--name`, `--forced-flag`) is touched.
    fn set_track(&self, media: &mut Media, result: Outcome) -> Res<Outcome> {
        if self.config.forced_flag.is_some() && media.container != Container::Mkv {
            return Ok(skip(
                result,
                "the forced flag can only be set for MKV files (mediainfo cannot read it from \
                 an MP4 to check the change)",
            ));
        }
        let mut result = result;
        let Some(track) = self.resolve_track(media, &mut result)? else { return Ok(result) };
        let mut edit = Edit::new(&track);
        if !self.config.lang.is_empty() && track.language != self.config.lang {
            edit.language = Some(self.config.lang.clone());
        }
        if let Some(name) = &self.config.name {
            if !same_name(&track.name, name) {
                edit.name = Some(name.clone());
            }
        }
        if let Some(forced) = self.config.forced_flag {
            if track.forced != forced {
                edit.forced = Some(forced);
            }
        }
        let changed = edit.language.is_some() || edit.name.is_some() || edit.forced.is_some();
        let unchanged = format!("{} is already as asked", track.label());
        self.edit_tracks(media, result, if changed { vec![edit] } else { Vec::new() }, &unchanged)
    }

    /// Make the first track in the (preferred) language the only default track.
    fn set_default_language(&self, media: &mut Media, result: Outcome, tracks: &[Track]) -> Res<Outcome> {
        if self.config.default == "none" {
            let unchanged = format!("there is already no default {} track", self.word());
            let edits = default_edits(tracks, None, "", false);
            if media.container == Container::Mp4 {
                // ffmpeg would enable the first track again, so the flags are changed in the file
                return self.edit_mp4_flags(media, result, edits, &unchanged);
            }
            return self.edit_tracks(media, result, edits, &unchanged);
        }
        let languages = self.languages(&self.config.default);
        let Some((lang, track)) = self.choose(tracks, &languages) else {
            return Ok(skip(result, &self.missing_message(&languages)));
        };
        let unchanged = format!("{} ({lang}) is already the default", track.label())
            + if self.config.move_first { " and the first track" } else { "" };
        let edits = default_edits(tracks, Some(&track), "", self.config.move_first);
        self.edit_tracks(media, result, edits, &unchanged)
    }

    fn do_swap(&self, media: &mut Media) -> Res<Outcome> {
        let mut result = self.result(media, "swap");
        let audio = self.selected(media)?;
        if audio.len() < 2 {
            return Ok(skip(result, "needs at least two audio tracks to swap"));
        }
        if media.container != Container::Avi {
            let unchanged =
                format!("{} ({}) is already the default", audio[1].label(), audio[1].language);
            let edits = default_edits(&audio, Some(&audio[1]), "", false);
            return self.edit_tracks(media, result, edits, &unchanged);
        }
        if !self.check_space(&mut result, &media.path, "rewrite the file") {
            return Ok(result);
        }
        let temp_file = temp_path(&media.path);
        self.pend(&temp_file);
        result.changes.push("audio tracks 1 and 2 swapped".to_string());
        let before = self.tracks(media)?;
        let mut expected = before.clone();
        let audio_at: Vec<usize> = expected
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == Kind::Audio)
            .map(|(i, _)| i)
            .take(2)
            .collect();
        expected.swap(audio_at[0], audio_at[1]);
        result.data = layout_data(&before, &expected);
        let mut cmd = vec!["ffmpeg".to_string()];
        cmd.extend(args(crate::model::FFMPEG_QUIET));
        cmd.extend(args(&[
            "-i", &media.path, "-map", "0:v?", "-map", "0:a:1", "-map", "0:a:0", "-map",
            "0:a:2?", "-map", "0:s?", "-disposition:a:0", "default", "-disposition:a:1", "none",
            "-c", "copy", &temp_file,
        ]));
        let done = self.apply_tracked(&mut result, cmd, &[0], &media.path, &temp_file, "Rewriting")?;
        if done && !self.config.dryrun {
            let rewritten = Media::new(&temp_file, media.container);
            self.verify_layout(&mut result, &rewritten, &expected, "ffmpeg")?;
        }
        self.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    fn do_delete(&self, media: &mut Media, what: &str) -> Res<Outcome> {
        let mut result = self.result(media, "delete");
        if what == "track" {
            let Some(track) = self.resolve_any_track(media, &mut result)? else { return Ok(result) };
            if track.kind == Kind::Video {
                return Ok(skip(result, "removing a video track is not supported (audio and subtitles only)"));
            }
            return self.with_kind(track.kind).remove_tracks(media, result, vec![track]);
        }
        if media.container == Container::Avi {
            return Ok(skip(
                result,
                "deleting by language is only supported for MKV and MP4 files",
            ));
        }
        let remove: Vec<Track> = self
            .selected(media)?
            .into_iter()
            .filter(|t| t.language == self.config.lang)
            .collect();
        self.remove_tracks(media, result, remove)
    }

    /// Add subtitle files to an MKV as new subtitle tracks, last among the subtitles, not default,
    /// with the language (and name) each was given. mkvmerge writes a copy with the files added,
    /// and the copy is read back before it replaces the original.
    fn do_add(&self, media: &mut Media, _what: &str) -> Res<Outcome> {
        let mut result = self.result(media, "add");
        if media.container == Container::Avi {
            return Ok(skip(result, "adding a subtitle is only supported for MKV and MP4 files"));
        }
        let specs: Vec<SubSpec> = if self.config.subtitles.is_empty() {
            vec![SubSpec {
                file: self.config.sub_file.clone(),
                lang: self.config.lang.clone(),
                name: self.config.name.clone().unwrap_or_default(),
                forced: false,
            }]
        } else {
            self.config.subtitles.clone()
        };
        let worker = self.with_kind(Kind::Subtitles);
        let all = worker.tracks(media)?;
        let mut number = all.iter().filter(|t| t.kind == Kind::Subtitles).count();
        if media.container == Container::Mp4 {
            if let Some(refusal) = Self::mp4_subtitle_refusal(&specs) {
                return Ok(skip(result, &refusal));
            }
        }
        if !worker.check_space(&mut result, &media.path, "rewrite the file") {
            return Ok(result);
        }
        if media.container == Container::Mp4 {
            return worker.add_to_mp4(media, result, &specs, &all);
        }
        let temp_file = format!("{}{TEMP_MARK}", media.path);
        let mut cmd = args(&["mkvmerge", "-o", &temp_file, &media.path]);
        let mut expected = all.clone();
        for spec in &specs {
            number += 1;
            let new = Track {
                id: expected.len() as i64,
                kind: Kind::Subtitles,
                number,
                codec: String::new(),
                language: spec.lang.clone(),
                default: false,
                forced: spec.forced,
                name: spec.name.clone(),
            };
            let file_name = Path::new(&spec.file).file_name().map(|n| n.to_string_lossy().into_owned());
            result.changes.push(format!(
                "added {} ({}) from {}",
                new.label(),
                new.language,
                file_name.unwrap_or_else(|| spec.file.clone())
            ));
            cmd.extend(args(&["--language", &format!("0:{}", new.language)]));
            if !new.name.is_empty() {
                cmd.extend(args(&["--track-name", &format!("0:{}", new.name)]));
            }
            cmd.extend(args(&[
                "--default-track-flag",
                "0:0",
                "--forced-display-flag",
                if spec.forced { "0:1" } else { "0:0" },
            ]));
            cmd.push(spec.file.clone());
            expected.push(new);
        }
        result.data = layout_data(&all, &expected);
        worker.pend(&temp_file);
        let done = worker.apply_tracked(&mut result, cmd, MKVMERGE_OK, &media.path, &temp_file, "Rewriting")?;
        if done && !self.config.dryrun {
            let rewritten = Media::new(&temp_file, media.container);
            worker.verify_layout(&mut result, &rewritten, &expected, "mkvmerge")?;
        }
        worker.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    /// Why a set of subtitle files can't be added to an MP4, if so: only text subtitles can be
    /// converted to `mov_text`, and the forced flag of an MP4 can't be read back to check it.
    fn mp4_subtitle_refusal(specs: &[SubSpec]) -> Option<String> {
        for spec in specs {
            let extension = Path::new(&spec.file)
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !MP4_SUBTITLE_FILES.contains(&extension.as_str()) {
                return Some(format!(
                    "a .{extension} subtitle can't be added to an MP4 (only {} can be converted)",
                    MP4_SUBTITLE_FILES.join(", ")
                ));
            }
            if spec.forced {
                return Some(
                    "the forced flag can only be set for MKV files (mediainfo cannot read it from \
                     an MP4 to check the change)"
                        .into(),
                );
            }
        }
        None
    }

    /// Add subtitle files to an MP4, converted to `mov_text`, with `ffmpeg`. The names of all
    /// the tracks are written back, as ffmpeg can't read them from an MP4, and the muxer makes the
    /// first subtitle track default if none is, which the check expects.
    fn add_to_mp4(&self, media: &mut Media, mut result: Outcome, specs: &[SubSpec], all: &[Track]) -> Res<Outcome> {
        let first_new = all.iter().filter(|t| t.kind == Kind::Subtitles).count();
        let mut subtitles: Vec<Track> = all.iter().filter(|t| t.kind == Kind::Subtitles).cloned().collect();
        let mut cmd = vec!["ffmpeg".to_string()];
        cmd.extend(args(crate::model::FFMPEG_QUIET));
        cmd.extend(args(&["-i", &media.path]));
        for spec in specs {
            cmd.extend(args(&["-i", &spec.file]));
        }
        cmd.extend(args(&["-map", "0"]));
        for index in 0..specs.len() {
            cmd.extend(args(&["-map", &format!("{}:0", index + 1)]));
        }
        cmd.extend(args(&["-c", "copy"]));
        for (index, spec) in specs.iter().enumerate() {
            let place = first_new + index;
            let new = Track {
                id: (all.len() + index) as i64,
                kind: Kind::Subtitles,
                number: place + 1,
                codec: String::new(),
                language: spec.lang.clone(),
                default: false,
                forced: false,
                name: spec.name.clone(),
            };
            let file_name = Path::new(&spec.file).file_name().map(|n| n.to_string_lossy().into_owned());
            result.changes.push(format!(
                "added {} ({}) from {}",
                new.label(),
                new.language,
                file_name.unwrap_or_else(|| spec.file.clone())
            ));
            cmd.extend(args(&[&format!("-c:s:{place}"), "mov_text"]));
            cmd.extend(args(&[&format!("-metadata:s:s:{place}"), &format!("language={}", spec.lang)]));
            subtitles.push(new);
        }
        // the muxer makes the first subtitle track default when none is
        if !subtitles.is_empty() && !subtitles.iter().any(|t| t.default) {
            subtitles[0].default = true;
        }
        cmd.extend(name_args(&kept_tracks(all, Some((Kind::Subtitles, subtitles.clone()))), &HashMap::new()));
        for index in 0..specs.len() {
            let place = first_new + index;
            if !subtitles[place].default {
                cmd.extend(args(&[&format!("-disposition:s:{place}"), "0"]));
            }
        }
        let temp_file = temp_path(&media.path);
        cmd.extend(Self::ffmpeg_output(media, &temp_file));
        let expected: Vec<Track> = all
            .iter()
            .filter(|t| t.kind != Kind::Subtitles)
            .cloned()
            .chain(subtitles)
            .collect();
        result.data = layout_data(all, &expected);
        self.pend(&temp_file);
        let done = self.apply_tracked(&mut result, cmd, &[0], &media.path, &temp_file, "Rewriting")?;
        if done && !self.config.dryrun {
            let rewritten = Media::new(&temp_file, media.container);
            self.verify_layout(&mut result, &rewritten, &expected, "ffmpeg")?;
        }
        self.replace_file(&mut result, &temp_file, &media.path)?;
        Ok(result.finish())
    }

    /// Write a subtitle track to a file of its own (`mkvextract` for an MKV, `ffmpeg` for an MP4,
    /// as `.srt`). The media file is not changed. The file is written under a temporary name and
    /// only given its own if the tool made something.
    fn do_extract(&self, media: &mut Media, _what: &str) -> Res<Outcome> {
        let mut result = self.result(media, "extract");
        if media.container == Container::Avi {
            return Ok(skip(result, "extracting a subtitle is only supported for MKV and MP4 files"));
        }
        let worker = self.with_kind(Kind::Subtitles);
        let Some(track) = worker.resolve_track(media, &mut result)? else { return Ok(result) };
        let Some(extension) = subtitle_extension(media.container, &track.codec) else {
            let codec = if track.codec.is_empty() { "this".to_string() } else { track.codec.clone() };
            return Ok(skip(result, &format!("a {codec} subtitle can't be extracted to a single file")));
        };
        if media.container == Container::Mkv && which("mkvextract").is_none() {
            result.error("mkvextract is not installed (it comes with mkvtoolnix)".to_string());
            return Ok(result);
        }
        let folder = Path::new(&media.path).parent().map(Path::to_path_buf).unwrap_or_default();
        let wanted = self.config.output.trim();
        let is_folder = wanted.ends_with(['/', '\\']) || Path::new(wanted).is_dir();
        let explicit = !wanted.is_empty() && !is_folder;
        let out: PathBuf = if explicit {
            PathBuf::from(wanted)
        } else {
            let stem = Path::new(&media.path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = format!(
                "{stem}.{}{}.{extension}",
                track.language,
                if track.forced { ".forced" } else { "" }
            );
            if wanted.is_empty() { folder.join(name) } else { Path::new(wanted).join(name) }
        };
        let out_text = out.to_string_lossy().into_owned();
        let parent = out.parent().filter(|p| !p.as_os_str().is_empty());
        if parent.is_some_and(|p| !p.is_dir()) {
            result.error(format!("the folder for {out_text} does not exist"));
            return Ok(result);
        }
        if out.exists() && !(explicit && self.config.force) {
            return Ok(skip(
                result,
                &format!("{out_text} already exists (name another file with --output, or use --force to replace it)"),
            ));
        }
        let temp_file = temp_path(&out_text);
        let cmd = if media.container == Container::Mkv {
            args(&["mkvextract", "tracks", &media.path, &format!("{}:{temp_file}", track.id)])
        } else {
            let mut cmd = vec!["ffmpeg".to_string()];
            cmd.extend(args(crate::model::FFMPEG_QUIET));
            cmd.extend(args(&["-i", &media.path, "-map", &format!("0:s:{}", track.number - 1)]));
            cmd.extend(args(&["-c:s", "srt", &temp_file]));
            cmd
        };
        result.changes.push(format!("extracted {} ({}) to {out_text}", track.label(), track.language));
        self.pend(&temp_file);
        let done = self.apply(&mut result, cmd, &[0, 1])?;
        if done && !self.config.dryrun {
            let made = fs::metadata(&temp_file).map(|m| m.len() > 0).unwrap_or(false);
            if !made {
                result.error(format!("{} wrote nothing", if media.container == Container::Mkv { "mkvextract" } else { "ffmpeg" }));
            } else if let Err(error) = fs::rename(&temp_file, &out) {
                result.error(format!("could not write {out_text}: {}", strerror(&error)));
            }
        }
        if result.status == Status::Error {
            let _ = fs::remove_file(&temp_file);
        }
        self.shared.pending.lock().unwrap().retain(|p| p != &temp_file);
        result.data = json!({"output": out_text});
        Ok(result)
    }

    fn do_preserve(&self, media: &mut Media) -> Res<Outcome> {
        let result = self.result(media, "preserve");
        if media.container == Container::Avi {
            return Ok(skip(
                result,
                "preserving by language is only supported for MKV and MP4 files",
            ));
        }
        let tracks = self.selected(media)?;
        let languages = self.languages(&self.config.lang);
        let keep = languages.iter().find(|lang| tracks.iter().any(|t| &t.language == *lang));
        let Some(keep) = keep else {
            if !self.config.force {
                let message = format!(
                    "{}, so nothing would be left (use --force to remove them all)",
                    self.missing_message(&languages)
                );
                return Ok(skip(result, &message));
            }
            return self.remove_tracks(media, result, tracks);
        };
        let remove = tracks.into_iter().filter(|t| &t.language != keep).collect();
        self.remove_tracks(media, result, remove)
    }

    // Dispatch -----------------------------------------------------------------

    /// Run a task on one file.
    pub fn process(&self, task: &Task, path: &str) -> Res<Option<Outcome>> {
        if !Path::new(path).is_file() {
            return Ok(Some(
                Outcome::new(path, &task.name, false).with_status(Status::Error, "does not exist"),
            ));
        }
        let Some(container) = detect_kind(path) else {
            if !self.config.file.is_empty() || !self.config.paths.is_empty() {
                return Ok(Some(
                    Outcome::new(path, &task.name, false)
                        .with_status(Status::Error, "is not a supported media file"),
                ));
            }
            self.note(&format!("Skipping {path}: not a supported media file"));
            return Ok(None);
        };
        let tools = if matches!(task.name.as_str(), "info" | "get" | "extract") {
            container.read_tools()
        } else {
            container.write_tools()
        };
        self.require(tools)?;
        let mut media = Media::new(path, container);
        let outcome = self.dispatch(task, &mut media);
        self.release_space(path);
        let outcome = outcome?;
        if !self.config.log.is_empty() && outcome.status == Status::Changed && !self.config.dryrun {
            self.write_log(&mut media, &outcome)?;
        }
        Ok(Some(outcome))
    }

    fn dispatch(&self, task: &Task, media: &mut Media) -> Res<Outcome> {
        match task.name.as_str() {
            "info" => self.do_info(media),
            "get" => self.do_get(media),
            "set" => self.do_set(media, &task.what),
            "add" => self.do_add(media, &task.what),
            "extract" => self.do_extract(media, &task.what),
            "swap" => self.do_swap(media),
            "delete" => self.do_delete(media, &task.what),
            _ => self.do_preserve(media),
        }
    }

    // The log and undoing ------------------------------------------------------

    /// Add a line to the log about a change: what the tracks were and are now.
    fn write_log(&self, media: &mut Media, result: &Outcome) -> Res<()> {
        let after = self.read_tracks(&Media::new(&media.path, media.container))?;
        let entry = json!({
            "time": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            "version": VERSION, "file": abspath(&media.path),
            "action": result.action, "kind": self.kind().name(), "changes": result.changes,
            "moved_from": if result.moved_from > 0 { json!(result.moved_from) } else { Value::Null },
            "order": if result.order.is_empty() { Value::Null } else { json!(result.order) },
            "backup": if result.backup.is_empty() { Value::Null }
                      else { json!(abspath(&result.backup)) },
            "before": media.tracks.clone().unwrap_or_default(),
            "after": after,
        });
        let _guard = self.shared.log.lock().unwrap();
        let written = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.log)
            .and_then(|mut handle| {
                use std::io::Write;
                writeln!(handle, "{entry}")
            });
        if let Err(error) = written {
            self.warn(&format!("could not write the log {}: {}", self.config.log, strerror(&error)));
        }
        Ok(())
    }

    /// The entries of a log, oldest first.
    pub fn read_log(&self, path: &str) -> io::Result<Vec<Value>> {
        let text = fs::read_to_string(path)?;
        let mut entries = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(line) {
                Ok(entry) => entries.push(entry),
                Err(_) => self.warn(&format!(
                    "{path}: line {} is not valid JSON, so it was ignored",
                    index + 1
                )),
            }
        }
        Ok(entries)
    }

    /// Whether a file in the log is one --file or --dir (if given) asks about.
    fn in_scope(&self, file: &str) -> bool {
        if !self.config.file.is_empty() {
            return abspath(&self.config.file) == file;
        }
        if !self.config.dir.is_empty() {
            let base = abspath(&self.config.dir);
            let folder = Path::new(file).parent().unwrap_or(Path::new(""));
            return folder == Path::new(&base)
                || (self.config.recursive && folder.starts_with(&base));
        }
        true
    }

    /// The edits that put the tracks of the kind back as they were.
    fn restore_edits(
        &self,
        media: &Media,
        current: &[Track],
        before: &[Track],
        moved_from: Option<usize>,
        order: &[usize],
    ) -> Vec<Edit> {
        let current: Vec<Track> = current.iter().filter(|t| t.kind == self.kind()).cloned().collect();
        let wanted: Vec<&Track> = before.iter().filter(|t| t.kind == self.kind()).collect();
        // the places the tracks go back to (by their number now), and the tracks in the old order
        let mut back: HashMap<usize, usize> = HashMap::new();
        let mut restored = current.clone();
        if order.len() == current.len() && !order.is_empty() {
            // current[p] was track order[p]
            restored = (1..=current.len())
                .filter_map(|old| order.iter().position(|o| *o == old).map(|p| current[p].clone()))
                .collect();
            for (p, track) in current.iter().enumerate() {
                if order[p] != p + 1 {
                    back.insert(track.number, order[p]);
                }
            }
        } else if let (Some(from), Some(first)) = (moved_from, current.first()) {
            if from != 1 {
                restored.retain(|t| t.number != first.number);
                restored.insert((from - 1).min(restored.len()), first.clone());
                back.insert(first.number, from);
            }
        }
        let mut edits = Vec::new();
        for (now, old) in restored.iter().zip(wanted) {
            let mut edit = Edit::new(now);
            edit.default = (now.default != old.default).then_some(old.default);
            edit.language = (now.language != old.language).then(|| old.language.clone());
            edit.name = (now.name != old.name).then(|| old.name.clone());
            // the forced flag of an MP4 can't be read back to check, so it is not touched
            edit.forced = (now.forced != old.forced && media.container == Container::Mkv)
                .then_some(old.forced);
            edit.move_to = back.get(&now.number).copied();
            if edit.default.is_some() || edit.language.is_some() || edit.name.is_some()
                || edit.forced.is_some() || edit.move_to.is_some()
            {
                edits.push(edit);
            }
        }
        edits
    }

    /// Put the kept original back, once it is checked to be what was logged.
    fn restore_backup(&self, mut result: Outcome, media: &Media, backup: &str, before: &[Track]) -> Res<Outcome> {
        let kept = self.read_tracks(&Media::new(backup, media.container))?;
        if !same_state(&kept, before) {
            return Ok(skip(
                result,
                &format!("the backup {backup} is not what was logged, so it was not used"),
            ));
        }
        result.changes.push(format!("restored from {backup}"));
        if !self.config.dryrun {
            if let Err(error) = fs::rename(backup, &media.path) {
                result.error(format!("could not restore the backup: {}", strerror(&error)));
                return Ok(result);
            }
        }
        result.status = Status::Changed;
        Ok(result)
    }

    /// Undo the changes in a log, newest first.
    pub fn undo_log(&self, path: &str, report: &mut dyn FnMut(Outcome)) -> Res<()> {
        let entries = self
            .read_log(path)
            .map_err(|e| Stop::Fatal(format!("could not read the log {path}: {}", strerror(&e))))?;
        let mut planned: Vec<String> = Vec::new(); // in a dry run, files with only a planned undo
        for entry in entries.iter().rev() {
            let file = entry.get("file").and_then(Value::as_str).unwrap_or("").to_string();
            if !self.in_scope(&file) {
                continue;
            }
            if self.config.dryrun && planned.contains(&file) {
                report(Outcome::new(&file, "undo", true).with_status(
                    Status::Unchanged,
                    "an older change: it can be checked once the newer one above has really been \
                     undone",
                ));
                continue;
            }
            let result = self.undo_entry(entry)?;
            if self.config.dryrun && result.status == Status::Changed {
                planned.push(file);
            }
            report(result);
        }
        Ok(())
    }

    /// Reverse one change from the log, if the file is as that change left it.
    fn undo_entry(&self, entry: &Value) -> Res<Outcome> {
        let path = entry.get("file").and_then(Value::as_str).unwrap_or("").to_string();
        let result = Outcome::new(&path, "undo", self.config.dryrun);
        let parsed = (|| -> Option<(Vec<Track>, Vec<Track>, Kind)> {
            let before = serde_json::from_value(entry.get("before")?.clone()).ok()?;
            let after = serde_json::from_value(entry.get("after")?.clone()).ok()?;
            let kind = serde_json::from_value(entry.get("kind")?.clone()).ok()?;
            Some((before, after, kind))
        })();
        let Some((before, after, kind)) = parsed else {
            return Ok(skip(result, "this log entry is not understood"));
        };
        if !Path::new(&path).is_file() {
            return Ok(skip(result, "no longer exists"));
        }
        let Some(container) = detect_kind(&path) else {
            return Ok(skip(result, "is not a supported media file"));
        };
        self.require(container.write_tools())?;
        let mut media = Media::new(&path, container);
        let current = self.tracks(&mut media)?;
        if same_state(&current, &before) && !same_state(&current, &after) {
            return Ok(result.with_status(Status::Unchanged, "is already as it was before this change"));
        }
        if !same_state(&current, &after) {
            return Ok(skip(result, "has changed since it was logged, so it was left alone"));
        }
        if let Some(backup) = entry.get("backup").and_then(Value::as_str) {
            if Path::new(backup).is_file() {
                return self.restore_backup(result, &media, backup, &before);
            }
        }
        if before.len() != after.len() || container == Container::Avi {
            return Ok(skip(
                result,
                "can't be undone without the backup it would need (the change removed tracks or \
                 rewrote an AVI; use --backup)",
            ));
        }
        let worker = self.with_kind(kind);
        let moved_from = entry.get("moved_from").and_then(Value::as_u64).map(|n| n as usize);
        let order: Vec<usize> = entry
            .get("order")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).map(|n| n as usize).collect())
            .unwrap_or_default();
        let edits = worker.restore_edits(&media, &current, &before, moved_from, &order);
        worker.edit_tracks(&mut media, result, edits, "is already as it was")
    }

    // Running ------------------------------------------------------------------

    fn report(&self, results: &mut Vec<Outcome>, outcome: Outcome) {
        (self.sink)(Event::Result(Box::new(outcome.clone())));
        results.push(outcome);
    }

    /// Process several files at once, reporting them in the order they were found, so the
    /// output doesn't depend on which finished first.
    fn process_in_parallel(&self, task: &Task, paths: &[String], results: &mut Vec<Outcome>) -> Res<()> {
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let (send, receive) = mpsc::channel::<(usize, Res<Option<Outcome>>)>();
        let mut failure = None;
        std::thread::scope(|scope| {
            for _ in 0..self.config.jobs.min(paths.len()) {
                let send = send.clone();
                let (next, stop) = (&next, &stop);
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    if index >= paths.len() || stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let _ = send.send((index, self.process(task, &paths[index])));
                });
            }
            drop(send);
            let mut waiting: BTreeMap<usize, Res<Option<Outcome>>> = BTreeMap::new();
            let mut want = 0;
            for (index, outcome) in receive {
                waiting.insert(index, outcome);
                while let Some(outcome) = waiting.remove(&want) {
                    want += 1;
                    match outcome {
                        Ok(Some(outcome)) => self.report(results, outcome),
                        Ok(None) => {}
                        Err(stopped) => {
                            // a stop or a fatal error: start no more, then let it go on
                            stop.store(true, Ordering::SeqCst);
                            failure.get_or_insert(stopped);
                            waiting.clear();
                            break;
                        }
                    }
                }
            }
        });
        match failure {
            Some(stopped) => Err(stopped),
            None => Ok(()),
        }
    }

    fn run_tasks(&self, results: &mut Vec<Outcome>) -> Res<()> {
        for task in &self.config.tasks {
            if task.name == "check" {
                let outcome = self.check_tools();
                self.report(results, outcome);
                continue;
            }
            if task.name == "undo" {
                let mut collected = Vec::new();
                let done = self.undo_log(&task.what, &mut |outcome| {
                    (self.sink)(Event::Result(Box::new(outcome.clone())));
                    collected.push(outcome);
                });
                results.extend(collected);
                done?;
                continue;
            }
            // each action works with its own settings on top of the shared ones
            let worker = self.for_task(task);
            let paths = worker.files()?;
            if worker.config.jobs > 1 && paths.len() > 1 {
                worker.process_in_parallel(task, &paths, results)?;
                continue;
            }
            for path in &paths {
                if cancelled() {
                    return Err(Stop::Interrupted);
                }
                if let Some(outcome) = worker.process(task, path)? {
                    worker.report(results, outcome);
                }
            }
        }
        Ok(())
    }

    /// Run the tasks; the results and the exit status.
    pub fn run(&self) -> RunEnd {
        let mut results = Vec::new();
        let ended = self.run_tasks(&mut results);
        self.cleanup();
        let code = match ended {
            Err(Stop::Fatal(message)) => {
                self.warn(&message);
                1
            }
            Err(Stop::Interrupted) => {
                self.warn("interrupted");
                130
            }
            Ok(()) => i32::from(results.iter().any(|r| r.status == Status::Error)),
        };
        RunEnd { results, code }
    }
}

/// The text a front end prints for an `info` result.
pub fn info_text(outcome: &Outcome) -> String {
    let container = outcome.data["container"].as_str().unwrap_or("");
    let tracks: Vec<Track> =
        serde_json::from_value(outcome.data["tracks"].clone()).unwrap_or_default();
    let mut text = format!("{} ({container})\n", outcome.file);
    if tracks.is_empty() {
        text.push_str("no tracks found");
    } else {
        text.push_str(&format_tracks(&tracks));
    }
    text
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
