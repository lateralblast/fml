//! Reading and changing the "enabled" flag of the tracks of an MP4 file, in place.
//!
//! In an MP4 the default track of each kind is the one whose track header (`tkhd`) has its
//! "enabled" bit set, and that is what `mediainfo` reports as `Default`. `ffmpeg` always
//! enables the first track of each kind when it writes a file, so it can't make a file with
//! no default subtitles; changing the bit in a copy of the file can, without touching any media.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::track::Kind;

/// Where a track's header flags are in the file: the kind it is, its place among the tracks of
/// that kind (from 1) and the offset of the last of the three flag bytes, whose lowest bit is
/// "enabled".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackFlag {
    pub kind: Kind,
    pub number: usize,
    pub offset: u64,
    pub enabled: bool,
}

/// The boxes in `data[start..end]`: type, where it starts, where its content starts, where it ends.
fn boxes(data: &[u8], start: usize, end: usize) -> Vec<([u8; 4], usize, usize, usize)> {
    let mut found = Vec::new();
    let mut at = start;
    while at + 8 <= end {
        let size = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as u64;
        let kind = [data[at + 4], data[at + 5], data[at + 6], data[at + 7]];
        let (header, size) = match size {
            0 => (8, (end - at) as u64),
            1 if at + 16 <= end => {
                let mut big = [0u8; 8];
                big.copy_from_slice(&data[at + 8..at + 16]);
                (16, u64::from_be_bytes(big))
            }
            size => (8, size),
        };
        if size < header as u64 || at as u64 + size > end as u64 {
            break; // not a box: stop rather than guess
        }
        found.push((kind, at, at + header, at + size as usize));
        at += size as usize;
    }
    found
}

fn kind_of_handler(handler: &[u8]) -> Option<Kind> {
    match handler {
        b"vide" => Some(Kind::Video),
        b"soun" => Some(Kind::Audio),
        b"sbtl" | b"text" | b"subt" | b"clcp" => Some(Kind::Subtitles),
        _ => None,
    }
}

/// The flags of the audio, video and subtitle tracks of the `moov` box of an MP4, in file order.
pub fn parse_track_flags(moov: &[u8], moov_offset: u64) -> Vec<TrackFlag> {
    let mut counts = [0usize; 3];
    let mut flags = Vec::new();
    for (kind, _, content, end) in boxes(moov, 0, moov.len()) {
        if &kind != b"moov" {
            continue;
        }
        for (child, _, trak_content, trak_end) in boxes(moov, content, end) {
            if &child != b"trak" {
                continue;
            }
            let (mut header, mut handler) = (None, None);
            for (name, _, inner, inner_end) in boxes(moov, trak_content, trak_end) {
                match &name {
                    b"tkhd" => header = Some(inner),
                    b"mdia" => {
                        for (m, _, hinner, _) in boxes(moov, inner, inner_end) {
                            // hdlr: version and flags (4), pre_defined (4), then the handler type
                            if &m == b"hdlr" && hinner + 12 <= moov.len() {
                                handler = Some(&moov[hinner + 8..hinner + 12]);
                            }
                        }
                    }
                    _ => {}
                }
            }
            let (Some(header), Some(handler)) = (header, handler) else { continue };
            let Some(kind) = kind_of_handler(handler) else { continue };
            if header + 4 > moov.len() {
                continue;
            }
            let slot = match kind {
                Kind::Video => 0,
                Kind::Audio => 1,
                Kind::Subtitles => 2,
            };
            counts[slot] += 1;
            flags.push(TrackFlag {
                kind,
                number: counts[slot],
                offset: moov_offset + header as u64 + 3,
                enabled: moov[header + 3] & 1 == 1,
            });
        }
    }
    flags
}

/// Read the track flags of an MP4 file (only its `moov` box is read).
pub fn read_track_flags(path: &Path) -> io::Result<Vec<TrackFlag>> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let mut at = 0u64;
    while at + 8 <= length {
        file.seek(SeekFrom::Start(at))?;
        let mut header = [0u8; 16];
        let got = file.read(&mut header)?;
        if got < 8 {
            break;
        }
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let size = match size32 {
            0 => length - at,
            1 if got >= 16 => u64::from_be_bytes(header[8..16].try_into().unwrap_or([0; 8])),
            size => size,
        };
        if size < 8 {
            break;
        }
        if &header[4..8] == b"moov" {
            let mut moov = vec![0u8; size.min(length - at) as usize];
            file.seek(SeekFrom::Start(at))?;
            file.read_exact(&mut moov)?;
            return Ok(parse_track_flags(&moov, at));
        }
        at += size;
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "no moov box: not an MP4 file"))
}

/// Set or clear the "enabled" (default) bit of tracks, each given as the offset of its flags.
pub fn set_enabled(path: &Path, changes: &[(u64, bool)]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    for (offset, enabled) in changes {
        let mut byte = [0u8; 1];
        file.seek(SeekFrom::Start(*offset))?;
        file.read_exact(&mut byte)?;
        byte[0] = if *enabled { byte[0] | 1 } else { byte[0] & !1 };
        file.seek(SeekFrom::Start(*offset))?;
        file.write_all(&byte)?;
    }
    file.flush()
}

/// A `trak` box of the `moov`, as `parse_traks` finds it: where it is in the moov (from the moov's start), its
/// kind (`None` for a track that is not audio, video or subtitles), where the flags of its `tkhd` are (the lowest
/// bit is "enabled") and where the language of its `mdhd` is (two bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trak {
    pub start: usize,
    pub end: usize,
    pub kind: Option<Kind>,
    pub enabled_at: usize,
    pub language_at: usize,
}

/// A change to one track of an MP4, in place: the language (three lowercase letters) and/or the default
/// ("enabled") flag. `trak` is the track's place among the `trak` boxes, from 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrakChange {
    pub trak: usize,
    pub language: Option<String>,
    pub enabled: Option<bool>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// The `trak` boxes of a `moov` box (the whole box, header included), in file order. A track whose handler
/// is not audio, video or subtitles has no kind.
pub fn parse_traks(moov: &[u8]) -> io::Result<Vec<Trak>> {
    let mut traks = Vec::new();
    for (kind, _, content, end) in boxes(moov, 0, moov.len()) {
        if &kind != b"moov" {
            continue;
        }
        for (child, at, trak_content, trak_end) in boxes(moov, content, end) {
            if &child != b"trak" {
                continue;
            }
            let (mut header, mut media, mut handler) = (None, None, None);
            for (name, _, inner, inner_end) in boxes(moov, trak_content, trak_end) {
                match &name {
                    b"tkhd" => header = Some(inner),
                    b"mdia" => {
                        for (m, _, mi, _) in boxes(moov, inner, inner_end) {
                            match &m {
                                b"mdhd" => media = Some(mi),
                                // hdlr: version and flags (4), pre_defined (4), then the handler type
                                b"hdlr" if mi + 12 <= moov.len() => handler = Some(&moov[mi + 8..mi + 12]),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            let header = header.ok_or_else(|| invalid("a track has no track header"))?;
            let media = media.ok_or_else(|| invalid("a track has no media header"))?;
            if header + 4 > moov.len() || media + 1 > moov.len() {
                return Err(invalid("a track header is cut short"));
            }
            // mdhd: version and flags (4), then the times and duration (16 bytes, or 28 for version 1), then the language
            let language_at = media + if moov[media] == 1 { 32 } else { 20 };
            if language_at + 2 > moov.len() {
                return Err(invalid("a media header is cut short"));
            }
            traks.push(Trak {
                start: at,
                end: trak_end,
                kind: handler.and_then(kind_of_handler),
                enabled_at: header + 3,
                language_at,
            });
        }
    }
    Ok(traks)
}

/// The two bytes of an mdhd language for a three letter lowercase code (`eng`), as the spec packs them.
fn language_bits(code: &str) -> io::Result<[u8; 2]> {
    let letters = code.as_bytes();
    if letters.len() != 3 || !letters.iter().all(|c| c.is_ascii_lowercase()) {
        return Err(invalid("a language must be three lowercase letters"));
    }
    let bits = letters.iter().fold(0u16, |acc, c| (acc << 5) | (*c - 0x60) as u16);
    Ok(bits.to_be_bytes())
}

/// The whole `moov` box of an MP4 file: where it starts, and its bytes.
fn read_moov(path: &Path) -> io::Result<(u64, Vec<u8>)> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let mut at = 0u64;
    while at + 8 <= length {
        file.seek(SeekFrom::Start(at))?;
        let mut header = [0u8; 16];
        let got = file.read(&mut header)?;
        if got < 8 {
            break;
        }
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let size = match size32 {
            0 => length - at,
            1 if got >= 16 => u64::from_be_bytes(header[8..16].try_into().unwrap_or([0; 8])),
            size => size,
        };
        if size < 8 {
            break;
        }
        if &header[4..8] == b"moov" {
            let mut moov = vec![0u8; size.min(length - at) as usize];
            file.seek(SeekFrom::Start(at))?;
            file.read_exact(&mut moov)?;
            return Ok((at, moov));
        }
        at += size;
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "no moov box: not an MP4 file"))
}

/// Change the tracks of an MP4 file in place: the languages and default flags in `changes`, and, when `order`
/// is given, the `trak` boxes put in that order (`order[i]` is the place of the trak that comes i-th). The
/// `trak` boxes are only moved within the `moov` box, which keeps its size, so the media data and the offsets
/// into it stay as they are. The boxes must be one run with nothing between them for a reorder.
pub fn edit_in_place(path: &Path, changes: &[TrakChange], order: Option<&[usize]>) -> io::Result<()> {
    let (moov_at, mut moov) = read_moov(path)?;
    let traks = parse_traks(&moov)?;
    for change in changes {
        let trak = traks.get(change.trak).ok_or_else(|| invalid("a track was not found in the file"))?;
        if let Some(language) = &change.language {
            moov[trak.language_at..trak.language_at + 2].copy_from_slice(&language_bits(language)?);
        }
        if let Some(enabled) = change.enabled {
            let flags = moov[trak.enabled_at];
            moov[trak.enabled_at] = if enabled { flags | 1 } else { flags & !1 };
        }
    }
    if let Some(order) = order {
        let mut seen = vec![false; traks.len()];
        if order.len() != traks.len() || order.iter().any(|&i| i >= traks.len() || std::mem::replace(&mut seen[i], true)) {
            return Err(invalid("the new order does not name every track once"));
        }
        if traks.windows(2).any(|pair| pair[0].end != pair[1].start) {
            return Err(invalid("the track boxes are not one run, so they cannot be moved"));
        }
        if let (Some(first), Some(last)) = (traks.first(), traks.last()) {
            let region: Vec<u8> = order.iter().flat_map(|&i| moov[traks[i].start..traks[i].end].to_vec()).collect();
            moov[first.start..last.end].copy_from_slice(&region);
        }
    }
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    file.seek(SeekFrom::Start(moov_at))?;
    file.write_all(&moov)?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut out = ((content.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(content);
        out
    }

    fn trak(handler: &[u8; 4], flags: u8) -> Vec<u8> {
        let mut tkhd = vec![0, 0, 0, flags]; // version 0, three flag bytes
        tkhd.extend_from_slice(&[0u8; 80]);
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0u8; 12]);
        let mdia = make_box(b"mdia", &make_box(b"hdlr", &hdlr));
        make_box(b"trak", &[make_box(b"tkhd", &tkhd), mdia].concat())
    }

    /// A track with a media header (version 0) that has `language`, for the in-place edits.
    fn trak_with_language(handler: &[u8; 4], flags: u8, language: &str) -> Vec<u8> {
        let mut tkhd = vec![0, 0, 0, flags];
        tkhd.extend_from_slice(&[0u8; 80]);
        let mut mdhd = vec![0u8; 12]; // version and flags, creation, modification
        mdhd.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        mdhd.extend_from_slice(&0u32.to_be_bytes()); // duration
        mdhd.extend_from_slice(&language_bits(language).unwrap());
        mdhd.extend_from_slice(&[0, 0]); // quality
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0u8; 12]);
        let mdia = make_box(b"mdia", &[make_box(b"mdhd", &mdhd), make_box(b"hdlr", &hdlr)].concat());
        make_box(b"trak", &[make_box(b"tkhd", &tkhd), mdia].concat())
    }

    #[test]
    fn a_language_is_packed_as_the_spec_packs_it() {
        assert_eq!(language_bits("eng").unwrap(), [0x15, 0xC7]); // e=5, n=14, g=7, each in five bits
        assert_eq!(language_bits("fre").unwrap(), [0x1A, 0x45]);
        assert!(language_bits("EN").is_err());
        assert!(language_bits("Eng").is_err());
        assert!(language_bits("english").is_err());
    }

    #[test]
    fn each_trak_has_its_flag_and_language_where_the_bytes_say() {
        let moov = make_box(b"moov", &[
            trak_with_language(b"vide", 3, "und"),
            trak_with_language(b"soun", 3, "ita"),
            trak_with_language(b"soun", 2, "eng"),
        ].concat());
        let traks = parse_traks(&moov).unwrap();
        let kinds: Vec<Option<Kind>> = traks.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, [Some(Kind::Video), Some(Kind::Audio), Some(Kind::Audio)]);
        assert_eq!(moov[traks[1].enabled_at], 3);
        assert_eq!(moov[traks[2].enabled_at], 2);
        assert_eq!(&moov[traks[1].language_at..traks[1].language_at + 2], language_bits("ita").unwrap());
        assert_eq!(&moov[traks[2].language_at..traks[2].language_at + 2], language_bits("eng").unwrap());
    }

    /// An MP4 with an mdat (the media, not read here) and a moov after it, with the given traks.
    fn mp4_with_traks(traks: &[Vec<u8>]) -> Vec<u8> {
        let mut data = make_box(b"ftyp", b"isom\0\0\0\0isom");
        data.extend_from_slice(&make_box(b"mdat", &[7u8; 50]));
        data.extend_from_slice(&make_box(b"moov", &traks.concat()));
        data
    }

    fn temp_mp4(name: &str, data: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mau-mp4-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name);
        std::fs::write(&file, data).unwrap();
        file
    }

    #[test]
    fn languages_flags_and_the_order_change_in_place_and_the_size_does_not() {
        let traks = [
            trak_with_language(b"vide", 3, "und"),
            trak_with_language(b"soun", 3, "ita"),
            trak_with_language(b"soun", 2, "eng"),
        ];
        let data = mp4_with_traks(&traks);
        let file = temp_mp4("order.mp4", &data);
        let changes = [
            TrakChange { trak: 1, language: Some("fra".to_string()), enabled: Some(false) },
            TrakChange { trak: 2, language: None, enabled: Some(true) },
        ];
        edit_in_place(&file, &changes, Some(&[0, 2, 1])).unwrap();
        let out = std::fs::read(&file).unwrap();
        assert_eq!(out.len(), data.len(), "the file is the same size");
        let (_, moov) = read_moov(&file).unwrap();
        let now = parse_traks(&moov).unwrap();
        // the English track, now enabled, is second; the Italian one (renamed fra and disabled) is third
        assert_eq!(moov[now[1].enabled_at], 3);
        assert_eq!(&moov[now[1].language_at..now[1].language_at + 2], language_bits("eng").unwrap());
        assert_eq!(moov[now[2].enabled_at] & 1, 0);
        assert_eq!(&moov[now[2].language_at..now[2].language_at + 2], language_bits("fra").unwrap());
        // the media is where it was
        assert_eq!(&out[..data.len() - moov.len()], &data[..data.len() - moov.len()]);
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn an_order_that_is_not_a_permutation_or_a_moov_with_gaps_is_refused() {
        let traks = [trak_with_language(b"soun", 3, "ita"), trak_with_language(b"soun", 2, "eng")];
        let file = temp_mp4("refused.mp4", &mp4_with_traks(&traks));
        let before = std::fs::read(&file).unwrap();
        assert!(edit_in_place(&file, &[], Some(&[0, 0])).is_err(), "a track twice");
        assert!(edit_in_place(&file, &[], Some(&[0])).is_err(), "a track missing");
        assert_eq!(std::fs::read(&file).unwrap(), before, "nothing is written when refused");
        // something other than a trak between two traks: the traks are not one run
        let gap = [traks[0].clone(), make_box(b"free", &[0u8; 8]), traks[1].clone()];
        let gapped = temp_mp4("gap.mp4", &mp4_with_traks(&gap));
        assert!(edit_in_place(&gapped, &[], Some(&[1, 0])).is_err());
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(&gapped);
    }

    #[test]
    fn the_flags_of_each_track_are_found_in_order_by_kind() {
        let moov = make_box(b"moov", &[
            make_box(b"mvhd", &[0u8; 100]),
            trak(b"vide", 3), trak(b"soun", 3), trak(b"soun", 2),
            trak(b"sbtl", 3), trak(b"tmcd", 3), trak(b"text", 2),
        ].concat());
        let found = parse_track_flags(&moov, 1000);
        let summary: Vec<(Kind, usize, bool)> = found.iter().map(|t| (t.kind, t.number, t.enabled)).collect();
        assert_eq!(summary, [
            (Kind::Video, 1, true), (Kind::Audio, 1, true), (Kind::Audio, 2, false),
            (Kind::Subtitles, 1, true), (Kind::Subtitles, 2, false), // the timecode track is not counted
        ]);
        // the offset is the last flag byte, counted from the start of the file
        let at = found[3].offset as usize - 1000;
        assert_eq!(moov[at], 3);
        assert_eq!(moov[at - 3], 0); // the version byte is three before it
    }

    #[test]
    fn broken_input_is_not_a_panic() {
        assert!(parse_track_flags(&[], 0).is_empty());
        assert!(parse_track_flags(b"\0\0\0\x08moov", 0).is_empty());
        let cut = make_box(b"moov", &trak(b"soun", 3));
        assert!(parse_track_flags(&cut[..cut.len() - 5], 0).is_empty()); // cut short: not trusted
        let mut lying = make_box(b"moov", &trak(b"soun", 3));
        lying[0..4].copy_from_slice(&0xFFFF_FF00u32.to_be_bytes()); // a size past the end
        assert!(parse_track_flags(&lying, 0).is_empty());
    }

    #[test]
    fn the_enabled_bit_is_changed_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("mau-mp4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mp4");
        let moov = make_box(b"moov", &[trak(b"soun", 3), trak(b"sbtl", 3)].concat());
        let mut data = make_box(b"ftyp", b"isom\0\0\0\0isom");
        data.extend_from_slice(&make_box(b"mdat", &[7u8; 50]));
        data.extend_from_slice(&moov);
        std::fs::write(&file, &data).unwrap();
        let flags = read_track_flags(&file).unwrap(); // moov after mdat is found
        assert_eq!(flags.len(), 2);
        set_enabled(&file, &[(flags[1].offset, false)]).unwrap();
        let after = read_track_flags(&file).unwrap();
        assert_eq!((after[0].enabled, after[1].enabled), (true, false));
        let changed: Vec<usize> = std::fs::read(&file).unwrap().iter().zip(&data).enumerate()
            .filter(|(_, (a, b))| a != b).map(|(i, _)| i).collect();
        assert_eq!(changed, [flags[1].offset as usize]); // one byte differs
        set_enabled(&file, &[(flags[1].offset, true)]).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), data);
        std::fs::write(&file, b"not an mp4 at all").unwrap();
        assert!(read_track_flags(&file).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// Each real MP4 named in MAU_MP4_CHECK (separated by `:`) is copied, its tracks are put in reverse order
    /// with the first audio track's language changed, and `mediainfo` must read the new order and language and
    /// `ffmpeg` must decode every stream. A file whose track boxes are not one run is reported and skipped.
    #[test]
    fn the_in_place_edits_are_read_back_on_the_real_files_named_in_mau_mp4_check() {
        let Ok(list) = std::env::var("MAU_MP4_CHECK") else { return };
        let dir = std::env::temp_dir().join(format!("mau-mp4-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut failures = Vec::new();
        for file in list.split(':').filter(|f| !f.is_empty()) {
            let copy = dir.join("copy.mp4");
            std::fs::copy(file, &copy).unwrap();
            let (_, moov) = read_moov(&copy).unwrap();
            let traks = parse_traks(&moov).unwrap();
            // (the reverse of all the traks: each kind's order is reversed among its own traks)
            let reversed: Vec<usize> = (0..traks.len()).rev().collect();
            let first_audio = traks.iter().position(|t| t.kind == Some(Kind::Audio));
            let changes: Vec<TrakChange> = first_audio
                .map(|trak| vec![TrakChange { trak, language: Some("fra".to_string()), enabled: None }])
                .unwrap_or_default();
            if let Err(error) = edit_in_place(&copy, &changes, Some(&reversed)) {
                eprintln!("{file}: not changed in place ({error})");
                continue;
            }
            // mediainfo lists the video first whatever the order, so the order is checked within each kind: the
            // tracks of a kind come out in the reversed order, and the first audio track (now last among the audio)
            // has its new language (mediainfo gives two letters for fra)
            let read_tracks = |path: &Path| -> Vec<(String, String)> {
                let out = std::process::Command::new("mediainfo").arg("--Output=JSON").arg(path).output().expect("mediainfo runs");
                let data: serde_json::Value = serde_json::from_slice(&out.stdout).expect("mediainfo reads the file");
                data["media"]["track"].as_array().unwrap().iter()
                    .filter(|t| t["@type"] != "General")
                    .map(|t| (t["@type"].as_str().unwrap_or("").to_string(), t["Language"].as_str().unwrap_or("").to_string()))
                    .collect()
            };
            let read = read_tracks(&copy);
            let before = read_tracks(Path::new(file));
            let mut expected = Vec::new();
            for kind in ["Video", "Audio", "Text"] {
                let mut same_kind: Vec<(String, String)> = before.iter().filter(|(k, _)| k == kind).cloned().collect();
                same_kind.reverse();
                if kind == "Audio" {
                    if let Some(last) = same_kind.last_mut() {
                        if first_audio.is_some() {
                            last.1 = "fr".to_string();
                        }
                    }
                }
                expected.extend(same_kind);
            }
            let mut got_sorted = read.clone();
            got_sorted.sort_by_key(|(k, _)| ["Video", "Audio", "Text"].iter().position(|x| x == k).unwrap_or(9));
            let kinds_match = got_sorted == expected;
            let decode = std::process::Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&copy).args(["-f", "null", "-"]).output();
            let decoded = decode.map(|o| o.status.success() && o.stderr.is_empty()).unwrap_or(false);
            if !kinds_match || !decoded {
                failures.push(format!("{file}: read {read:?}, expected {expected:?}, decodes: {decoded}"));
            } else {
                eprintln!("{file}: reversed and changed in place; mediainfo reads {read:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

}
