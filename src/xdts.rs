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
//! element. This crate writes all of that. `header.cut` and `header.scene`
//! are `\d{1,4}`, which files in the wild do not keep to — the sheet in
//! `tests/data/` has `"scene": ""` — and those two go out as the caller has
//! them: see [`header`].
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
//!
//! # The p1 extension
//!
//! One unofficial extension is carried as well, and it is the one with a
//! reader: `options` on a cell, holding [`crate::OPTION_KEYFRAME`] or
//! [`crate::OPTION_REFERENCEFRAME`], with `subversion: "p1"` on the document
//! to say which extension that is. OpenToonz writes and reads these keys —
//! they are where it keeps a sheet's circled and triangled numbers — and the
//! schema they come from is published at
//! <https://github.com/shun-iwasawa/XDTS-uext>.
//!
//! They are not in the official schema, so a sheet that does not use them is
//! written without either key and remains exactly what a validator expects:
//! see [`subversion`].

use crate::json::{self, Value};
use crate::{
    Cell, Error, Field, Sheet, TimeTable, Track, ValueTrack, WriteOptions, FIELD_CAMERA,
    FIELD_CELL, FIELD_DIALOG,
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

/// The `subversion` of the one unofficial extension this crate writes:
/// *Studio Ponoc extended version 1*.
///
/// It adds nothing but [`crate::OPTION_KEYFRAME`] and
/// [`crate::OPTION_REFERENCEFRAME`] on a cell, as an `options` array beside
/// that cell's `values`. OpenToonz reads and writes it — the key it keeps a
/// sheet's circled and triangled numbers in — so this is an extension with a
/// reader, not a guess.
///
/// `subversion` is not in the published schema, and a validator that refuses
/// unknown keys will refuse a file that has it. That is why it is written
/// only for a sheet that uses the marks: see [`subversion`].
pub const SUBVERSION_P1: &str = "p1";

/// `Some(SUBVERSION_P1)` if any cell in this sheet carries an option, and
/// `None` otherwise.
///
/// Like the version, the subversion is a consequence and not a choice. A
/// sheet with no marks on it is the published format and says nothing about
/// any extension; the moment one cell is a key, the file has to admit which
/// extension put it there. This is what OpenToonz does with the same keys.
pub fn subversion(sheet: &Sheet) -> Option<&'static str> {
    sheet
        .tables
        .iter()
        .flat_map(|t| &t.tracks)
        .any(Track::has_options)
        .then_some(SUBVERSION_P1)
}

/// A cut or scene number reduced to what an anchored reading of the schema
/// would take: one to four digits, or nothing.
///
/// This crate does **not** use it — [`header`] writes what the caller has —
/// and it is here for a caller who would rather hand a reader `112` than
/// `C-112`. See [`header`] for why that is the caller's decision and not
/// this crate's.
pub fn header_number(text: &str) -> String {
    if text.len() <= 4 && text.bytes().all(|b| b.is_ascii_digit()) {
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
/// camera move in version 10. `options` is the p1 extension's, and is left
/// out when there is none, which is every sheet the published format can
/// describe.
pub(crate) fn frame_entry(
    frame: usize,
    values: &[String],
    options: &[String],
    extra: Option<(&str, u32)>,
) -> Value {
    let mut data = Value::obj();
    // TDTS puts a colour before the instruction type; the keys are in
    // alphabetical order, which is the order the files have them in.
    if let Some((key, v)) = extra {
        data = data.with(key, v);
    }
    data = data.with("id", 0u32);
    if !options.is_empty() {
        data = data.with(
            "options",
            Value::Array(options.iter().map(|o| Value::from(o.as_str())).collect()),
        );
    }
    let values: Vec<Value> = values.iter().map(|v| Value::from(v.as_str())).collect();
    Value::obj()
        .with(
            "data",
            // The schema asks for at least one value, and a frame with
            // nothing in it has no business being a change point.
            Value::Array(vec![data.with("values", Value::Array(values))]),
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
///
/// `options` says whether a cell's marks may be written: they are the p1
/// extension of XDTS, and nothing says the application behind TDTS knows the
/// key, so that writer leaves them out.
pub(crate) fn cell_frames<'a>(
    t: &'a TimeTable,
    options: bool,
    extra: Option<(&'a str, u32)>,
) -> impl Iterator<Item = Vec<Value>> + 'a {
    t.tracks.iter().map(move |tr| {
        tr.frames
            .iter()
            .map(|(f, c)| {
                let marks: &[String] = if options { &c.options } else { &[] };
                frame_entry(*f, std::slice::from_ref(&c.value), marks, extra)
            })
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
            .map(|(fr, v)| frame_entry(*fr, v, &[], extra))
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
    let mut fields = vec![field(FIELD_CELL, field_tracks(cell_frames(t, true, None)))];
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

/// The cut and scene numbers, written as the caller has them.
///
/// The schema says both are `\d{1,4}`, and this crate used to reduce anything
/// else to its digits — `C-112` to `112` — or to the empty string. That was
/// wrong twice over.
///
/// A JSON Schema `pattern` is a search and not an anchored match, so
/// `C-112` and `12345` both satisfy `\d{1,4}` as written: the reduction threw
/// away values that were already valid. And for the one kind of value that
/// genuinely cannot conform, a number with no digits in it at all, the empty
/// string does not conform either — `""` does not match `\d{1,4}` — so the
/// reduction bought nothing there and lost the number as well. The sheet this
/// crate is checked against has `"scene": ""`, which is to say the
/// application writes a value its own pattern refuses.
///
/// What is left is a choice between handing a stricter-than-specified reader
/// something it may refuse, and silently changing a cut number. A refusal is
/// visible and can be fixed; a quietly rewritten number is the kind of thing
/// this crate exists because of. So the value goes out as it came in, and
/// [`header_number`] is there for a caller who wants the other behaviour.
pub fn header(sheet: &Sheet) -> Value {
    Value::obj()
        .with("cut", sheet.cut.as_str())
        .with("scene", sheet.scene.as_str())
}

pub fn to_value(sheet: &Sheet) -> Value {
    let tables: Vec<Value> = sheet.tables.iter().map(|t| time_table(t, sheet)).collect();
    let mut v = Value::obj().with("header", header(sheet));
    // Alphabetically between `header` and `timeTables`, which is where a
    // writer that sorts its keys — OpenToonz's does — puts it.
    if let Some(sub) = subversion(sheet) {
        v = v.with("subversion", sub);
    }
    v.with("timeTables", Value::Array(tables))
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

/// One decoded column before it is given to a field: `(frame, values,
/// options)` per change point, since `options` belongs to a cell and the
/// other two fields have no use for it.
type Column = Vec<(usize, Vec<String>, Vec<String>)>;

/// One field's columns, in track order, with the highest frame number seen.
///
/// `trackNo` is what orders the columns, not the order the array has them in,
/// because that is what the column names are indexed by.
fn tracks_of(tt: &Value, id: u32) -> (Vec<Column>, usize) {
    let mut decoded: Vec<(usize, Column)> = Vec::new();
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
                let data = fr
                    .get("data")
                    .and_then(|d| d.as_array())
                    .and_then(|d| d.first());
                let strings = |key: &str| -> Vec<String> {
                    data.and_then(|d| d.get(key))
                        .and_then(|v| v.as_array())
                        .unwrap_or(&[])
                        .iter()
                        .map(|v| v.to_text())
                        .collect()
                };
                max_frame = max_frame.max(f);
                // `options` is the p1 extension's and is simply absent from a
                // file that does not use it.
                frames.push((f, strings("values"), strings("options")));
            }
            frames.sort_by_key(|(f, ..)| *f);
            decoded.push((no, frames));
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
    let (decoded, max_frame) = tracks_of(tt, id);
    // Dialogue and camerawork have no marks on them, so the options go no
    // further here.
    let tracks = decoded
        .into_iter()
        .map(|frames| ValueTrack {
            frames: frames.into_iter().map(|(f, v, _)| (f, v)).collect(),
        })
        .collect();
    (Field { columns, tracks }, max_frame)
}

/// The cell field, whose change points are single values with the marks on
/// them.
fn cells_from_value(tt: &Value) -> (Vec<Track>, usize) {
    let (decoded, max_frame) = tracks_of(tt, FIELD_CELL);
    let tracks = decoded
        .into_iter()
        .map(|frames| Track {
            frames: frames
                .into_iter()
                .map(|(f, mut values, options)| {
                    // A cell holds one value; anything past the first is not
                    // something the format puts in this field.
                    let value = if values.is_empty() {
                        String::new()
                    } else {
                        values.swap_remove(0)
                    };
                    (f, Cell { value, options })
                })
                .collect(),
        })
        .collect();
    (tracks, max_frame)
}

/// Decode one `timeTables[]` entry, all three fields of it.
///
/// Cells become [`Track`]s of [`Cell`]s, since one change point is one value
/// there, with the p1 marks on it where a file has them; dialogue and
/// camerawork keep the whole list the file gave, which for camerawork in a
/// version 10 file is the camera position as well as the direction.
pub fn time_table_from_value(tt: &Value) -> TimeTable {
    let duration = tt.get("duration").and_then(|d| d.as_f64()).unwrap_or(0.0) as usize;
    let (tracks, cell_end) = cells_from_value(tt);
    let columns = tt
        .get("timeTableHeaders")
        .and_then(|h| h.as_array())
        .unwrap_or(&[])
        .iter()
        .find_map(|h| header_names(h, FIELD_CELL))
        .unwrap_or_default();
    let (dialogue, dialogue_end) = field_from_value(tt, FIELD_DIALOG);
    let (camera, camera_end) = field_from_value(tt, FIELD_CAMERA);
    let max_frame = duration.max(cell_end).max(dialogue_end).max(camera_end);
    TimeTable {
        name: tt.get("name").map(|n| n.to_text()).unwrap_or_default(),
        duration: if duration > 0 { duration } else { max_frame },
        columns,
        tracks,
        dialogue,
        camera,
        // A TDTS key, read here because both formats' time tables are decoded
        // by this one function. XDTS has no `direction` and simply has none.
        direction: tt.get("direction").map(|d| d.to_text()).unwrap_or_default(),
    }
}

/// Decode a document body. Shared with [`crate::tdts`], whose time tables
/// have the same shape inside their wrapper.
/// A sheet with its header filled in and no time tables yet.
///
/// `episode` is a TDTS key. Reading it here costs nothing and means the one
/// decoder serves both formats.
pub(crate) fn sheet_from_header(v: &Value) -> Sheet {
    let mut sheet = Sheet::default();
    if let Some(h) = v.get("header") {
        sheet.cut = h.get("cut").map(|c| c.to_text()).unwrap_or_default();
        sheet.scene = h.get("scene").map(|c| c.to_text()).unwrap_or_default();
        sheet.episode = h.get("episode").map(|c| c.to_text()).unwrap_or_default();
    }
    sheet
}

/// The `operatorName` on a time table, which is where both formats keep it.
pub(crate) fn operator_of(tt: &Value) -> String {
    tt.get("operatorName")
        .map(|o| o.to_text())
        .unwrap_or_default()
}

pub fn from_value(v: &Value) -> Result<Sheet, Error> {
    let mut sheet = sheet_from_header(v);
    let tables = v
        .get("timeTables")
        .and_then(|t| t.as_array())
        .ok_or_else(|| Error::Parse("no timeTables".into()))?;
    if let Some(first) = tables.first() {
        sheet.operator = operator_of(first);
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

    /// A cut number goes out as it came in. `C-112` is what someone wrote on
    /// the sheet, and it is not this crate's to shorten.
    #[test]
    fn the_header_numbers_are_the_callers_own() {
        for raw in ["", "12", "0001", "C-112", "no.3", "12345", "カット"] {
            let h = header(&Sheet {
                cut: raw.into(),
                scene: raw.into(),
                ..Sheet::default()
            });
            assert_eq!(h.get("cut").unwrap().to_text(), raw, "{raw:?}");
            assert_eq!(h.get("scene").unwrap().to_text(), raw, "{raw:?}");
        }
    }

    /// The reduction a caller can ask for instead. `""` is in here as the
    /// reason the writer no longer does this by itself: it does not match
    /// `\d{1,4}` any more than `カット` does, so turning one into the other
    /// buys no conformance and loses the number.
    #[test]
    fn header_number_reduces_what_it_can_and_empties_the_rest() {
        for (raw, want) in [
            ("", ""),
            ("12", "12"),
            ("0001", "0001"),
            ("C-112", "112"),
            ("no.3", "3"),
            ("12345", ""),
            ("カット", ""),
        ] {
            assert_eq!(header_number(raw), want, "{raw:?}");
        }
    }

    #[test]
    fn an_empty_sheet_still_says_one_frame() {
        let t = time_table(&TimeTable::default(), &Sheet::default());
        assert_eq!(t.get("duration").and_then(|d| d.as_f64()), Some(1.0));
    }
}
