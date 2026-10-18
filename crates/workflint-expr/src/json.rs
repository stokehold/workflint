//! The type of a JSON literal passed to `fromJSON()`, and the errors of broken JSON.
//!
//! The reference passes the literal to Go's `json.Unmarshal` (`expr_sema.go:861-906`) and types
//! the decoded value with `typeOfJSONValue` (`expr_type.go:441-474`). Its messages embed the
//! text and the offset of Go's `SyntaxError`. [`Scanner`] ports the state machine in Go's
//! `encoding/json/scanner.go`, which `Unmarshal` runs over the whole input before it decodes
//! anything, so the first error and its offset match Go's byte for byte. [`type_of`] then reads
//! the valid input a second time to build its type.

use std::collections::BTreeMap;

use workflint_diagnostics::quote::quote_rune;

use crate::types::{MAX_TYPE_DEPTH, Ty};

/// The deepest nesting Go's scanner accepts.
const GO_MAX_DEPTH: usize = 10_000;

/// A syntax error in JSON, as Go's `json.SyntaxError` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxError {
    /// The number of bytes read when the error occurred: the unexpected byte is the last one,
    /// or the input ended.
    pub(crate) offset: usize,
    /// Go's message, such as `unexpected end of JSON input`.
    pub(crate) message: String,
}

/// Returns the type of the JSON value in `text`, or Go's syntax error.
///
/// Valid JSON with a number beyond the range of a double gives `None`: Go's decoder then
/// fails with an error that is not a syntax error, and the reference reports nothing.
pub(crate) fn type_of(text: &str) -> Result<Option<Ty>, SyntaxError> {
    Scanner::default().check(text.as_bytes())?;
    Ok(TypeBuilder::new(text.as_bytes()).build())
}

/// The scanner's state, which Go keeps as a function pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    BeginValueOrEmpty,
    BeginValue,
    BeginStringOrEmpty,
    BeginString,
    EndValue,
    EndTop,
    InString,
    InStringEsc,
    InStringEscU,
    InStringEscU1,
    InStringEscU12,
    InStringEscU123,
    Neg,
    One,
    Zero,
    Dot,
    Dot0,
    E,
    ESign,
    E0,
    T,
    Tr,
    Tru,
    F,
    Fa,
    Fal,
    Fals,
    N,
    Nu,
    Nul,
    Error,
}

/// What the scanner expects next inside an object or array, Go's `parseObjectKey`,
/// `parseObjectValue`, and `parseArrayValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parse {
    ObjectKey,
    ObjectValue,
    ArrayValue,
}

/// Go's `scanner`, reduced to what `checkValid` needs: each step either continues or fails.
#[derive(Debug)]
struct Scanner {
    step: Step,
    parse_state: Vec<Parse>,
    end_top: bool,
    error: Option<SyntaxError>,
    bytes: usize,
}

impl Default for Scanner {
    fn default() -> Self {
        Self {
            step: Step::BeginValue,
            parse_state: Vec::new(),
            end_top: false,
            error: None,
            bytes: 0,
        }
    }
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

impl Scanner {
    /// Go's `checkValid`: feeds each byte, then the end of the input.
    fn check(mut self, data: &[u8]) -> Result<(), SyntaxError> {
        for &c in data {
            self.bytes += 1;
            self.step(c);
            if let Some(error) = self.error {
                return Err(error);
            }
        }
        self.eof()
    }

    /// Go's `eof`. A value that only a following byte can end, such as a number, ends when
    /// the scanner sees a space. If the space is wrong there instead, its error stands.
    fn eof(mut self) -> Result<(), SyntaxError> {
        if self.end_top {
            return Ok(());
        }
        self.step(b' ');
        if self.end_top {
            return Ok(());
        }
        Err(self.error.unwrap_or_else(|| SyntaxError {
            offset: self.bytes,
            message: "unexpected end of JSON input".to_owned(),
        }))
    }

    /// Records Go's error for the unexpected byte `c`.
    fn fail(&mut self, c: u8, context: &str) {
        self.step = Step::Error;
        self.error = Some(SyntaxError {
            offset: self.bytes,
            message: format!("invalid character {} {context}", quote_char(c)),
        });
    }

    /// Go's `pushParseState`.
    fn push(&mut self, c: u8, parse: Parse) {
        self.parse_state.push(parse);
        if self.parse_state.len() > GO_MAX_DEPTH {
            self.fail(c, "exceeded max depth");
        }
    }

    /// Go's `popParseState`.
    fn pop(&mut self) {
        self.parse_state.pop();
        if self.parse_state.is_empty() {
            self.step = Step::EndTop;
            self.end_top = true;
        } else {
            self.step = Step::EndValue;
        }
    }

    /// Moves to `next` if `c` is `want`, else fails as Go's literal states do.
    fn literal(&mut self, c: u8, want: u8, next: Step, context: &str) {
        if c == want {
            self.step = next;
        } else {
            self.fail(c, context);
        }
    }

    fn step(&mut self, c: u8) {
        match self.step {
            Step::BeginValueOrEmpty => {
                if is_space(c) {
                } else if c == b']' {
                    self.end_value(c);
                } else {
                    self.begin_value(c);
                }
            }
            Step::BeginValue => self.begin_value(c),
            Step::BeginStringOrEmpty => {
                if is_space(c) {
                } else if c == b'}' {
                    if let Some(top) = self.parse_state.last_mut() {
                        *top = Parse::ObjectValue;
                    }
                    self.end_value(c);
                } else {
                    self.begin_string(c);
                }
            }
            Step::BeginString => self.begin_string(c),
            Step::EndValue => self.end_value(c),
            Step::EndTop => {
                if !is_space(c) {
                    self.fail(c, "after top-level value");
                }
            }
            Step::InString => match c {
                b'"' => self.step = Step::EndValue,
                b'\\' => self.step = Step::InStringEsc,
                0..0x20 => self.fail(c, "in string literal"),
                _ => {}
            },
            Step::InStringEsc => match c {
                b'b' | b'f' | b'n' | b'r' | b't' | b'\\' | b'/' | b'"' => {
                    self.step = Step::InString;
                }
                b'u' => self.step = Step::InStringEscU,
                _ => self.fail(c, "in string escape code"),
            },
            Step::InStringEscU => self.hex(c, Step::InStringEscU1),
            Step::InStringEscU1 => self.hex(c, Step::InStringEscU12),
            Step::InStringEscU12 => self.hex(c, Step::InStringEscU123),
            Step::InStringEscU123 => self.hex(c, Step::InString),
            Step::Neg => match c {
                b'0' => self.step = Step::Zero,
                b'1'..=b'9' => self.step = Step::One,
                _ => self.fail(c, "in numeric literal"),
            },
            Step::One => {
                if !c.is_ascii_digit() {
                    self.zero(c);
                }
            }
            Step::Zero => self.zero(c),
            Step::Dot => {
                if c.is_ascii_digit() {
                    self.step = Step::Dot0;
                } else {
                    self.fail(c, "after decimal point in numeric literal");
                }
            }
            Step::Dot0 => match c {
                b'0'..=b'9' => {}
                b'e' | b'E' => self.step = Step::E,
                _ => self.end_value(c),
            },
            Step::E => {
                if c == b'+' || c == b'-' {
                    self.step = Step::ESign;
                } else {
                    self.e_sign(c);
                }
            }
            Step::ESign => self.e_sign(c),
            Step::E0 => {
                if !c.is_ascii_digit() {
                    self.end_value(c);
                }
            }
            Step::T => self.literal(c, b'r', Step::Tr, "in literal true (expecting 'r')"),
            Step::Tr => self.literal(c, b'u', Step::Tru, "in literal true (expecting 'u')"),
            Step::Tru => self.literal(c, b'e', Step::EndValue, "in literal true (expecting 'e')"),
            Step::F => self.literal(c, b'a', Step::Fa, "in literal false (expecting 'a')"),
            Step::Fa => self.literal(c, b'l', Step::Fal, "in literal false (expecting 'l')"),
            Step::Fal => self.literal(c, b's', Step::Fals, "in literal false (expecting 's')"),
            Step::Fals => {
                self.literal(c, b'e', Step::EndValue, "in literal false (expecting 'e')");
            }
            Step::N => self.literal(c, b'u', Step::Nu, "in literal null (expecting 'u')"),
            Step::Nu => self.literal(c, b'l', Step::Nul, "in literal null (expecting 'l')"),
            Step::Nul => self.literal(c, b'l', Step::EndValue, "in literal null (expecting 'l')"),
            Step::Error => {}
        }
    }

    /// Go's `stateBeginValue`.
    fn begin_value(&mut self, c: u8) {
        match c {
            _ if is_space(c) => {}
            b'{' => {
                self.step = Step::BeginStringOrEmpty;
                self.push(c, Parse::ObjectKey);
            }
            b'[' => {
                self.step = Step::BeginValueOrEmpty;
                self.push(c, Parse::ArrayValue);
            }
            b'"' => self.step = Step::InString,
            b'-' => self.step = Step::Neg,
            b'0' => self.step = Step::Zero,
            b't' => self.step = Step::T,
            b'f' => self.step = Step::F,
            b'n' => self.step = Step::N,
            b'1'..=b'9' => self.step = Step::One,
            _ => self.fail(c, "looking for beginning of value"),
        }
    }

    /// Go's `stateBeginString`.
    fn begin_string(&mut self, c: u8) {
        if is_space(c) {
        } else if c == b'"' {
            self.step = Step::InString;
        } else {
            self.fail(c, "looking for beginning of object key string");
        }
    }

    /// Go's `stateEndValue`.
    fn end_value(&mut self, c: u8) {
        let Some(&parse) = self.parse_state.last() else {
            self.step = Step::EndTop;
            self.end_top = true;
            self.step(c);
            return;
        };
        if is_space(c) {
            self.step = Step::EndValue;
            return;
        }
        match parse {
            Parse::ObjectKey => {
                if c == b':' {
                    self.set_top(Parse::ObjectValue);
                    self.step = Step::BeginValue;
                } else {
                    self.fail(c, "after object key");
                }
            }
            Parse::ObjectValue => match c {
                b',' => {
                    self.set_top(Parse::ObjectKey);
                    self.step = Step::BeginString;
                }
                b'}' => self.pop(),
                _ => self.fail(c, "after object key:value pair"),
            },
            Parse::ArrayValue => match c {
                b',' => self.step = Step::BeginValue,
                b']' => self.pop(),
                _ => self.fail(c, "after array element"),
            },
        }
    }

    fn set_top(&mut self, parse: Parse) {
        if let Some(top) = self.parse_state.last_mut() {
            *top = parse;
        }
    }

    /// Go's `state0`, which `state1` falls through to.
    fn zero(&mut self, c: u8) {
        match c {
            b'.' => self.step = Step::Dot,
            b'e' | b'E' => self.step = Step::E,
            _ => self.end_value(c),
        }
    }

    /// Go's `stateESign`.
    fn e_sign(&mut self, c: u8) {
        if c.is_ascii_digit() {
            self.step = Step::E0;
        } else {
            self.fail(c, "in exponent of numeric literal");
        }
    }

    /// One hexadecimal digit of a `\u` escape.
    fn hex(&mut self, c: u8, next: Step) {
        if c.is_ascii_hexdigit() {
            self.step = next;
        } else {
            self.fail(c, r"in \u hexadecimal character escape");
        }
    }
}

/// Go's `quoteChar`. It converts the byte to the code point of the same value, so a byte of a
/// multi-byte character prints as a Latin-1 character, such as `Ã` for `0xC3`.
fn quote_char(c: u8) -> String {
    quote_rune(char::from(c))
}

/// An object or array whose elements the builder is reading.
enum Frame {
    /// The merged type of the elements so far.
    Array(Option<Ty>),
    /// The properties so far, and the key of the value being read.
    Object(BTreeMap<String, Ty>, String),
}

/// Reads valid JSON and builds its type, as the reference's `typeOfJSONValue` types the value
/// that Go decodes. It keeps its own stack, so deep nesting cannot overflow the call stack.
struct TypeBuilder<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// Whether a number is beyond the range of a double.
    overflow: bool,
}

impl<'a> TypeBuilder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            overflow: false,
        }
    }

    fn build(mut self) -> Option<Ty> {
        let mut stack: Vec<Frame> = Vec::new();
        loop {
            // Read the start of a value. An object or array that is not empty pushes a frame
            // and reads its first element.
            self.skip_space();
            let mut ty = match self.peek() {
                b'[' | b'{' if stack.len() >= MAX_TYPE_DEPTH => {
                    self.skip_value();
                    Ty::Any
                }
                b'[' => {
                    self.pos += 1;
                    self.skip_space();
                    if self.peek() == b']' {
                        self.pos += 1;
                        Ty::array(Ty::Any)
                    } else {
                        stack.push(Frame::Array(None));
                        continue;
                    }
                }
                b'{' => {
                    self.pos += 1;
                    self.skip_space();
                    if self.peek() == b'}' {
                        self.pos += 1;
                        Ty::empty_strict()
                    } else {
                        let key = self.key();
                        stack.push(Frame::Object(BTreeMap::new(), key));
                        continue;
                    }
                }
                b'"' => {
                    self.skip_string();
                    Ty::String
                }
                b't' | b'f' => {
                    self.skip_word();
                    Ty::Bool
                }
                b'n' => {
                    self.skip_word();
                    Ty::Null
                }
                _ => {
                    self.number();
                    Ty::Number
                }
            };

            // Add the value to its object or array. A `,` leads to the next element, and `]`
            // or `}` ends the object or array, which is then a value of its own parent.
            loop {
                let Some(frame) = stack.last_mut() else {
                    return (!self.overflow).then_some(ty);
                };
                self.skip_space();
                let close = self.peek();
                self.pos += 1;
                match frame {
                    Frame::Array(elem) => {
                        // The frame owns the element type, so merging changes it in place and
                        // typing an array takes time linear in its size.
                        let merged = match elem.take() {
                            Some(mut elem) => {
                                elem.merge_in_place(&ty);
                                elem
                            }
                            None => ty,
                        };
                        if close == b',' {
                            *elem = Some(merged);
                            break;
                        }
                        ty = Ty::array(merged);
                    }
                    Frame::Object(props, key) => {
                        props.insert(std::mem::take(key), ty);
                        if close == b',' {
                            *key = self.key();
                            break;
                        }
                        ty = Ty::strict(std::mem::take(props));
                    }
                }
                stack.pop();
            }
        }
    }

    /// The byte at the cursor, or `0` at the end, which valid input never reaches where this
    /// is called.
    fn peek(&self) -> u8 {
        self.bytes.get(self.pos).copied().unwrap_or(0)
    }

    fn skip_space(&mut self) {
        while is_space(self.peek()) {
            self.pos += 1;
        }
    }

    /// Reads an object key and the `:` after it.
    fn key(&mut self) -> String {
        self.skip_space();
        let start = self.pos;
        self.skip_string();
        let key = unquote(self.bytes.get(start..self.pos).unwrap_or_default());
        self.skip_space();
        // The `:`.
        self.pos += 1;
        key
    }

    /// Skips a string, quotes included.
    fn skip_string(&mut self) {
        self.pos += 1;
        while let Some(&c) = self.bytes.get(self.pos) {
            self.pos += 1;
            match c {
                b'"' => return,
                b'\\' => self.pos += 1,
                _ => {}
            }
        }
    }

    /// Skips `true`, `false`, or `null`.
    fn skip_word(&mut self) {
        while self.peek().is_ascii_lowercase() {
            self.pos += 1;
        }
    }

    /// Reads a number and notes whether it is beyond the range of a double, as Go's
    /// `strconv.ParseFloat` reports.
    fn number(&mut self) {
        let start = self.pos;
        while matches!(self.peek(), b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
            self.pos += 1;
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.pos).unwrap_or_default());
        let value = text.ok().and_then(|text| text.parse::<f64>().ok());
        if !value.is_some_and(f64::is_finite) {
            self.overflow = true;
        }
    }

    /// Skips an object or array with everything in it. Go decodes the numbers in it all the
    /// same, so a number beyond the range of a double is noted here too.
    fn skip_value(&mut self) {
        let mut depth = 0_usize;
        while let Some(&c) = self.bytes.get(self.pos) {
            match c {
                b'"' => {
                    self.skip_string();
                    continue;
                }
                b'-' | b'0'..=b'9' => {
                    self.number();
                    continue;
                }
                b'[' | b'{' => depth += 1,
                b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.pos += 1;
                        return;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
    }
}

/// Decodes a JSON string, quotes included, as Go's `unquote` does. A `\u` escape of a
/// surrogate that is not part of a pair becomes U+FFFD.
fn unquote(quoted: &[u8]) -> String {
    let inner = quoted
        .get(1..quoted.len().saturating_sub(1))
        .unwrap_or_default();
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0;
    while let Some(&c) = inner.get(i) {
        i += 1;
        if c != b'\\' {
            out.push(c);
            continue;
        }
        let Some(&escape) = inner.get(i) else {
            break;
        };
        i += 1;
        let decoded = match escape {
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let Some(unit) = hex4(inner.get(i..i + 4)) else {
                    break;
                };
                i += 4;
                if (0xD800..0xDC00).contains(&unit)
                    && inner.get(i..i + 2) == Some(b"\\u")
                    && let Some(low) = hex4(inner.get(i + 2..i + 6))
                    && (0xDC00..0xE000).contains(&low)
                {
                    i += 6;
                    let c = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                    char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER)
                } else {
                    char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER)
                }
            }
            // `"`, `\`, and `/` stand for themselves.
            other => char::from(other),
        };
        let mut buf = [0; 4];
        out.extend_from_slice(decoded.encode_utf8(&mut buf).as_bytes());
    }
    String::from_utf8(out)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

/// Reads four hexadecimal digits.
fn hex4(digits: Option<&[u8]>) -> Option<u32> {
    let digits = std::str::from_utf8(digits?).ok()?;
    u32::from_str_radix(digits, 16).ok()
}
