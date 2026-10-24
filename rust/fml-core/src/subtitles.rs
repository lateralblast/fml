//! Matching subtitle files to media files by name, and guessing each one's language.
//!
//! `Show.S01E03.ita.srt` belongs to `Show.S01E03.mkv`: the subtitle's name starts with the
//! media file's name, and what is left (`ita`) says the language. Failing that, the season
//! and episode (`S01E03`, `1x03`) in the two names are compared.

use std::path::Path;

use crate::languages::known_language;
use crate::model::SUBTITLE_FILES;

/// A subtitle file matched to a media file, with what its name says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubMatch {
    pub media: String,
    pub file: String,
    /// The ISO 639-2/B code the name holds, if it holds a language.
    pub lang: Option<String>,
    /// A name for the track made from tags in the file name (`Forced`, `SDH`).
    pub name: String,
    pub forced: bool,
}

/// The result of matching: each subtitle file that belongs to a media file, and the rest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Matched {
    pub matches: Vec<SubMatch>,
    pub unmatched: Vec<String>,
}

/// Whether a path is a subtitle file that can be added (by its extension).
pub fn is_subtitle_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| SUBTITLE_FILES.contains(&e.as_str()))
}

/// A file's name without its folder or its last extension.
fn stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name.to_string(),
    }
}

/// What is left of `text` after `prefix`, ignoring case, if it starts with it.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut chars = text.char_indices();
    for wanted in prefix.chars() {
        let (_, got) = chars.next()?;
        if !got.to_lowercase().eq(wanted.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or("", |(at, _)| &text[at..]))
}

/// The season and episode in a name (`S01E03`, `s1e3`, `1x03`) and where the tag ends.
fn episode_token(name: &str) -> Option<((u32, u32), usize)> {
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    let digits = |from: usize| -> (u32, usize) {
        let mut at = from;
        let mut value = 0u32;
        while at < chars.len() && chars[at].1.is_ascii_digit() && at - from < 4 {
            value = value * 10 + chars[at].1.to_digit(10).unwrap_or(0);
            at += 1;
        }
        (value, at)
    };
    for start in 0..chars.len() {
        if start > 0 && chars[start - 1].1.is_alphanumeric() {
            continue; // not the start of a word
        }
        let first = chars[start].1.to_ascii_lowercase();
        let (season_from, separator) = if first == 's' { (start + 1, 'e') } else { (start, 'x') };
        let (season, at) = digits(season_from);
        if at == season_from || at >= chars.len() || chars[at].1.to_ascii_lowercase() != separator {
            continue;
        }
        let (episode, end) = digits(at + 1);
        if end == at + 1 || (end < chars.len() && chars[end].1.is_alphanumeric()) {
            continue;
        }
        let end_byte = chars.get(end).map_or(name.len(), |(b, _)| *b);
        return Some(((season, episode), end_byte));
    }
    None
}

/// The language, forced flag and track name that the words after a media name suggest.
/// The last word that is a known language is the language (release names put it last).
fn guess(rest: &str) -> (Option<String>, String, bool) {
    let words: Vec<String> = rest
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    let lang = words.iter().rev().find_map(|w| known_language(w));
    let forced = words.iter().any(|w| w == "forced");
    let mut tags: Vec<&str> = Vec::new();
    if forced {
        tags.push("Forced");
    }
    for (word, tag) in [("sdh", "SDH"), ("hi", "SDH"), ("cc", "CC")] {
        if words.iter().any(|w| w == word) && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    (lang, tags.join(" "), forced)
}

/// Match subtitle files to media files by name. A subtitle belongs to the media file whose
/// name it starts with (the longest such name), else to the only media file with the same
/// season and episode. A media file can have several subtitles.
pub fn match_subtitles(media: &[String], subs: &[String]) -> Matched {
    let stems: Vec<String> = media.iter().map(|m| stem(m)).collect();
    let mut result = Matched::default();
    for file in subs {
        let base = stem(file);
        let mut best: Option<(usize, &str)> = None;
        for (index, name) in stems.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            if let Some(rest) = strip_prefix_ci(&base, name) {
                let boundary = rest.chars().next().map_or(true, |c| !c.is_alphanumeric());
                if boundary && best.map_or(true, |(i, _)| stems[i].chars().count() < name.chars().count()) {
                    best = Some((index, rest));
                }
            }
        }
        if best.is_none() {
            if let Some((token, end)) = episode_token(&base) {
                let same: Vec<usize> = stems
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| episode_token(s).is_some_and(|(t, _)| t == token))
                    .map(|(i, _)| i)
                    .collect();
                if let [only] = same.as_slice() {
                    best = Some((*only, &base[end..]));
                }
            }
        }
        match best {
            Some((index, rest)) => {
                let (lang, name, forced) = guess(rest);
                result.matches.push(SubMatch {
                    media: media[index].clone(),
                    file: file.clone(),
                    lang,
                    name,
                    forced,
                });
            }
            None => result.unmatched.push(file.clone()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_subtitle_belongs_to_the_media_file_whose_name_it_starts_with() {
        let media = list(&["/tv/Show.S01E01.mkv", "/tv/Show.S01E02.mkv", "/tv/Show.S01E02.Extended.mkv"]);
        let subs = list(&[
            "/subs/Show.S01E01.ita.srt",
            "/subs/show.s01e02.eng.forced.srt",
            "/subs/Show.S01E02.Extended.fre.srt",
            "/subs/Show.S01E10.ita.srt",
        ]);
        let found = match_subtitles(&media, &subs);
        let pairs: Vec<(&str, Option<&str>, bool)> = found
            .matches
            .iter()
            .map(|m| (m.media.rsplit('/').next().unwrap(), m.lang.as_deref(), m.forced))
            .collect();
        assert_eq!(pairs, [
            ("Show.S01E01.mkv", Some("ita"), false),
            ("Show.S01E02.mkv", Some("eng"), true),
            ("Show.S01E02.Extended.mkv", Some("fre"), false), // the longest name wins
        ]);
        assert_eq!(found.matches[1].name, "Forced");
        assert_eq!(found.unmatched, ["/subs/Show.S01E10.ita.srt"]);
    }

    #[test]
    fn the_season_and_episode_match_when_the_names_differ() {
        let media = list(&["Chainsmoker.Cat.S01E01.1080p.NF.WEB-DL.DUAL.mkv", "Chainsmoker.Cat.S01E02.1080p.NF.WEB-DL.DUAL.mkv"]);
        let subs = list(&["Chainsmoker Cat - 1x02 - Italian.srt", "Chainsmoker.Cat.S01E01.English.SDH.srt", "Other - S02E01.srt"]);
        let found = match_subtitles(&media, &subs);
        assert_eq!(found.matches.len(), 2);
        assert_eq!(found.matches[0].media, media[1]);
        assert_eq!(found.matches[0].lang.as_deref(), Some("ita"));
        assert_eq!(found.matches[1].media, media[0]);
        assert_eq!((found.matches[1].lang.as_deref(), found.matches[1].name.as_str()), (Some("eng"), "SDH"));
        assert_eq!(found.unmatched, ["Other - S02E01.srt"]);
    }

    #[test]
    fn an_episode_that_two_media_files_share_is_not_guessed() {
        let media = list(&["A.S01E01.mkv", "B.S01E01.mkv"]);
        let found = match_subtitles(&media, &list(&["C.S01E01.ita.srt"]));
        assert!(found.matches.is_empty());
        assert_eq!(found.unmatched.len(), 1);
    }

    #[test]
    fn a_name_that_is_only_the_start_of_a_word_does_not_match() {
        let found = match_subtitles(&list(&["Show1.mkv"]), &list(&["Show10.ita.srt", "Show1.srt"]));
        assert_eq!(found.matches.len(), 1);
        assert_eq!(found.matches[0].file, "Show1.srt");
        assert_eq!(found.matches[0].lang, None); // nothing in the name says the language
    }

    #[test]
    fn language_words_are_known_languages_only() {
        assert_eq!(guess(".the.web.dl").0, None);
        assert_eq!(guess(".en").0.as_deref(), Some("eng"));
        assert_eq!(guess(".Italian").0.as_deref(), Some("ita"));
        assert_eq!(guess(".ita.eng").0.as_deref(), Some("eng")); // the last one
        assert_eq!(guess("_forced_ita"), (Some("ita".into()), "Forced".into(), true));
    }

    #[test]
    fn only_subtitle_extensions_count() {
        assert!(is_subtitle_file("a/b.SRT") && is_subtitle_file("x.ass"));
        assert!(!is_subtitle_file("x.txt") && !is_subtitle_file("srt"));
    }
}
