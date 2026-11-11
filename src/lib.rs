//! Read and write animation exposure sheets in the XDTS and TDTS formats,
//! with no dependencies.
//!
//! * **XDTS** — "eXchange Digital Time Sheet" (CELSYS with 東映アニメーション,
//!   2018). The interchange format, read and written by CLIP STUDIO PAINT,
//!   OpenToonz and others.
//! * **TDTS** — the native format of 東映アニメーション デジタルタイムシート.
//!   Close to XDTS but not the same, and the differences are not optional:
//!   see [`tdts`].
//!
//! # The model
//!
//! A sheet here is a list of **change points**, which is what both file
//! formats store: a frame holds whatever the last change before it said, and
//! only the changes are written down. An application that thinks in whole
//! frames — one cell per frame, as a drawn sheet looks — converts to this
//! form on the way out and back on the way in.
//!
//! ```
//! use xdts::{Sheet, TimeTable, Track};
//!
//! let sheet = Sheet {
//!     cut: "12".into(),
//!     tables: vec![TimeTable {
//!         name: "cut012".into(),
//!         duration: 24,
//!         columns: vec!["A".into()],
//!         tracks: vec![Track {
//!             frames: vec![
//!                 (0, "1".into()),
//!                 (12, "2".into()),
//!                 (24, xdts::NULL_CELL.into()),
//!             ],
//!         }],
//!     }],
//!     ..Sheet::default()
//! };
//! let text = xdts::xdts::to_string(&sheet, &Default::default());
//! assert!(text.starts_with("exchangeDigitalTimeSheet Save Data\n"));
//! assert_eq!(xdts::xdts::from_str(&text).unwrap().cut, "12");
//! ```
//!
//! # Values
//!
//! A cell's value is the drawing number as written, or one of the special
//! strings: [`NULL_CELL`] (×, an empty cell), [`TICK_1`] (○), [`TICK_2`] (●)
//! and [`HYPHEN`] (continue the previous instruction).

#![forbid(unsafe_code)]

use std::fmt;

pub mod json;
pub mod tdts;
pub mod xdts;

/// × — the cell is empty from here.
pub const NULL_CELL: &str = "SYMBOL_NULL_CELL";
/// ○ — an in-between mark.
pub const TICK_1: &str = "SYMBOL_TICK_1";
/// ● — the second in-between mark (a reverse sheet).
pub const TICK_2: &str = "SYMBOL_TICK_2";
/// Continue the previous instruction. Valid in every field.
pub const HYPHEN: &str = "SYMBOL_HYPHEN";

/// Cell numbers and the symbols above.
pub const FIELD_CELL: u32 = 0;
/// Dialogue. Not written by this crate, but read back where present.
pub const FIELD_DIALOG: u32 = 3;
/// Camerawork. Not written by this crate, but read back where present.
pub const FIELD_CAMERA: u32 = 5;

#[derive(Debug)]
pub enum Error {
    /// The text is not a sheet of this format.
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl std::error::Error for Error {}

/// One column of one time table, as the changes written in it.
///
/// `frames` is `(frame, value)` with the frame zero-based, in order. A value
/// holds until the next entry; an entry at `frame == duration` closes the
/// column past the end of the sheet, which is how an exposed drawing is
/// stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub frames: Vec<(usize, String)>,
}

impl Track {
    /// The value in effect at `frame`, if any.
    pub fn at(&self, frame: usize) -> Option<&str> {
        self.frames
            .iter()
            .rfind(|(f, _)| *f <= frame)
            .map(|(_, v)| v.as_str())
    }
}

/// One sheet of a cut: its length, its column names and its columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimeTable {
    pub name: String,
    /// Length in frames. At least 1 in a valid file.
    pub duration: usize,
    /// Column names, indexed by track number.
    pub columns: Vec<String>,
    pub tracks: Vec<Track>,
}

/// A whole document: the sheet header and its time tables.
///
/// `episode` has a place in TDTS and none in XDTS, so it survives one format
/// and not the other.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    pub cut: String,
    pub scene: String,
    pub episode: String,
    pub operator: String,
    pub tables: Vec<TimeTable>,
}

/// How to write a sheet out.
#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// Lay the JSON out over several lines, as the applications do.
    pub pretty: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self { pretty: true }
    }
}

/// Read a sheet from text in either format, by its header line.
pub fn from_str(text: &str) -> Result<Sheet, Error> {
    let trimmed = text.trim_start_matches('\u{feff}');
    let first = trimmed.split_once('\n').map(|(a, _)| a).unwrap_or(trimmed);
    match first.trim() {
        xdts::HEADER => xdts::from_str(text),
        tdts::HEADER => tdts::from_str(text),
        other => Err(Error::Parse(format!(
            "not a time sheet: first line is {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_holds_until_the_next_change() {
        let t = Track {
            frames: vec![(0, "1".into()), (6, "2".into())],
        };
        assert_eq!(t.at(0), Some("1"));
        assert_eq!(t.at(5), Some("1"));
        assert_eq!(t.at(6), Some("2"));
        assert_eq!(t.at(99), Some("2"));
        assert_eq!(Track::default().at(0), None);
    }

    /// A column that starts late holds nothing before its first change.
    #[test]
    fn nothing_is_exposed_before_the_first_change() {
        let t = Track {
            frames: vec![(4, "1".into())],
        };
        assert_eq!(t.at(3), None);
        assert_eq!(t.at(4), Some("1"));
    }

    #[test]
    fn either_format_is_recognised_by_its_header_line() {
        let s = Sheet {
            cut: "7".into(),
            tables: vec![TimeTable {
                name: "a".into(),
                duration: 4,
                columns: vec!["A".into()],
                tracks: vec![Track {
                    frames: vec![(0, "1".into())],
                }],
            }],
            ..Sheet::default()
        };
        let o = WriteOptions::default();
        assert_eq!(from_str(&xdts::to_string(&s, &o)).unwrap().cut, "7");
        assert_eq!(from_str(&tdts::to_string(&s, &o)).unwrap().cut, "7");
        assert!(from_str("something else\n{}").is_err());
    }
}
