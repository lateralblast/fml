//! The combinations of settings that make no sense, shared by every front end.

use std::path::Path;

use crate::model::{Config, Task, SUBTITLE_FILES};
use crate::track::Kind;

/// The track numbers of an `--order` such as `2,1,3`: each from 1, none twice.
pub fn parse_order(text: &str) -> Result<Vec<usize>, String> {
    let bad = || format!("\"{text}\" is not an order of tracks (numbers from 1 separated by commas, such as 2,1,3)");
    let numbers = text
        .split(',')
        .map(|part| part.trim().parse::<usize>().ok().filter(|n| *n >= 1).ok_or_else(bad))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = numbers.clone();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != numbers.len() {
        return Err(format!("\"{text}\" names a track twice"));
    }
    Ok(numbers)
}

/// Check a subtitle file exists and has an extension that can be added.
fn check_subtitle_file(file: &str) -> Result<(), String> {
    if !Path::new(file).is_file() {
        return Err(format!("--sub-file: \"{file}\" is not a file"));
    }
    if !crate::subtitles::is_subtitle_file(file) {
        return Err(format!(
            "--sub-file: \"{file}\" is not a subtitle file I can add ({})",
            SUBTITLE_FILES.join(", ")
        ));
    }
    Ok(())
}

/// Check settings, and say what is wrong with the first problem found.
pub fn validate(args: &Config) -> Result<(), String> {
    let tasks: &[Task] = &args.tasks;
    if tasks.is_empty() {
        return Err("no action given (use --check, --info, --get, --set, --add, --extract, \
                    --swap, --delete or --preserve)"
            .into());
    }
    let has_target = !args.file.is_empty() || !args.dir.is_empty() || !args.paths.is_empty();
    if tasks.iter().any(|t| t.name != "check" && t.name != "undo") && !has_target {
        return Err("no file given (use --file or --dir)".into());
    }
    for task in tasks {
        if task.name == "undo" && !Path::new(&task.what).is_file() {
            return Err(format!("--undo: \"{}\" is not a file", task.what));
        }
    }
    if (!args.include.is_empty() || !args.exclude.is_empty()) && args.dir.is_empty() {
        return Err("--include and --exclude only work with --dir".into());
    }
    if !args.log.is_empty() && tasks.iter().any(|t| t.name == "undo") {
        return Err("--undo reads a log and can't be used with --log".into());
    }
    // each action is checked with its own settings on top of the shared ones
    for task in tasks {
        validate_task(&args.for_task(task))?;
    }
    Ok(())
}

/// The rules for one action's settings (`args` has just that one task).
fn validate_task(args: &Config) -> Result<(), String> {
    let tasks: &[Task] = &args.tasks;

    if args.forced && args.kind != Kind::Subtitles {
        return Err("--forced only applies to subtitles (use --kind sub)".into());
    }
    if args.name.is_some()
        && !tasks.iter().any(|t| {
            (t.name == "set" && matches!(t.what.as_str(), "name" | "track")) || t.name == "add"
        })
    {
        return Err("--name only works with --set name, --set track and --add subtitle".into());
    }
    if args.forced_flag.is_some() && !tasks.iter().any(|t| t.name == "set" && t.what == "track") {
        return Err("--forced-flag only works with --set track".into());
    }
    if !args.order_lang.is_empty() && !tasks.iter().any(|t| t.name == "set" && t.what == "order") {
        return Err("--order-lang only works with --set order".into());
    }
    if !args.order.is_empty() && !tasks.iter().any(|t| t.name == "set" && t.what == "order") {
        return Err("--order only works with --set order".into());
    }
    if !args.output.is_empty() && !tasks.iter().any(|t| t.name == "extract") {
        return Err("--output only works with --extract subtitle".into());
    }
    if (!args.sub_file.is_empty() || !args.subtitles.is_empty())
        && !tasks.iter().any(|t| t.name == "add" && t.what == "subtitle") {
        return Err("--sub-file only works with --add subtitle".into());
    }
    if args.default_first && !tasks.iter().any(|t| t.name == "set" && t.what == "order") {
        return Err("--default-first only works with --set order".into());
    }
    if args.move_first && args.in_place {
        return Err("--move rewrites the file, so it can't be used with --in-place".into());
    }
    let has_default = !args.default.is_empty();
    let has_prefer = !args.prefer.is_empty();
    if args.move_first
        && !tasks
            .iter()
            .any(|t| t.name == "set" && (t.what == "default" || has_default || has_prefer))
    {
        return Err("--move only works with --set lang --default (or --prefer), or with --set \
                    default --track"
            .into());
    }
    if args.move_first && args.default == "none" {
        return Err("--move can't be used with --default none".into());
    }
    for task in tasks {
        let (name, what) = (task.name.as_str(), task.what.as_str());
        if name == "set" && what == "lang" {
            let given: Vec<&str> = [
                ("--default", has_default),
                ("--prefer", has_prefer),
                ("--lang", !args.lang.is_empty()),
            ]
            .iter()
            .filter(|(_, present)| *present)
            .map(|(flag, _)| *flag)
            .collect();
            if given.len() != 1 {
                let extra = if given.is_empty() {
                    String::new()
                } else {
                    format!(", not {}", given.join(" and "))
                };
                return Err(format!(
                    "--set lang needs one of --default (a language), --prefer (languages in \
                     order) or --lang (to set the language of a track){extra}"
                ));
            }
        }
        if name == "set" && matches!(what, "name" | "forced" | "unforced") {
            if args.track.is_empty() {
                return Err(format!("--set {what} needs --track"));
            }
            if has_default || has_prefer || args.move_first || args.forced {
                return Err(format!(
                    "--set {what} takes --track (and --name for a name), not --default, \
                     --prefer, --move or --forced"
                ));
            }
        }
        if name == "extract" {
            if args.track.is_empty() {
                return Err("--extract subtitle needs --track (the subtitle track to write)".into());
            }
            if has_default || has_prefer || args.move_first || args.forced || !args.order.is_empty()
                || !args.lang.is_empty()
            {
                return Err("--extract subtitle takes --track and --output, not --lang, --default, \
                            --prefer, --move, --forced or --order"
                    .into());
            }
        }
        if name == "add" {
            if args.subtitles.is_empty() {
                if args.sub_file.is_empty() {
                    return Err("--add subtitle needs --sub-file".into());
                }
                check_subtitle_file(&args.sub_file)?;
                if args.lang.is_empty() {
                    return Err("--add subtitle needs --lang (the language of the new track)".into());
                }
            }
            for spec in &args.subtitles {
                check_subtitle_file(&spec.file)?;
                if spec.lang.is_empty() {
                    return Err(format!("the subtitle \"{}\" has no language", spec.file));
                }
            }
            if has_default || has_prefer || args.move_first || args.forced || !args.track.is_empty()
                || !args.order.is_empty()
            {
                return Err("--add subtitle takes --sub-file, --lang and --name, not --track, \
                            --default, --prefer, --move, --forced or --order"
                    .into());
            }
            if args.in_place {
                return Err("--add subtitle rewrites the file, so it can't be used with --in-place".into());
            }
        }
        if name == "set" && what == "order" {
            if args.order.is_empty() == args.order_lang.is_empty() {
                return Err("--set order needs one of --order (track numbers) or --order-lang \
                            (languages)"
                    .into());
            }
            if has_default || has_prefer || args.move_first || args.forced || !args.track.is_empty()
                || !args.lang.is_empty()
            {
                return Err("--set order takes --order (and --kind), not --track, --lang, \
                            --default, --prefer, --move or --forced"
                    .into());
            }
            if args.in_place {
                return Err("--set order rewrites the file, so it can't be used with --in-place".into());
            }
        }
        if name == "set" && what == "track" {
            if args.track.is_empty() {
                return Err("--set track needs --track".into());
            }
            if args.lang.is_empty() && args.name.is_none() && args.forced_flag.is_none() {
                return Err("--set track needs at least one of --lang, --name and --forced-flag".into());
            }
            if has_default || has_prefer || args.move_first || args.forced {
                return Err("--set track takes --track with --lang, --name and --forced-flag, not \
                            --default, --prefer, --move or --forced"
                    .into());
            }
        }
        if name == "set" && what == "name" && args.name.is_none() {
            return Err("--set name needs --name".into());
        }
        if name == "set" && what == "default" {
            if args.track.is_empty() {
                return Err("--set default needs --track".into());
            }
            if has_default || has_prefer {
                return Err("--set default takes --track (and --lang to set its language), not \
                            --default or --prefer"
                    .into());
            }
        }
        if args.default == "none"
            && !(name == "set" && what == "lang" && args.kind == Kind::Subtitles)
        {
            return Err("--default none only works with --set lang --kind sub".into());
        }
        if has_prefer && !(name == "preserve" || (name == "set" && what == "lang")) {
            return Err("--prefer only works with --set lang and --preserve".into());
        }
        if name == "swap" && args.kind != Kind::Audio {
            return Err("--swap only works on audio".into());
        }
        let needs_lang = name == "preserve" || (name == "delete" && what == "lang");
        if needs_lang && !(!args.lang.is_empty() || (name == "preserve" && has_prefer)) {
            return Err(format!(
                "--{name} {what} needs --lang{}",
                if name == "preserve" { " or --prefer" } else { "" }
            ));
        }
        if !args.track.is_empty() {
            crate::track::TrackSpec::parse(&args.track)?;
        }
        if name == "delete" && what == "track" && args.track.is_empty() {
            return Err("--delete track needs --track".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tasks: &[(&str, &str)]) -> Config {
        Config {
            file: "a.mkv".into(),
            tasks: tasks.iter().map(|(n, w)| Task::new(n, w)).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn a_plain_default_change_is_fine() {
        let mut c = config(&[("set", "lang")]);
        c.default = "eng".into();
        assert!(validate(&c).is_ok());
        c.move_first = true;
        assert!(validate(&c).is_ok());
        c.in_place = true;
        assert!(validate(&c).unwrap_err().contains("can't be used with --in-place"));
    }

    #[test]
    fn adding_a_subtitle_needs_a_subtitle_file_and_a_language() {
        let dir = std::env::temp_dir().join(format!("fml-validate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let srt = dir.join("a.SRT");
        let txt = dir.join("a.txt");
        std::fs::write(&srt, b"").unwrap();
        std::fs::write(&txt, b"").unwrap();
        let mut c = config(&[("add", "subtitle")]);
        assert!(validate(&c).unwrap_err().contains("needs --sub-file"));
        c.sub_file = dir.join("missing.srt").to_string_lossy().into_owned();
        assert!(validate(&c).unwrap_err().contains("is not a file"));
        c.sub_file = txt.to_string_lossy().into_owned();
        assert!(validate(&c).unwrap_err().contains("not a subtitle file"));
        c.sub_file = srt.to_string_lossy().into_owned();
        assert!(validate(&c).unwrap_err().contains("needs --lang"));
        c.lang = "ita".into();
        assert!(validate(&c).is_ok()); // the extension is not case sensitive
        c.name = Some("Italiano".into());
        assert!(validate(&c).is_ok());
        c.track = "1".into();
        assert!(validate(&c).unwrap_err().contains("not --track"));
        c.track.clear();
        c.in_place = true;
        assert!(validate(&c).unwrap_err().contains("--in-place"));
        assert!(validate(&{ let mut d = config(&[("info", "")]); d.sub_file = "x.srt".into(); d })
            .unwrap_err()
            .contains("only works with --add subtitle"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn several_subtitle_files_each_need_a_language() {
        let dir = std::env::temp_dir().join(format!("fml-validate-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.srt"), dir.join("b.ass"));
        std::fs::write(&a, b"").unwrap();
        std::fs::write(&b, b"").unwrap();
        let spec = |path: &std::path::Path, lang: &str| crate::model::SubSpec {
            file: path.to_string_lossy().into_owned(),
            lang: lang.into(),
            ..Default::default()
        };
        let mut c = config(&[("add", "subtitle")]);
        c.subtitles = vec![spec(&a, "ita"), spec(&b, "eng")];
        assert!(validate(&c).is_ok()); // no --sub-file or --lang needed
        c.subtitles[1].lang.clear();
        assert!(validate(&c).unwrap_err().contains("has no language"));
        c.subtitles[1] = spec(&dir.join("gone.srt"), "eng");
        assert!(validate(&c).unwrap_err().contains("is not a file"));
        let mut other = config(&[("info", "")]);
        other.subtitles = vec![spec(&a, "ita")];
        assert!(validate(&other).unwrap_err().contains("only works with --add subtitle"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn each_action_is_checked_with_its_own_settings() {
        let mut c = config(&[("set", "lang"), ("delete", "lang")]);
        c.tasks[0].settings.default = Some("eng".into());
        c.tasks[1].settings.lang = Some("ita".into());
        assert!(validate(&c).is_ok()); // shared, this would be --default and --lang together
        // shared settings still apply to every action, and an action's own are added on top
        let mut shared = config(&[("set", "lang"), ("delete", "lang")]);
        shared.lang = "ita".into();
        shared.tasks[0].settings.default = Some("eng".into());
        assert!(validate(&shared).unwrap_err().contains("not --default and --lang"));
        // an action's own settings are checked for that action
        let mut bad = config(&[("get", "lang"), ("delete", "lang")]);
        bad.tasks[0].settings.lang = Some("eng".into());
        assert!(validate(&bad).unwrap_err().contains("--delete lang needs --lang"));
    }

    #[test]
    fn an_actions_settings_go_on_top_of_the_shared_ones() {
        let mut c = config(&[("set", "lang"), ("delete", "lang")]);
        c.lang = "ita".into();
        c.kind = Kind::Subtitles;
        c.tasks[1].settings.lang = Some("fre".into());
        c.tasks[1].settings.kind = Some(Kind::Audio);
        c.tasks[1].settings.name = Some("x".into());
        let first = c.for_task(&c.tasks[0].clone());
        assert_eq!((first.lang.as_str(), first.kind, first.name.clone()), ("ita", Kind::Subtitles, None));
        let second = c.for_task(&c.tasks[1].clone());
        assert_eq!((second.lang.as_str(), second.kind, second.name.clone()), ("fre", Kind::Audio, Some("x".to_string())));
        assert_eq!(second.tasks.len(), 1); // an action's config holds only that action
    }

    #[test]
    fn missing_pieces_are_named() {
        assert!(validate(&config(&[])).unwrap_err().contains("no action given"));
        assert!(validate(&config(&[("set", "lang")])).unwrap_err().contains("--set lang needs one of"));
        assert!(validate(&config(&[("set", "name")])).unwrap_err().contains("needs --track"));
        assert!(validate(&config(&[("delete", "lang")])).unwrap_err().contains("needs --lang"));
        let mut c = config(&[("info", "")]);
        c.file.clear();
        assert!(validate(&c).unwrap_err().contains("no file given"));
        c.paths = vec!["x.mkv".into()];
        assert!(validate(&c).is_ok());
    }

    #[test]
    fn subtitle_only_settings_are_checked() {
        let mut c = config(&[("set", "lang")]);
        c.default = "none".into();
        assert!(validate(&c).unwrap_err().contains("--default none only works"));
        c.kind = Kind::Subtitles;
        assert!(validate(&c).is_ok());
        let mut swap = config(&[("swap", "lang")]);
        swap.kind = Kind::Subtitles;
        assert!(validate(&swap).unwrap_err().contains("--swap only works on audio"));
    }
}
