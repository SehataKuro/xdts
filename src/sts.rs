//! STS — the binary sheet of シラヘイタイムシート (ShiraheiTimeSheet), and of
//! DiTiS, which reads and writes it today.
//!
//! The application that defined the format is gone (its site closed), and
//! there is no published description. What is here is what the two
//! implementations still around agree on, and where they disagree, the
//! reading that accepts both:
//!
//! * the converter in *remaping* (`lib_STS.js`), written against files the
//!   original application saved, and
//! * DiTiS (`js/parsers.js`), which writes the files people make now.
//!
//! ```text
//! offset  size  contents
//! 0       1     0x11, the length of the name that follows
//! 1       17    "ShiraheiTimeSheet"
//! 18      1     number of columns
//! 19      2     number of frames, little-endian
//! 21      2     reserved, 0
//! 23      2·C·F the cells, column by column, frame by frame: one
//!               little-endian u16 per frame, the drawing number, 0 for empty
//! …             the column names, one after another, each a length byte and
//!               that many bytes of name
//! ```
//!
//! The two disagree in three places, and in each the narrower reading is a
//! special case of the wider one, so a file either of them wrote reads the
//! same here:
//!
//! * **Frames.** remaping reads a u16 at 19; DiTiS reads the byte at 19 and
//!   writes 0 at 20, and so refuses sheets over 255 frames.
//! * **Cells.** remaping reads a u16; DiTiS reads the low byte and writes 0
//!   in the high one (its "type" byte, always 0).
//! * **Names.** The original application ran on Japanese Windows and remaping
//!   decodes Shift_JIS; DiTiS writes UTF-8. A name that is valid UTF-8 is
//!   read as UTF-8 — every ASCII name is both — and anything else as
//!   Shift_JIS, the way Japanese Windows has it (CP932, NEC and IBM
//!   extensions included). The table is in the crate, generated from
//!   Python's `cp932` codec by `tools/gen-cp932.py`, so there is still no
//!   dependency; a code with no character comes back as U+FFFD.
//!
//! Names are written as UTF-8, which is what the application still in use
//! reads.
//!
//! # What the format holds
//!
//! One sheet of drawing numbers: every frame of every column is a number
//! from 1 to 65535, or empty. There is no cut or scene, no frame rate, no
//! dialogue or camerawork, no in-between marks and no letters. So
//! [`to_bytes`] writes the cell columns of a sheet's one time table and
//! nothing else, and refuses a cell it cannot write rather than writing
//! something else in its place:
//!
//! * a sheet with more or less than one time table,
//! * a drawing that is not a plain number from 1 to 65535 (`a0001`, `1a`,
//!   `0`),
//! * more than 255 columns, a name over 255 bytes, or more than 65535 frames.
//!
//! [`HYPHEN`] and the in-between marks
//! [`TICK_1`] / [`TICK_2`] hold the drawing
//! before them, as OpenToonz reads them, and the mark itself is not kept;
//! nor are a cell's p1 options. A sheet over 255 frames is valid here and in
//! remaping, and is one DiTiS will not open.

use crate::{cp932, Error, Sheet, TimeTable, Track, HYPHEN, NULL_CELL, TICK_1, TICK_2};

/// The 18 bytes a file starts with: the length 17, then the name.
pub const SIGNATURE: &[u8; 18] = b"\x11ShiraheiTimeSheet";

/// Where the cells start.
const HEADER_LEN: usize = 23;

/// Whether `bytes` starts like an STS file.
pub fn is_sts(bytes: &[u8]) -> bool {
    bytes.starts_with(SIGNATURE)
}

/// Read an STS file.
///
/// The sheet has one time table, with no name, and the columns as the file
/// has them: a change point where the drawing changes, [`NULL_CELL`] where a
/// column goes empty, and nothing before a column's first drawing.
pub fn from_bytes(bytes: &[u8]) -> Result<Sheet, Error> {
    if !is_sts(bytes) {
        return Err(Error::Parse("not an STS file".into()));
    }
    if bytes.len() < HEADER_LEN {
        return Err(Error::Parse("STS header is cut short".into()));
    }
    let columns = bytes[18] as usize;
    let duration = u16::from_le_bytes([bytes[19], bytes[20]]) as usize;
    if duration == 0 {
        return Err(Error::Parse("STS sheet has no frames".into()));
    }
    let names_at = HEADER_LEN + columns * duration * 2;
    if bytes.len() < names_at {
        return Err(Error::Parse(format!(
            "STS cells are cut short: {columns} columns of {duration} frames need {names_at} bytes, the file has {}",
            bytes.len()
        )));
    }

    let mut tracks = Vec::with_capacity(columns);
    for c in 0..columns {
        let start = HEADER_LEN + c * duration * 2;
        let mut frames = Vec::new();
        let mut last = 0u16;
        for f in 0..duration {
            let at = start + f * 2;
            let n = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
            if n != last {
                let value = if n == 0 {
                    NULL_CELL.to_string()
                } else {
                    n.to_string()
                };
                frames.push((f, value.into()));
                last = n;
            }
        }
        tracks.push(Track { frames });
    }

    // Both implementations stop short of an error on a missing name: one
    // falls back to A, B, …, the other to nothing. An empty name is the one
    // that does not invent anything.
    let mut names = Vec::with_capacity(columns);
    let mut at = names_at;
    for _ in 0..columns {
        let Some(&len) = bytes.get(at) else {
            names.push(String::new());
            continue;
        };
        let end = (at + 1 + len as usize).min(bytes.len());
        names.push(decode_name(&bytes[at + 1..end]));
        at = end;
    }

    Ok(Sheet {
        tables: vec![TimeTable {
            duration,
            columns: names,
            tracks,
            ..TimeTable::default()
        }],
        ..Sheet::default()
    })
}

/// Write a sheet as STS.
///
/// See the [module documentation](self) for what the format cannot hold;
/// any of it is an [`Error::Unsupported`] naming the column and the frame.
pub fn to_bytes(sheet: &Sheet) -> Result<Vec<u8>, Error> {
    let table = match sheet.tables.as_slice() {
        [t] => t,
        ts => {
            return Err(Error::Unsupported(format!(
                "STS holds one time table, and this sheet has {}",
                ts.len()
            )))
        }
    };
    let duration = table.duration;
    if duration == 0 || duration > u16::MAX as usize {
        return Err(Error::Unsupported(format!(
            "STS holds 1 to 65535 frames, and this sheet has {duration}"
        )));
    }
    let columns = table.columns.len().max(table.tracks.len());
    if columns > u8::MAX as usize {
        return Err(Error::Unsupported(format!(
            "STS holds up to 255 columns, and this sheet has {columns}"
        )));
    }

    let mut out = Vec::with_capacity(HEADER_LEN + columns * duration * 2 + columns * 2);
    out.extend_from_slice(SIGNATURE);
    out.push(columns as u8);
    out.extend_from_slice(&(duration as u16).to_le_bytes());
    out.extend_from_slice(&[0, 0]);

    let empty = Track::default();
    for c in 0..columns {
        let track = table.tracks.get(c).unwrap_or(&empty);
        for n in frame_numbers(track, duration, &column_name(table, c))? {
            out.extend_from_slice(&n.to_le_bytes());
        }
    }

    for c in 0..columns {
        let name = table.columns.get(c).map(String::as_str).unwrap_or("");
        if name.len() > u8::MAX as usize {
            return Err(Error::Unsupported(format!(
                "STS holds column names up to 255 bytes, and column {} is {} bytes",
                c + 1,
                name.len()
            )));
        }
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
    }
    Ok(out)
}

fn column_name(table: &TimeTable, c: usize) -> String {
    match table.columns.get(c) {
        Some(n) if !n.is_empty() => format!("column {n}"),
        _ => format!("column {}", c + 1),
    }
}

/// One column as the number every frame shows, 0 for empty.
fn frame_numbers(track: &Track, duration: usize, column: &str) -> Result<Vec<u16>, Error> {
    let mut out = vec![0u16; duration];
    let mut current = 0u16;
    let mut changes = track.frames.iter().peekable();
    for (f, slot) in out.iter_mut().enumerate() {
        while let Some((at, cell)) = changes.peek() {
            if *at > f {
                break;
            }
            current = match cell.value.as_str() {
                NULL_CELL => 0,
                HYPHEN | TICK_1 | TICK_2 => current,
                v => drawing_number(v).ok_or_else(|| {
                    Error::Unsupported(format!(
                        "STS holds drawing numbers from 1 to 65535, and {column} has {v:?} at frame {}",
                        at + 1
                    ))
                })?,
            };
            changes.next();
        }
        *slot = current;
    }
    Ok(out)
}

fn drawing_number(v: &str) -> Option<u16> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse::<u16>().ok().filter(|&n| n != 0)
}

/// A column name: UTF-8 if it is UTF-8, Shift_JIS otherwise.
fn decode_name(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => decode_shift_jis(bytes),
    }
}

/// Shift_JIS as Japanese Windows has it (CP932), by the table in
/// `cp932.rs`. A byte or pair with no character is U+FFFD.
fn decode_shift_jis(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            0x00..=0x7f => {
                out.push(b as char);
                i += 1;
            }
            0xa1..=0xdf => {
                out.push(char::from_u32(0xff61 + (b - 0xa1) as u32).unwrap_or('\u{fffd}'));
                i += 1;
            }
            0x81..=0x9f | 0xe0..=0xfc if i + 1 < bytes.len() => {
                out.push(double_byte(b, bytes[i + 1]).unwrap_or('\u{fffd}'));
                i += 2;
            }
            _ => {
                out.push('\u{fffd}');
                i += 1;
            }
        }
    }
    out
}

fn double_byte(lead: u8, trail: u8) -> Option<char> {
    if !(0x40..=0xfc).contains(&trail) {
        return None;
    }
    let row = match lead {
        0x81..=0x9f => lead - 0x81,
        _ => lead - 0xe0 + 0x1f,
    } as usize;
    let code = *cp932::TABLE.get(row * cp932::ROW + (trail - 0x40) as usize)?;
    (code != 0).then(|| char::from_u32(code as u32)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_jis_decodes_as_windows_has_it() {
        // あ ア Ａ １ ー 背景, and ① from the NEC extension.
        let bytes = [
            0x82, 0xa0, 0x83, 0x41, 0x82, 0x60, 0x82, 0x50, 0x81, 0x5b, 0x94, 0x77, 0x8c, 0x69,
            0x87, 0x40,
        ];
        assert_eq!(decode_name(&bytes), "あアＡ１ー背景①");
        assert_eq!(decode_name(&[0xb1, b'A']), "ｱA");
        // A pair with no character, and a lead byte with nothing after it.
        assert_eq!(decode_name(&[0x85, 0x40, b'B', 0x88]), "\u{fffd}B\u{fffd}");
    }

    #[test]
    fn a_utf8_name_is_read_as_utf8() {
        assert_eq!(decode_name("背景".as_bytes()), "背景");
    }
}
