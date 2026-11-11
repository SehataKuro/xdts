//! TDTS — the native format of 東映アニメーション デジタルタイムシート.
//!
//! A UTF-8 text file with no BOM: the fixed header line [`HEADER`] followed
//! by JSON, LF line endings, four spaces to a level. Reading accepts CRLF
//! and a BOM as well.
//!
//! ```text
//! toeiDigitalTimeSheet Save Data
//! { "timeSheets": [ { "free": {}, "header": {...}, "timeTables": [...] } ], "version": 10 }
//! ```
//!
//! There is no published specification. What is written here was read off a
//! sheet the application itself saved: `tests/data/saved_v10.tdts` is that
//! shape, holding none of that sheet's contents, and `tests/formats.rs`
//! checks this crate's output against it key for key and reads it back byte
//! for byte.
//!
//! It is close to XDTS but **not** XDTS with a different header line:
//!
//! | | XDTS | TDTS |
//! |---|---|---|
//! | `version` | 5 or 10 | **10** |
//! | sheets | `timeTables` at the top | wrapped in **`timeSheets`**, beside a **`free`** |
//! | written as | LF, two spaces | LF, **four spaces** |
//! | `data[]` | `id`, `values` | plus **`fontColorId`** |
//! | `timeTableHeaders` | only the fields written | **every field 0..5** |
//! | sheet header | `cut`, `scene` | plus `episode`, `showHeadDummy`, `timeTableFontColors`, `workSlip` |
//! | time table | `duration`, `name`, `fields`, `timeTableHeaders` | plus `color`, `direction`, `headDummykomas`, `footDummykomas` |
//!
//! Version 5 is the older form, with `timeTables` at the top level and no
//! wrapper, so `timeSheets` carrying `version: 5` is a shape no reader
//! expects. [`from_str`] accepts both, and [`version`] says what is written
//! back: the version a file was read with, where that shape can be written.
//!
//! Two of those keys are worth a note, because this crate had them wrong
//! until a saved sheet turned up. It wrote `showHeaderDummy`, which no saved
//! sheet has — the key is `showHeadDummy`, the head of the sheet, as
//! `headDummykomas` counts its dummy frames — and it wrote a `books`, which
//! no saved sheet has either. Both came from following another editor's
//! sample file as though it were the format.
//!
//! # Sheets and revisions
//!
//! The `timeTables` inside one `timeSheet` are **revisions of that one
//! sheet**, not separate sheets: the application shows the latest and keeps
//! the earlier ones when it saves. Several sheets are several `timeSheets`.
//!
//! So [`from_str`] reads the last time table of each `timeSheet` as the sheet
//! and the ones before it into [`TimeTable::revisions`], and [`to_value`]
//! writes one `timeSheet` per time table with its history back in front of
//! it. A file with a history keeps it.
//!
//! The only description of this that could be found is a third party's, in
//! the DiTiS editor's notes on the format, and every sheet seen so far has
//! one revision in one `timeSheet`, so it does not settle the question.
//!
//! The sheet's `direction` — the written instruction above the columns — and
//! the time table's own `name` are kept as they were read. Writing a fixed
//! empty string and a sheet number in their place is how a real sheet loses
//! its camerawork note by passing through a converter.
//!
//! Cell data goes in field 0, exactly as in XDTS, and the dialogue and
//! camerawork fields are written where the sheet has them. A converter whose
//! TDTS column ids this crate was checked against calls field 0 ACTION and
//! field 4 CELL, and says the application shows field 4 in preference to
//! field 0 where both have data. So a sheet is read from field 4 when that
//! column has anything in it and from field 0 otherwise, and is written back
//! to the column it came from; a new sheet goes in field 0, which is where
//! the sheet in `tests/data/` has its cells. None of this is official, and
//! `tests/data/` has nothing in field 4, so the tests for it are built by
//! hand.
//!
//! The p1 extension's `options` and `subversion` are XDTS's, and nothing says
//! the application behind TDTS knows those keys, so a cell's marks are left
//! out here rather than written into a file that may be refused for having
//! them.
//!
//! Fields 1, 2 and 4 and the work slip are written as the empty structure the
//! application saves: this crate has nothing to put in them, and a file
//! without them is not the format.

use crate::json::{self, Value};
use crate::xdts;
use crate::{
    Error, Process, Sheet, TimeTable, Work, WorkSlip, WorkTrack, WriteOptions, FIELD_CAMERA,
    FIELD_CELL, FIELD_CELL_ALT, FIELD_DIALOG,
};

/// The first line of every TDTS file, which is how one is recognised.
pub const HEADER: &str = "toeiDigitalTimeSheet Save Data";

/// The version a sheet saved by the application carries, and what this crate
/// writes. See the module note on why this is not XDTS's 5.
pub const VERSION: u32 = 10;

/// The indent a sheet saved by the application has, in spaces per level.
const INDENT: usize = 4;

/// The application's own text colours: black, red, green, blue, purple,
/// orange.
const FONT_COLORS: [[u8; 3]; 6] = [
    [0, 0, 0],
    [224, 0, 0],
    [32, 128, 32],
    [32, 32, 192],
    [192, 32, 192],
    [255, 128, 32],
];

fn strings(v: &[&str]) -> Value {
    Value::Array(v.iter().map(|s| Value::from(*s)).collect())
}

/// The work slip, with the rest of the form filled out blank.
///
/// A saved sheet has three processes and two works of seven tracks whether
/// or not anyone wrote in them, so a shorter [`WorkSlip`] is padded rather
/// than written short: a file without the whole form is not the format.
fn work_slip(w: &WorkSlip) -> Value {
    let process = |p: Option<&Process>| {
        let d = Process::default();
        let p = p.unwrap_or(&d);
        Value::obj()
            .with("assistant", p.assistant.as_str())
            .with("chief", p.chief.as_str())
            .with("director", p.director.as_str())
            .with("free", p.free.as_str())
            .with("freeRole", p.free_role.as_str())
    };
    let work = |w: Option<&Work>| {
        let d = Work::default();
        let w = w.unwrap_or(&d);
        let tracks: Vec<Value> = (0..WORK_TRACKS)
            .map(|i| {
                let d = WorkTrack::default();
                let t = w.tracks.get(i).unwrap_or(&d);
                Value::obj()
                    .with("artist", t.artist.as_str())
                    .with("numberOfSheets", t.sheets)
            })
            .collect();
        Value::obj()
            .with("enable", w.enable)
            .with("tracks", Value::Array(tracks))
    };
    Value::obj()
        .with("memo", w.memo.as_str())
        .with(
            "processes",
            Value::Array(
                (0..PROCESSES)
                    .map(|i| process(w.processes.get(i)))
                    .collect(),
            ),
        )
        .with(
            "works",
            Value::Array((0..WORKS).map(|i| work(w.works.get(i))).collect()),
        )
}

/// One `workSlip` as the application lays it out.
const PROCESSES: usize = 3;
const WORKS: usize = 2;
const WORK_TRACKS: usize = 7;

/// Read a work slip back.
fn work_slip_from_value(v: &Value) -> WorkSlip {
    let text = |o: &Value, k: &str| o.get(k).map(|v| v.to_text()).unwrap_or_default();
    let array = |o: &Value, k: &str| o.get(k).and_then(|v| v.as_array()).unwrap_or(&[]).to_vec();
    WorkSlip {
        memo: text(v, "memo"),
        processes: array(v, "processes")
            .iter()
            .map(|p| Process {
                assistant: text(p, "assistant"),
                chief: text(p, "chief"),
                director: text(p, "director"),
                free: text(p, "free"),
                free_role: text(p, "freeRole"),
            })
            .collect(),
        works: array(v, "works")
            .iter()
            .map(|w| Work {
                enable: w.get("enable").and_then(|e| e.as_bool()).unwrap_or(false),
                tracks: array(w, "tracks")
                    .iter()
                    .map(|t| WorkTrack {
                        artist: text(t, "artist"),
                        sheets: t
                            .get("numberOfSheets")
                            .and_then(|n| n.as_index())
                            .unwrap_or(0),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Column names, or the application's own defaults for a field this sheet
/// has nothing in. Unlike XDTS, every field 0..=5 gets a header entry
/// whether or not it is used.
fn names_or(columns: &[String], default: &[&str]) -> Value {
    if columns.is_empty() {
        strings(default)
    } else {
        Value::Array(columns.iter().map(|n| Value::from(n.as_str())).collect())
    }
}

/// Headers for fields 0..=5. Fields 1, 2 and 4 are the application's
/// defaults: this crate has no model for them.
fn headers(t: &TimeTable) -> Value {
    // The cell names belong to whichever of the two cell columns this sheet
    // keeps its cells in; the other one gets the empty header it has in a
    // sheet that does not use it.
    let defaults: [&str; 7] = ["a", "b", "c", "d", "e", "f", "g"];
    let cells = |id: u32, default: &[&str]| {
        if t.cell_field == id {
            names_or(&t.columns, default)
        } else {
            names_or(&[], default)
        }
    };
    Value::Array(vec![
        Value::obj()
            .with("fieldId", 0u32)
            .with("names", cells(0, &[])),
        Value::obj()
            .with("fieldId", 1u32)
            .with("names", strings(&[""])),
        Value::obj()
            .with("fieldId", 2u32)
            .with("names", strings(&[""])),
        Value::obj()
            .with("fieldId", 3u32)
            .with("names", names_or(&t.dialogue.columns, &["S1", "S2"])),
        Value::obj()
            .with("fieldId", 4u32)
            .with("names", cells(4, &defaults)),
        Value::obj()
            .with("fieldId", 5u32)
            .with("names", names_or(&t.camera.columns, &["1", "2"])),
    ])
}

fn time_table(t: &TimeTable, sheet: &Sheet, index: usize) -> Value {
    // Back into the column the cells came from: a sheet read out of field 4
    // is a sheet whose cells the application looks for in field 4.
    let cell_field = if t.cell_field == FIELD_CELL_ALT {
        FIELD_CELL_ALT
    } else {
        FIELD_CELL
    };
    let mut fields = vec![Value::obj().with("fieldId", cell_field).with(
        "tracks",
        xdts::field_tracks(xdts::cell_frames(t, xdts::TDTS_DIALECT), xdts::TDTS_DIALECT),
    )];
    for (id, f) in [(FIELD_DIALOG, &t.dialogue), (FIELD_CAMERA, &t.camera)] {
        if f.is_empty() {
            continue;
        }
        fields.push(Value::obj().with("fieldId", id).with(
            "tracks",
            xdts::field_tracks(
                xdts::value_frames(f, xdts::TDTS_DIALECT),
                xdts::TDTS_DIALECT,
            ),
        ));
    }
    // Keys in the order a sheet saved by the application has them.
    Value::obj()
        .with("color", t.color)
        .with("direction", t.direction.as_str())
        .with("duration", t.duration.max(1))
        .with("fields", Value::Array(fields))
        .with("footDummykomas", t.foot_dummy)
        // The application names its sheets sheet1, sheet2, …, which is what a
        // table with no name of its own gets. A name the caller has is not
        // overwritten with a number.
        .with("headDummykomas", t.head_dummy)
        .with(
            "name",
            if t.name.is_empty() {
                format!("sheet{}", index + 1)
            } else {
                t.name.clone()
            },
        )
        .with("operatorName", sheet.operator.as_str())
        .with("timeTableHeaders", headers(t))
}

/// The sheet header. Unlike XDTS this has somewhere to put the episode.
pub fn header(sheet: &Sheet) -> Value {
    // The application's own six colours, unless the sheet came with others.
    let colors: Vec<Value> = if sheet.font_colors.is_empty() {
        FONT_COLORS
            .iter()
            .map(|c| Value::Array(c.iter().map(|&v| Value::from(v as i64)).collect()))
            .collect()
    } else {
        sheet
            .font_colors
            .iter()
            .map(|c| Value::Array(c.iter().map(|&v| Value::from(v as i64)).collect()))
            .collect()
    };
    Value::obj()
        .with("cut", sheet.cut.as_str())
        .with("episode", sheet.episode.as_str())
        .with("scene", sheet.scene.as_str())
        // `showHeadDummy`, not `showHeaderDummy`: the dummy frames at the
        // head of the sheet, as `headDummykomas` below counts them.
        .with("showHeadDummy", sheet.show_head_dummy)
        .with("timeTableFontColors", Value::Array(colors))
        .with("workSlip", work_slip(&sheet.work_slip))
}

/// One `timeSheets[]` entry: a sheet, and the one revision of it this crate
/// has.
fn time_sheet(sheet: &Sheet, tables: Vec<Value>) -> Value {
    Value::obj()
        // Empty in a saved sheet, and this crate has nothing to put in it.
        .with("free", Value::obj())
        .with("header", header(sheet))
        .with("timeTables", Value::Array(tables))
}

/// The whole document, ready to be written.
pub fn to_value(sheet: &Sheet) -> Value {
    // One `timeSheet` per time table, because the `timeTables` inside one of
    // them are revisions of a single sheet and not sheets of their own.
    // Putting several tables in one would say they are the same sheet saved
    // over and over, and an application that shows only the latest would show
    // one of them and hide the rest.
    let mut sheets: Vec<Value> = sheet
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| {
            // The history first, the current sheet last, which is the order
            // the application saves them in and reads them back by.
            let mut tables: Vec<Value> = t
                .revisions
                .iter()
                .map(|r| time_table(r, sheet, i))
                .collect();
            tables.push(time_table(t, sheet, i));
            time_sheet(sheet, tables)
        })
        .collect();
    // A sheet with nothing in it is still a document with a header.
    if sheets.is_empty() {
        sheets.push(time_sheet(sheet, Vec::new()));
    }
    Value::obj()
        .with("timeSheets", Value::Array(sheets))
        .with("version", version(sheet))
}

/// The version to write: the one the sheet was read with, where this crate
/// can write that shape, and [`VERSION`] otherwise.
///
/// Versions 7 and up all wrap their sheets in `timeSheets`, and nothing
/// shows that a saved version 7 sheet has different keys from a saved
/// version 10 one, so a file read as 7 is written back as 7 rather than
/// silently moved forward. Version 5 is a different shape — the time tables
/// at the top level, no wrapper — which this crate reads and does not write,
/// so a version 5 file comes back as 10. That is an upgrade, and saying 5
/// over the wrapper would be the exact lie this crate exists to prevent.
pub fn version(sheet: &Sheet) -> u32 {
    if sheet.version >= 7 {
        sheet.version
    } else {
        VERSION
    }
}

/// The whole file: the header line, the JSON, LF throughout.
pub fn to_string(sheet: &Sheet, opts: &WriteOptions) -> String {
    let v = to_value(sheet);
    let body = if opts.pretty {
        v.to_pretty_indent(INDENT)
    } else {
        v.to_compact()
    };
    // LF, as a sheet saved by the application is: it has no BOM and no CR
    // either, and this crate used to write CRLF on the strength of nothing.
    format!("{HEADER}\n{body}\n")
}

/// Read a TDTS file, with or without a BOM and with either kind of line
/// ending.
///
/// Fails if the first line is not [`HEADER`], or the JSON does not hold a
/// sheet.
pub fn from_str(text: &str) -> Result<Sheet, Error> {
    let text = text.trim_start_matches('\u{feff}');
    let (first, body) = text.split_once('\n').unwrap_or((text, ""));
    if first.trim() != HEADER {
        return Err(Error::Parse(format!("expected header line {HEADER:?}")));
    }
    let v = json::parse(body)?;
    // Version 5 has the time tables at the top level, with no wrapper, and is
    // then the same document XDTS has.
    let file_version = v.get("version").and_then(|n| n.as_index()).unwrap_or(0);
    let Some(sheets) = v.get("timeSheets").and_then(|t| t.as_array()) else {
        let mut sheet = xdts::from_value(&v)?;
        sheet.version = file_version;
        return Ok(sheet);
    };
    let Some(first) = sheets.first() else {
        return Err(Error::Parse("timeSheets is empty".into()));
    };
    // Every sheet in the file is read, one time table each. The header is the
    // first sheet's: this crate has one header per document, and the sheets of
    // a file are the sheets of a cut.
    let mut sheet = xdts::sheet_from_header(first);
    sheet.version = file_version;
    if let Some(h) = first.get("header") {
        sheet.show_head_dummy = h
            .get("showHeadDummy")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        if let Some(w) = h.get("workSlip") {
            sheet.work_slip = work_slip_from_value(w);
        }
        // Only when they are not the ones the application always writes, so
        // that a sheet with the usual colours stays a sheet with no colours
        // of its own.
        // All or nothing: a cell's colour is an index into this list, so
        // dropping one entry that does not read would quietly recolour every
        // cell after it.
        let colors: Option<Vec<[u8; 3]>> = h
            .get("timeTableFontColors")
            .and_then(|c| c.as_array())
            .unwrap_or(&[])
            .iter()
            .map(|c| {
                let c = c.as_array()?;
                let get = |i: usize| u8::try_from(c.get(i)?.as_index()?).ok();
                Some([get(0)?, get(1)?, get(2)?])
            })
            .collect();
        if let Some(colors) = colors.filter(|c| !c.is_empty() && c[..] != FONT_COLORS) {
            sheet.font_colors = colors;
        }
    }
    for ts in sheets {
        let revisions = ts
            .get("timeTables")
            .and_then(|t| t.as_array())
            .unwrap_or(&[]);
        // The last revision is the current sheet; the ones before it are the
        // history the application keeps, and are kept here too so that saving
        // the file again does not throw them away.
        if let Some((latest, earlier)) = revisions.split_last() {
            if sheet.operator.is_empty() {
                sheet.operator = xdts::operator_of(latest);
            }
            let mut table = xdts::time_table_from_value(latest);
            table.revisions = earlier.iter().map(xdts::time_table_from_value).collect();
            sheet.tables.push(table);
        }
    }
    Ok(sheet)
}
