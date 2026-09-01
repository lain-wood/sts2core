//! 最小 JSON 读取器。存在的唯一理由是让 `sts2core` 保持**零外部依赖**。
//!
//! 这里用 `Vec` / `String` 是可以的：POD + `Copy` 那条不变量约束的是
//! [`crate::state::State`]，不是加载器。trace 在进入内核之前就已经被翻译成
//! 定长结构了，堆分配只发生在启动阶段。

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key).filter(|v| !matches!(v, Json::Null)),
            _ => None,
        }
    }

    /// 取字段，缺失或 null 都当作缺失 —— mod 会把不适用的字段写成 null。
    pub fn i64(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_i64()
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    pub fn arr(&self, key: &str) -> Option<&[Json]> {
        self.get(key)?.as_arr()
    }

    pub fn obj(&self, key: &str) -> Option<&BTreeMap<String, Json>> {
        self.get(key)?.as_obj()
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) => Some(*n as i64),
            // 卡费在 mod 里是显示字符串（"1" / "X" / "无"），数字化在调用方做
            Json::Str(s) => s.parse::<i64>().ok(),
            Json::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_obj(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Obj(m) => Some(m),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ParseError {
    pub offset: usize,
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON 解析失败于字节 {}: {}", self.offset, self.msg)
    }
}

pub fn parse(src: &str) -> Result<Json, ParseError> {
    let mut p = Parser { b: src.as_bytes(), i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(p.err("文档结尾之后还有内容"));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, msg: &str) -> ParseError {
        ParseError { offset: self.i, msg: msg.to_string() }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), ParseError> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(&format!("期望 '{}'", c as char)))
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, ParseError> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("无法识别的字面量"))
        }
    }

    fn value(&mut self) -> Result<Json, ParseError> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            _ => Err(self.err("期望一个值")),
        }
    }

    fn object(&mut self) -> Result<Json, ParseError> {
        self.eat(b'{')?;
        let mut m = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(m));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.eat(b':')?;
            self.ws();
            let v = self.value()?;
            m.insert(k, v);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(m));
                }
                _ => return Err(self.err("对象里期望 ',' 或 '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<Json, ParseError> {
        self.eat(b'[')?;
        let mut v = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(v));
        }
        loop {
            self.ws();
            v.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(v));
                }
                _ => return Err(self.err("数组里期望 ',' 或 ']'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or_else(|| self.err("字符串没有闭合"))?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let e = self.peek().ok_or_else(|| self.err("转义序列被截断"))?;
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
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return Err(self.err("未知转义")),
                    }
                }
                _ => {
                    // 非 ASCII 直接按 UTF-8 字节搬运（录制器用 ensure_ascii=False，
                    // 中文卡名本来就是裸 UTF-8）
                    let start = self.i;
                    let len = utf8_len(c);
                    if self.i + len > self.b.len() {
                        return Err(self.err("UTF-8 序列被截断"));
                    }
                    self.i += len;
                    match std::str::from_utf8(&self.b[start..self.i]) {
                        Ok(s) => out.push_str(s),
                        Err(_) => return Err(self.err("非法 UTF-8")),
                    }
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        if self.i + 4 > self.b.len() {
            return Err(self.err("\\u 转义被截断"));
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + 4])
            .map_err(|_| self.err("\\u 转义不是 ASCII"))?;
        let v = u32::from_str_radix(s, 16).map_err(|_| self.err("\\u 转义不是十六进制"))?;
        self.i += 4;
        Ok(v)
    }

    fn unicode_escape(&mut self) -> Result<char, ParseError> {
        let hi = self.hex4()?;
        // 代理对：高位后面必须跟 \uDC00-\uDFFF
        if (0xD800..0xDC00).contains(&hi) {
            if self.peek() == Some(b'\\') && self.b.get(self.i + 1) == Some(&b'u') {
                self.i += 2;
                let lo = self.hex4()?;
                if (0xDC00..0xE000).contains(&lo) {
                    let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                    return char::from_u32(c).ok_or_else(|| self.err("非法码点"));
                }
            }
            return Err(self.err("孤立的高位代理"));
        }
        char::from_u32(hi).ok_or_else(|| self.err("非法码点"))
    }

    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| self.err("数字不是 ASCII"))?;
        s.parse::<f64>().map(Json::Num).map_err(|_| self.err("数字格式非法"))
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shapes_a_trace_actually_uses() {
        let src = r#"
        { "version": 1,
          "frames": [
            { "i": 0,
              "obs": { "energy": 3, "player": { "hp": 70, "block": 0 },
                       "hand": [ { "name": "打击", "cost": "1", "upgraded": false } ] },
              "action": null } ] }
        "#;
        let j = parse(src).expect("应当解析成功");
        assert_eq!(j.i64("version"), Some(1));
        let frames = j.arr("frames").unwrap();
        assert_eq!(frames.len(), 1);
        let obs = frames[0].get("obs").unwrap();
        assert_eq!(obs.i64("energy"), Some(3));
        assert_eq!(obs.get("player").unwrap().i64("hp"), Some(70));
        let hand = obs.arr("hand").unwrap();
        assert_eq!(hand[0].str("name"), Some("打击"));
        // 卡费是显示字符串，as_i64 要能把它拉成数字
        assert_eq!(hand[0].i64("cost"), Some(1));
        assert_eq!(hand[0].get("upgraded").unwrap().as_bool(), Some(false));
        // null 字段等同于缺失：动作为 null 的收尾帧不该被当成有动作
        assert!(frames[0].get("action").is_none());
    }

    #[test]
    fn rejects_garbage_instead_of_silently_succeeding() {
        assert!(parse("{").is_err());
        assert!(parse("{\"a\": }").is_err());
        assert!(parse("[1, 2").is_err());
        assert!(parse("{} trailing").is_err());
    }

    #[test]
    fn handles_escapes_and_surrogate_pairs() {
        let j = parse(r#"{"a":"line\nbreak 中 😀"}"#).unwrap();
        assert_eq!(j.str("a"), Some("line\nbreak 中 😀"));
    }
}
