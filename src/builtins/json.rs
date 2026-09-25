use std::cell::RefCell;
use std::rc::Rc;

use crate::value::Value;

// A small dependency-free JSON parser/serializer. Dynamic objects come back
// as `Map`, arrays as `Array`, values as Int/Float/Str/Bool, null as Nil.

pub fn parse(s: &str) -> Result<Value, String> {
    let b: Vec<u8> = s.as_bytes().to_vec();
    let mut p = P { b: &b, pos: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.pos != b.len() {
        return Err(format!("trailing characters at byte {}", p.pos));
    }
    Ok(v)
}

struct P<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.pos < self.b.len() && matches!(self.b[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: u8, what: &str) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!("expected `{}` while {}", c as char, what))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(Rc::from(self.string()?.as_str()))),
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Value::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Value::Bool(false))
            }
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Value::Nil)
            }
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected byte `{}`", c as char)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn literal(&mut self, lit: &[u8]) -> Result<(), String> {
        if self.b.get(self.pos..self.pos + lit.len()) == Some(lit) {
            self.pos += lit.len();
            Ok(())
        } else {
            Err(format!("invalid literal (expected {:?})", String::from_utf8_lossy(lit)))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{', "object")?;
        self.ws();
        let mut fields = Vec::new();
        if self.eat(b'}') {
            return Ok(Value::Map(Rc::new(RefCell::new(fields))));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err("object key must be a string".to_string());
            }
            let key = self.string()?;
            self.ws();
            self.expect(b':', "object")?;
            self.ws();
            let v = self.value()?;
            fields.push((key, v));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b'}', "object")?;
            break;
        }
        Ok(Value::Map(Rc::new(RefCell::new(fields))))
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[', "array")?;
        self.ws();
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Value::Array(Rc::new(RefCell::new(items))));
        }
        loop {
            self.ws();
            let v = self.value()?;
            items.push(v);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b']', "array")?;
            break;
        }
        Ok(Value::Array(Rc::new(RefCell::new(items))))
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.eat(b'-') {}
        while self.peek().map_or(false, |c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let raw = std::str::from_utf8(&self.b[start..self.pos])
            .map_err(|_| "invalid number".to_string())?;
        if is_float {
            raw.parse::<f64>()
                .map(Value::Float)
                .map_err(|_| format!("invalid number `{}`", raw))
        } else {
            raw.parse::<i64>()
                .map(Value::Int)
                .map_err(|_| format!("invalid number `{}`", raw))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if !self.eat(b'"') {
            return Err("expected `\"`".to_string());
        }
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.pos += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = self
                        .peek()
                        .ok_or("unterminated escape")?;
                    self.pos += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..=0xDBFF).contains(&hi) {
                                // low surrogate must follow
                                if self.eat(b'\\') && self.eat(b'u') {
                                    let lo = self.hex4()?;
                                    let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                    out.push(char::from_u32(cp).ok_or("invalid surrogate pair")?);
                                } else {
                                    return Err("invalid surrogate pair".to_string());
                                }
                            } else if (0xDC00..=0xDFFF).contains(&hi) {
                                return Err("unexpected low surrogate".to_string());
                            } else {
                                out.push(char::from_u32(hi).ok_or("invalid unicode escape")?);
                            }
                        }
                        other => return Err(format!("invalid escape `\\{}`", other as char)),
                    }
                }
                _ => out.push(c as char),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.peek().ok_or("unterminated unicode escape")?;
            let d = (c as char).to_digit(16).ok_or("invalid unicode escape")?;
            self.pos += 1;
            v = v * 16 + d;
        }
        Ok(v)
    }
}

// ---------------- serialization ----------------

pub fn stringify(v: &Value, pretty: bool) -> Result<String, String> {
    let mut out = String::new();
    let mut ind = 0isize;
    write_value(v, pretty, &mut ind, &mut out)?;
    Ok(out)
}

fn indent(pretty: bool, ind: isize, out: &mut String) {
    if pretty {
        out.push('\n');
        for _ in 0..ind {
            out.push_str("  ");
        }
    }
}

fn write_value(v: &Value, pretty: bool, ind: &mut isize, out: &mut String) -> Result<(), String> {
    match v {
        Value::Nil => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => out.push_str(&json_float(*f)),
        Value::Str(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\u{8}' => out.push_str("\\b"),
                    '\u{c}' => out.push_str("\\f"),
                    c if (c as u32) < 0x20 => {
                        out.push_str(&format!("\\u{:04x}", c as u32))
                    }
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Array(a) => {
            let a = a.borrow();
            if a.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            *ind += 1;
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(pretty, *ind, out);
                write_value(item, pretty, ind, out)?;
            }
            *ind -= 1;
            indent(pretty, *ind, out);
            out.push(']');
        }
        Value::Map(m) => {
            let m = m.borrow();
            if m.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push('{');
            *ind += 1;
            for (i, (k, val)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(pretty, *ind, out);
                out.push('"');
                out.push_str(k);
                out.push_str("\": ");
                write_value(val, pretty, ind, out)?;
            }
            *ind -= 1;
            indent(pretty, *ind, out);
            out.push('}');
        }
        Value::Struct(s) => {
            let s = s.borrow();
            out.push('{');
            *ind += 1;
            for (i, (k, val)) in s.fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(pretty, *ind, out);
                out.push_str(&format!("\"{}\": ", k));
                write_value(val, pretty, ind, out)?;
            }
            *ind -= 1;
            indent(pretty, *ind, out);
            out.push('}');
        }
        Value::Opt(o) => match o {
            crate::value::OptV::Some(v) => {
                write_value(v, pretty, ind, out)?;
            }
            crate::value::OptV::None => {
                out.push_str("null");
            }
        },
        other => {
            return Err(format!(
                "cannot serialize `{}` to JSON",
                other.type_name()
            ))
        }
    }
    Ok(())
}

fn json_float(f: f64) -> String {
    if f == f.trunc() && f.is_finite() && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else if f.is_nan() {
        "null".to_string()
    } else {
        format!("{}", f)
    }
}