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
//!         ..TimeTable::default()
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
//!
//! # Fields and versions
//!
//! A time table has three fields: cells ([`FIELD_CELL`]), dialogue
//! ([`FIELD_DIALOG`]) and camerawork ([`FIELD_CAMERA`]). Cells are a column
//! of single values, so they are [`Track`]s; the other two carry a list per
//! change point and are [`Field`]s of [`ValueTrack`]s. All three are read and
//! written, so a sheet does not lose its dialogue or its camera moves by
//! passing through this crate.
//!
//! XDTS has two published versions, and the difference between them is
//! exactly one thing: version 10 (2022-12-06) lets a camerawork instruction
//! carry the camera position after the direction, where version 5
//! (2018-11-29) had only the direction. [`xdts::version`] picks the lower
//! version that can hold a given sheet, and [`xdts::to_string`] writes it.
//!
//! One unofficial extension is also carried: p1 adds [`OPTION_KEYFRAME`] and
//! [`OPTION_REFERENCEFRAME`] to a [`Cell`], which is how a sheet says a
//! drawing is a key or a reference for the in-betweens. A sheet that uses
//! neither is written as the published format and says nothing about the
//! extension; see [`xdts::SUBVERSION_P1`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

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
/// Dialogue: a speaker and a line.
pub const FIELD_DIALOG: u32 = 3;
/// Camerawork: a direction, and from version 10 a camera position.
pub const FIELD_CAMERA: u32 = 5;
/// TDTS's other cell column.
///
/// A converter whose column ids this crate was checked against calls field 0
/// ACTION and this one CELL, and says the application shows this one where it
/// has data and field 0 otherwise. None of that is official, and XDTS has no
/// such field at all — its `fieldId` is 0, 3 or 5 — so this is read and
/// written only in TDTS. See [`TimeTable::cell_field`].
pub const FIELD_CELL_ALT: u32 = 4;

/// 原画 — this drawing is a key, and its number is circled.
///
/// A [`Cell::options`] string, and not part of the published format: see
/// [`xdts::SUBVERSION_P1`].
pub const OPTION_KEYFRAME: &str = "OPTION_KEYFRAME";
/// 中割参考 — this drawing is a reference for the in-betweens, and its number
/// is enclosed in a triangle.
///
/// A [`Cell::options`] string, and not part of the published format: see
/// [`xdts::SUBVERSION_P1`].
pub const OPTION_REFERENCEFRAME: &str = "OPTION_REFERENCEFRAME";

#[derive(Debug)]
/// What can go wrong reading a sheet.
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

/// What is written in one cell: the drawing number or a symbol, and the marks
/// on it.
///
/// `"1".into()` is a plain cell, which is all the published format has. The
/// options are the unofficial p1 extension — [`OPTION_KEYFRAME`] and
/// [`OPTION_REFERENCEFRAME`] — and a cell with none of them is written
/// exactly as it was before this crate could carry them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// The drawing number as written, or one of the symbol strings.
    pub value: String,
    /// The p1 marks on it, usually none.
    pub options: Vec<String>,
    /// The index into the sheet's own text colours that this cell is written
    /// in. A TDTS key (`fontColorId`), 0 for the first colour, which is
    /// black; XDTS has no colours and ignores it.
    pub font_color: u32,
}

impl Cell {
    /// A cell with marks on it, as in
    /// `Cell::marked("1", [OPTION_KEYFRAME])`.
    pub fn marked<I, S>(value: &str, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            value: value.to_string(),
            options: options.into_iter().map(Into::into).collect(),
            font_color: 0,
        }
    }

    /// Whether this cell carries `option`.
    pub fn has(&self, option: &str) -> bool {
        self.options.iter().any(|o| o == option)
    }
}

impl From<&str> for Cell {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl From<String> for Cell {
    fn from(value: String) -> Self {
        Self {
            value,
            options: Vec::new(),
            font_color: 0,
        }
    }
}

/// Compared against a string, a cell is its value: the marks on a drawing do
/// not change which drawing it is.
impl PartialEq<str> for Cell {
    fn eq(&self, other: &str) -> bool {
        self.value == other
    }
}

impl PartialEq<&str> for Cell {
    fn eq(&self, other: &&str) -> bool {
        self.value == *other
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

/// One column of one time table, as the changes written in it.
///
/// `frames` is `(frame, cell)` with the frame zero-based, in order. A value
/// holds until the next entry; an entry at `frame == duration` closes the
/// column past the end of the sheet, which is how an exposed drawing is
/// stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    /// The change points, in frame order.
    pub frames: Vec<(usize, Cell)>,
}

impl Track {
    /// The cell in effect at `frame`, if any.
    pub fn at(&self, frame: usize) -> Option<&Cell> {
        self.frames
            .iter()
            .rfind(|(f, _)| *f <= frame)
            .map(|(_, v)| v)
    }

    /// The value in effect at `frame`, without its marks.
    pub fn value_at(&self, frame: usize) -> Option<&str> {
        self.at(frame).map(|c| c.value.as_str())
    }

    /// Whether any cell here carries an option, which is what makes a sheet
    /// need the p1 extension rather than the published format.
    pub fn has_options(&self) -> bool {
        self.frames.iter().any(|(_, c)| !c.options.is_empty())
    }
}

/// One column of a field whose instructions carry more than one value.
///
/// The cell field holds a single string per change point, which is what
/// [`Track`] is for. The other two fields hold a list, in the order the
/// specification gives:
///
/// * dialogue ([`FIELD_DIALOG`]) — the speaker, then the line (1–2 values)
/// * camerawork ([`FIELD_CAMERA`]) — the direction, then, from XDTS version
///   10, the camera position X and Y, the magnification percentage, the
///   rotation angle, and the camera centre X and Y (1–7 values)
///
/// A list that stops early is what the file had; nothing is padded. Every
/// value is the string as written, because that is all the format says it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValueTrack {
    /// The change points, in frame order, each with the whole list of
    /// values its instruction carries.
    pub frames: Vec<(usize, Vec<String>)>,
}

impl ValueTrack {
    /// The values in effect at `frame`, if any.
    pub fn at(&self, frame: usize) -> Option<&[String]> {
        self.frames
            .iter()
            .rfind(|(f, _)| *f <= frame)
            .map(|(_, v)| v.as_slice())
    }
}

/// The dialogue or camerawork field of one time table: its column names and
/// its columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    /// Column names, indexed by track number.
    pub columns: Vec<String>,
    /// The columns, indexed by track number.
    pub tracks: Vec<ValueTrack>,
}

impl Field {
    /// Whether there is nothing here to write. A field like this is left out
    /// of the file rather than written as an empty shell.
    pub fn is_empty(&self) -> bool {
        self.tracks.iter().all(|t| t.frames.is_empty())
    }

    /// Whether any instruction carries more than the first value — for
    /// camerawork, whether the camera position is there, which is what makes
    /// a sheet version 10 rather than version 5.
    pub fn has_extra_values(&self) -> bool {
        self.tracks
            .iter()
            .flat_map(|t| &t.frames)
            .any(|(_, v)| v.len() > 1)
    }
}

/// One sheet of a cut: its length, its column names and its columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimeTable {
    /// The sheet's name. The application numbers them sheet1, sheet2, …
    pub name: String,
    /// Length in frames. At least 1 in a valid file.
    pub duration: usize,
    /// Cell column names, indexed by track number.
    pub columns: Vec<String>,
    /// The cell field ([`FIELD_CELL`]).
    pub tracks: Vec<Track>,
    /// The dialogue field ([`FIELD_DIALOG`]). Empty unless a file had one.
    pub dialogue: Field,
    /// The camerawork field ([`FIELD_CAMERA`]). Empty unless a file had one.
    pub camera: Field,
    /// Which column the cells are in: [`FIELD_CELL`], or [`FIELD_CELL_ALT`]
    /// for a TDTS sheet that keeps them in its other one.
    ///
    /// A file is written back to the column it was read from. The XDTS writer
    /// ignores this, because field 4 is not a field that format has.
    pub cell_field: u32,
    /// The earlier revisions of this sheet, oldest first.
    ///
    /// TDTS keeps the history of a sheet beside it: the `timeTables` of one
    /// `timeSheet` are the same sheet saved over and over, and the last is
    /// the current one. This table is that current one, and these are the
    /// ones before it. A revision's own `revisions` is empty.
    ///
    /// XDTS has nowhere for them, so they survive one format and not the
    /// other.
    pub revisions: Vec<TimeTable>,
    /// The colour the sheet is shown in, as an index into the application's
    /// own list. A TDTS key, and 0 — black — in every sheet seen so far.
    pub color: u32,
    /// Dummy frames before the sheet starts (`headDummykomas`) and after it
    /// ends (`footDummykomas`). TDTS keys; a real sheet had 24 and 48.
    pub head_dummy: usize,
    /// Dummy frames after the sheet ends (`footDummykomas`).
    pub foot_dummy: usize,
    /// The sheet's written direction — the line of text above the columns,
    /// such as `◇PAN A→B 全尺`.
    ///
    /// Like [`Sheet::episode`], this has a place in TDTS and none in XDTS, so
    /// it survives one format and not the other.
    pub direction: String,
}

/// One of the work slip's rows of names: who did a process, and in what
/// role.
///
/// A saved sheet has three of these, blank or not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Process {
    /// 作監補 / assistant.
    pub assistant: String,
    /// 作監 / chief.
    pub chief: String,
    /// 演出 / director.
    pub director: String,
    /// The free row's name.
    pub free: String,
    /// What the free row is called.
    pub free_role: String,
}

/// One artist and the number of sheets they drew.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkTrack {
    /// Who drew them.
    pub artist: String,
    /// 枚数 — how many sheets.
    pub sheets: u32,
}

/// One of the work slip's two tables of artists, and whether it is in use.
///
/// A saved sheet has seven [`WorkTrack`]s in each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    /// Whether the application shows this table.
    pub enable: bool,
    /// The artists, seven rows of them in a saved sheet.
    pub tracks: Vec<WorkTrack>,
}

/// The work slip (作業伝票) a TDTS sheet carries: the memo, who worked on it
/// and how many sheets they drew.
///
/// A saved sheet always has the whole form, blank or not — three
/// [`Process`]es and two [`Work`]s of seven tracks — and the writer fills out
/// whatever this does not have, because a file without the form is not the
/// format. XDTS has no work slip, so it survives one format and not the
/// other.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkSlip {
    /// The free note at the top of the slip.
    pub memo: String,
    /// Who did each process. A saved sheet has three.
    pub processes: Vec<Process>,
    /// Who drew how much. A saved sheet has two.
    pub works: Vec<Work>,
}

impl WorkSlip {
    /// Whether anything has been written on it.
    pub fn is_empty(&self) -> bool {
        self.memo.is_empty()
            && self.processes.iter().all(|p| p == &Process::default())
            && self
                .works
                .iter()
                .all(|w| !w.enable && w.tracks.iter().all(|t| t == &WorkTrack::default()))
    }
}

/// A whole document: the sheet header and its time tables.
///
/// `episode` has a place in TDTS and none in XDTS, so it survives one format
/// and not the other.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    /// Cut number, as whoever wrote it wrote it.
    pub cut: String,
    /// Scene number, likewise.
    pub scene: String,
    /// Episode. A TDTS key with no XDTS counterpart.
    pub episode: String,
    /// The name on the sheet. Written to `operatorName`, a key no published
    /// schema has.
    pub operator: String,
    /// The sheets of this cut.
    pub tables: Vec<TimeTable>,
    /// The work slip. A TDTS key with no XDTS counterpart.
    pub work_slip: WorkSlip,
    /// Whether the application shows the dummy frames at the head of the
    /// sheet (`showHeadDummy`). A TDTS key.
    pub show_head_dummy: bool,
    /// The six text colours a cell's [`Cell::font_color`] indexes into, as
    /// RGB. Empty means the application's own six, which is what every sheet
    /// seen so far has. A TDTS key.
    pub font_colors: Vec<[u8; 3]>,
    /// The TDTS version this sheet was read from, or 0 for a sheet that was
    /// not read from one. See [`tdts::to_value`] for what is written back.
    pub version: u32,
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
        assert_eq!(t.value_at(0), Some("1"));
        assert_eq!(t.value_at(5), Some("1"));
        assert_eq!(t.value_at(6), Some("2"));
        assert_eq!(t.value_at(99), Some("2"));
        assert_eq!(Track::default().value_at(0), None);
    }

    /// The marks on a drawing do not change which drawing it is, so a cell
    /// compares equal to its value and the options ride along beside it.
    #[test]
    fn a_marked_cell_is_still_its_value() {
        let t = Track {
            frames: vec![(0, Cell::marked("1", [OPTION_KEYFRAME])), (6, "2".into())],
        };
        assert_eq!(t.value_at(0), Some("1"));
        assert!(t.at(0).unwrap().has(OPTION_KEYFRAME));
        assert_eq!(t.at(0).unwrap(), "1");
        assert!(t.has_options());

        // And a plain cell carries none, which is every sheet the published
        // format can describe.
        assert!(t.at(6).unwrap().options.is_empty());
        assert!(!Track {
            frames: vec![(0, "1".into())]
        }
        .has_options());
    }

    /// A column that starts late holds nothing before its first change.
    #[test]
    fn nothing_is_exposed_before_the_first_change() {
        let t = Track {
            frames: vec![(4, "1".into())],
        };
        assert_eq!(t.value_at(3), None);
        assert_eq!(t.value_at(4), Some("1"));
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
                ..TimeTable::default()
            }],
            ..Sheet::default()
        };
        let o = WriteOptions::default();
        assert_eq!(from_str(&xdts::to_string(&s, &o)).unwrap().cut, "7");
        assert_eq!(from_str(&tdts::to_string(&s, &o)).unwrap().cut, "7");
        assert!(from_str("something else\n{}").is_err());
    }
}
