//! TNZ — the scene file of OpenToonz and Tahoma2D, read for its exposure
//! sheet.
//!
//! A scene is much more than a sheet: cameras, output settings, the level
//! files, pegbars, the effects graph. What is read here is the part an
//! exchange sheet has a place for — which drawing each column shows on each
//! frame — and it is read **the way OpenToonz's own XDTS export reads it**
//! (`toonz/sources/toonz/xdtsio.cpp`), so that a sheet from here is the
//! sheet OpenToonz would have written for the same scene:
//!
//! * The columns are the top xsheet's, in order. A sound column has no
//!   cells and takes no place; every other column does, and one with nothing
//!   exposed is a column with an empty name.
//! * A column is named after the **level** it exposes first, not after the
//!   column's own name, and only drawings of that level are written: a cell
//!   of another level in the same column is written as empty. A column that
//!   exposes no drawing level or sub-xsheet — a palette, an effect, a
//!   dialogue column — is written as an empty column.
//! * A drawing is its frame number without the padding, and its letter if it
//!   has one: `0001` is `1`, `0012a` is `12a`.
//! * **The one place this differs from the export on purpose:** a level of
//!   one image with no frame number (`-` in the file, a single `.tif`) is
//!   drawing `1`. OpenToonz writes the internal number of "no frame" there,
//!   `-2` (`fid2Str` in `xdtsio.cpp`), which every other application reads
//!   as a drawing called −2.
//! * A column that starts late gets [`NULL_CELL`] at frame 0, and every
//!   column gets one on the frame after its last cell, as the export does.
//! * The sheet is as long as the output range when one is set, and as the
//!   scene otherwise.
//! * In-between and key marks are read from **cell marks**, as the export
//!   reads them: which mark means what is the export dialog's choice, and
//!   [`CellMarks`] is that choice, its defaults the dialog's.
//!
//! Beyond what the export writes, **dialogue** is read too: each sound text
//! column becomes a column of the dialogue field, named after the column (or
//! its level), with the speaker — that same name — and the line where the
//! line starts and a hyphen on each frame it goes on. That layout is taken
//! from the only sheet with dialogue at hand, `tests/data/synthetic_v5.xdts`
//! — another editor's invented sample, so it is a shape to follow, not
//! evidence of what an application expects. The column still takes its
//! empty place among the cell columns, as in the export.
//!
//! Not read: sub-xsheet contents and the frame rate. The time table has no
//! name, because OpenToonz names it after the scene's file, which is not in
//! the file.
//!
//! # The file
//!
//! A scene is written by OpenToonz's `TOStream`, which looks like XML and is
//! not: `&` and `<` go out unescaped, text is a list of tokens with quoted
//! strings escaped by backslash, and an object written a second time is a
//! reference to the first (`<level id='9'/>`). So it has its own reader
//! here, which is that format's and nothing more. Text is UTF-8, which is
//! what OpenToonz writes.
//!
//! Writing scenes is not supported: a scene that opens needs the level files
//! it names, and an exchange sheet does not have them. OpenToonz reads XDTS
//! itself.

use crate::{
    Cell, Error, Field, Instruction, Sheet, TimeTable, Track, ValueTrack, HYPHEN, NULL_CELL,
    OPTION_KEYFRAME, OPTION_REFERENCEFRAME, TICK_1, TICK_2,
};
use std::collections::HashMap;

/// Which of the scene's cell marks stand for which XDTS symbol.
///
/// A cell mark is OpenToonz's coloured dot on a cell, numbered by its place
/// in the scene's list of marks. Which mark means ○ is not in the file: the
/// export dialog asks. The [`Default`] is what that dialog offers, on the
/// scene's default list — Blue for ○, Purple for ●, Red for a key and Green
/// for a reference — and `None` reads no symbol from marks at all.
///
/// As in the export, ○ and ● are read on a frame that holds the drawing
/// before it, and the key and reference marks on a frame where a drawing
/// starts; a key or reference mark makes the sheet need the p1 extension,
/// which the XDTS writer then declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellMarks {
    /// The mark that means ○ ([`TICK_1`]).
    pub tick_1: Option<u32>,
    /// The mark that means ● ([`TICK_2`]).
    pub tick_2: Option<u32>,
    /// The mark that means a key drawing ([`OPTION_KEYFRAME`]).
    pub keyframe: Option<u32>,
    /// The mark that means a reference drawing ([`OPTION_REFERENCEFRAME`]).
    pub reference: Option<u32>,
}

impl Default for CellMarks {
    /// The export dialog's defaults (`xdtsio.cpp`): marks 6, 8, 0 and 4.
    fn default() -> Self {
        Self {
            tick_1: Some(6),
            tick_2: Some(8),
            keyframe: Some(0),
            reference: Some(4),
        }
    }
}

impl CellMarks {
    /// Read no symbols from marks.
    pub const NONE: Self = Self {
        tick_1: None,
        tick_2: None,
        keyframe: None,
        reference: None,
    };
}

/// Whether `text` starts like a scene file.
pub fn is_tnz(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("<tnz")
}

/// Read a scene file from bytes. Invalid UTF-8 is replaced rather than
/// refused, because only the level names and dialogue are text.
pub fn from_bytes(bytes: &[u8]) -> Result<Sheet, Error> {
    from_str(&String::from_utf8_lossy(bytes))
}

/// Read a scene file, with the export dialog's default [`CellMarks`].
pub fn from_str(text: &str) -> Result<Sheet, Error> {
    from_str_with(text, &CellMarks::default())
}

/// Read a scene file, reading the symbols from the marks given.
pub fn from_str_with(text: &str, marks: &CellMarks) -> Result<Sheet, Error> {
    let root = parse(text)?;
    if root.tag != "tnz" {
        return Err(Error::Parse(format!(
            "not an OpenToonz scene: the document is <{}>",
            root.tag
        )));
    }

    let mut levels = HashMap::new();
    collect_levels(&root, &mut levels);

    let xsheet = root
        .child("xsheet")
        .ok_or_else(|| Error::Parse("scene has no xsheet".into()))?;
    let names = column_names(xsheet);

    let mut columns = Vec::new();
    let mut tracks = Vec::new();
    let mut dialogue = Field::default();
    let mut last_row = 0usize;
    let all = xsheet.child("columns").map(|c| c.elements());
    for (index, column) in all.into_iter().flatten().enumerate() {
        if column.tag == "soundColumn" {
            continue;
        }
        let cells = match column.tag.as_str() {
            "levelColumn" | "meshColumn" => read_cells(column)?,
            "soundTextColumn" => {
                let lines = read_text_cells(column, &levels)?;
                if let Some((_, r1, _)) = lines.last() {
                    last_row = last_row.max(r1 + 1);
                }
                if !lines.is_empty() {
                    let speaker = names.get(&index).cloned().unwrap_or_else(|| {
                        lines
                            .first()
                            .and_then(|(_, _, (id, _))| levels.get(id))
                            .map(|l| l.name.clone())
                            .unwrap_or_default()
                    });
                    dialogue.tracks.push(dialogue_track(&lines, &speaker));
                    dialogue.columns.push(speaker);
                }
                Vec::new()
            }
            _ => Vec::new(),
        };
        if let Some((r1, _)) = cells.last() {
            last_row = last_row.max(r1 + 1);
        }
        let (name, track) = build_column(&cells, &levels, &read_marks(column), marks);
        columns.push(name);
        tracks.push(track);
    }
    while columns.last().is_some_and(String::is_empty) {
        columns.pop();
        tracks.pop();
    }

    let duration = output_range_end(&root)
        .or_else(|| root.attr("framecount").and_then(|n| n.parse().ok()))
        .filter(|&n| n > 0)
        .unwrap_or(last_row);
    if duration == 0 {
        return Err(Error::Parse("scene has no frames".into()));
    }

    Ok(Sheet {
        tables: vec![TimeTable {
            duration,
            columns,
            tracks,
            dialogue,
            ..TimeTable::default()
        }],
        ..Sheet::default()
    })
}

/// What a cell refers to: a level's id and the frame as written.
type CellRef = (String, String);

/// One run of a dialogue column: its first and last row, the level's id and
/// the line.
type Line = (usize, usize, (String, String));

/// A level, as far as a sheet needs it.
struct Level {
    /// Whether it is a drawing level or a sub-xsheet, the two kinds the
    /// export writes.
    exposable: bool,
    name: String,
    /// A dialogue level's lines, which its cells number from 1.
    lines: Vec<String>,
}

/// The names columns were given (the pegbar `Col1` is column 0), where one
/// was.
fn column_names(xsheet: &Element) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for p in xsheet
        .child("pegbars")
        .map(|p| p.elements())
        .into_iter()
        .flatten()
    {
        let Some(n) = p.attr("id").and_then(|id| id.strip_prefix("Col")) else {
            continue;
        };
        let (Ok(n), Some(name)) = (
            n.parse::<usize>(),
            p.child("name").and_then(|e| e.texts().next()),
        ) else {
            continue;
        };
        if n > 0 && !name.is_empty() {
            out.insert(n - 1, name.to_string());
        }
    }
    out
}

/// A column's cell marks: row, and the mark's number.
fn read_marks(column: &Element) -> HashMap<usize, u32> {
    let mut out = HashMap::new();
    for m in column
        .child("cellMarks")
        .map(|m| m.elements())
        .into_iter()
        .flatten()
    {
        let Some(id) = m.attr("id").and_then(|id| id.parse().ok()) else {
            continue;
        };
        for row in m.texts().flat_map(str::split_whitespace) {
            if let Ok(row) = row.parse() {
                out.insert(row, id);
            }
        }
    }
    out
}

/// A dialogue column's cells: first row, last row, and the line.
fn read_text_cells(column: &Element, levels: &HashMap<String, Level>) -> Result<Vec<Line>, Error> {
    let mut out = Vec::new();
    let Some(cells) = column.child("cells") else {
        return Ok(out);
    };
    for cell in cells.elements().filter(|c| c.tag == "cell") {
        let bad = || Error::Parse("a dialogue cell is not \"rows line level\"".into());
        let mut texts = cell.texts();
        let rows = texts.next().ok_or_else(bad)?;
        let line: usize = texts.next().and_then(|t| t.parse().ok()).ok_or_else(bad)?;
        let id = cell
            .elements()
            .next()
            .and_then(|l| l.attr("id"))
            .ok_or_else(bad)?
            .to_string();
        let (r0, r1) = match rows.split_once('-') {
            Some((a, b)) => (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?),
            None => {
                let r = rows.parse().map_err(|_| bad())?;
                (r, r)
            }
        };
        let text = levels
            .get(&id)
            .and_then(|l| l.lines.get(line.wrapping_sub(1)))
            .cloned()
            .unwrap_or_default();
        out.push((r0, r1, (id, text)));
    }
    out.sort_by_key(|(r0, _, _)| *r0);
    Ok(out)
}

/// A dialogue column as the dialogue field has it: the speaker and the line
/// where it starts, and the line held by a hyphen on every frame after; see
/// the module documentation for where that shape comes from.
fn dialogue_track(lines: &[Line], speaker: &str) -> ValueTrack {
    let mut frames = Vec::new();
    for (r0, r1, (_, text)) in lines {
        frames.push((*r0, Instruction::from([speaker, text.as_str()])));
        for r in r0 + 1..=*r1 {
            frames.push((r, Instruction::from([HYPHEN])));
        }
    }
    ValueTrack { frames }
}

/// Every level the scene defines, wherever it is defined. Ids are unique
/// across the whole file, sub-xsheets included.
fn collect_levels(e: &Element, out: &mut HashMap<String, Level>) {
    if let (Some(id), false) = (e.attr("id"), e.empty) {
        let level = match e.tag.as_str() {
            "level" => Some(Level {
                exposable: true,
                name: e.texts().next().unwrap_or_default().to_string(),
                lines: Vec::new(),
            }),
            "childLevel" => Some(Level {
                exposable: true,
                name: e
                    .child("name")
                    .and_then(|n| n.texts().next())
                    .unwrap_or_default()
                    .to_string(),
                lines: Vec::new(),
            }),
            "soundTextLevel" => Some(Level {
                exposable: false,
                name: e.texts().next().unwrap_or_default().to_string(),
                lines: e
                    .elements()
                    .filter(|f| f.tag == "frame")
                    .map(|f| f.texts().collect::<Vec<_>>().join(" "))
                    .collect(),
            }),
            t if t.ends_with("Level") => Some(Level {
                exposable: false,
                name: String::new(),
                lines: Vec::new(),
            }),
            _ => None,
        };
        if let Some(level) = level {
            out.entry(id.to_string()).or_insert(level);
        }
    }
    for c in e.elements() {
        collect_levels(c, out);
    }
}

/// A column's `<cells>`, one entry per row that has something, in order.
fn read_cells(column: &Element) -> Result<Vec<(usize, CellRef)>, Error> {
    let mut out: Vec<(usize, CellRef)> = Vec::new();
    let Some(cells) = column.child("cells") else {
        return Ok(out);
    };
    for cell in cells.elements().filter(|c| c.tag == "cell") {
        let mut texts = cell.texts();
        let bad = || Error::Parse("a cell is not \"row count level frame increment\"".into());
        let row: usize = texts.next().and_then(|t| t.parse().ok()).ok_or_else(bad)?;
        let count: usize = texts.next().and_then(|t| t.parse().ok()).ok_or_else(bad)?;
        let frame = texts.next().ok_or_else(bad)?;
        let inc: i64 = texts.next().unwrap_or("0").parse().map_err(|_| bad())?;
        let level = cell
            .elements()
            .next()
            .and_then(|l| l.attr("id"))
            .ok_or_else(bad)?
            .to_string();

        let (number, letter) = split_frame(frame);
        for i in 0..count {
            // More than one row means no letter: OpenToonz writes a lettered
            // frame one cell at a time.
            let value = match number {
                Some(n) if i > 0 => (n + inc * i as i64).to_string(),
                _ => drawing(frame, number, letter),
            };
            let r = row + i;
            match out.last_mut() {
                Some((last, slot)) if *last == r => *slot = (level.clone(), value),
                _ => out.push((r, (level.clone(), value))),
            }
        }
    }
    out.sort_by_key(|(r, _)| *r);
    Ok(out)
}

/// A frame id as OpenToonz writes it: digits, padded or not, then a letter.
fn split_frame(frame: &str) -> (Option<i64>, &str) {
    let digits = frame.bytes().take_while(u8::is_ascii_digit).count();
    (frame[..digits].parse().ok(), &frame[digits..])
}

fn drawing(frame: &str, number: Option<i64>, letter: &str) -> String {
    match number {
        Some(n) => format!("{n}{letter}"),
        None if frame == "-" => "1".to_string(),
        None => frame.to_string(),
    }
}

/// One column as the export builds it: its name, and its changes.
fn build_column(
    cells: &[(usize, CellRef)],
    levels: &HashMap<String, Level>,
    column_marks: &HashMap<usize, u32>,
    marks: &CellMarks,
) -> (String, Track) {
    let Some(&(r0, _)) = cells.first() else {
        return (String::new(), Track::default());
    };
    let r1 = cells.last().map(|(r, _)| *r).unwrap_or(r0);
    let level = cells
        .iter()
        .map(|(_, (id, _))| id)
        .find(|id| levels.get(*id).is_some_and(|l| l.exposable));
    let Some(level) = level else {
        return (String::new(), Track::default());
    };
    let mark = |row: usize, which: Option<u32>| {
        which.is_some() && column_marks.get(&row).copied() == which
    };

    let mut frames = Vec::new();
    if r0 > 0 {
        frames.push((0, NULL_CELL.into()));
    }
    let mut at = cells.iter().peekable();
    let mut prev: Option<&str> = None;
    for row in r0..=r1 {
        let here = match at.peek() {
            Some((r, (id, value))) if *r == row => {
                at.next();
                (id == level).then_some(value.as_str())
            }
            _ => None,
        };
        if here == prev {
            // A held drawing can carry ○ or ●.
            if mark(row, marks.tick_1) {
                frames.push((row, TICK_1.into()));
            } else if mark(row, marks.tick_2) {
                frames.push((row, TICK_2.into()));
            }
            continue;
        }
        let cell = match here {
            None => Cell::from(NULL_CELL),
            Some(v) if mark(row, marks.keyframe) => Cell::marked(v, [OPTION_KEYFRAME]),
            Some(v) if mark(row, marks.reference) => Cell::marked(v, [OPTION_REFERENCEFRAME]),
            Some(v) => Cell::from(v),
        };
        frames.push((row, cell));
        prev = here;
    }
    frames.push((r1 + 1, NULL_CELL.into()));

    let name = levels
        .get(level)
        .map(|l| l.name.clone())
        .unwrap_or_default();
    (name, Track { frames })
}

/// The end of the main output's range, as a length, if a range is set.
fn output_range_end(root: &Element) -> Option<usize> {
    let outputs = root.child("properties")?.child("outputs")?;
    let main = outputs
        .elements()
        .find(|o| o.tag == "output" && o.attr("name") == Some("main"))?;
    let mut range = main.child("range")?.texts();
    let r0: i64 = range.next()?.parse().ok()?;
    let r1: i64 = range.next()?.parse().ok()?;
    (r0 >= 0 && r1 >= r0).then(|| r1 as usize + 1)
}

// ---------------------------------------------------------------------------
// The TOStream reader.

/// One tag and what is inside it.
#[derive(Debug, Default)]
struct Element {
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    /// Written as `<tag/>`: a reference, not a definition.
    empty: bool,
}

#[derive(Debug)]
enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    fn child(&self, tag: &str) -> Option<&Element> {
        self.elements().find(|e| e.tag == tag)
    }

    fn texts(&self) -> impl Iterator<Item = &str> {
        self.children.iter().filter_map(|n| match n {
            Node::Text(t) => Some(t.as_str()),
            Node::Element(_) => None,
        })
    }
}

/// Read a whole stream into its one top element.
fn parse(text: &str) -> Result<Element, Error> {
    let s = text.trim_start_matches('\u{feff}');
    let b = s.as_bytes();
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'<' if b.get(i + 1) == Some(&b'?') || s[i..].starts_with("<!--") => {
                let end = if b[i + 1] == b'?' { "?>" } else { "-->" };
                i = s[i..]
                    .find(end)
                    .map(|e| i + e + end.len())
                    .unwrap_or(b.len());
            }
            b'<' if b.get(i + 1) == Some(&b'/') => {
                let end = s[i..]
                    .find('>')
                    .ok_or_else(|| Error::Parse("unterminated closing tag".into()))?;
                let tag = s[i + 2..i + end].trim();
                i += end + 1;
                close(&mut stack, tag)?;
            }
            b'<' => {
                let (e, next) = open_tag(s, i)?;
                i = next;
                if e.empty {
                    push(&mut stack, Node::Element(e));
                } else {
                    stack.push(e);
                }
            }
            b'"' => {
                let (t, next) = quoted(s, i)?;
                i = next;
                push(&mut stack, Node::Text(t));
            }
            _ => {
                let start = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'<' && b[i] != b'"' {
                    i += 1;
                }
                push(&mut stack, Node::Text(s[start..i].to_string()));
            }
        }
    }
    if stack.len() > 1 {
        return Err(Error::Parse(format!(
            "<{}> is never closed",
            stack.last().map(|e| e.tag.as_str()).unwrap_or_default()
        )));
    }
    stack
        .pop()
        .and_then(|doc| {
            doc.children.into_iter().find_map(|n| match n {
                Node::Element(e) => Some(e),
                Node::Text(_) => None,
            })
        })
        .ok_or_else(|| Error::Parse("empty document".into()))
}

fn push(stack: &mut [Element], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

fn close(stack: &mut Vec<Element>, tag: &str) -> Result<(), Error> {
    if stack.len() < 2 || stack.last().map(|e| e.tag.as_str()) != Some(tag) {
        return Err(Error::Parse(format!("</{tag}> closes nothing open")));
    }
    let e = stack.pop().unwrap_or_default();
    push(stack, Node::Element(e));
    Ok(())
}

/// `<tag a='1' b="2">` or `<tag/>`, starting at the `<`.
fn open_tag(s: &str, start: usize) -> Result<(Element, usize), Error> {
    let b = s.as_bytes();
    let mut i = start + 1;
    let name_end = |i: usize| {
        let mut j = i;
        while j < b.len() && !b[j].is_ascii_whitespace() && !matches!(b[j], b'>' | b'/' | b'=') {
            j += 1;
        }
        j
    };
    let end = name_end(i);
    let mut e = Element {
        tag: s[i..end].to_string(),
        ..Element::default()
    };
    if e.tag.is_empty() {
        return Err(Error::Parse("a tag with no name".into()));
    }
    i = end;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        match b.get(i) {
            None => return Err(Error::Parse(format!("<{} is never finished", e.tag))),
            Some(b'>') => return Ok((e, i + 1)),
            Some(b'/') if b.get(i + 1) == Some(&b'>') => {
                e.empty = true;
                return Ok((e, i + 2));
            }
            Some(_) => {
                let key_end = name_end(i);
                let key = s[i..key_end].to_string();
                i = key_end;
                if b.get(i) != Some(&b'=') {
                    return Err(Error::Parse(format!("attribute {key:?} has no value")));
                }
                i += 1;
                let q = *b
                    .get(i)
                    .filter(|q| matches!(q, b'"' | b'\''))
                    .ok_or_else(|| Error::Parse(format!("attribute {key:?} is not quoted")))?;
                let close = s[i + 1..]
                    .find(q as char)
                    .ok_or_else(|| Error::Parse(format!("attribute {key:?} is never closed")))?;
                e.attrs.push((key, s[i + 1..i + 1 + close].to_string()));
                i += close + 2;
            }
        }
    }
}

/// A quoted string starting at the `"`, with its backslashes undone.
fn quoted(s: &str, start: usize) -> Result<(String, usize), Error> {
    let mut out = String::new();
    let mut chars = s[start + 1..].char_indices();
    while let Some((k, c)) = chars.next() {
        match c {
            '"' => return Ok((out, start + 1 + k + 1)),
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    out.push(n);
                }
            }
            c => out.push(c),
        }
    }
    Err(Error::Parse("a quoted string is never closed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stream_is_read_as_tokens_not_as_xml() {
        let e = parse(
            "<a x='1' y=\"&\">\n  0 72 <level id='9'/>\"-\"0 \n  \"say \\\"hi\\\" & <b>\"\n</a>",
        )
        .unwrap();
        assert_eq!(e.attr("x"), Some("1"));
        assert_eq!(e.attr("y"), Some("&"));
        let texts: Vec<_> = e.texts().collect();
        assert_eq!(texts, ["0", "72", "-", "0", "say \"hi\" & <b>"]);
        let r = e.elements().next().unwrap();
        assert!(r.empty);
        assert_eq!(r.attr("id"), Some("9"));
    }

    #[test]
    fn frame_ids_lose_their_padding_and_keep_their_letter() {
        let d = |f: &str| {
            let (n, l) = split_frame(f);
            drawing(f, n, l)
        };
        assert_eq!(d("0001"), "1");
        assert_eq!(d("0012a"), "12a");
        assert_eq!(d("-"), "1");
        assert_eq!(d("7"), "7");
    }

    #[test]
    fn a_mismatched_tag_is_an_error() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
    }
}
