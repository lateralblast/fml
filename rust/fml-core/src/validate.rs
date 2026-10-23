//! The combinations of settings that make no sense, shared by every front end.

use std::path::Path;

use crate::model::{Config, Task};
use crate::track::Kind;

/// Check settings, and say what is wrong with the first problem found.
pub fn validate(args: &Config) -> Result<(), String> {
    let tasks: &[Task] = &args.tasks;
    if tasks.is_empty() {
        return Err("no action given (use --check, --info, --get, --set, --swap, --delete or \
                    --preserve)"
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
    if args.forced && args.kind != Kind::Subtitles {
        return Err("--forced only applies to subtitles (use --kind sub)".into());
    }
    if args.name.is_some() && !tasks.iter().any(|t| t.name == "set" && t.what == "name") {
        return Err("--name only works with --set name".into());
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
