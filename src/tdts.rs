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
//! Cell data goes in field 0, exactly as in XDTS. The dialogue and camerawork
//! columns and the work slip are written empty: this crate has nothing to put
//! in them, and the application expects the structure to be there.

use crate::json::{self, Value};
use crate::xdts;
use crate::{Error, Sheet, TimeTable, WriteOptions, FIELD_CELL};

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
        .with("processes", Value::Array((0..3).map(|_| process()).collect()))
        .with("works", Value::Array((0..2).map(|_| work()).collect()))
}

/// Headers for fields 0..=5. Only field 0 carries real names; the rest are
/// the application's defaults for the columns this crate does not fill.
fn headers(names: Vec<Value>) -> Value {
    Value::Array(vec![
        Value::obj()
            .with("fieldId", 0u32)
            .with("names", Value::Array(names)),
        Value::obj().with("fieldId", 1u32).with("names", strings(&[""])),
        Value::obj().with("fieldId", 2u32).with("names", strings(&[""])),
        Value::obj()
            .with("fieldId", 3u32)
            .with("names", strings(&["S1", "S2"])),
        Value::obj()
            .with("fieldId", 4u32)
            .with("names", Value::Array(Vec::new())),
        Value::obj()
            .with("fieldId", 5u32)
            .with("names", strings(&["1", "2"])),
    ])
}

/// One frame entry. Unlike XDTS, `data` carries a `fontColorId`.
fn frame_entry(frame: usize, value: &str) -> Value {
    Value::obj()
        .with(
            "data",
            Value::Array(vec![Value::obj()
                .with("fontColorId", 0u32)
                .with("id", 0u32)
                .with("values", Value::Array(vec![value.into()]))]),
        )
        .with("frame", frame)
}

fn time_table(t: &TimeTable, sheet: &Sheet, index: usize) -> Value {
    let tracks: Vec<Value> = t
        .tracks
        .iter()
        .enumerate()
        .map(|(i, tr)| {
            let frames: Vec<Value> = tr
                .frames
                .iter()
                .map(|(f, v)| frame_entry(*f, v))
                .collect();
            Value::obj()
                .with("frames", Value::Array(frames))
                .with("trackNo", i)
        })
        .collect();
    let names: Vec<Value> = t.columns.iter().map(|n| Value::from(n.as_str())).collect();
    // Keys in the order a sheet saved by the application has them.
    Value::obj()
        .with(
            "books",
            Value::Array(vec![Value::obj()
                .with("fieldId", 0u32)
                .with("tracks", Value::Array(Vec::new()))]),
        )
        .with("color", 0u32)
        .with("direction", "")
        .with("duration", t.duration.max(1))
        .with(
            "fields",
            Value::Array(vec![Value::obj()
                .with("fieldId", FIELD_CELL)
                .with("tracks", Value::Array(tracks))]),
        )
        .with("name", format!("sheet{}", index + 1))
        .with("operatorName", sheet.operator.as_str())
        .with("timeTableHeaders", headers(names))
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
