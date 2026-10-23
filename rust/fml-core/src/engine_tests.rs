//! Tests of the engine's pure parts: choosing tracks, building edits and the commands
//! that carry them out. (Real files and tools are covered by the pytest suite, run with
//! `FML_BIN` set to this build.)

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::*;

fn track(number: usize, kind: Kind, language: &str, default: bool, id: i64) -> Track {
    Track {
        id,
        kind,
        number,
        codec: "AAC".into(),
        language: language.into(),
        default,
        forced: false,
        name: String::new(),
    }
}

fn audio(number: usize, language: &str, default: bool) -> Track {
    track(number, Kind::Audio, language, default, number as i64)
}

fn four_audio() -> Vec<Track> {
    vec![audio(1, "ita", true), audio(2, "eng", false), audio(3, "fre", false), audio(4, "eng", false)]
}

fn engine(kind: Kind) -> Fml {
    let config = Config { kind, ..Config::default() };
    Fml::new(config, Arc::new(|_| {}))
}

fn describe(edits: &[Edit]) -> Vec<String> {
    edits.iter().map(Edit::describe).collect()
}

#[test]
fn default_edits_change_only_what_differs() {
    let tracks = four_audio();
    let edits = default_edits(&tracks, Some(&tracks[1]), "", false);
    assert_eq!(
        describe(&edits),
        ["audio track 1 (ita): default yes -> no", "audio track 2 (eng): default no -> yes"]
    );
    assert!(default_edits(&tracks, Some(&tracks[0]), "", false).is_empty());
}

#[test]
fn default_edits_with_no_chosen_track_clear_defaults() {
    let tracks = four_audio();
    let edits = default_edits(&tracks, None, "", false);
    assert_eq!(describe(&edits), ["audio track 1 (ita): default yes -> no"]);
}

#[test]
fn default_edits_set_a_language_only_when_it_differs() {
    let tracks = four_audio();
    let edits = default_edits(&tracks, Some(&tracks[2]), "ger", false);
    assert_eq!(edits.last().unwrap().language.as_deref(), Some("ger"));
    let same = default_edits(&tracks, Some(&tracks[2]), "fre", false);
    assert!(same.iter().all(|e| e.language.is_none()));
}

#[test]
fn default_edits_move_everything_but_the_first_track() {
    let tracks = four_audio();
    let moved = default_edits(&tracks, Some(&tracks[2]), "", true);
    assert_eq!(moved.iter().filter_map(|e| e.move_to).collect::<Vec<_>>(), [1]);
    assert_eq!(
        describe(&moved)[1],
        "audio track 3 (fre): default no -> yes, moved to the first audio track"
    );
    let first = default_edits(&tracks, Some(&tracks[0]), "", true);
    assert!(first.is_empty());
}

#[test]
fn reordered_moves_to_any_place() {
    let mut all = vec![track(1, Kind::Video, "und", false, 0)];
    all.extend(four_audio().into_iter().take(3));
    all.push(track(1, Kind::Subtitles, "eng", false, 9));
    let order = |moving: &Track, to: usize| -> Vec<usize> {
        reordered(&all, Some(moving), to)
            .iter()
            .filter(|t| t.kind == Kind::Audio)
            .map(|t| t.number)
            .collect()
    };
    assert_eq!(order(&all[3], 1), [3, 1, 2]);
    assert_eq!(order(&all[1], 2), [2, 1, 3]);
    assert_eq!(order(&all[1], 3), [2, 3, 1]);
    assert_eq!(order(&all[1], 9), [2, 3, 1]); // past the end: last
    let kinds: Vec<Kind> = reordered(&all, Some(&all[3]), 1).iter().map(|t| t.kind).collect();
    assert_eq!(kinds, [Kind::Video, Kind::Audio, Kind::Audio, Kind::Audio, Kind::Subtitles]);
    assert_eq!(reordered(&all, None, 1), all);
}

#[test]
fn expected_layout_applies_edits_and_moves() {
    let mut all = vec![track(1, Kind::Video, "und", false, 0)];
    all.extend(four_audio());
    all.push(track(1, Kind::Subtitles, "eng", false, 9));
    let app = engine(Kind::Audio);
    let edits = default_edits(&four_audio(), Some(&four_audio()[2]), "ger", true);
    let layout: Vec<(Kind, String, bool)> = app
        .expected_layout(&all, &edits)
        .into_iter()
        .map(|t| (t.kind, t.language, t.default))
        .collect();
    let want = |kind, language: &str, default| (kind, language.to_string(), default);
    assert_eq!(
        layout,
        [
            want(Kind::Video, "und", false),
            want(Kind::Audio, "ger", true),
            want(Kind::Audio, "ita", false),
            want(Kind::Audio, "eng", false),
            want(Kind::Audio, "eng", false),
            want(Kind::Subtitles, "eng", false),
        ]
    );
}

#[test]
fn temp_path_keeps_the_extension() {
    assert_eq!(temp_path("/a/b/film.mp4"), "/a/b/film.fml-tmp.mp4");
    assert_eq!(temp_path("film.avi"), "film.fml-tmp.avi");
}

#[test]
fn ffmpeg_command_for_a_move_maps_the_streams_in_the_new_order() {
    let mut all = vec![track(1, Kind::Video, "und", false, 0)];
    all.extend(four_audio().into_iter().take(3));
    all.push(track(1, Kind::Subtitles, "eng", false, 9));
    let media = Media::new("x.mp4", Container::Mp4);
    let audio3: Vec<Track> = four_audio().into_iter().take(3).collect();
    let edits = default_edits(&audio3, Some(&audio3[2]), "", true);
    let moving = edits.last().unwrap().track.clone();
    let cmd = engine(Kind::Audio).ffmpeg_command(&media, &all, &edits, Some(&moving), "out.fml-tmp.mp4");
    let maps: Vec<&String> = cmd.iter().enumerate().filter(|(_, a)| *a == "-map").map(|(i, _)| &cmd[i + 1]).collect();
    assert_eq!(maps, ["0:v?", "0:a:2", "0:a:0", "0:a:1", "0:s?", "0:d?", "0:t?"]);
    // the dispositions refer to the new positions: ita was 0 and is now 1, fre was 2 and is now 0
    let after = |flag: &str| cmd[cmd.iter().position(|a| a == flag).unwrap() + 1].clone();
    assert_eq!(after("-disposition:a:1"), "-default");
    assert_eq!(after("-disposition:a:0"), "+default");
    assert_eq!(cmd[cmd.len() - 3..], ["-movflags", "+faststart", "out.fml-tmp.mp4"]);
}

#[test]
fn mkvmerge_command_for_a_move() {
    let all = vec![
        track(1, Kind::Video, "und", false, 0),
        track(1, Kind::Audio, "ita", true, 1),
        track(2, Kind::Audio, "eng", false, 2),
        track(3, Kind::Audio, "fre", false, 3),
        track(1, Kind::Subtitles, "eng", false, 4),
    ];
    let media = Media::new("x.mkv", Container::Mkv);
    let edits = default_edits(&all[1..4], Some(&all[2]), "", true);
    let moving = edits.last().unwrap().track.clone();
    let cmd = engine(Kind::Audio).mkvmerge_command(&media, &all, &edits, Some(&moving), "out.fml-tmp");
    assert_eq!(cmd[..5], ["mkvmerge", "-o", "out.fml-tmp", "--track-order", "0:0,0:2,0:1,0:3,0:4"]);
    assert_eq!(cmd[5..], ["--default-track-flag", "1:0", "--default-track-flag", "2:1", "x.mkv"]);
}

#[test]
fn name_args_write_each_name_at_its_new_place() {
    let named = |number, kind, name: &str| Track { name: name.into(), ..track(number, kind, "und", false, 0) };
    let kept = vec![
        (Kind::Video, vec![named(1, Kind::Video, "Video")]),
        (Kind::Audio, vec![named(3, Kind::Audio, "Français"), named(1, Kind::Audio, "Italiano"), named(2, Kind::Audio, "")]),
        (Kind::Subtitles, vec![named(1, Kind::Subtitles, "Forced")]),
    ];
    assert_eq!(
        name_args(&kept, &HashMap::new()),
        [
            "-metadata:s:v:0", "title=Video", "-metadata:s:a:0", "title=Français",
            "-metadata:s:a:1", "title=Italiano", "-metadata:s:s:0", "title=Forced"
        ]
    );
}

#[test]
fn name_args_can_rename_and_remove() {
    let named = |number, name: &str| Track { name: name.into(), ..audio(number, "und", false) };
    let kept = vec![(Kind::Audio, vec![named(1, "One"), named(2, "Two")])];
    let renamed = HashMap::from([((Kind::Audio, 1), "Uno".to_string()), ((Kind::Audio, 2), String::new())]);
    assert_eq!(name_args(&kept, &renamed), ["-metadata:s:a:0", "title=Uno"]);
}

#[test]
fn promotion_follows_the_container() {
    let left = [audio(2, "eng", false), audio(3, "fre", false)];
    assert_eq!(promotion(Container::Mkv, &left), Some(left[0].clone()));
    assert_eq!(promotion(Container::Mp4, &left), Some(left[0].clone()));
    assert_eq!(promotion(Container::Avi, &left), None); // AVI has no flags
    assert_eq!(promotion(Container::Mkv, &[audio(2, "eng", true), audio(3, "fre", false)]), None);
    assert_eq!(promotion(Container::Mkv, &[]), None);
    let subs = [track(1, Kind::Subtitles, "eng", false, 5)];
    assert_eq!(promotion(Container::Mkv, &subs), None); // an MKV may have no default subtitles
    assert_eq!(promotion(Container::Mp4, &subs), Some(subs[0].clone())); // an MP4 insists
}

#[test]
fn pick_track_prefers_unforced_subtitles_and_honours_forced() {
    let forced = Track { forced: true, ..track(1, Kind::Subtitles, "eng", false, 5) };
    let plain = track(2, Kind::Subtitles, "eng", false, 6);
    let subs = vec![forced.clone(), plain.clone()];
    let app = engine(Kind::Subtitles);
    assert_eq!(app.pick_track(&subs, "eng"), Some(plain));
    assert_eq!(app.pick_track(&[forced.clone()], "eng"), Some(forced.clone()));
    let wants_forced = Fml::new(
        Config { kind: Kind::Subtitles, forced: true, ..Config::default() },
        Arc::new(|_| {}),
    );
    assert_eq!(wants_forced.pick_track(&subs, "eng"), Some(forced));
    assert_eq!(wants_forced.pick_track(&[subs[1].clone()], "eng"), None);
}

#[test]
fn choose_uses_the_first_language_that_is_present() {
    let tracks = four_audio();
    let app = engine(Kind::Audio);
    let languages = ["jpn", "fre", "eng"].map(String::from);
    let (lang, track) = app.choose(&tracks, &languages).unwrap();
    assert_eq!((lang.as_str(), track.number), ("fre", 3));
    assert!(app.choose(&tracks, &["jpn".to_string()]).is_none());
}

#[test]
fn patterns_match_names_and_paths() {
    let hit = |relative: &str, patterns: &[&str]| {
        let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        pattern_hit(Path::new(relative), &patterns)
    };
    assert!(hit("a.mkv", &["*.mkv"]));
    assert!(hit("a.MKV", &["*.mkv"]));
    assert!(!hit("a.mp4", &["*.mkv"]));
    assert!(hit("Season 1/a.mkv", &["Season 1"]));
    assert!(!hit("Season 1/a.mkv", &["season 2"]));
    assert!(hit("Season 1/Extras/x.mkv", &["extras"]));
    assert!(hit("Season 1/a.mkv", &["Season 1/*"]));
    assert!(!hit("Season 1/a.mkv", &["Season 2/*"]));
    assert!(hit("Season 1/a.mkv", &["*/a.mkv"]));
    assert!(!hit("a.mkv", &["*/a.mkv"]));
    assert!(!hit("a.mkv", &[]));
    assert!(!hit("Extras 2/a.mkv", &["Extras"])); // the whole name has to match
}

#[test]
fn same_state_compares_what_matters() {
    let a = four_audio();
    let mut b = a.clone();
    b[0].id = 99; // ids and codecs do not matter
    b[0].codec = "AC3".into();
    assert!(same_state(&a, &b));
    b[1].default = true;
    assert!(!same_state(&a, &b));
    assert!(!same_state(&a, &a[..3]));
}

#[test]
fn restore_edits_put_back_a_move_and_a_flag() {
    let before = four_audio()[..3].to_vec();
    // after moving track 3 first and making it default: fre, ita, eng
    let now = vec![
        Track { default: true, ..audio(1, "fre", true) },
        audio(2, "ita", false),
        audio(3, "eng", false),
    ];
    let media = Media::new("x.mkv", Container::Mkv);
    let edits = engine(Kind::Audio).restore_edits(&media, &now, &before, Some(3));
    let moved = edits.iter().find(|e| e.move_to.is_some()).unwrap();
    assert_eq!((moved.track.number, moved.move_to), (1, Some(3)));
    assert_eq!(moved.default, Some(false));
    let ita = edits.iter().find(|e| e.track.number == 2).unwrap();
    assert_eq!(ita.default, Some(true));
}

#[test]
fn restore_edits_leave_the_forced_flag_of_an_mp4_alone() {
    let before = vec![Track { forced: true, ..track(1, Kind::Subtitles, "eng", false, 1) }];
    let now = vec![track(1, Kind::Subtitles, "eng", false, 1)];
    let engine = engine(Kind::Subtitles);
    let mp4 = engine.restore_edits(&Media::new("x.mp4", Container::Mp4), &now, &before, None);
    assert!(mp4.is_empty());
    let mkv = engine.restore_edits(&Media::new("x.mkv", Container::Mkv), &now, &before, None);
    assert_eq!(mkv[0].forced, Some(true));
}

#[test]
fn backup_path_is_numbered_when_taken() {
    let dir = std::env::temp_dir().join(format!("fml-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.mkv").to_string_lossy().into_owned();
    assert_eq!(Fml::backup_path(&file), format!("{file}.fml-bak"));
    std::fs::write(format!("{file}.fml-bak"), b"x").unwrap();
    assert_eq!(Fml::backup_path(&file), format!("{file}.fml-bak.1"));
    std::fs::remove_dir_all(&dir).unwrap();
}
