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
        let dir = std::env::temp_dir().join(format!("fml-mp4-{}", std::process::id()));
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
}
