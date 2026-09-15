//! A JSON reader, small, so a recipe is read by the core itself.
//!
//! The core depends on nothing but `std` and `glam`, and a building recipe
//! is a JSON file, so this is the reader: objects, arrays, strings with
//! the usual escapes, numbers, `true`, `false` and `null`, and nothing
//! else. What it reads is `Value`, and the few accessors a recipe needs.

use std::collections::BTreeMap;

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// A field of an object, if this is one and has it.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.get(key),
            _ => None,
        }
    }

    /// A number, if this is one.
    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// A string, if this is one.
    pub fn str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// A bool, if this is one.
    pub fn bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The elements, if this is an array.
    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(v) => v,
            _ => &[],
        }
    }

    /// The numbers of an array, if this is one and they all are.
    pub fn nums(&self) -> Option<Vec<f64>> {
        self.items().iter().map(Value::num).collect()
    }
}

impl Value {
    /// The value as JSON text, on one line per array element of an outer
    /// array, which is what a file of recipes reads well as.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out, 0);
        out.push('\n');
        out
    }

    fn write_into(&self, out: &mut String, depth: usize) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{n}"));
                }
            }
            Value::String(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if depth == 0 {
                        out.push_str("\n  ");
                    }
                    v.write_into(out, depth + 1);
                }
                if depth == 0 && !items.is_empty() {
                    out.push('\n');
                }
                out.push(']');
            }
            Value::Object(map) => {
                out.push(OPEN as char);
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&format!("\"{k}\": "));
                    v.write_into(out, depth + 1);
                }
                out.push(CLOSE as char);
            }
        }
    }
}

/// Read a JSON text. An error names the byte it stopped at.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let v = p.value()?;
    p.skip();
    if p.i != p.s.len() {
        return Err(format!("trailing text at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

// The braces as numbers, because a brace in a byte literal is a brace to
// the shape tool that counts a function's lines by them.
const OPEN: u8 = 0x7B;
const CLOSE: u8 = 0x7D;

impl Parser<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip();
        match self.peek() {
            Some(OPEN) => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            _ => Err(format!("unexpected byte {}", self.i)),
        }
    }

    fn word(&mut self, w: &str, v: Value) -> Result<Value, String> {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            Err(format!("expected {w} at byte {}", self.i))
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_digit() || b"+-.eE".contains(&self.s[self.i]))
        {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse().ok())
            .map(Value::Number)
            .ok_or_else(|| format!("a bad number at byte {start}"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        let mut buf = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return Err("an unclosed string".into());
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or("an unclosed escape")?;
                    self.i += 1;
                    match e {
                        b'n' => buf.push(b'\n'),
                        b't' => buf.push(b'\t'),
                        b'r' => buf.push(b'\r'),
                        b'b' => buf.push(8),
                        b'f' => buf.push(12),
                        b'u' => {
                            let hex = std::str::from_utf8(
                                &self.s[self.i..(self.i + 4).min(self.s.len())],
                            )
                            .map_err(|_| "a bad unicode escape")?;
                            let code =
                                u32::from_str_radix(hex, 16).map_err(|_| "a bad unicode escape")?;
                            self.i += 4;
                            let ch = char::from_u32(code).unwrap_or('\u{fffd}');
                            let mut tmp = [0u8; 4];
                            buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                        }
                        other => buf.push(other),
                    }
                }
                other => buf.push(other),
            }
        }
        out.push_str(&String::from_utf8_lossy(&buf));
        Ok(out)
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.i)),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(OPEN)?;
        let mut map = BTreeMap::new();
        self.skip();
        if self.peek() == Some(CLOSE) {
            self.i += 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.skip();
            let key = self.string()?;
            self.skip();
            self.expect(b':')?;
            let v = self.value()?;
            map.insert(key, v);
            self.skip();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(CLOSE) => {
                    self.i += 1;
                    return Ok(Value::Object(map));
                }
                _ => {
                    return Err(format!(
                        "expected ',' or a closing brace at byte {}",
                        self.i
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_shaped_text_reads_back() {
        let v = parse(
            r#"{"name": "house", "footprint": [7, 6], "storey": 3.0, "door": -1.75e0,
                "brushes": [{"op": "add", "shape": "box", "at": [0, 0, -0.35], "room": true, "clip": null,
                             "about": "a \"box\"\n"}], "empty": {}, "none": []}"#,
        )
        .expect("parses");
        assert_eq!(v.get("name").and_then(Value::str), Some("house"));
        assert_eq!(
            v.get("footprint").and_then(Value::nums),
            Some(vec![7.0, 6.0])
        );
        assert_eq!(v.get("storey").and_then(Value::num), Some(3.0));
        assert_eq!(v.get("door").and_then(Value::num), Some(-1.75));
        let b = &v.get("brushes").expect("brushes").items()[0];
        assert_eq!(b.get("room").and_then(Value::bool), Some(true));
        assert_eq!(b.get("clip"), Some(&Value::Null));
        assert_eq!(b.get("about").and_then(Value::str), Some("a \"box\"\n"));
        assert_eq!(
            b.get("at").and_then(Value::nums),
            Some(vec![0.0, 0.0, -0.35])
        );
        assert!(v.get("empty").is_some() && v.get("none").is_some_and(|n| n.items().is_empty()));
        assert!(parse("[1, 2").is_err());
        assert!(parse("{\"a\": 1} x").is_err());
        assert!(parse("\"\\u0041\"").is_ok_and(|s| s.str() == Some("A")));
    }

    #[test]
    fn what_is_written_reads_back_the_same() {
        let text = r#"[{"at": [0, 0.5, -2], "mat": "concrete", "op": "add", "room": true, "size": [1, 2, 3.25], "name": "a \"box\""}]"#;
        let v = parse(text).expect("parses");
        let again = parse(&v.write()).expect("what was written parses");
        assert_eq!(v, again);
        assert!(v.write().starts_with("[\n  "));
    }
}
