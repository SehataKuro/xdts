//! XDTS — "eXchange Digital Time Sheet" (CELSYS with 東映アニメーション, 2018).
//!
//! A UTF-8 text file: the fixed header line [`HEADER`] followed by a JSON
//! document.
//!
//! ```text
//! exchangeDigitalTimeSheet Save Data
//! {
//!   "header": { "cut": "1", "scene": "1" },
//!   "timeTables": [{
//!     "duration": 72,
//!     "fields": [{ "fieldId": 0, "tracks": [{
//!         "frames": [ { "data": [{ "id": 0, "values": ["1"] }], "frame": 0 }, ... ],
//!         "trackNo": 0 }] }],
//!     "name": "cut001",
//!     "timeTableHeaders": [{ "fieldId": 0, "names": ["A", "B"] }]
//!   }],
//!   "version": 5
//! }
//! ```
//!
//! The published schema requires `timeTables` and `version` (which must be 5)
//! and makes `header` optional. Inside, `duration` has a minimum of 1,
//! `fieldId` must be 0, 3 or 5, `data[].id` must be 0, and `values` needs at
//! least one element. `header.cut` and `header.scene` are `\d{1,4}`, which is
//! stricter than files in the wild — this crate writes what conforms and
//! leaves the rest out, rather than producing a file a validator rejects.
//!
//! Only field 0 (cells) is written. Fields 3 (dialogue) and 5 (camerawork)
//! are read back where a file has them, so a round trip through this crate
//! does not silently drop them.

use crate::json::{self, Value};
use crate::{Error, Sheet, TimeTable, Track, WriteOptions, FIELD_CELL};

pub const HEADER: &str = "exchangeDigitalTimeSheet Save Data";
pub const VERSION: u32 = 5;

/// A cut or scene number as the schema will take it: empty, or one to four
/// digits.
///
/// Both fields are `\d{1,4}`, and a number read off paper or typed by hand is
/// often not (`C-112`, `no.3`). Keeping the digits is the useful half of such
/// a value; one that leaves no digits, or far too many to be a cut number, is
/// written as the empty string that files with an unknown field carry.
fn number_field(text: &str) -> String {
    if text.is_empty() || (text.len() <= 4 && text.bytes().all(|b| b.is_ascii_digit())) {
        return text.to_string();
    }
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 4 {
        String::new()
    } else {
        digits
    }
}

fn frame_entry(frame: usize, value: &str) -> Value {
    Value::obj()
        .with(
            "data",
            Value::Array(vec![Value::obj()
                .with("id", 0u32)
                .with("values", Value::Array(vec![value.into()]))]),
        )
        .with("frame", frame)
}

fn time_table(t: &TimeTable, sheet: &Sheet) -> Value {
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
    let mut v = Value::obj()
        // The schema asks for at least one frame.
        .with("duration", t.duration.max(1))
        .with(
            "fields",
            Value::Array(vec![Value::obj()
                .with("fieldId", FIELD_CELL)
                .with("tracks", Value::Array(tracks))]),
        )
        .with("name", t.name.as_str());
    // Not in the published schema and not written by every application, but
    // an unknown key is accepted and the alternative is dropping a name the
    // caller has in hand. Left out entirely when there is none, so a sheet
    // with no operator is byte for byte what the schema describes. It sits
    // here, in its alphabetical place, because that is the order the files
    // these are read beside put their keys in.
    if !sheet.operator.is_empty() {
        v = v.with("operatorName", sheet.operator.as_str());
    }
    v.with(
        "timeTableHeaders",
        Value::Array(vec![Value::obj()
            .with("fieldId", FIELD_CELL)
            .with("names", Value::Array(names))]),
    )
}

pub fn header(sheet: &Sheet) -> Value {
    Value::obj()
        .with("cut", number_field(&sheet.cut))
        .with("scene", number_field(&sheet.scene))
}

pub fn to_value(sheet: &Sheet) -> Value {
    let tables: Vec<Value> = sheet.tables.iter().map(|t| time_table(t, sheet)).collect();
    Value::obj()
        .with("header", header(sheet))
        .with("timeTables", Value::Array(tables))
        .with("version", VERSION)
}

pub fn to_string(sheet: &Sheet, opts: &WriteOptions) -> String {
    let v = to_value(sheet);
    let body = if opts.pretty {
        v.to_pretty()
    } else {
        v.to_compact()
    };
    format!("{HEADER}\n{body}\n")
}

/// Decode one `timeTables[]` entry.
///
/// Only field 0 is kept: a track here is a column of cell values, and the
/// dialogue and camerawork fields hold a different kind of thing. A file
/// that has them keeps them on disk; this crate neither reads nor rewrites
/// them.
pub fn time_table_from_value(tt: &Value) -> TimeTable {
    let duration = tt.get("duration").and_then(|d| d.as_f64()).unwrap_or(0.0) as usize;
    let mut columns: Vec<String> = Vec::new();
    for h in tt
        .get("timeTableHeaders")
        .and_then(|h| h.as_array())
        .unwrap_or(&[])
    {
        if h.get("fieldId").and_then(|f| f.as_f64()) == Some(FIELD_CELL as f64) {
            columns = h
                .get("names")
                .and_then(|n| n.as_array())
                .unwrap_or(&[])
                .iter()
                .map(|n| n.to_text())
                .collect();
        }
    }
    let mut decoded: Vec<(usize, Track)> = Vec::new();
    let mut max_frame = duration;
    for field in tt.get("fields").and_then(|f| f.as_array()).unwrap_or(&[]) {
        if field.get("fieldId").and_then(|f| f.as_f64()) != Some(FIELD_CELL as f64) {
            continue;
        }
        for tr in field
            .get("tracks")
            .and_then(|t| t.as_array())
            .unwrap_or(&[])
        {
            let no = tr
                .get("trackNo")
                .and_then(|n| n.as_f64())
                .unwrap_or(decoded.len() as f64) as usize;
            let mut frames = Vec::new();
            for fr in tr.get("frames").and_then(|f| f.as_array()).unwrap_or(&[]) {
                let Some(f) = fr.get("frame").and_then(|f| f.as_f64()) else {
                    continue;
                };
                if f < 0.0 {
                    continue;
                }
                let f = f as usize;
                let val = fr
                    .get("data")
                    .and_then(|d| d.as_array())
                    .and_then(|d| d.first())
                    .and_then(|d| d.get("values"))
                    .and_then(|v| v.as_array())
                    .and_then(|v| v.first())
                    .map(|v| v.to_text())
                    .unwrap_or_default();
                max_frame = max_frame.max(f);
                frames.push((f, val));
            }
            frames.sort_by_key(|(f, _)| *f);
            decoded.push((no, Track { frames }));
        }
    }
    decoded.sort_by_key(|d| d.0);
    TimeTable {
        name: tt.get("name").map(|n| n.to_text()).unwrap_or_default(),
        duration: if duration > 0 { duration } else { max_frame },
        columns,
        tracks: decoded.into_iter().map(|(_, t)| t).collect(),
    }
}

/// Decode a document body. Shared with [`crate::tdts`], whose time tables
/// have the same shape inside their wrapper.
pub fn from_value(v: &Value) -> Result<Sheet, Error> {
    let mut sheet = Sheet::default();
    if let Some(h) = v.get("header") {
        sheet.cut = h.get("cut").map(|c| c.to_text()).unwrap_or_default();
        sheet.scene = h.get("scene").map(|c| c.to_text()).unwrap_or_default();
        sheet.episode = h.get("episode").map(|c| c.to_text()).unwrap_or_default();
    }
    let tables = v
        .get("timeTables")
        .and_then(|t| t.as_array())
        .ok_or_else(|| Error::Parse("no timeTables".into()))?;
    if let Some(first) = tables.first() {
        sheet.operator = first
            .get("operatorName")
            .map(|o| o.to_text())
            .unwrap_or_default();
    }
    sheet.tables = tables.iter().map(time_table_from_value).collect();
    Ok(sheet)
}

pub fn from_str(text: &str) -> Result<Sheet, Error> {
    let text = text.trim_start_matches('\u{feff}');
    let (first, body) = text.split_once('\n').unwrap_or((text, ""));
    if first.trim() != HEADER {
        return Err(Error::Parse(format!("expected header line {HEADER:?}")));
    }
    from_value(&json::parse(body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cut` and `scene` are `\d{1,4}` in the published schema, and what an
    /// application has in hand often is not.
    #[test]
    fn the_header_numbers_are_what_the_schema_will_take() {
        for (raw, want) in [
            ("", ""),
            ("12", "12"),
            ("0001", "0001"),
            ("C-112", "112"),
            ("no.3", "3"),
            ("12345", ""),
            ("カット", ""),
        ] {
            assert_eq!(number_field(raw), want, "{raw:?}");
        }
    }

    #[test]
    fn an_empty_sheet_still_says_one_frame() {
        let t = time_table(&TimeTable::default(), &Sheet::default());
        assert_eq!(t.get("duration").and_then(|d| d.as_f64()), Some(1.0));
    }
}
