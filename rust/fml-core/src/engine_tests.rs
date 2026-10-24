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
    let cmd = engine(Kind::Audio).ffmpeg_command(&media, &all, &edits, "out.fml-tmp.mp4");
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
    let cmd = engine(Kind::Audio).mkvmerge_command(&media, &all, &edits, "out.fml-tmp");
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
    let edits = engine(Kind::Audio).restore_edits(&media, &now, &before, Some(3), &[]);
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
    let mp4 = engine.restore_edits(&Media::new("x.mp4", Container::Mp4), &now, &before, None, &[]);
    assert!(mp4.is_empty());
    let mkv = engine.restore_edits(&Media::new("x.mkv", Container::Mkv), &now, &before, None, &[]);
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

#[test]
fn names_are_the_same_whatever_form_the_accents_are_in() {
    assert!(same_name("Fran\u{e7}ais", "Franc\u{327}ais"));
    assert!(same_name(" Italiano ", "Italiano"));
    assert!(!same_name("Francais", "Franc\u{327}ais"));
}

fn mixed_media() -> Media {
    let mut media = Media::new("x.mkv", Container::Mkv);
    let mut tracks = vec![track(1, Kind::Video, "und", true, 0)];
    tracks.extend([track(1, Kind::Audio, "jpn", true, 1), track(2, Kind::Audio, "eng", false, 2)]);
    for (i, name) in ["", "Forced", "forced", "SDH"].iter().enumerate() {
        let mut t = track(i + 1, Kind::Subtitles, "eng", i == 0, 3 + i as i64);
        t.name = name.to_string();
        tracks.push(t);
    }
    media.tracks = Some(tracks);
    media
}

fn resolve(kind: Kind, spec: &str) -> Result<i64, String> {
    let config = Config { kind, track: spec.to_string(), ..Config::default() };
    let fml = Fml::new(config, Arc::new(|_| {}));
    let mut media = mixed_media();
    let mut result = Outcome::new("x.mkv", "delete", false);
    match fml.resolve_any_track(&mut media, &mut result).unwrap() {
        Some(t) => Ok(t.id),
        None => Err(result.message),
    }
}

#[test]
fn a_track_spec_is_a_number_a_kind_and_number_an_id_or_a_name() {
    assert_eq!(TrackSpec::parse("3"), Ok(TrackSpec::Number(3)));
    assert_eq!(TrackSpec::parse("last"), Ok(TrackSpec::Last));
    assert_eq!(TrackSpec::parse("subtitles:18"), Ok(TrackSpec::Of(Kind::Subtitles, Some(18))));
    assert_eq!(TrackSpec::parse("sub:last"), Ok(TrackSpec::Of(Kind::Subtitles, None)));
    assert_eq!(TrackSpec::parse("audio:2"), Ok(TrackSpec::Of(Kind::Audio, Some(2))));
    assert_eq!(TrackSpec::parse("id:0"), Ok(TrackSpec::Id(0)));
    assert_eq!(TrackSpec::parse("name:Latin America"), Ok(TrackSpec::Name("Latin America".into())));
    for bad in ["0", "-1", "x", "id:", "id:x", "name:", "audio:0", "audio:", "tv:1", ""] {
        assert!(TrackSpec::parse(bad).unwrap_err().contains("not a track number"), "{bad}");
    }
}

#[test]
fn a_track_is_found_by_place_id_or_name_whatever_the_kind_being_worked_on() {
    assert_eq!(resolve(Kind::Audio, "2"), Ok(2));
    assert_eq!(resolve(Kind::Audio, "last"), Ok(2));
    assert_eq!(resolve(Kind::Audio, "subtitles:4"), Ok(6));
    assert_eq!(resolve(Kind::Audio, "sub:last"), Ok(6));
    assert_eq!(resolve(Kind::Subtitles, "audio:1"), Ok(1));
    assert_eq!(resolve(Kind::Audio, "id:5"), Ok(5));
    assert_eq!(resolve(Kind::Audio, "name:sdh"), Ok(6));
}

#[test]
fn a_track_that_is_not_there_or_not_unique_is_reported() {
    assert_eq!(resolve(Kind::Audio, "20"), Err("has no audio track 20 (2 audio tracks)".into()));
    assert_eq!(resolve(Kind::Audio, "subtitles:9"), Err("has no subtitle track subtitles:9 (4 subtitle tracks)".into()));
    assert_eq!(resolve(Kind::Audio, "id:99"), Err("has no track with ID 99".into()));
    assert_eq!(resolve(Kind::Audio, "name:Nope"), Err("has no track named \"Nope\"".into()));
    let both = resolve(Kind::Audio, "name:forced").unwrap_err();
    assert!(both.contains("has 2 tracks named") && both.contains("4, 5"), "{both}");
}

#[test]
fn reordered_by_puts_several_tracks_at_their_places() {
    let mut all = vec![track(1, Kind::Video, "und", false, 0)];
    all.extend(four_audio());
    all.push(track(1, Kind::Subtitles, "eng", false, 9));
    let numbers = |moves: &[(Track, usize)]| -> Vec<usize> {
        reordered_by(&all, moves).iter().filter(|t| t.kind == Kind::Audio).map(|t| t.number).collect()
    };
    // swap the first two
    assert_eq!(numbers(&[(all[1].clone(), 2), (all[2].clone(), 1)]), [2, 1, 3, 4]);
    // a rotation: 4 first, the others after it in their old order
    assert_eq!(numbers(&[(all[4].clone(), 1), (all[1].clone(), 2), (all[2].clone(), 3), (all[3].clone(), 4)]), [4, 1, 2, 3]);
    // tracks that are not named fill the places left, in their old order
    assert_eq!(numbers(&[(all[3].clone(), 1), (all[1].clone(), 4)]), [3, 2, 4, 1]);
    // other kinds stay where they are
    let kinds: Vec<Kind> = reordered_by(&all, &[(all[2].clone(), 1)]).iter().map(|t| t.kind).collect();
    assert_eq!(kinds, all.iter().map(|t| t.kind).collect::<Vec<_>>());
    assert_eq!(reordered_by(&all, &[]), all);
}

#[test]
fn restore_edits_put_back_a_reorder() {
    // ita eng fre became fre ita eng: the log has the old numbers in the new order, 3 1 2
    let before = four_audio()[..3].to_vec();
    let now = vec![audio(1, "fre", false), audio(2, "ita", true), audio(3, "eng", false)];
    let media = Media::new("x.mkv", Container::Mkv);
    let edits = engine(Kind::Audio).restore_edits(&media, &now, &before, None, &[3, 1, 2]);
    let place = |number: usize| edits.iter().find(|e| e.track.number == number).and_then(|e| e.move_to);
    // fre (now 1) goes back to 3, ita (now 2) to 1, eng (now 3) to 2
    assert_eq!((place(1), place(2), place(3)), (Some(3), Some(1), Some(2)));
    let expected = engine(Kind::Audio).expected_layout(&now, &edits);
    assert_eq!(expected.iter().map(|t| t.language.as_str()).collect::<Vec<_>>(), ["ita", "eng", "fre"]);
}

#[test]
fn tracks_are_ordered_by_a_list_of_languages() {
    let tracks = four_audio(); // ita, eng, fre, eng
    let langs = |l: &[&str]| -> Vec<String> { l.iter().map(|s| s.to_string()).collect() };
    assert_eq!(order_by_languages(&tracks, &langs(&["eng", "ita"])), [2, 4, 1, 3]);
    assert_eq!(order_by_languages(&tracks, &langs(&["fre"])), [3, 1, 2, 4]);
    assert_eq!(order_by_languages(&tracks, &langs(&["ger"])), [1, 2, 3, 4]); // none present: unchanged
    assert_eq!(order_by_languages(&tracks, &[]), [1, 2, 3, 4]);
}

#[test]
fn an_order_is_numbers_from_one_none_twice() {
    assert_eq!(crate::parse_order("2,1,3"), Ok(vec![2, 1, 3]));
    assert_eq!(crate::parse_order(" 2 , 1 "), Ok(vec![2, 1]));
    for bad in ["", "0,1", "a", "1,,2", "-1,2"] {
        assert!(crate::parse_order(bad).unwrap_err().contains("not an order of tracks"), "{bad}");
    }
    assert!(crate::parse_order("1,2,1").unwrap_err().contains("twice"));
}

#[test]
fn a_renamed_file_is_followed_in_the_log() {
    let dir = std::env::temp_dir().join(format!("fml-renamelog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("log.jsonl").to_string_lossy().into_owned();
    let (a, b, c) = (dir.join("a.mkv"), dir.join("b.mkv"), dir.join("c.mkv"));
    let entry = |file: &std::path::Path, change: &str| {
        serde_json::json!({"file": file.to_string_lossy(), "changes": [change], "backup": null}).to_string()
    };
    let other = "not json, kept as it is";
    std::fs::write(&log, format!("{}\n{other}\n{}\n{}\n", entry(&a, "one"), entry(&b, "two"), entry(&a, "three"))).unwrap();
    let changed = rename_in_log(&log, &a.to_string_lossy(), &c.to_string_lossy()).unwrap();
    assert_eq!(changed, 2);
    let text = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], entry(&c, "one"));
    assert_eq!(lines[1], other); // a line that is not an entry is left alone
    assert_eq!(lines[2], entry(&b, "two")); // so is another file's entry
    assert_eq!(lines[3], entry(&c, "three"));
    assert!(text.ends_with('\n'));
    assert!(!std::path::Path::new(&format!("{log}{TEMP_MARK}")).exists());
    // nothing to change: the log is not rewritten; a log that is not there has no entries
    let before = std::fs::metadata(&log).unwrap().modified().unwrap();
    assert_eq!(rename_in_log(&log, &a.to_string_lossy(), &b.to_string_lossy()).unwrap(), 0);
    assert_eq!(std::fs::metadata(&log).unwrap().modified().unwrap(), before);
    assert_eq!(rename_in_log(&dir.join("none.jsonl").to_string_lossy(), "a", "b").unwrap(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn progress_follows_the_size_of_the_new_file() {
    use std::sync::Mutex;
    type Seen = Vec<(String, String, u64, u64)>;
    let dir = std::env::temp_dir().join(format!("fml-progress-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("src.bin");
    let temp = dir.join("new.bin");
    std::fs::write(&source, vec![0u8; 1000]).unwrap();
    let events: Arc<Mutex<Seen>> = Arc::default();
    let collected = Arc::clone(&events);
    let sink: Sink = Arc::new(move |event| {
        if let Event::Progress { file, stage, done, total } = event {
            collected.lock().unwrap().push((file, stage, done, total));
        }
    });
    let fml = Fml::new(Config { progress: true, ..Config::default() }, sink);
    let (s, t) = (source.to_string_lossy().into_owned(), temp.to_string_lossy().into_owned());
    let outcome = fml.track_progress(&s, &s, &t, "Rewriting", || {
        for step in 1..=4u64 {
            std::fs::write(&temp, vec![0u8; (step * 400) as usize]).unwrap(); // more than the source
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        "done"
    });
    assert_eq!(outcome, "done");
    let at_end = events.lock().unwrap().clone();
    assert!(at_end.len() >= 3, "{at_end:?}");
    assert!(at_end.iter().all(|(file, stage, _, total)| *file == s && stage == "Rewriting" && *total == 1000));
    let done: Vec<u64> = at_end.iter().map(|e| e.2).collect();
    assert!(done.windows(2).all(|w| w[0] <= w[1]), "{done:?}"); // never goes back
    assert_eq!(*done.last().unwrap(), 1000); // capped at the size of the source
    // the thread that looks at the file stopped with the work: nothing more arrives
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(events.lock().unwrap().len(), at_end.len());
    // not asked for, or a dry run: nothing is sent
    let quiet = Fml::new(Config::default(), Arc::new(|_| panic!("no progress was asked for")));
    assert_eq!(quiet.track_progress(&s, &s, &t, "Rewriting", || 7), 7);
    let dry = Config { progress: true, dryrun: true, ..Config::default() };
    let dry = Fml::new(dry, Arc::new(|_| panic!("a dry run has no progress")));
    assert_eq!(dry.track_progress(&s, &s, &t, "Rewriting", || 8), 8);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn install_uses_the_package_manager_of_the_system_once_per_package() {
    use std::sync::Mutex;
    let heard: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink_heard = Arc::clone(&heard);
    let sink: Sink = Arc::new(move |event| match event {
        Event::Note(text) | Event::Warn(text) => sink_heard.lock().unwrap().push(text),
        _ => {}
    });
    let config = Config { dryrun: true, verbose: true, install: true, ..Config::default() };
    let mut fml = Fml::new(config, sink);
    let missing: Vec<String> = ["mkvmerge", "mkvpropedit", "mediainfo", "ffmpeg"].map(String::from).to_vec();
    fml.installer = Some(Installer::Dnf);
    fml.install_missing(&missing);
    assert_eq!(*heard.lock().unwrap(), [
        "Executing:\tsudo dnf install -y mkvtoolnix",
        "Executing:\tsudo dnf install -y mediainfo",
        "Executing:\tsudo dnf install -y ffmpeg-free",
    ]); // mkvmerge and mkvpropedit are one package
    heard.lock().unwrap().clear();
    fml.installer = Some(Installer::Winget);
    fml.install_missing(&missing[2..3].to_vec());
    assert!(heard.lock().unwrap()[0].contains("winget install --id MediaArea.MediaInfo --exact"));
    heard.lock().unwrap().clear();
    fml.installer = None;
    fml.install_missing(&missing[..1].to_vec());
    assert!(heard.lock().unwrap()[0].starts_with("don't know how to install \"mkvtoolnix\" on this OS"));
}

// The free-space check, and the commands that are missing: a disk that is full and a PATH without
// the tools are put in place of the real ones, so none of it needs a full disk or an empty PATH.

use std::sync::Mutex;

type Heard = Arc<Mutex<Vec<String>>>;

fn listening(config: Config) -> (Fml, Heard) {
    let heard: Heard = Arc::default();
    let sink_heard = Arc::clone(&heard);
    let sink: Sink = Arc::new(move |event| match event {
        Event::Note(text) => sink_heard.lock().unwrap().push(text),
        Event::Warn(text) => sink_heard.lock().unwrap().push(format!("warning: {text}")),
        _ => {}
    });
    (Fml::new(config, sink), heard)
}

/// A disk with `free` bytes; a folder called `other` is a second disk.
fn disk_with(free: u64) -> DiskFn {
    Box::new(move |folder: &Path| {
        let device = if folder.ends_with("other") { 2 } else { 1 };
        Ok((free, device))
    })
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fml-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn file_of(dir: &Path, name: &str, size: usize) -> String {
    let path = dir.join(name);
    std::fs::write(&path, vec![0u8; size]).unwrap();
    path.to_string_lossy().into_owned()
}

fn set_disk(fml: &mut Fml, disk: DiskFn) {
    Arc::get_mut(&mut fml.shared).expect("not shared yet").disk = disk;
}

fn set_finder(fml: &mut Fml, present: &[&str]) {
    let present: Vec<String> = present.iter().map(|s| s.to_string()).collect();
    Arc::get_mut(&mut fml.shared).expect("not shared yet").finder = Box::new(move |tool: &str| {
        present.iter().any(|p| p == tool).then(|| std::path::PathBuf::from(format!("/usr/bin/{tool}")))
    });
}

#[test]
fn room_is_set_aside_for_each_file_until_it_is_done() {
    let dir = scratch("space");
    let (a, b) = (file_of(&dir, "a.bin", 1000), file_of(&dir, "b.bin", 1000));
    let (mut fml, _) = listening(Config::default());
    set_disk(&mut fml, disk_with(1500));
    let mut first = Outcome::new(&a, "set", false);
    assert!(fml.check_space(&mut first, &a, "rewrite the file"));
    // 1500 bytes are free and 1000 are set aside for the first file: the second does not fit
    let mut second = Outcome::new(&b, "set", false);
    second.changes.push("a change that is not going to happen".into());
    assert!(!fml.check_space(&mut second, &b, "rewrite the file"));
    assert_eq!(second.status, Status::Error);
    assert_eq!(second.message, "not enough free space to rewrite the file: needs 1000 bytes, 500 bytes free");
    assert!(second.changes.is_empty(), "a refused change is not reported as made");
    // when the first file is done its room is free again
    fml.release_space(&a);
    let mut again = Outcome::new(&b, "set", false);
    assert!(fml.check_space(&mut again, &b, "rewrite the file"));
    // a file's room is released once, and a file that set none aside releases nothing
    fml.release_space(&b);
    fml.release_space(&b);
    fml.release_space("never.bin");
    let mut once_more = Outcome::new(&a, "set", false);
    assert!(fml.check_space(&mut once_more, &a, "rewrite the file"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn another_disk_has_its_own_room() {
    let dir = scratch("disks");
    let other = dir.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let (a, b) = (file_of(&dir, "a.bin", 1000), file_of(&other, "b.bin", 1000));
    let (mut fml, _) = listening(Config::default());
    set_disk(&mut fml, disk_with(1200));
    let (mut one, mut two) = (Outcome::new(&a, "set", false), Outcome::new(&b, "set", false));
    assert!(fml.check_space(&mut one, &a, "copy the file"));
    assert!(fml.check_space(&mut two, &b, "copy the file"), "the second file is on another disk");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_disk_that_cannot_be_read_stops_the_file_with_a_reason() {
    let dir = scratch("nodisk");
    let a = file_of(&dir, "a.bin", 10);
    let (mut fml, _) = listening(Config::default());
    set_disk(&mut fml, Box::new(|_: &Path| Err(io::Error::new(io::ErrorKind::PermissionDenied, "no way"))));
    let mut result = Outcome::new(&a, "set", false);
    assert!(!fml.check_space(&mut result, &a, "copy the file"));
    assert_eq!(result.status, Status::Error);
    assert!(result.message.starts_with("could not check the free space: "), "{}", result.message);
    let mut missing = Outcome::new("gone.bin", "set", false);
    assert!(!fml.check_space(&mut missing, &dir.join("gone.bin").to_string_lossy(), "copy the file"));
    assert!(missing.message.starts_with("could not check the free space: "));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_copy_that_would_not_fit_is_not_started() {
    let dir = scratch("nocopy");
    let a = file_of(&dir, "a.bin", 4000);
    let temp = format!("{a}{TEMP_MARK}");
    let (mut fml, _) = listening(Config::default());
    set_disk(&mut fml, disk_with(3999));
    let mut result = Outcome::new(&a, "set", false);
    assert!(!fml.start_copy(&mut result, &a, &temp).unwrap());
    assert!(result.message.contains("not enough free space to copy the file (use --in-place to edit it directly)"));
    assert!(!Path::new(&temp).exists(), "nothing was copied");
    assert!(fml.shared.pending.lock().unwrap().is_empty(), "and no temporary file is waiting to be removed");
    // with enough room the copy is made, and is the same size
    set_disk(&mut fml, disk_with(4000));
    let mut fits = Outcome::new(&a, "set", false);
    assert!(fml.start_copy(&mut fits, &a, &temp).unwrap());
    assert_eq!(std::fs::metadata(&temp).unwrap().len(), 4000);
    fml.release_space(&a);
    let _ = std::fs::remove_file(&temp);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_says_which_commands_are_missing() {
    let (mut fml, _) = listening(Config::default());
    set_finder(&mut fml, &["mkvmerge", "ffmpeg"]);
    let result = fml.check_tools();
    assert_eq!(result.status, Status::Error);
    assert_eq!(result.message, "missing: mkvpropedit, mediainfo (use --install to install them)");
    let listed: Vec<(String, bool)> = result.data.as_array().unwrap().iter()
        .map(|e| (e["tool"].as_str().unwrap().to_string(), e["path"].is_string())).collect();
    assert_eq!(listed, [("mkvmerge".into(), true), ("mkvpropedit".into(), false), ("mediainfo".into(), false), ("ffmpeg".into(), true)]);
    assert_eq!(result.data[1]["package"], "mkvtoolnix");
    // everything there: nothing to say
    set_finder(&mut fml, &["mkvmerge", "mkvpropedit", "mediainfo", "ffmpeg"]);
    assert_ne!(fml.check_tools().status, Status::Error);
}

#[test]
fn check_with_install_shows_what_it_would_run_and_is_not_an_error_in_a_dry_run() {
    let config = Config { install: true, dryrun: true, verbose: true, ..Config::default() };
    let (mut fml, heard) = listening(config);
    set_finder(&mut fml, &["mkvmerge", "ffmpeg"]);
    fml.installer = Some(Installer::Apt);
    let result = fml.check_tools();
    assert_ne!(result.status, Status::Error, "{}", result.message);
    assert_eq!(*heard.lock().unwrap(), ["Executing:\tsudo apt install -y mkvtoolnix", "Executing:\tsudo apt install -y mediainfo"]);
}

#[test]
fn check_with_install_on_a_system_it_does_not_know_still_reports_the_commands_missing() {
    let config = Config { install: true, ..Config::default() };
    let (mut fml, heard) = listening(config);
    set_finder(&mut fml, &["mkvmerge", "mkvpropedit", "ffmpeg"]);
    fml.installer = None;
    let result = fml.check_tools();
    assert_eq!(result.status, Status::Error);
    assert_eq!(result.message, "missing: mediainfo"); // no "(use --install ...)": it was asked for
    assert!(heard.lock().unwrap()[0].starts_with("warning: don't know how to install \"mediainfo\" on this OS"));
}

#[test]
fn an_action_names_the_commands_it_needs_and_stops_before_touching_anything() {
    let (mut fml, _) = listening(Config::default());
    set_finder(&mut fml, &["ffmpeg"]);
    let Err(Stop::Fatal(message)) = fml.require(&["mkvmerge", "mkvpropedit"]) else { panic!("should stop") };
    assert_eq!(message, "command not found: mkvmerge, mkvpropedit (use --install to install, or --check to see what is missing)");
    assert!(fml.require(&["ffmpeg"]).is_ok());
    // only what the action uses is asked for
    assert!(fml.require(&[]).is_ok());
    let config = Config { install: true, dryrun: true, ..Config::default() };
    let (mut dry, _) = listening(config);
    set_finder(&mut dry, &[]);
    dry.installer = Some(Installer::Brew);
    assert!(dry.require(&["mkvmerge"]).is_ok(), "a dry run with --install goes on, the install is only shown");
}
