//! Small order-preserving JSON value, parser and writer (keeps the crate
//! dependency-free; XDTS / TDTS are JSON documents with a text header line).

use crate::Error;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
/// One JSON value.
///
/// Enough of JSON to read and write a time sheet, and no more: the
/// numbers are `f64`, and an object keeps its keys in the order they were
/// written, because matching a file an application saved depends on it.
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number. JSON has only the one kind, and so does this: an `f64`, so
    /// a whole number past 2^53 is rounded, as it is in JavaScript. Nothing in
    /// a time sheet comes near that.
    Number(f64),
    /// A string, unescaped.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, in the order its keys were written.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// An object with nothing in it yet, to be filled by [`Value::with`].
    pub fn obj() -> Self {
        Value::Object(Vec::new())
    }

    /// Builder-style insert for objects.
    pub fn with(mut self, key: &str, v: impl Into<Value>) -> Self {
        if let Value::Object(ref mut m) = self {
            m.push((key.to_string(), v.into()));
        }
        self
    }

    /// The value under `key`, if this is an object that has one.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The text if this is a string, and nothing for any other kind.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// The value if it is `true` or `false`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The number if it is a whole number from 0 to `u32::MAX`: what a frame,
    /// a track number, a count or an id can be. `1.5`, `-1` and `1e30` are
    /// none of those, and reading them as one — `1e30 as usize` is
    /// `usize::MAX` — hands the caller a frame count it will loop over for
    /// ever.
    pub fn as_index(&self) -> Option<u32> {
        let n = self.as_f64()?;
        (n >= 0.0 && n.fract() == 0.0 && n <= u32::MAX as f64).then_some(n as u32)
    }

    /// The number if this is one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The elements if this is an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Lenient string view: numbers are formatted, strings returned as-is.
    /// This value as text: a string as itself, a number or a boolean as it
    /// would be written, and anything else as nothing.
    pub fn to_text(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Number(n) => fmt_num(*n),
            Value::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    /// Laid out over several lines, two spaces to a level.
    pub fn to_pretty(&self) -> String {
        self.to_pretty_indent(2)
    }

    /// The same, with the indent the writer being matched uses: the 東映
    /// digital timesheet writes four spaces to a level, and a sheet saved by
    /// it is only reproduced byte for byte that way.
    pub fn to_pretty_indent(&self, width: usize) -> String {
        let mut s = String::new();
        write_value(&mut s, self, 0, Some(width));
        s
    }

    /// All on one line, which is how one of the editors writes a sheet.
    pub fn to_compact(&self) -> String {
        let mut s = String::new();
        write_value(&mut s, self, 0, None);
        s
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Value::Number(n as f64)
    }
}
impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Value::Number(n as f64)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Number(n as f64)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}

/// A number as JSON writes it, and as it reads back: exactly.
///
/// A whole number is written without a point. Anything else is written with
/// Rust's `Display`, which is the shortest decimal that parses back to the
/// same `f64`; this used to round to four places, so `0.00001` was written
/// as `0`.
fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn escape(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn indent(out: &mut String, n: usize, width: usize) {
    for _ in 0..n * width {
        out.push(' ');
    }
}

fn write_value(out: &mut String, v: &Value, depth: usize, pretty: Option<usize>) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&fmt_num(*n)),
        Value::String(s) => escape(out, s),
        Value::Array(a) => {
            // The applications put every element on its own line, however
            // small; a saved sheet is matched byte for byte only this way.
            let Some(width) = pretty.filter(|_| !a.is_empty()) else {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        if pretty.is_some() {
                            out.push(' ');
                        }
                    }
                    write_value(out, x, depth + 1, pretty);
                }
                out.push(']');
                return;
            };
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                indent(out, depth + 1, width);
                write_value(out, x, depth + 1, pretty);
                if i + 1 < a.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth, width);
            out.push(']');
        }
        Value::Object(m) => {
            // An object with nothing in it still takes two lines: the
            // application writes the braces on their own, and `free` in a
            // saved sheet is exactly that.
            if m.is_empty() {
                out.push('{');
                if let Some(width) = pretty {
                    out.push('\n');
                    indent(out, depth, width);
                }
                out.push('}');
                return;
            }
            out.push('{');
            if pretty.is_some() {
                out.push('\n');
            }
            for (i, (k, x)) in m.iter().enumerate() {
                if let Some(width) = pretty {
                    indent(out, depth + 1, width);
                }
                escape(out, k);
                out.push(':');
                if pretty.is_some() {
                    out.push(' ');
                }
                write_value(out, x, depth + 1, pretty);
                if i + 1 < m.len() {
                    out.push(',');
                }
                if pretty.is_some() {
                    out.push('\n');
                }
            }
            if let Some(width) = pretty {
                indent(out, depth, width);
            }
            out.push('}');
        }
    }
}

/// How deep objects and arrays may nest before a document is refused.
///
/// A sheet nests about ten deep. Without a limit, a file of a few thousand
/// `[` runs the recursive descent out of stack, and that is an abort, not an
/// error a caller can handle: one bad file would take down whatever read it.
/// 128 is serde_json's limit too, so the two refuse the same files.
pub const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn err<T>(&self, msg: &str) -> Result<T, Error> {
        Err(Error::Parse(format!("{msg} at byte {}", self.i)))
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    /// JSON's whitespace is these four and nothing else; Rust's
    /// `is_ascii_whitespace` also counts form feed.
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), Error> {
        self.ws();
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", c as char))
        }
    }

    fn literal(&mut self, word: &[u8], v: Value) -> Result<Value, Error> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            self.err("unexpected character")
        }
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth >= MAX_DEPTH {
            return self.err("nested too deep");
        }
        Ok(())
    }

    fn value(&mut self) -> Result<Value, Error> {
        self.ws();
        match self.peek() {
            None => self.err("unexpected end"),
            Some(b'{') => {
                self.enter()?;
                self.i += 1;
                let mut m: Vec<(String, Value)> = Vec::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    self.depth -= 1;
                    return Ok(Value::Object(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.eat(b':')?;
                    let v = self.value()?;
                    // A repeated key takes the later value, in the first
                    // one's place, as serde_json and most readers do.
                    match m.iter_mut().find(|(key, _)| *key == k) {
                        Some(slot) => slot.1 = v,
                        None => m.push((k, v)),
                    }
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            self.depth -= 1;
                            return Ok(Value::Object(m));
                        }
                        _ => return self.err("expected ',' or '}'"),
                    }
                }
            }
            Some(b'[') => {
                self.enter()?;
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    self.depth -= 1;
                    return Ok(Value::Array(a));
                }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            self.depth -= 1;
                            return Ok(Value::Array(a));
                        }
                        _ => return self.err("expected ',' or ']'"),
                    }
                }
            }
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.err("unexpected character"),
        }
    }

    /// `-? (0 | [1-9][0-9]*) (\.[0-9]+)? ([eE][+-]?[0-9]+)?`, which is all
    /// RFC 8259 allows: no `+1`, no `01`, no `.5`, no `1.`.
    fn number(&mut self) -> Result<Value, Error> {
        fn digits(p: &mut Parser) -> bool {
            let from = p.i;
            while matches!(p.peek(), Some(b'0'..=b'9')) {
                p.i += 1;
            }
            p.i > from
        }
        let st = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                digits(self);
            }
            _ => return self.err("bad number"),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !digits(self) {
                return self.err("bad number");
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !digits(self) {
                return self.err("bad number");
            }
        }
        let t = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("");
        match t.parse::<f64>() {
            // `1e400` is a valid JSON number and not a valid f64: refuse it
            // rather than hold an infinity no JSON writer can write.
            Ok(n) if n.is_finite() => Ok(Value::Number(n)),
            Ok(_) => self.err("number out of range"),
            Err(_) => self.err("bad number"),
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        self.ws();
        if self.peek() != Some(b'"') {
            return self.err("expected string");
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            // The input is a &str, so a run of plain bytes is valid UTF-8 as
            // it stands: copy it whole, up to the next quote, backslash or
            // control character.
            let from = self.i;
            while let Some(c) = self.peek() {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.i += 1;
            }
            out.push_str(std::str::from_utf8(&self.s[from..self.i]).unwrap_or(""));
            match self.peek() {
                None => return self.err("unterminated string"),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let Some(e) = self.peek() else {
                        return self.err("unterminated string");
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode()?),
                        _ => return self.err("bad escape"),
                    }
                }
                Some(_) => return self.err("control character in string"),
            }
        }
    }

    /// The character after `\u`, which takes a second `\u` when it is the
    /// first half of a surrogate pair. A half on its own is not a character,
    /// and is refused rather than turned into U+FFFD — or, worse, combined
    /// with whatever escape follows it, which used to swallow that one too.
    fn unicode(&mut self) -> Result<char, Error> {
        let hi = self.hex4()?;
        match hi {
            0xd800..=0xdbff => {
                if self.s.get(self.i..self.i + 2) != Some(b"\\u".as_slice()) {
                    return self.err("lone surrogate");
                }
                self.i += 2;
                let lo = self.hex4()?;
                if !(0xdc00..=0xdfff).contains(&lo) {
                    return self.err("lone surrogate");
                }
                let cp = 0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00);
                match char::from_u32(cp) {
                    Some(c) => Ok(c),
                    None => self.err("bad \\u escape"),
                }
            }
            0xdc00..=0xdfff => self.err("lone surrogate"),
            _ => match char::from_u32(hi) {
                Some(c) => Ok(c),
                None => self.err("bad \\u escape"),
            },
        }
    }

    /// Exactly four hex digits. `from_str_radix` on its own takes `+1ab`.
    fn hex4(&mut self) -> Result<u32, Error> {
        let h = match self.s.get(self.i..self.i + 4) {
            Some(h) if h.iter().all(u8::is_ascii_hexdigit) => h,
            _ => return self.err("bad \\u escape"),
        };
        self.i += 4;
        Ok(h.iter()
            .fold(0, |n, &c| n * 16 + (c as char).to_digit(16).unwrap_or(0)))
    }
}

/// Read one JSON document, keeping the order of every object's keys.
///
/// Strict to RFC 8259, with two allowances for the files this crate reads: a
/// leading BOM is skipped, and nesting is limited to [`MAX_DEPTH`].
pub fn parse(text: &str) -> Result<Value, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
        depth: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return p.err("trailing characters");
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let src = r#"{"a": [1, 2.5, "x\"y", true, null], "b": {"c": "日本語あ"}}"#;
        let v = parse(src).unwrap();
        let again = parse(&v.to_pretty()).unwrap();
        assert_eq!(v, again);
        assert_eq!(
            v.get("b").unwrap().get("c").unwrap().as_str(),
            Some("日本語あ")
        );
    }
}
