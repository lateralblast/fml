//! Guessing the language of an audio track from its speech, with whisper.cpp's language
//! detection (`whisper-cli -dl`). Nothing here changes a media file: the clips are cut into a
//! temporary folder, which is removed when the `ClipDir` is dropped.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::util::which;

/// Seconds of speech cut from each place. whisper.cpp only listens to the first 30 seconds.
pub const CLIP_SECONDS: f64 = 30.0;

/// The whisper.cpp program fml would run, if it is there: `FML_WHISPER` when it names a file,
/// else `whisper-cli` or `whisper-cpp` on `PATH` (the same lookup as `--check`).
pub fn whisper_path() -> Option<PathBuf> {
    match std::env::var("FML_WHISPER") {
        Ok(path) if !path.trim().is_empty() => Some(PathBuf::from(path)).filter(|p| p.is_file()),
        _ => which("whisper-cli").or_else(|| which("whisper-cpp")),
    }
}

/// The model `--check --install` downloads when there is none: whisper.cpp's base model. Its size
/// and SHA-256 are the ones Hugging Face reports for the file.
pub const MODEL_FILE: &str = "ggml-base.bin";
pub const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
pub const MODEL_BYTES: u64 = 147_951_465;
pub const MODEL_SHA256: &str = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";

/// Where the model is: `FML_WHISPER_MODEL` if it is set, else the model in fml's data folder
/// (whether or not it is there yet, so that `--install` knows where to put it).
pub fn model_path() -> Option<PathBuf> {
    match std::env::var("FML_WHISPER_MODEL") {
        Ok(path) if !path.trim().is_empty() => Some(PathBuf::from(path)),
        _ => default_model_in(data_folder()),
    }
}

/// The folder fml keeps its own files in: the user's application data folder.
pub(crate) fn data_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".local").join("share")))
    }
}

/// The default model inside a data folder.
pub fn default_model_in(data: Option<PathBuf>) -> Option<PathBuf> {
    data.map(|folder| folder.join("fml").join(MODEL_FILE))
}

/// Whether a disk with `free` bytes has room for a model of `bytes`, with 10% to spare.
pub fn room_for(free: u64, bytes: u64) -> bool {
    free >= bytes + bytes / 10
}

/// The SHA-256 of a file, as lowercase hex.
pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The program that does the detection: `FML_WHISPER` if it is set, else `whisper-cli` or
/// `whisper-cpp` on `PATH`.
pub fn whisper_program() -> Option<String> {
    if let Ok(path) = std::env::var("FML_WHISPER") {
        if !path.trim().is_empty() {
            return Some(path);
        }
    }
    ["whisper-cli", "whisper-cpp"]
        .iter()
        .find(|tool| which(tool).is_some())
        .map(|tool| tool.to_string())
}

/// Where the clips start, in seconds: three places spread through the track, so that a quiet
/// intro or a song does not decide it, or the start of a track too short for that.
pub fn clip_starts(duration: Option<f64>) -> Vec<f64> {
    match duration {
        Some(seconds) if seconds > CLIP_SECONDS * 1.5 => [0.2, 0.5, 0.8]
            .iter()
            .map(|share| (seconds * share).min(seconds - CLIP_SECONDS))
            .collect(),
        _ => vec![0.0],
    }
}

/// The language code and its probability from whisper.cpp's report, which says
/// `auto-detected language: it (p = 0.977524)`.
pub fn parse_detected(text: &str) -> Option<(String, f64)> {
    let line = text.lines().find(|line| line.contains("auto-detected language:"))?;
    let rest = line.split("auto-detected language:").nth(1)?.trim();
    let code = rest.split_whitespace().next()?.to_lowercase();
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let probability = rest
        .split("p =")
        .nth(1)?
        .trim()
        .trim_end_matches(')')
        .trim()
        .parse::<f64>()
        .ok()?;
    Some((code, probability))
}

/// What the clips of one track say about its language.
#[derive(Debug, PartialEq)]
pub struct Pick {
    pub language: String,
    /// The average probability over all the clips (a clip that did not name it counts as 0).
    pub confidence: f64,
    /// How many clips named this language.
    pub agreed: usize,
    pub clips: usize,
}

/// The language with the highest total probability over the clips, or `None` with no clips.
pub fn pick_language(clips: &[(String, f64)]) -> Option<Pick> {
    if clips.is_empty() {
        return None;
    }
    let mut totals: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    for (language, probability) in clips {
        let entry = totals.entry(language.as_str()).or_insert((0.0, 0));
        entry.0 += probability;
        entry.1 += 1;
    }
    let (language, &(total, agreed)) = totals.iter().max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
    Some(Pick {
        language: language.to_string(),
        confidence: total / clips.len() as f64,
        agreed,
        clips: clips.len(),
    })
}

static FOLDERS: AtomicUsize = AtomicUsize::new(0);

/// A temporary folder for the clips, removed when it goes out of scope (also when a run stops).
pub struct ClipDir(pub PathBuf);

impl ClipDir {
    pub fn new() -> std::io::Result<ClipDir> {
        let number = FOLDERS.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("fml-detect-{}-{number}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(ClipDir(dir))
    }
}

impl Drop for ClipDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_report_gives_the_language_and_probability() {
        let text = "whisper_init: loading\nwhisper_full: auto-detected language: it (p = 0.977524)\n";
        assert_eq!(parse_detected(text), Some(("it".to_string(), 0.977524)));
        // the report may come on stderr, with other lines around it
        assert_eq!(
            parse_detected("auto-detected language: en (p = 0.5)"),
            Some(("en".to_string(), 0.5))
        );
    }

    #[test]
    fn text_without_a_report_gives_nothing() {
        assert_eq!(parse_detected(""), None);
        assert_eq!(parse_detected("error: model not found"), None);
        assert_eq!(parse_detected("auto-detected language: ?? (p = x)"), None);
    }

    #[test]
    fn long_tracks_are_sampled_in_three_places_and_short_ones_at_the_start() {
        assert_eq!(clip_starts(None), vec![0.0]);
        assert_eq!(clip_starts(Some(40.0)), vec![0.0]);
        let starts = clip_starts(Some(1000.0));
        assert_eq!(starts.len(), 3);
        assert!((starts[0] - 200.0).abs() < 1e-9 && (starts[2] - 800.0).abs() < 1e-9);
        // the last clip still fits in the track
        let end = clip_starts(Some(50.0));
        assert!(end.iter().all(|s| s + CLIP_SECONDS <= 50.0 + 1e-9));
    }

    #[test]
    fn the_language_with_the_most_probability_wins() {
        let clips = vec![
            ("it".to_string(), 0.9),
            ("en".to_string(), 0.6),
            ("it".to_string(), 0.8),
        ];
        let pick = pick_language(&clips).unwrap();
        assert_eq!(pick.language, "it");
        assert_eq!((pick.agreed, pick.clips), (2, 3));
        assert!((pick.confidence - 1.7 / 3.0).abs() < 1e-9);
        assert_eq!(pick_language(&[]), None);
    }

    #[test]
    fn a_language_named_by_one_loud_clip_loses_to_a_steady_one() {
        let clips = vec![
            ("de".to_string(), 0.99),
            ("fr".to_string(), 0.7),
            ("fr".to_string(), 0.7),
        ];
        assert_eq!(pick_language(&clips).unwrap().language, "fr");
    }

    #[test]
    fn the_clip_folder_is_removed_when_dropped() {
        let path = {
            let dir = ClipDir::new().unwrap();
            std::fs::write(dir.0.join("clip.wav"), b"x").unwrap();
            assert!(dir.0.is_dir());
            dir.0.clone()
        };
        assert!(!path.exists());
    }

    #[test]
    fn a_model_is_downloaded_only_with_a_tenth_to_spare() {
        assert!(room_for(MODEL_BYTES + MODEL_BYTES / 10, MODEL_BYTES));
        assert!(!room_for(MODEL_BYTES + MODEL_BYTES / 10 - 1, MODEL_BYTES));
        assert!(!room_for(0, MODEL_BYTES));
    }

    #[test]
    fn the_default_model_is_in_the_data_folder_of_fml() {
        let data = PathBuf::from("/home/someone/.local/share");
        assert_eq!(
            default_model_in(Some(data.clone())),
            Some(data.join("fml").join("ggml-base.bin"))
        );
        assert_eq!(default_model_in(None), None);
        assert_eq!(MODEL_SHA256.len(), 64);
    }

    #[test]
    fn the_checksum_is_the_sha256_of_the_file() {
        let path = std::env::temp_dir().join(format!("fml-sha-{}.txt", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let hash = file_sha256(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
