//! Go's `fmt` printing and `strconv` quoting for [`Value`]s.
//!
//! This is a port of the parts of Go's `fmt/print.go`, `fmt/format.go`, and `strconv` that the
//! built-in functions `print`, `printf`, and `println` reach. It follows the Go code closely, so
//! that a reader can compare the two side by side.

use std::fmt::Write as _;

use crate::value::Value;

/// Formats the values as Go's `fmt.Sprint` does: a space goes between two operands when neither
/// is a string. A pointer to a struct prints as the struct, as in the template package's
/// `evalArgs`.
pub(crate) fn sprint(args: &[Value]) -> String {
    let mut p = Printer {
        deref: true,
        ..Printer::default()
    };
    let mut prev_string = false;
    for (index, arg) in args.iter().enumerate() {
        let is_string = matches!(arg, Value::String(_));
        if index > 0 && !is_string && !prev_string {
            p.buf.push(' ');
        }
        p.print_arg(arg, 'v');
        prev_string = is_string;
    }
    p.buf
}

/// Formats the values as Go's `fmt.Sprintln` does: a space between all operands and a newline
/// at the end.
pub(crate) fn sprintln(args: &[Value]) -> String {
    let mut p = Printer::default();
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            p.buf.push(' ');
        }
        p.print_arg(arg, 'v');
    }
    p.buf.push('\n');
    p.buf
}

/// Formats the values as Go's `fmt.Sprintf` does with the format `format`.
pub(crate) fn sprintf(format: &str, args: &[Value]) -> String {
    let mut p = Printer::default();
    p.do_printf(format, args);
    p.buf
}

/// Formats one value as a template action prints it: as Go's `fmt.Sprint` does, after taking
/// the struct out of a pointer.
pub(crate) fn sprint_one(value: &Value) -> String {
    let mut p = Printer {
        deref: true,
        ..Printer::default()
    };
    p.print_arg(value, 'v');
    p.buf
}

#[derive(Default, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct Flags {
    wid: usize,
    prec: usize,
    wid_present: bool,
    prec_present: bool,
    minus: bool,
    plus: bool,
    sharp: bool,
    space: bool,
    zero: bool,
    plus_v: bool,
    sharp_v: bool,
}

#[derive(Default)]
struct Printer {
    buf: String,
    /// Print a pointer to a struct at the top level as the struct, without `&`.
    deref: bool,
    f: Flags,
    reordered: bool,
    good_arg_num: bool,
}

/// Go's `tooLarge`: widths and precisions above a million are malformed.
const MAX_NUMBER: usize = 1_000_000;

const LOWER_DIGITS: &[u8; 17] = b"0123456789abcdefx";
const UPPER_DIGITS: &[u8; 17] = b"0123456789ABCDEFX";

impl Printer {
    fn print_arg(&mut self, arg: &Value, verb: char) {
        match arg {
            Value::Nil => {
                if verb == 'v' {
                    self.pad_string("<nil>");
                } else {
                    self.bad_verb(verb, Some(arg));
                }
            }
            _ => self.print_value(arg, verb, 0),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn print_value(&mut self, value: &Value, verb: char, depth: usize) {
        match value {
            Value::Nil => {
                // A nil inside a composite value is a nil interface.
                if self.f.sharp_v {
                    self.buf.push_str("interface {}(nil)");
                } else {
                    self.buf.push_str("<nil>");
                }
            }
            Value::Bool(b) => match verb {
                't' | 'v' => self.pad_string(if *b { "true" } else { "false" }),
                _ => self.bad_verb(verb, Some(value)),
            },
            Value::Int(n) => self.fmt_integer(*n, verb, value),
            Value::Float(x) => match verb {
                'v' => self.fmt_float(*x, 'g', -1),
                'g' | 'G' => self.fmt_float(*x, verb, -1),
                'f' | 'e' | 'E' => self.fmt_float(*x, verb, 6),
                'F' => self.fmt_float(*x, 'f', 6),
                _ => self.bad_verb(verb, Some(value)),
            },
            Value::String(s) => match verb {
                'v' if self.f.sharp_v => self.fmt_q(s),
                'v' | 's' => self.fmt_s(s),
                'x' => self.fmt_sx(s, LOWER_DIGITS),
                'X' => self.fmt_sx(s, UPPER_DIGITS),
                'q' => self.fmt_q(s),
                _ => self.bad_verb(verb, Some(value)),
            },
            Value::List(list) => {
                if self.f.sharp_v {
                    self.buf.push_str(list.type_name());
                    self.buf.push('{');
                    for (index, item) in list.iter().enumerate() {
                        if index > 0 {
                            self.buf.push_str(", ");
                        }
                        self.print_value(item, verb, depth + 1);
                    }
                    self.buf.push('}');
                } else {
                    self.buf.push('[');
                    for (index, item) in list.iter().enumerate() {
                        if index > 0 {
                            self.buf.push(' ');
                        }
                        self.print_value(item, verb, depth + 1);
                    }
                    self.buf.push(']');
                }
            }
            Value::Map(map) => {
                if self.f.sharp_v {
                    self.buf.push_str(map.type_name());
                    self.buf.push('{');
                } else {
                    self.buf.push_str("map[");
                }
                for (index, (key, item)) in map.iter().enumerate() {
                    if index > 0 {
                        if self.f.sharp_v {
                            self.buf.push_str(", ");
                        } else {
                            self.buf.push(' ');
                        }
                    }
                    self.print_value(&Value::String(key.clone()), verb, depth + 1);
                    self.buf.push(':');
                    self.print_value(item, verb, depth + 1);
                }
                self.buf.push(if self.f.sharp_v { '}' } else { ']' });
            }
            Value::Struct(record) => {
                // Go prints the address of a pointer below the top level. An address is
                // different in each run, so this prints the struct as at the top level.
                let type_name = record.type_name();
                let (pointer, name) = match type_name.strip_prefix('*') {
                    Some(name) => (true, name),
                    None => (false, type_name),
                };
                if pointer && !(self.deref && depth == 0) {
                    self.buf.push('&');
                }
                if self.f.sharp_v {
                    self.buf.push_str(name);
                }
                self.buf.push('{');
                for (index, (field, item)) in record.fields().enumerate() {
                    if index > 0 {
                        if self.f.sharp_v {
                            self.buf.push_str(", ");
                        } else {
                            self.buf.push(' ');
                        }
                    }
                    if self.f.plus_v || self.f.sharp_v {
                        self.buf.push_str(field);
                        self.buf.push(':');
                    }
                    self.print_value(item, verb, depth + 1);
                }
                self.buf.push('}');
            }
        }
    }

    /// Go's `badVerb`: `%!verb(type=value)`, or `%!verb(<nil>)` for nil.
    fn bad_verb(&mut self, verb: char, arg: Option<&Value>) {
        self.buf.push_str("%!");
        self.buf.push(verb);
        self.buf.push('(');
        match arg {
            Some(value) if !matches!(value, Value::Nil) => {
                self.buf.push_str(&value.type_name());
                self.buf.push('=');
                self.print_arg(value, 'v');
            }
            _ => self.buf.push_str("<nil>"),
        }
        self.buf.push(')');
    }

    fn write_padding(&mut self, n: usize) {
        let pad = if self.f.zero && !self.f.minus {
            '0'
        } else {
            ' '
        };
        for _ in 0..n {
            self.buf.push(pad);
        }
    }

    fn pad_string(&mut self, s: &str) {
        if !self.f.wid_present || self.f.wid == 0 {
            self.buf.push_str(s);
            return;
        }
        let width = self.f.wid.saturating_sub(s.chars().count());
        if self.f.minus {
            self.buf.push_str(s);
            self.write_padding(width);
        } else {
            self.write_padding(width);
            self.buf.push_str(s);
        }
    }

    fn truncate<'s>(&self, s: &'s str) -> &'s str {
        if self.f.prec_present
            && let Some((index, _)) = s.char_indices().nth(self.f.prec)
        {
            return &s[..index];
        }
        s
    }

    fn fmt_s(&mut self, s: &str) {
        let s = self.truncate(s);
        self.pad_string(s);
    }

    fn fmt_q(&mut self, s: &str) {
        let s = self.truncate(s);
        if self.f.sharp && can_backquote(s) {
            self.pad_string(&format!("`{s}`"));
        } else if self.f.plus {
            self.pad_string(&quote_with(s, '"', true));
        } else {
            self.pad_string(&quote_with(s, '"', false));
        }
    }

    /// Go's `fmtSbx`: the bytes of `s` in hexadecimal.
    fn fmt_sx(&mut self, s: &str, digits: &[u8; 17]) {
        let bytes = s.as_bytes();
        let mut length = bytes.len();
        if self.f.prec_present && self.f.prec < length {
            length = self.f.prec;
        }
        let mut width = 2 * length;
        if width > 0 {
            if self.f.space {
                if self.f.sharp {
                    width *= 2;
                }
                width += length - 1;
            } else if self.f.sharp {
                width += 2;
            }
        } else {
            if self.f.wid_present {
                self.write_padding(self.f.wid);
            }
            return;
        }
        if self.f.wid_present && self.f.wid > width && !self.f.minus {
            self.write_padding(self.f.wid - width);
        }
        if self.f.sharp {
            self.buf.push('0');
            self.buf.push(char::from(digits[16]));
        }
        for (index, byte) in bytes.iter().take(length).enumerate() {
            if self.f.space && index > 0 {
                self.buf.push(' ');
                if self.f.sharp {
                    self.buf.push('0');
                    self.buf.push(char::from(digits[16]));
                }
            }
            self.buf.push(char::from(digits[usize::from(byte >> 4)]));
            self.buf.push(char::from(digits[usize::from(byte & 0xf)]));
        }
        if self.f.wid_present && self.f.wid > width && self.f.minus {
            self.write_padding(self.f.wid - width);
        }
    }

    fn fmt_integer(&mut self, n: i64, verb: char, value: &Value) {
        match verb {
            'v' | 'd' => self.fmt_integer_base(n, 10, verb, LOWER_DIGITS),
            'b' => self.fmt_integer_base(n, 2, verb, LOWER_DIGITS),
            'o' | 'O' => self.fmt_integer_base(n, 8, verb, LOWER_DIGITS),
            'x' => self.fmt_integer_base(n, 16, verb, LOWER_DIGITS),
            'X' => self.fmt_integer_base(n, 16, verb, UPPER_DIGITS),
            'c' => {
                let c = rune(n);
                self.pad_string(c.encode_utf8(&mut [0; 4]));
            }
            'q' => {
                let quoted = quote_rune(rune(n), self.f.plus);
                self.pad_string(&quoted);
            }
            'U' => self.fmt_unicode(n),
            _ => self.bad_verb(verb, Some(value)),
        }
    }

    /// Go's `fmt.fmtInteger` for a signed integer.
    fn fmt_integer_base(&mut self, n: i64, base: u64, verb: char, digits: &[u8; 17]) {
        let negative = n < 0;
        let mut u = n.unsigned_abs();
        let mut prec = 0;
        if self.f.prec_present {
            prec = self.f.prec;
            if prec == 0 && u == 0 {
                let zero = self.f.zero;
                self.f.zero = false;
                self.write_padding(self.f.wid);
                self.f.zero = zero;
                return;
            }
        } else if self.f.zero && !self.f.minus && self.f.wid_present {
            prec = self.f.wid;
            if negative || self.f.plus || self.f.space {
                prec = prec.saturating_sub(1);
            }
        }
        // Built right to left, then reversed.
        let mut out: Vec<u8> = Vec::new();
        loop {
            let digit = usize::try_from(u % base).unwrap_or(0);
            out.push(digits[digit]);
            u /= base;
            if u == 0 {
                break;
            }
        }
        while out.len() < prec {
            out.push(b'0');
        }
        if self.f.sharp {
            match base {
                2 => out.extend_from_slice(b"b0"),
                8 => {
                    if out.last() != Some(&b'0') {
                        out.push(b'0');
                    }
                }
                16 => {
                    out.push(digits[16]);
                    out.push(b'0');
                }
                _ => {}
            }
        }
        if verb == 'O' {
            out.extend_from_slice(b"o0");
        }
        if negative {
            out.push(b'-');
        } else if self.f.plus {
            out.push(b'+');
        } else if self.f.space {
            out.push(b' ');
        }
        out.reverse();
        let text: String = out.into_iter().map(char::from).collect();
        let zero = self.f.zero;
        self.f.zero = false;
        self.pad_string(&text);
        self.f.zero = zero;
    }

    /// Go's `fmtUnicode`: `U+0078`, or `U+0078 'x'` with the `#` flag.
    fn fmt_unicode(&mut self, n: i64) {
        // Go formats the value as an unsigned integer.
        let u = n.cast_unsigned();
        let prec = if self.f.prec_present && self.f.prec > 4 {
            self.f.prec
        } else {
            4
        };
        let mut text = format!("U+{u:0prec$X}");
        if self.f.sharp
            && let Some(c) = u32::try_from(u).ok().and_then(char::from_u32)
            && is_print(c)
        {
            text.push_str(" '");
            text.push(c);
            text.push('\'');
        }
        let zero = self.f.zero;
        self.f.zero = false;
        self.pad_string(&text);
        self.f.zero = zero;
    }

    /// Go's `fmt.fmtFloat`.
    fn fmt_float(&mut self, v: f64, verb: char, prec: i64) {
        let prec = if self.f.prec_present {
            i64::try_from(self.f.prec).unwrap_or(i64::MAX)
        } else {
            prec
        };
        let formatted = format_float(v, verb, prec);
        let mut num: Vec<char> = Vec::with_capacity(formatted.len() + 1);
        if formatted.starts_with('-') || formatted.starts_with('+') {
            num.extend(formatted.chars());
        } else {
            num.push('+');
            num.extend(formatted.chars());
        }
        if self.f.space && num[0] == '+' && !self.f.plus {
            num[0] = ' ';
        }
        if num.get(1) == Some(&'I') || num.get(1) == Some(&'N') {
            let zero = self.f.zero;
            self.f.zero = false;
            if num[1] == 'N' && !self.f.space && !self.f.plus {
                num.remove(0);
            }
            let text: String = num.into_iter().collect();
            self.pad_string(&text);
            self.f.zero = zero;
            return;
        }
        if self.f.sharp {
            let mut digits: i64 = match verb {
                'v' | 'g' | 'G' => {
                    if prec == -1 {
                        6
                    } else {
                        prec
                    }
                }
                _ => 0,
            };
            let mut tail: Vec<char> = Vec::new();
            let mut has_decimal_point = false;
            let mut saw_nonzero_digit = false;
            let mut i = 1;
            while i < num.len() {
                match num[i] {
                    '.' => has_decimal_point = true,
                    'e' | 'E' => {
                        tail.extend(num.drain(i..));
                        break;
                    }
                    c => {
                        if c != '0' {
                            saw_nonzero_digit = true;
                        }
                        if saw_nonzero_digit {
                            digits -= 1;
                        }
                    }
                }
                i += 1;
            }
            if !has_decimal_point {
                if num.len() == 2 && num[1] == '0' {
                    digits -= 1;
                }
                num.push('.');
            }
            while digits > 0 {
                num.push('0');
                digits -= 1;
            }
            num.extend(tail);
        }
        if self.f.plus || num[0] != '+' {
            if self.f.zero && !self.f.minus && self.f.wid_present && self.f.wid > num.len() {
                self.buf.push(num[0]);
                self.write_padding(self.f.wid - num.len());
                self.buf.extend(&num[1..]);
                return;
            }
            let text: String = num.into_iter().collect();
            self.pad_string(&text);
            return;
        }
        let text: String = num[1..].iter().collect();
        self.pad_string(&text);
    }

    /// Go's `doPrintf`.
    #[allow(clippy::too_many_lines)]
    fn do_printf(&mut self, format: &str, args: &[Value]) {
        let bytes = format.as_bytes();
        let end = bytes.len();
        let mut arg_num = 0;
        let mut after_index;
        self.reordered = false;
        let mut i = 0;
        'format: while i < end {
            self.good_arg_num = true;
            let last = i;
            while i < end && bytes[i] != b'%' {
                i += 1;
            }
            if i > last {
                self.buf.push_str(&format[last..i]);
            }
            if i >= end {
                break;
            }
            i += 1;
            self.f = Flags::default();
            while i < end {
                match bytes[i] {
                    b'#' => self.f.sharp = true,
                    b'0' => self.f.zero = !self.f.minus,
                    b'+' => self.f.plus = true,
                    b'-' => {
                        self.f.minus = true;
                        self.f.zero = false;
                    }
                    b' ' => self.f.space = true,
                    c => {
                        if c.is_ascii_lowercase() && arg_num < args.len() {
                            if c == b'v' {
                                self.f.sharp_v = self.f.sharp;
                                self.f.sharp = false;
                                self.f.plus_v = self.f.plus;
                                self.f.plus = false;
                            }
                            self.print_arg(&args[arg_num], char::from(c));
                            arg_num += 1;
                            i += 1;
                            continue 'format;
                        }
                        break;
                    }
                }
                i += 1;
            }

            (arg_num, i, after_index) = self.arg_number(arg_num, format, i, args.len());

            if i < end && bytes[i] == b'*' {
                i += 1;
                let (num, present, next) = int_from_arg(args, arg_num);
                arg_num = next;
                self.f.wid_present = present;
                if !present {
                    self.buf.push_str("%!(BADWIDTH)");
                }
                if num < 0 {
                    self.f.minus = true;
                    self.f.zero = false;
                }
                self.f.wid = usize::try_from(num.unsigned_abs()).unwrap_or(0);
                after_index = false;
            } else {
                let (num, present, next) = parse_num(bytes, i, end);
                self.f.wid = num;
                self.f.wid_present = present;
                i = next;
                if after_index && present {
                    self.good_arg_num = false;
                }
            }

            if i + 1 < end && bytes[i] == b'.' {
                i += 1;
                if after_index {
                    self.good_arg_num = false;
                }
                (arg_num, i, after_index) = self.arg_number(arg_num, format, i, args.len());
                if i < end && bytes[i] == b'*' {
                    i += 1;
                    let (num, present, next) = int_from_arg(args, arg_num);
                    arg_num = next;
                    if num < 0 {
                        self.f.prec = 0;
                        self.f.prec_present = false;
                    } else {
                        self.f.prec = usize::try_from(num).unwrap_or(0);
                        self.f.prec_present = present;
                    }
                    if !self.f.prec_present {
                        self.buf.push_str("%!(BADPREC)");
                    }
                    after_index = false;
                } else {
                    let (num, present, next) = parse_num(bytes, i, end);
                    i = next;
                    self.f.prec = if present { num } else { 0 };
                    self.f.prec_present = true;
                }
            }

            if !after_index {
                (arg_num, i, _) = self.arg_number(arg_num, format, i, args.len());
            }

            let Some(verb) = format[i..].chars().next() else {
                self.buf.push_str("%!(NOVERB)");
                break;
            };
            i += verb.len_utf8();

            if verb == '%' {
                self.buf.push('%');
            } else if !self.good_arg_num {
                self.buf.push_str("%!");
                self.buf.push(verb);
                self.buf.push_str("(BADINDEX)");
            } else if arg_num >= args.len() {
                self.buf.push_str("%!");
                self.buf.push(verb);
                self.buf.push_str("(MISSING)");
            } else {
                if verb == 'v' {
                    self.f.sharp_v = self.f.sharp;
                    self.f.sharp = false;
                    self.f.plus_v = self.f.plus;
                    self.f.plus = false;
                }
                self.print_arg(&args[arg_num], verb);
                arg_num += 1;
            }
        }

        if !self.reordered && arg_num < args.len() {
            self.f = Flags::default();
            self.buf.push_str("%!(EXTRA ");
            for (index, arg) in args[arg_num..].iter().enumerate() {
                if index > 0 {
                    self.buf.push_str(", ");
                }
                if matches!(arg, Value::Nil) {
                    self.buf.push_str("<nil>");
                } else {
                    self.buf.push_str(&arg.type_name());
                    self.buf.push('=');
                    self.print_arg(arg, 'v');
                }
            }
            self.buf.push(')');
        }
    }

    /// Go's `argNumber`: an explicit argument index such as `[2]` at `format[i..]`.
    fn arg_number(
        &mut self,
        arg_num: usize,
        format: &str,
        i: usize,
        num_args: usize,
    ) -> (usize, usize, bool) {
        let bytes = format.as_bytes();
        if bytes.len() <= i || bytes[i] != b'[' {
            return (arg_num, i, false);
        }
        self.reordered = true;
        let (index, wid, ok) = parse_arg_number(&bytes[i..]);
        if let Some(index) = index.filter(|index| ok && *index < num_args) {
            return (index, i + wid, true);
        }
        self.good_arg_num = false;
        (arg_num, i + wid, ok)
    }
}

/// Go's `parsenum`.
fn parse_num(bytes: &[u8], start: usize, end: usize) -> (usize, bool, usize) {
    if start >= end {
        return (0, false, end);
    }
    let mut num: usize = 0;
    let mut is_num = false;
    let mut i = start;
    while i < end && bytes[i].is_ascii_digit() {
        if num > MAX_NUMBER {
            return (0, false, end);
        }
        num = num * 10 + usize::from(bytes[i] - b'0');
        is_num = true;
        i += 1;
    }
    (num, is_num, i)
}

/// Go's `parseArgNumber`: the zero-based index in `[n]`, the bytes to consume, and whether it
/// parsed. The index is `None` for `[0]`, which Go turns into the invalid index -1.
fn parse_arg_number(bytes: &[u8]) -> (Option<usize>, usize, bool) {
    if bytes.len() < 3 {
        return (None, 1, false);
    }
    for i in 1..bytes.len() {
        if bytes[i] == b']' {
            let (width, ok, next) = parse_num(bytes, 1, i);
            if !ok || next != i {
                return (None, i + 1, false);
            }
            return (width.checked_sub(1), i + 1, true);
        }
    }
    (None, 1, false)
}

/// Go's `intFromArg`: the integer for a `*` width or precision.
fn int_from_arg(args: &[Value], arg_num: usize) -> (i64, bool, usize) {
    let Some(arg) = args.get(arg_num) else {
        return (0, false, arg_num);
    };
    let (num, is_int) = match arg {
        Value::Int(n) => (*n, true),
        _ => (0, false),
    };
    let max = i64::try_from(MAX_NUMBER).unwrap_or(i64::MAX);
    if num > max || num < -max {
        return (0, false, arg_num + 1);
    }
    (num, is_int, arg_num + 1)
}

/// The character with code point `n`, or U+FFFD if there is none, as Go's `rune(c)` with
/// `utf8.RuneError` for invalid values.
fn rune(n: i64) -> char {
    u32::try_from(n)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Formats `v` as Go's `strconv.FormatFloat(v, verb, prec, 64)` does, for the verbs `e`, `E`,
/// `f`, `g`, and `G`. A precision of -1 means the fewest digits that read back as `v`.
pub(crate) fn format_float(v: f64, verb: char, prec: i64) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "+Inf" } else { "-Inf" }.to_owned();
    }
    let negative = v.is_sign_negative();
    let a = v.abs();
    let shortest = prec < 0;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    match verb {
        'f' => {
            if shortest {
                let digits = Digits::shortest(a);
                let prec = (digits.nd() - digits.dp).max(0);
                fmt_f(&mut out, &digits, prec);
            } else {
                // Rust rounds the exact binary value like Go does.
                let _ = write!(out, "{a:.prec$}", prec = to_usize(prec));
            }
        }
        'e' | 'E' => {
            let (digits, prec) = if shortest {
                let digits = Digits::shortest(a);
                let prec = digits.nd() - 1;
                (digits, prec)
            } else {
                (Digits::significant(a, prec + 1), prec)
            };
            fmt_e(&mut out, &digits, prec, verb);
        }
        _ => {
            let upper = verb == 'G';
            let (digits, mut prec) = if shortest {
                let digits = Digits::shortest(a);
                let nd = digits.nd();
                (digits, nd)
            } else {
                let prec = prec.max(1);
                (Digits::significant(a, prec), prec)
            };
            let nd = digits.nd();
            let mut eprec = prec;
            if eprec > nd && nd >= digits.dp {
                eprec = nd;
            }
            if shortest {
                eprec = 6;
            }
            let exp = digits.dp - 1;
            if exp < -4 || exp >= eprec {
                if prec > nd {
                    prec = nd;
                }
                fmt_e(&mut out, &digits, prec - 1, if upper { 'E' } else { 'e' });
            } else {
                if prec > digits.dp {
                    prec = nd;
                }
                fmt_f(&mut out, &digits, (prec - digits.dp).max(0));
            }
        }
    }
    out
}

fn to_usize(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// The decimal digits of a non-negative float and the position of the decimal point, as Go's
/// `decimalSlice`: the value is `0.d[0]d[1]... * 10^dp`. Zero has no digits.
struct Digits {
    d: Vec<u8>,
    dp: i64,
}

impl Digits {
    fn nd(&self) -> i64 {
        i64::try_from(self.d.len()).unwrap_or(i64::MAX)
    }

    /// The shortest digits that read back as `a`.
    fn shortest(a: f64) -> Digits {
        Digits::from_scientific(&format!("{a:e}"))
    }

    /// `n` significant digits of `a`, rounded, without trailing zeros.
    fn significant(a: f64, n: i64) -> Digits {
        let decimals = to_usize(n - 1);
        Digits::from_scientific(&format!("{a:.decimals$e}"))
    }

    /// Reads Rust's `{:e}` output, such as `1.25e-3`.
    fn from_scientific(s: &str) -> Digits {
        let (mantissa, exponent) = s.split_once('e').unwrap_or((s, "0"));
        let exponent: i64 = exponent.parse().unwrap_or(0);
        let mut d: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
        while d.last() == Some(&b'0') {
            d.pop();
        }
        if d.is_empty() {
            return Digits { d, dp: 0 };
        }
        Digits {
            d,
            dp: exponent + 1,
        }
    }

    fn digit(&self, index: i64) -> char {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.d.get(index))
            .map_or('0', |digit| char::from(*digit))
    }
}

/// Go's `fmtE`: `d.ddde±dd`.
fn fmt_e(out: &mut String, digits: &Digits, prec: i64, verb: char) {
    out.push(digits.digit(0));
    if prec > 0 {
        out.push('.');
        for i in 1..=prec {
            out.push(digits.digit(i));
        }
    }
    out.push(verb);
    let mut exp = digits.dp - 1;
    if digits.d.is_empty() {
        exp = 0;
    }
    if exp < 0 {
        out.push('-');
        exp = -exp;
    } else {
        out.push('+');
    }
    if exp < 10 {
        out.push('0');
    }
    let _ = write!(out, "{exp}");
}

/// Go's `fmtF`: `ddd.ddd`.
fn fmt_f(out: &mut String, digits: &Digits, prec: i64) {
    if digits.dp > 0 {
        for i in 0..digits.dp {
            out.push(digits.digit(i));
        }
    } else {
        out.push('0');
    }
    if prec > 0 {
        out.push('.');
        for i in 1..=prec {
            out.push(digits.digit(digits.dp + i - 1));
        }
    }
}

/// Quotes `s` with double quotes, as Go's `strconv.Quote` does.
pub(crate) fn quote(s: &str) -> String {
    quote_with(s, '"', false)
}

/// Quotes `s` as Go's `strconv.Quote` does, or `strconv.QuoteToASCII` if `ascii` is set.
fn quote_with(s: &str, delimiter: char, ascii: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(delimiter);
    for c in s.chars() {
        push_escaped(&mut out, c, delimiter, ascii);
    }
    out.push(delimiter);
    out
}

/// Quotes `c` with single quotes, as Go's `strconv.QuoteRune` does, or
/// `strconv.QuoteRuneToASCII` if `ascii` is set.
fn quote_rune(c: char, ascii: bool) -> String {
    let mut out = String::with_capacity(6);
    out.push('\'');
    push_escaped(&mut out, c, '\'', ascii);
    out.push('\'');
    out
}

/// Appends `c` with Go's escaping rules for a literal delimited by `delimiter`.
fn push_escaped(out: &mut String, c: char, delimiter: char, ascii: bool) {
    if c == delimiter || c == '\\' {
        out.push('\\');
        out.push(c);
        return;
    }
    if ascii {
        if c.is_ascii() && is_print(c) {
            out.push(c);
            return;
        }
    } else if is_print(c) {
        out.push(c);
        return;
    }
    let escape = match c {
        '\u{7}' => "\\a",
        '\u{8}' => "\\b",
        '\u{c}' => "\\f",
        '\n' => "\\n",
        '\r' => "\\r",
        '\t' => "\\t",
        '\u{b}' => "\\v",
        _ => "",
    };
    if !escape.is_empty() {
        out.push_str(escape);
        return;
    }
    let code = u32::from(c);
    // Writing to a `String` cannot fail.
    let _ = if c < ' ' || c == '\u{7f}' {
        write!(out, "\\x{code:02x}")
    } else if code < 0x1_0000 {
        write!(out, "\\u{code:04x}")
    } else {
        write!(out, "\\U{code:08x}")
    };
}

/// Go's `strconv.CanBackquote`: whether `s` can be written as a raw string literal.
fn can_backquote(s: &str) -> bool {
    s.chars()
        .all(|c| c != '`' && c != '\u{feff}' && (c == '\t' || !(c < ' ' || c == '\u{7f}')))
}

/// Go's `strconv.IsPrint` and `unicode.IsPrint`: letters, marks, numbers, punctuation,
/// symbols, and the ASCII space.
///
/// The answer depends on the Unicode version. Go 1.21 to 1.26 use Unicode 15.0.0, and so does
/// `unicode-general-category` 0.6.0, which is why `Cargo.toml` pins that version.
pub(crate) fn is_print(c: char) -> bool {
    use unicode_general_category::GeneralCategory as Gc;

    if c == ' ' {
        return true;
    }
    !matches!(
        unicode_general_category::get_general_category(c),
        Gc::SpaceSeparator
            | Gc::LineSeparator
            | Gc::ParagraphSeparator
            | Gc::Control
            | Gc::Format
            | Gc::Surrogate
            | Gc::PrivateUse
            | Gc::Unassigned
    )
}

/// Go's `unicode.IsLetter`.
pub(crate) fn is_letter(c: char) -> bool {
    use unicode_general_category::GeneralCategory as Gc;

    matches!(
        unicode_general_category::get_general_category(c),
        Gc::UppercaseLetter
            | Gc::LowercaseLetter
            | Gc::TitlecaseLetter
            | Gc::ModifierLetter
            | Gc::OtherLetter
    )
}

/// Go's `unicode.IsDigit`.
pub(crate) fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(
            unicode_general_category::get_general_category(c),
            unicode_general_category::GeneralCategory::DecimalNumber
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_float_matches_go_shortest_g() {
        assert_eq!(format_float(3.5, 'g', -1), "3.5");
        assert_eq!(format_float(2.0, 'g', -1), "2");
        assert_eq!(format_float(1e21, 'g', -1), "1e+21");
        assert_eq!(format_float(100_000.0, 'g', -1), "100000");
        assert_eq!(format_float(1_000_000.0, 'g', -1), "1e+06");
        assert_eq!(format_float(0.0001, 'g', -1), "0.0001");
        assert_eq!(format_float(0.00001, 'g', -1), "1e-05");
        assert_eq!(format_float(0.0, 'g', -1), "0");
        assert_eq!(format_float(-0.0, 'g', -1), "-0");
        assert_eq!(format_float(123_456_789.0, 'g', -1), "1.23456789e+08");
    }

    #[test]
    fn format_float_matches_go_with_precision() {
        assert_eq!(format_float(3.5, 'f', 6), "3.500000");
        assert_eq!(format_float(3.5, 'e', 6), "3.500000e+00");
        assert_eq!(format_float(1e21, 'e', 6), "1.000000e+21");
        assert_eq!(format_float(3.5, 'g', 3), "3.5");
        assert_eq!(format_float(1234.5678, 'g', 3), "1.23e+03");
        assert_eq!(format_float(0.0, 'e', 2), "0.00e+00");
        assert_eq!(format_float(1e100, 'e', 1), "1.0e+100");
    }

    #[test]
    fn quote_escapes_like_go() {
        assert_eq!(quote("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(quote("\u{7}\t\u{0}\u{7f}"), r#""\a\t\x00\x7f""#);
        assert_eq!(quote("é\u{200b}\u{1f600}"), "\"é\\u200b\u{1f600}\"");
        assert_eq!(quote_with("é\u{1f600}", '"', true), r#""\u00e9\U0001f600""#);
        assert_eq!(quote_rune('\'', false), r"'\''");
    }

    #[test]
    fn can_backquote_rejects_backquotes_and_control_characters() {
        assert!(can_backquote("a\tb \"c\""));
        assert!(!can_backquote("a`b"));
        assert!(!can_backquote("a\nb"));
        assert!(!can_backquote("\u{feff}"));
    }
}
