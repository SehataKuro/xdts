//! DiTiS — the native file of DiTiS (`.ditis`, or `.json`), which holds
//! everything the application knows about a sheet.
//!
//! DiTiS is an open-source digital timesheet for compositing. Its own file is
//! JSON with no header line, as `buildDitisJson` and `loadSheetData` in
//! DiTiS's `js/fileOperations.js` write and read it:
//!
//! ```text
//! {
//!   "version": "1.0",
//!   "sheets": [
//!     {
//!       "name": "Sheet1",
//!       "fps": 24,
//!       "layers": ["A", "B", …],          // column names
//!       "frames": 144,
//!       "framePageSize": 144,
//!       "data": [["1", "1", "2", …], …],  // per column, one value per frame
//!       "insertedFrames": [],
//!       "disabledFrames": []
//!     }
//!   ]
//! }
//! ```
//!
//! Unlike the exchange formats, a column stores **every frame**, not the
//! changes: `"1", "1", "2"` is drawing 1 for two frames, then 2. A column
//! stops at its last value and the list of columns at its last non-empty
//! one. The values are DiTiS's own: a drawing as written, `×` for a frame
//! marked empty, `○` and `●` for the in-between marks, `""` for nothing, and
//! `-`, which older versions wrote to mean "as the frame before". Files from
//! before the column lists (`layers` of `{id, name}` and `data` keyed by
//! frame, then layer id) are read too.
//!
//! # From frames to changes, and back
//!
//! Reading works out the changes the way DiTiS's own XDTS export does
//! (`buildTimesheetTable` in `js/parsers.js`): `×` and `""` are both
//! [`NULL_CELL`], `○` and `●` are [`TICK_1`] and [`TICK_2`], `-` is
//! [`HYPHEN`], and a change is recorded where the value differs from the one
//! before. Writing goes the way DiTiS's import does (`importExchangeTimesheet`)
//! — each value fills the frames up to the next change, [`NULL_CELL`] as `×`
//! and [`HYPHEN`] as the drawing it continues — except that empty frames at
//! the end of a column are left out rather than filled with `×`, as DiTiS
//! itself saves a column, so that a DiTiS file read and written again is the
//! same sheet. A sheet saved as DiTiS creates one comes back byte for byte;
//! one DiTiS imported from XDTS or TDTS comes back with `frames` in a new
//! sheet's place and the `×` that filled a column to its end gone, which is
//! the same sheet to DiTiS.
//!
//! # Everything else
//!
//! A DiTiS sheet carries a good deal this crate does not model — the frame
//! rate, the page size, inserted and disabled rows, notes, and
//! `timesheetSource`, the XDTS or TDTS document a sheet was imported from,
//! which is how DiTiS keeps dialogue, camerawork and colours it has no editor
//! for. All of it is kept as it was read, in [`TimeTable::extra`] and
//! [`Sheet::extra`], and written back into a DiTiS file, and only into one:
//! see [`Sheet::read_from`]. A sheet that did not come from DiTiS is written
//! with what a new DiTiS sheet has (24 fps, one page, no inserted or
//! disabled rows). DiTiS has only cell columns, so a sheet's dialogue and
//! camerawork fields, and a cell's p1 marks, are not written.
//!
//! # Byte for byte
//!
//! A DiTiS file read and written again is the file it was, byte for byte,
//! whichever way DiTiS made the sheet. Two things make that so, and both
//! are about a sheet read from DiTiS:
//!
//! * A sheet's [`TimeTable::extra`] holds **every** key of the sheet, in the
//!   file's order — the four this crate models (`name`, `frames`, `layers`,
//!   `data`) as well — and the writer goes through them in that order. The
//!   values of those four in the extras are not used for anything but this:
//!   what is written for them comes from the model.
//! * `data` is written back exactly as it was read while the cells are
//!   unchanged — the same length and columns, and the same changes in every
//!   column. That keeps what the changes cannot say: whether an empty frame
//!   was `×` or `""`, a `-` where a drawing goes on. Once a cell is changed
//!   the whole of `data` is written from the model, as above.
//!
//! The extras of the other formats never hold a modelled key, and their
//! writers would not write one twice.

use crate::json::{self, Value};
use crate::{
    Error, Format, Sheet, TimeTable, Track, WriteOptions, HYPHEN, NULL_CELL, TICK_1, TICK_2,
};

/// The version a DiTiS file is written with when the sheet did not come
/// from one.
pub const VERSION: &str = "1.0";

/// The marks DiTiS writes in a cell for the symbols.
pub const EMPTY_MARK: &str = "×";
/// ○ in a DiTiS cell.
pub const TICK_1_MARK: &str = "○";
/// ● in a DiTiS cell.
pub const TICK_2_MARK: &str = "●";

/// The sheet keys this crate models. Everything else is kept in the extras.
const SHEET_KEYS: &[&str] = &["name", "frames", "layers", "data"];

/// Whether `text` looks like a DiTiS file: a JSON object with a `sheets`
/// list.
pub fn is_ditis(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('{') && t.contains("\"sheets\"")
}

/// Read a DiTiS file. Every sheet in it is a time table.
pub fn from_str(text: &str) -> Result<Sheet, Error> {
    from_value(&json::parse(text.trim_start_matches('\u{feff}'))?)
}

/// Read a DiTiS document already parsed.
pub fn from_value(v: &Value) -> Result<Sheet, Error> {
    let Value::Object(root) = v else {
        return Err(Error::Parse("a DiTiS file is a JSON object".into()));
    };
    let sheets = v
        .get("sheets")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Parse("a DiTiS file has a \"sheets\" list".into()))?;
    if sheets.is_empty() {
        return Err(Error::Parse("a DiTiS file with no sheets".into()));
    }
    let tables = sheets
        .iter()
        .map(table_from_value)
        .collect::<Result<_, _>>()?;
    Ok(Sheet {
        tables,
        read_from: Some(Format::Ditis),
        extra: root
            .iter()
            .filter(|(k, _)| k != "sheets")
            .cloned()
            .collect(),
        ..Sheet::default()
    })
}

fn table_from_value(v: &Value) -> Result<TimeTable, Error> {
    let Value::Object(m) = v else {
        return Err(Error::Parse("a DiTiS sheet is a JSON object".into()));
    };
    let duration = v
        .get("frames")
        .and_then(Value::as_index)
        .filter(|&n| n > 0)
        .ok_or_else(|| Error::Parse("a DiTiS sheet has no length (\"frames\")".into()))?
        as usize;

    // Column names: a list of names, or, in older files, of {id, name}.
    let layers = v.get("layers").and_then(Value::as_array).unwrap_or(&[]);
    let columns: Vec<String> = layers
        .iter()
        .map(|l| match l {
            Value::Object(_) => l.get("name").map(Value::to_text).unwrap_or_default(),
            other => other.to_text(),
        })
        .collect();
    let ids: Vec<String> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            l.get("id")
                .map(Value::to_text)
                .unwrap_or_else(|| format!("L{}", i + 1))
        })
        .collect();

    let cell = |c: usize, f: usize| -> String {
        match v.get("data") {
            Some(Value::Array(cols)) => cols
                .get(c)
                .and_then(Value::as_array)
                .and_then(|col| col.get(f))
                .map(Value::to_text)
                .unwrap_or_default(),
            // { "1": { "L1": "1" } }: frames from 1, by layer id.
            Some(data @ Value::Object(_)) => data
                .get(&(f + 1).to_string())
                .and_then(|row| row.get(&ids[c]))
                .map(Value::to_text)
                .unwrap_or_default(),
            _ => String::new(),
        }
    };

    let tracks = (0..columns.len())
        .map(|c| track_from_frames((0..duration).map(|f| cell(c, f))))
        .collect();

    Ok(TimeTable {
        name: v.get("name").map(Value::to_text).unwrap_or_default(),
        duration,
        columns,
        tracks,
        // Every key, the modelled ones too: see "Byte for byte".
        extra: m.clone(),
        ..TimeTable::default()
    })
}

/// One column's frames as changes, as DiTiS's XDTS export works them out.
fn track_from_frames(values: impl Iterator<Item = String>) -> Track {
    let mut frames = Vec::new();
    let mut previous = NULL_CELL.to_string();
    for (f, v) in values.enumerate() {
        let value = match v.as_str() {
            "-" => HYPHEN,
            "" | EMPTY_MARK => NULL_CELL,
            TICK_1_MARK => TICK_1,
            TICK_2_MARK => TICK_2,
            other => other,
        };
        if value != previous || value == HYPHEN {
            frames.push((f, value.into()));
        }
        if value != HYPHEN {
            previous = value.to_string();
        }
    }
    Track { frames }
}

/// One column as the value DiTiS stores on every frame.
fn frames_from_track(track: &Track, duration: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(duration);
    let mut current = String::new();
    let mut changes = track.frames.iter().peekable();
    for f in 0..duration {
        while let Some((_, cell)) = changes.next_if(|(at, _)| *at <= f) {
            current = match cell.value.as_str() {
                NULL_CELL => EMPTY_MARK.to_string(),
                HYPHEN => continue,
                TICK_1 => TICK_1_MARK.to_string(),
                TICK_2 => TICK_2_MARK.to_string(),
                v => v.to_string(),
            };
        }
        out.push(current.clone());
    }
    // Empty at the end is nothing at all, as DiTiS saves a column.
    while out.last().is_some_and(|v| v.is_empty() || v == EMPTY_MARK) {
        out.pop();
    }
    out
}

/// The document a sheet is written as.
pub fn to_value(sheet: &Sheet) -> Value {
    let own = sheet.read_from == Some(Format::Ditis);
    let mut root = Vec::new();
    let version = own
        .then(|| sheet.extra.iter().find(|(k, _)| k == "version"))
        .flatten()
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| VERSION.into());
    root.push(("version".to_string(), version));
    let sheets = sheet
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| sheet_value(t, i, own))
        .collect::<Vec<_>>();
    root.push(("sheets".to_string(), Value::Array(sheets)));
    if own {
        root.extend(sheet.extra.iter().filter(|(k, _)| k != "version").cloned());
    }
    Value::Object(root)
}

fn sheet_value(t: &TimeTable, index: usize, own: bool) -> Value {
    let name = if t.name.is_empty() {
        format!("Sheet{}", index + 1)
    } else {
        t.name.clone()
    };
    let columns = t.columns.len().max(t.tracks.len());
    let layers = Value::Array(
        (0..columns)
            .map(|c| Value::from(t.columns.get(c).cloned().unwrap_or_default()))
            .collect(),
    );
    let read = own
        .then(|| t.extra.iter().find(|(k, _)| k == "data"))
        .flatten()
        .map(|(_, v)| v);
    let data = match read {
        Some(v) if unchanged(t, v) => v.clone(),
        _ => data_value(t, columns),
    };
    let modelled = |key: &str| -> Option<Value> {
        match key {
            "name" => Some(name.clone().into()),
            "frames" => Some(t.duration.into()),
            "layers" => Some(layers.clone()),
            "data" => Some(data.clone()),
            _ => None,
        }
    };

    // A sheet from DiTiS: its keys, in its order, with the modelled ones
    // filled from the model, and any of those it lacked at the end.
    if own {
        let mut m: Vec<(String, Value)> = t
            .extra
            .iter()
            .map(|(k, v)| (k.clone(), modelled(k).unwrap_or_else(|| v.clone())))
            .collect();
        for key in SHEET_KEYS {
            if !m.iter().any(|(k, _)| k == key) {
                m.extend(modelled(key).map(|v| (key.to_string(), v)));
            }
        }
        return Value::Object(m);
    }

    // Any other sheet: the keys a new DiTiS sheet has, in its order.
    let mut m = vec![("name".to_string(), Value::from(name.clone()))];
    m.push(("fps".into(), 24usize.into()));
    m.push(("layers".into(), layers.clone()));
    m.push(("frames".into(), t.duration.into()));
    m.push(("framePageSize".into(), t.duration.into()));
    m.push(("data".into(), data.clone()));
    m.push(("insertedFrames".into(), Value::Array(Vec::new())));
    m.push(("disabledFrames".into(), Value::Array(Vec::new())));
    Value::Object(m)
}

/// `data` written from the model.
fn data_value(t: &TimeTable, columns: usize) -> Value {
    let empty = Track::default();
    let mut data: Vec<Value> = (0..columns)
        .map(|c| {
            let values = frames_from_track(t.tracks.get(c).unwrap_or(&empty), t.duration);
            Value::Array(values.into_iter().map(Value::from).collect())
        })
        .collect();
    while matches!(data.last(), Some(Value::Array(a)) if a.is_empty()) {
        data.pop();
    }
    Value::Array(data)
}

/// Whether `data`, as read, still says what the model says: the same
/// changes in every column. Only the column-list form is written back; the
/// older keyed form is always written anew.
fn unchanged(t: &TimeTable, data: &Value) -> bool {
    let Value::Array(cols) = data else {
        return false;
    };
    let columns = t.columns.len().max(t.tracks.len());
    if cols.len() > columns {
        return false;
    }
    let empty = Track::default();
    (0..columns).all(|c| {
        let read = cols.get(c).and_then(Value::as_array).unwrap_or(&[]);
        if read.len() > t.duration {
            return false;
        }
        let track = track_from_frames(
            (0..t.duration).map(|f| read.get(f).map(Value::to_text).unwrap_or_default()),
        );
        &track == t.tracks.get(c).unwrap_or(&empty)
    })
}

/// Write a sheet as a DiTiS file.
pub fn to_string(sheet: &Sheet, opts: &WriteOptions) -> String {
    let v = to_value(sheet);
    if opts.pretty {
        v.to_pretty()
    } else {
        v.to_compact()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(t: &Track) -> Vec<(usize, &str)> {
        t.frames
            .iter()
            .map(|(f, c)| (*f, c.value.as_str()))
            .collect()
    }

    #[test]
    fn every_frame_becomes_the_changes() {
        let t = track_from_frames(
            ["", "1", "1", "-", "2", "×", "○", "○", "", "3"]
                .iter()
                .map(|s| s.to_string()),
        );
        assert_eq!(
            values(&t),
            [
                (1, "1"),
                (3, HYPHEN),
                (4, "2"),
                (5, NULL_CELL),
                (6, TICK_1),
                (8, NULL_CELL),
                (9, "3"),
            ]
        );
    }

    #[test]
    fn the_changes_fill_every_frame_and_stop_at_the_last_drawing() {
        let t = Track {
            frames: vec![
                (1, "1".into()),
                (3, HYPHEN.into()),
                (4, NULL_CELL.into()),
                (5, TICK_2.into()),
                (6, "2".into()),
                (7, NULL_CELL.into()),
            ],
        };
        assert_eq!(
            frames_from_track(&t, 10),
            ["", "1", "1", "1", "×", "●", "2"]
        );
    }
}
