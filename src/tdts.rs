//! TDTS — the native format of 東映アニメーション デジタルタイムシート.
//!
//! A UTF-8 text file: the fixed header line [`HEADER`] followed by JSON, with
//! **CRLF** line endings.
//!
//! ```text
//! toeiDigitalTimeSheet Save Data
//! { "timeSheets": [ { "header": {...}, "timeTables": [...] } ], "version": 7 }
//! ```
//!
//! There is no published specification. The structure written here was read
//! off a sheet the application itself saved, and `tests/formats.rs` holds
//! that file and checks this crate's output against it key for key.
//!
//! It is close to XDTS but **not** XDTS with a different header line:
//!
//! | | XDTS | TDTS |
//! |---|---|---|
//! | `version` | 5 | **7** |
//! | sheets | `timeTables` at the top | wrapped in **`timeSheets`** |
//! | line endings | LF | **CRLF** |
//! | `data[]` | `id`, `values` | plus **`fontColorId`** |
//! | `timeTableHeaders` | only the fields written | **every field 0..5** |
//! | sheet header | `cut`, `scene` | plus `episode`, `showHeaderDummy`, `timeTableFontColors`, `workSlip` |
//! | time table | `duration`, `name`, `fields`, `timeTableHeaders` | plus `books`, `color`, `direction` |
//!
//! Version 5 is the older form, with `timeTables` at the top level and no
//! wrapper, so `timeSheets` carrying `version: 5` is a shape no reader
//! expects. [`from_str`] accepts both.
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
//! field 0 where both have data; the sheet in `tests/data/` has its cells in
//! field 0 and nothing in field 4, which is what this crate writes.
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
use crate::{Error, Sheet, TimeTable, WriteOptions, FIELD_CAMERA, FIELD_CELL, FIELD_DIALOG};

pub const HEADER: &str = "toeiDigitalTimeSheet Save Data";

/// TDTS is version 7; see the module note on why this is not XDTS's 5.
pub const VERSION: u32 = 7;

/// The application's own text colours: black, red, green, blue, purple,
/// orange.
const FONT_COLORS: [[i64; 3]; 6] = [
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

/// A work slip with every field blank.
fn empty_work_slip() -> Value {
    let process = || {
        Value::obj()
            .with("assistant", "")
            .with("chief", "")
            .with("director", "")
            .with("free", "")
            .with("freeRole", "")
    };
    let work = || {
        let tracks: Vec<Value> = (0..7)
            .map(|_| Value::obj().with("artist", "").with("numberOfSheets", 0u32))
            .collect();
        Value::obj()
            .with("enable", false)
            .with("tracks", Value::Array(tracks))
    };
    Value::obj()
        .with("memo", "")
        .with(
            "processes",
            Value::Array((0..3).map(|_| process()).collect()),
        )
        .with("works", Value::Array((0..2).map(|_| work()).collect()))
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
    Value::Array(vec![
        Value::obj()
            .with("fieldId", 0u32)
            .with("names", names_or(&t.columns, &[])),
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
            .with("names", Value::Array(Vec::new())),
        Value::obj()
            .with("fieldId", 5u32)
            .with("names", names_or(&t.camera.columns, &["1", "2"])),
    ])
}

/// Unlike XDTS, `data` carries a `fontColorId` before the instruction type.
const FONT_COLOR: Option<(&str, u32)> = Some(("fontColorId", 0));

fn time_table(t: &TimeTable, sheet: &Sheet, index: usize) -> Value {
    let mut fields = vec![Value::obj().with("fieldId", FIELD_CELL).with(
        "tracks",
        xdts::field_tracks(xdts::cell_frames(t, false, FONT_COLOR)),
    )];
    for (id, f) in [(FIELD_DIALOG, &t.dialogue), (FIELD_CAMERA, &t.camera)] {
        if f.is_empty() {
            continue;
        }
        fields.push(Value::obj().with("fieldId", id).with(
            "tracks",
            xdts::field_tracks(xdts::value_frames(f, FONT_COLOR)),
        ));
    }
    // Keys in the order a sheet saved by the application has them.
    Value::obj()
        .with(
            "books",
            Value::Array(vec![Value::obj()
                .with("fieldId", 0u32)
                .with("tracks", Value::Array(Vec::new()))]),
        )
        .with("color", 0u32)
        .with("direction", t.direction.as_str())
        .with("duration", t.duration.max(1))
        .with("fields", Value::Array(fields))
        // The application names its sheets sheet1, sheet2, …, which is what a
        // table with no name of its own gets. A name the caller has is not
        // overwritten with a number.
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
    let colors: Vec<Value> = FONT_COLORS
        .iter()
        .map(|c| Value::Array(c.iter().map(|&v| Value::from(v)).collect()))
        .collect();
    Value::obj()
        .with("cut", sheet.cut.as_str())
        .with("episode", sheet.episode.as_str())
        .with("scene", sheet.scene.as_str())
        .with("showHeaderDummy", false)
        .with("timeTableFontColors", Value::Array(colors))
        .with("workSlip", empty_work_slip())
}

pub fn to_value(sheet: &Sheet) -> Value {
    let tables: Vec<Value> = sheet
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| time_table(t, sheet, i))
        .collect();
    let ts = Value::obj()
        .with("header", header(sheet))
        .with("timeTables", Value::Array(tables));
    Value::obj()
        .with("timeSheets", Value::Array(vec![ts]))
        .with("version", VERSION)
}

pub fn to_string(sheet: &Sheet, opts: &WriteOptions) -> String {
    let v = to_value(sheet);
    let body = if opts.pretty {
        v.to_pretty()
    } else {
        v.to_compact()
    };
    // Sheets saved by the application are CRLF throughout.
    format!("{HEADER}\n{body}\n").replace('\n', "\r\n")
}

pub fn from_str(text: &str) -> Result<Sheet, Error> {
    let text = text.trim_start_matches('\u{feff}');
    let (first, body) = text.split_once('\n').unwrap_or((text, ""));
    if first.trim() != HEADER {
        return Err(Error::Parse(format!("expected header line {HEADER:?}")));
    }
    let v = json::parse(body)?;
    // Version 7 and later wrap the sheets; version 5 has them at the top.
    match v
        .get("timeSheets")
        .and_then(|t| t.as_array())
        .and_then(|t| t.first())
    {
        Some(ts) => xdts::from_value(ts),
        None => xdts::from_value(&v),
    }
}
