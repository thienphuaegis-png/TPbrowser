// Minimal RFC 8259 JSON parser & serializer (zero external dependencies)

#[derive(Debug, Clone, PartialEq)]
pub enum JsonVal {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JsonVal>),
    Obj(Vec<(String, JsonVal)>),
}

impl JsonVal {
    pub fn obj(pairs: Vec<(&str, JsonVal)>) -> Self {
        JsonVal::Obj(
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&JsonVal> {
        match self {
            JsonVal::Obj(pairs) => pairs.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonVal::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonVal::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JsonVal::Num(n) if n.is_finite() => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            JsonVal::Num(n) if n.is_finite() && *n >= 0.0 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[JsonVal]> {
        match self {
            JsonVal::Arr(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            JsonVal::Null => out.push_str("null"),
            JsonVal::Bool(true) => out.push_str("true"),
            JsonVal::Bool(false) => out.push_str("false"),
            JsonVal::Num(n) => {
                if !n.is_finite() {
                    out.push_str("null");
                } else if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            JsonVal::Str(s) => write_escaped_str(s, out),
            JsonVal::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            JsonVal::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_escaped_str(k, out);
                    out.push(':');
                    v.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

pub fn write_escaped_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn parse_json(input: &str) -> Result<JsonVal, String> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let val = p.parse_val(0)?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("Trailing characters at byte {}", p.pos));
    }
    Ok(val)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_val(&mut self, depth: usize) -> Result<JsonVal, String> {
        if depth > 64 {
            return Err("JSON nesting too deep".to_string());
        }
        self.skip_ws();
        let b = self.peek().ok_or_else(|| "Unexpected EOF".to_string())?;
        match b {
            b'n' => {
                if self.bytes[self.pos..].starts_with(b"null") {
                    self.pos += 4;
                    Ok(JsonVal::Null)
                } else {
                    Err(format!("Invalid literal at {}", self.pos))
                }
            }
            b't' => {
                if self.bytes[self.pos..].starts_with(b"true") {
                    self.pos += 4;
                    Ok(JsonVal::Bool(true))
                } else {
                    Err(format!("Invalid literal at {}", self.pos))
                }
            }
            b'f' => {
                if self.bytes[self.pos..].starts_with(b"false") {
                    self.pos += 5;
                    Ok(JsonVal::Bool(false))
                } else {
                    Err(format!("Invalid literal at {}", self.pos))
                }
            }
            b'"' => self.parse_str().map(JsonVal::Str),
            b'[' => self.parse_arr(depth + 1),
            b'{' => self.parse_obj(depth + 1),
            b'-' | b'0'..=b'9' => self.parse_num(),
            _ => Err(format!("Unexpected byte '{}' at {}", b as char, self.pos)),
        }
    }

    fn parse_str(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err("Expected '\"'".to_string());
        }
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let esc = *self
                    .bytes
                    .get(self.pos)
                    .ok_or_else(|| "Unterminated escape".to_string())?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\x08'),
                    b'f' => out.push('\x0c'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let code = self.parse_hex4()?;
                        if (0xD800..=0xDBFF).contains(&code) {
                            if self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u") {
                                self.pos += 2;
                                let low = self.parse_hex4()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    let cp = 0x10000
                                        + (((code as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                                    if let Some(ch) = char::from_u32(cp) {
                                        out.push(ch);
                                    }
                                }
                            }
                        } else if let Some(ch) = char::from_u32(code as u32) {
                            out.push(ch);
                        }
                    }
                    _ => return Err(format!("Invalid escape '\\{}'", esc as char)),
                }
            } else if b < 0x80 {
                out.push(b as char);
                self.pos += 1;
            } else {
                let rest = std::str::from_utf8(&self.bytes[self.pos..])
                    .map_err(|_| "Invalid UTF-8 in JSON string".to_string())?;
                let ch = rest.chars().next().unwrap();
                out.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Err("Unterminated string".to_string())
    }

    fn parse_hex4(&mut self) -> Result<u16, String> {
        if self.pos + 4 > self.bytes.len() {
            return Err("Incomplete \\uXXXX escape".to_string());
        }
        let s = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| "Invalid hex".to_string())?;
        let v = u16::from_str_radix(s, 16).map_err(|_| format!("Invalid hex '{}'", s))?;
        self.pos += 4;
        Ok(v)
    }

    fn parse_num(&mut self) -> Result<JsonVal, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b'0'..=b'9') = self.peek() {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }
        let slice = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "Invalid number".to_string())?;
        let n = slice
            .parse::<f64>()
            .map_err(|_| format!("Bad number '{}'", slice))?;
        Ok(JsonVal::Num(n))
    }

    fn parse_arr(&mut self, depth: usize) -> Result<JsonVal, String> {
        self.pos += 1; // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonVal::Arr(items));
        }
        loop {
            items.push(self.parse_val(depth)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonVal::Arr(items));
                }
                _ => return Err(format!("Expected ',' or ']' at {}", self.pos)),
            }
        }
    }

    fn parse_obj(&mut self, depth: usize) -> Result<JsonVal, String> {
        self.pos += 1; // '{'
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonVal::Obj(pairs));
        }
        loop {
            self.skip_ws();
            let k = self.parse_str()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("Expected ':' at {}", self.pos));
            }
            self.pos += 1;
            let v = self.parse_val(depth)?;
            pairs.push((k, v));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonVal::Obj(pairs));
                }
                _ => return Err(format!("Expected ',' or '}}' at {}", self.pos)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_unicode_and_nested() {
        let raw = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"tp_eval_js","arguments":{"code":"return 'Xin chào \u2728';","tab_id":2,"flag":true}}}"#;
        let val = parse_json(raw).unwrap();
        assert_eq!(val.get("id").and_then(|v| v.as_i64()), Some(7));
        let args = val.get("params").unwrap().get("arguments").unwrap();
        assert_eq!(args.get("code").and_then(|v| v.as_str()), Some("return 'Xin chào ✨';"));
        assert_eq!(args.get("tab_id").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(args.get("flag").and_then(|v| v.as_bool()), Some(true));
        let dumped = val.to_json();
        let reparsed = parse_json(&dumped).unwrap();
        assert_eq!(val, reparsed);
    }

    #[test]
    fn test_surrogate_pair() {
        let raw = r#"{"emoji":"\uD83D\uDE80"}"#;
        let val = parse_json(raw).unwrap();
        assert_eq!(val.get("emoji").and_then(|v| v.as_str()), Some("🚀"));
    }
}
