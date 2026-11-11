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
//! The published schema requires `timeTables` and `version` and makes
//! `header` optional. Inside, `duration` has a minimum of 1, `fieldId` must
//! be 0, 3 or 5, `data[].id` must be 0, and `values` needs at least one
//! element. `header.cut` and `header.scene` are `\d{1,4}`, which is stricter
//! than files in the wild — this crate writes what conforms and leaves the
//! rest out, rather than producing a file a validator rejects.
//!
//! # Versions
//!
//! Two versions are published, and `version` is the one key that tells them
//! apart:
//!
//! | version | document | what it adds |
//! |---|---|---|
//! | 5 | *XDTS File Format*, 2018-11-29 | — |
//! | 10 | *XDTS File Format*, 2022-12-06 | camerawork carries the camera position |
//!
//! Nothing else in the schema changed: the 2022 document is the 2018 one with
//! `version` widened to `[5, 10]` and the camerawork field's `values` grown
//! from the direction alone to the direction plus six numbers (position X and
//! Y, magnification percentage, rotation angle, centre X and Y).
//!
//! So the version is not a choice but a consequence, and [`version`] reads it
//! off the sheet: 10 once a camera move has coordinates, 5 otherwise, which
//! keeps a sheet that does not need the newer format readable by the readers
//! that only have the older one. [`from_str`] accepts both, and does not
//! refuse a version it has not heard of — version 11 files exist in the wild
//! and their time tables are the same shape.
//!
//! All three fields are written: 0 (cells), and 3 (dialogue) and 5
//! (camerawork) where the sheet has them.

use crate::json::{self, Value};
use crate::{
    Error, Field, Sheet, TimeTable, Track, ValueTrack, WriteOptions, FIELD_CAMERA, FIELD_CELL,
    FIELD_DIALOG,
};

pub const HEADER: &str = "exchangeDigitalTimeSheet Save Data";

/// The 2018 version, and all a sheet without camera coordinates needs.
pub const VERSION: u32 = 5;

/// The 2022 version, which is version 5 plus the camera position on a
/// camerawork instruction.
pub const VERSION_CAMERA: u32 = 10;

/// The lower published version that can hold this sheet: [`VERSION_CAMERA`]
/// if any camera move carries a position, [`VERSION`] otherwise.
pub fn version(sheet: &Sheet) -> u32 {
    if sheet.tables.iter().any(|t| t.camera.has_extra_values()) {
        VERSION_CAMERA
    } else {
        VERSION
    }
}

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

/// One `frames[]` entry. `values` is the whole list the field's instruction
/// takes: one string for a cell, up to two for dialogue, up to seven for a
/// camera move in version 10.
pub(crate) fn frame_entry(frame: usize, values: &[String], extra: Option<(&str, u32)>) -> Value {
    let mut data = Value::obj();
    // TDTS puts a colour before the instruction type; the key order is the
    // order the files have.
    if let Some((key, v)) = extra {
        data = data.with(key, v);
    }
    let values: Vec<Value> = values.iter().map(|v| Value::from(v.as_str())).collect();
    Value::obj()
        .with(
            "data",
            Value::Array(vec![data
                .with("id", 0u32)
                // The schema asks for at least one value, and a frame with
                // nothing in it has no business being a change point.
                .with("values", Value::Array(values))]),
        )
        .with("frame", frame)
}

/// The `tracks[]` of one field, numbered from the bottom layer up.
pub(crate) fn field_tracks<'a>(frames: impl Iterator<Item = Vec<Value>> + 'a) -> Value {
    Value::Array(
        frames
            .enumerate()
            .map(|(i, frames)| {
                Value::obj()
                    .with("frames", Value::Array(frames))
                    .with("trackNo", i)
            })
            .collect(),
    )
}

/// The cell field's frame entries, one list per column.
pub(crate) fn cell_frames<'a>(
    t: &'a TimeTable,
    extra: Option<(&'a str, u32)>,
) -> impl Iterator<Item = Vec<Value>> + 'a {
    t.tracks.iter().map(move |tr| {
        tr.frames
            .iter()
            .map(|(f, v)| frame_entry(*f, std::slice::from_ref(v), extra))
            .collect()
    })
}

/// A dialogue or camerawork field's frame entries, one list per column.
pub(crate) fn value_frames<'a>(
    f: &'a Field,
    extra: Option<(&'a str, u32)>,
) -> impl Iterator<Item = Vec<Value>> + 'a {
    f.tracks.iter().map(move |tr| {
        tr.frames
            .iter()
            .map(|(fr, v)| frame_entry(*fr, v, extra))
            .collect()
    })
}

fn field(id: u32, tracks: Value) -> Value {
    Value::obj().with("fieldId", id).with("tracks", tracks)
}

fn names(columns: &[String]) -> Value {
    Value::Array(columns.iter().map(|n| Value::from(n.as_str())).collect())
}

fn time_table(t: &TimeTable, sheet: &Sheet) -> Value {
    // Field 0 is always there; the other two are written only where the sheet
    // has something to put in them, so a cells-only sheet is byte for byte
    // what it was before this crate could carry the rest.
    let mut fields = vec![field(FIELD_CELL, field_tracks(cell_frames(t, None)))];
    let mut headers = vec![Value::obj()
        .with("fieldId", FIELD_CELL)
        .with("names", names(&t.columns))];
    for (id, f) in [(FIELD_DIALOG, &t.dialogue), (FIELD_CAMERA, &t.camera)] {
        if f.is_empty() {
            continue;
        }
        fields.push(field(id, field_tracks(value_frames(f, None))));
        headers.push(
            Value::obj()
                .with("fieldId", id)
                .with("names", names(&f.columns)),
        );
    }
    let mut v = Value::obj()
        // The schema asks for at least one frame.
        .with("duration", t.duration.max(1))
        .with("fields", Value::Array(fields))
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
    v.with("timeTableHeaders", Value::Array(headers))
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
        .with("version", version(sheet))
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

/// The `names` of one `timeTableHeaders[]` entry, or nothing if it is not the
/// field asked for.
fn header_names(h: &Value, id: u32) -> Option<Vec<String>> {
    if h.get("fieldId").and_then(|f| f.as_f64()) != Some(id as f64) {
        return None;
    }
    Some(
        h.get("names")
            .and_then(|n| n.as_array())
            .unwrap_or(&[])
            .iter()
            .map(|n| n.to_text())
            .collect(),
    )
}

/// One field's columns, in track order, with the highest frame number seen.
///
/// `trackNo` is what orders the columns, not the order the array has them in,
/// because that is what the column names are indexed by.
fn tracks_of(tt: &Value, id: u32) -> (Vec<ValueTrack>, usize) {
    let mut decoded: Vec<(usize, ValueTrack)> = Vec::new();
    let mut max_frame = 0;
    for field in tt.get("fields").and_then(|f| f.as_array()).unwrap_or(&[]) {
        if field.get("fieldId").and_then(|f| f.as_f64()) != Some(id as f64) {
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
                let values: Vec<String> = fr
                    .get("data")
                    .and_then(|d| d.as_array())
                    .and_then(|d| d.first())
                    .and_then(|d| d.get("values"))
                    .and_then(|v| v.as_array())
                    .unwrap_or(&[])
                    .iter()
                    .map(|v| v.to_text())
                    .collect();
                max_frame = max_frame.max(f);
                frames.push((f, values));
            }
            frames.sort_by_key(|(f, _)| *f);
            decoded.push((no, ValueTrack { frames }));
        }
    }
    decoded.sort_by_key(|d| d.0);
    (decoded.into_iter().map(|(_, t)| t).collect(), max_frame)
}

/// One field, with its column names.
fn field_from_value(tt: &Value, id: u32) -> (Field, usize) {
    let columns = tt
        .get("timeTableHeaders")
        .and_then(|h| h.as_array())
        .unwrap_or(&[])
        .iter()
        .find_map(|h| header_names(h, id))
        .unwrap_or_default();
    let (tracks, max_frame) = tracks_of(tt, id);
    (Field { columns, tracks }, max_frame)
}

/// Decode one `timeTables[]` entry, all three fields of it.
///
/// Cells become [`Track`]s, since one change point is one string there;
/// dialogue and camerawork keep the whole list the file gave, which for
/// camerawork in a version 10 file is the camera position as well as the
/// direction.
pub fn time_table_from_value(tt: &Value) -> TimeTable {
    let duration = tt.get("duration").and_then(|d| d.as_f64()).unwrap_or(0.0) as usize;
    let (cells, cell_end) = field_from_value(tt, FIELD_CELL);
    let (dialogue, dialogue_end) = field_from_value(tt, FIELD_DIALOG);
    let (camera, camera_end) = field_from_value(tt, FIELD_CAMERA);
    let max_frame = duration.max(cell_end).max(dialogue_end).max(camera_end);
    TimeTable {
        name: tt.get("name").map(|n| n.to_text()).unwrap_or_default(),
        duration: if duration > 0 { duration } else { max_frame },
        columns: cells.columns,
        // A cell holds one value; anything past the first is not something
        // the format puts in this field.
        tracks: cells
            .tracks
            .into_iter()
            .map(|t| Track {
                frames: t
                    .frames
                    .into_iter()
                    .map(|(f, mut v)| {
                        (
                            f,
                            if v.is_empty() {
                                String::new()
                            } else {
                                v.swap_remove(0)
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
        dialogue,
        camera,
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
