//! Languages: names and ISO 639 codes, and the ISO 639-2/B code `mkvmerge` uses.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::language_table::{LANGUAGES, LANGUAGE_ALIASES};

type Row = (&'static str, &'static str, &'static str, &'static str);

fn index() -> &'static HashMap<String, &'static str> {
    static INDEX: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        for &(code, terminology, alpha2, name) in LANGUAGES {
            for key in [code, terminology, alpha2, name] {
                if !key.is_empty() {
                    index.insert(key.to_lowercase(), code);
                }
            }
        }
        for &(alias, code) in LANGUAGE_ALIASES {
            index.insert(alias.to_string(), code);
        }
        index
    })
}

/// Every known language, for a language picker.
pub fn all() -> &'static [Row] {
    LANGUAGES
}

/// The ISO 639-2/B code of a language that is in the table (a name, or a two or three letter
/// code), unlike `resolve_language`, which lets any three letter code through.
pub fn known_language(text: &str) -> Option<String> {
    index().get(&text.trim().to_lowercase()).map(|code| (*code).to_string())
}

/// Convert a language name or ISO 639 code to the ISO 639-2/B code `mkvmerge` uses.
///
/// Names and two letter codes must be known. Any other three letter code is passed
/// through, as the table does not hold every ISO 639-2 code.
pub fn resolve_language(text: &str) -> Result<String, String> {
    let key = text.trim().to_lowercase();
    if let Some(code) = index().get(&key) {
        return Ok((*code).to_string());
    }
    if key.chars().count() == 3 && key.chars().all(char::is_alphabetic) {
        return Ok(key);
    }
    Err(format!(
        "unknown language \"{text}\" (use an ISO 639 name or code, e.g. English, en or eng)"
    ))
}

/// ISO 639-2/B code for a language read from a file (`und` if it has none).
pub fn normalise_language(value: Option<&str>) -> String {
    match value {
        Some(text) if !text.trim().is_empty() => {
            resolve_language(text).unwrap_or_else(|_| text.trim().to_lowercase())
        }
        _ => "und".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_codes_resolve_to_one_code() {
        for text in ["English", "en", "eng", " ENG "] {
            assert_eq!(resolve_language(text).unwrap(), "eng");
        }
        assert_eq!(resolve_language("fra").unwrap(), "fre");
        assert_eq!(resolve_language("deu").unwrap(), "ger");
        assert_eq!(resolve_language("Castilian").unwrap(), "spa");
    }

    #[test]
    fn unknown_three_letter_codes_pass_through() {
        assert_eq!(resolve_language("xyz").unwrap(), "xyz");
        assert!(resolve_language("Klingon").is_err());
        assert!(resolve_language("zz").is_err());
    }

    #[test]
    fn languages_read_from_files_are_normalised() {
        assert_eq!(normalise_language(None), "und");
        assert_eq!(normalise_language(Some("  ")), "und");
        assert_eq!(normalise_language(Some("en")), "eng");
        assert_eq!(normalise_language(Some("Weird!")), "weird!");
    }
}
