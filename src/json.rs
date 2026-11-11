//! Small order-preserving JSON value, parser and writer (keeps the crate
//! dependency-free; XDTS / TDTS are JSON documents with a text header line).

use crate::Error;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
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

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Lenient string view: numbers are formatted, strings returned as-is.
    pub fn to_text(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Number(n) => fmt_num(*n),
            Value::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    pub fn to_pretty(&self) -> String {
        let mut s = String::new();
        write_value(&mut s, self, 0, true);
        s
    }

    pub fn to_compact(&self) -> String {
        let mut s = String::new();
        write_value(&mut s, self, 0, false);
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

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
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

fn indent(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push_str("  ");
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Array(_) | Value::Object(_))
}

fn write_value(out: &mut String, v: &Value, depth: usize, pretty: bool) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&fmt_num(*n)),
        Value::String(s) => escape(out, s),
        Value::Array(a) => {
            // arrays of scalars stay on one line for readability
            if !pretty || a.iter().all(is_scalar) {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        if pretty {
                            out.push(' ');
                        }
                    }
                    write_value(out, x, depth + 1, pretty);
                }
                out.push(']');
            } else {
                out.push_str("[\n");
                for (i, x) in a.iter().enumerate() {
                    indent(out, depth + 1);
                    write_value(out, x, depth + 1, pretty);
                    if i + 1 < a.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                indent(out, depth);
                out.push(']');
            }
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            if pretty {
                out.push('\n');
            }
            for (i, (k, x)) in m.iter().enumerate() {
                if pretty {
                    indent(out, depth + 1);
                }
                escape(out, k);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write_value(out, x, depth + 1, pretty);
                if i + 1 < m.len() {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                }
            }
            if pretty {
                indent(out, depth);
            }
            out.push('}');
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err<T>(&self, msg: &str) -> Result<T, Error> {
        Err(Error::Parse(format!("{msg} at byte {}", self.i)))
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), Error> {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", c as char))
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        self.ws();
        match self.s.get(self.i) {
            None => self.err("unexpected end"),
            Some(b'{') => {
                self.i += 1;
                let mut m = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Value::Object(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.eat(b':')?;
                    let v = self.value()?;
                    m.push((k, v));
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Value::Object(m));
                        }
                        _ => return self.err("expected ',' or '}'"),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Value::Array(a));
                }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Value::Array(a));
                        }
                        _ => return self.err("expected ',' or ']'"),
                    }
                }
            }
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') if self.s[self.i..].starts_with(b"true") => {
                self.i += 4;
                Ok(Value::Bool(true))
            }
            Some(b'f') if self.s[self.i..].starts_with(b"false") => {
                self.i += 5;
                Ok(Value::Bool(false))
            }
            Some(b'n') if self.s[self.i..].starts_with(b"null") => {
                self.i += 4;
                Ok(Value::Null)
            }
            Some(_) => {
                let st = self.i;
                while self.i < self.s.len()
                    && matches!(
                        self.s[self.i],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("");
                t.parse::<f64>()
                    .map(Value::Number)
                    .or_else(|_| self.err("bad number"))
            }
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        self.ws();
        if self.s.get(self.i) != Some(&b'"') {
            return self.err("expected string");
        }
        self.i += 1;
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let c = match self.s.get(self.i) {
                Some(&c) => c,
                None => return self.err("unterminated string"),
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self
                        .s
                        .get(self.i)
                        .ok_or(Error::Parse("bad escape".into()))?;
                    self.i += 1;
                    match e {
                        b'n' => buf.push(b'\n'),
                        b't' => buf.push(b'\t'),
                        b'r' => buf.push(b'\r'),
                        b'b' => buf.push(8),
                        b'f' => buf.push(12),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xd800..0xdc00).contains(&cp)
                                && self.s.get(self.i) == Some(&b'\\')
                                && self.s.get(self.i + 1) == Some(&b'u')
                            {
                                self.i += 2;
                                let lo = self.hex4()?;
                                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo.wrapping_sub(0xdc00));
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
                            let mut tmp = [0u8; 4];
                            buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                        }
                        other => buf.push(other),
                    }
                }
                c => buf.push(c),
            }
        }
        String::from_utf8(buf).or_else(|_| self.err("invalid utf-8"))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .ok_or(Error::Parse("bad \\u".into()))?;
        self.i += 4;
        u32::from_str_radix(std::str::from_utf8(h).unwrap_or("x"), 16)
            .map_err(|_| Error::Parse("bad \\u escape".into()))
    }
}

pub fn parse(text: &str) -> Result<Value, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
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
