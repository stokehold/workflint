//! Property tests of the scalar source maps.
//!
//! Each test generates scalar content, writes it in one style with random
//! indentation, line folding, chomping, and escapes, and parses the result.
//! The writer records the source offset that each decoded character comes
//! from, and whether it is copied unchanged, so the tests compare the source
//! map against it. For folded block scalars the writer does not predict the
//! folding, and the tests check the general properties only.

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use proptest::sample::select;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::{NodeKind, ScalarStyle, parse};

/// A YAML file with one scalar under test.
#[derive(Debug)]
struct Written {
    yaml: String,
    /// Where the scalar's node starts.
    start: usize,
    /// The value the parser must decode, if the writer knows it.
    value: Option<String>,
    /// For each character of the value, the source offset it must map to
    /// and whether it is copied unchanged from there.
    expected: Option<Vec<(usize, bool)>>,
}

/// Random decisions for a writer, read in a cycle.
struct Choices {
    bytes: Vec<u8>,
    next: usize,
}

impl Choices {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, next: 0 }
    }

    /// Returns a number below `n`.
    fn pick(&mut self, n: usize) -> usize {
        let byte = self.bytes.get(self.next % self.bytes.len().max(1));
        self.next += 1;
        byte.map_or(0, |&b| usize::from(b) % n.max(1))
    }

    fn coin(&mut self) -> bool {
        self.pick(2) == 0
    }

    /// Returns a string of `n` spaces.
    fn spaces(n: usize) -> String {
        " ".repeat(n)
    }

    /// Returns blanks that the decoder drops, after an indentation of at
    /// least one space.
    fn dropped_blanks(&mut self) -> &'static str {
        ["", "", " ", "\t", " \t"][self.pick(5)]
    }
}

/// The source being written and the decoded characters it must produce.
struct Out {
    yaml: String,
    value: String,
    expected: Vec<(usize, bool)>,
    line_break: &'static str,
}

impl Out {
    fn new(line_break: &'static str) -> Self {
        Self {
            yaml: String::new(),
            value: String::new(),
            expected: Vec::new(),
            line_break,
        }
    }

    fn raw(&mut self, text: &str) {
        self.yaml.push_str(text);
    }

    /// Writes `ch` to the source and expects it unchanged in the value.
    fn copy(&mut self, ch: char) {
        self.expected.push((self.yaml.len(), true));
        self.value.push(ch);
        self.yaml.push(ch);
    }

    /// Expects `ch` in the value, produced by the source at `at`.
    fn produced(&mut self, ch: char, at: usize) {
        self.expected.push((at, false));
        self.value.push(ch);
    }

    /// Writes a line break and returns its offset.
    fn line_break(&mut self) -> usize {
        let at = self.yaml.len();
        self.yaml.push_str(self.line_break);
        at
    }

    fn finish(self, start: usize, predicted: bool) -> Written {
        Written {
            yaml: self.yaml,
            start,
            value: predicted.then_some(self.value),
            expected: predicted.then_some(self.expected),
        }
    }
}

/// What separates two words of flow scalar content.
#[derive(Debug, Clone)]
enum Gap {
    /// Spaces and tabs.
    Blanks(String),
    /// This many line feeds.
    Newlines(usize),
}

const PLAIN: &[char] = &[
    'a', 'b', 'z', '0', '9', '日', 'é', '😀', '$', '(', ')', '{', '}', '_', '.', '/', '!', '\'',
    '"', '\\', '%',
];

const QUOTED: &[char] = &[
    'a', 'b', 'z', '0', '日', 'é', '😀', '☺', '$', '{', '}', '#', ':', '\'', '"', '\\', '-', '[',
    ',', '&', '*', '!', '|', '>', '%', '@', '`', '?',
];

const DOUBLE: &[char] = &[
    'a', 'b', 'a', 'b', 'z', '日', '😀', '☺', '$', '{', '}', '#', ':', '\'', ' ', ' ', '\t', '\n',
    '\n', '"', '\\', '/', '\0', '\x07', '\x08', '\x0b', '\x0c', '\r', '\x1b', '\u{85}', '\u{a0}',
    '\u{2028}', '\u{2029}',
];

const BLOCK: &[char] = &[
    'a', 'b', 'a', 'b', 'z', '日', '😀', '☺', '$', '{', '}', '#', ':', '\'', '"', '\\', '-', ' ',
    ' ', '\t', '>', '|',
];

fn word(alphabet: &'static [char]) -> impl Strategy<Value = String> {
    prop::collection::vec(select(alphabet), 1..6).prop_map(|chars| chars.into_iter().collect())
}

fn gap() -> impl Strategy<Value = Gap> {
    prop_oneof![
        3 => Just(Gap::Blanks(" ".to_owned())),
        1 => prop::collection::vec(select(&[' ', '\t'][..]), 1..4)
            .prop_map(|blanks| Gap::Blanks(blanks.into_iter().collect())),
        1 => (1usize..4).prop_map(Gap::Newlines),
    ]
}

fn line_break() -> impl Strategy<Value = &'static str> {
    select(&["\n", "\n", "\r\n"][..])
}

fn choices() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..64)
}

/// Writes a line break with the next line's indentation, which the decoder
/// folds into a space.
fn fold(out: &mut Out, choices: &mut Choices, indent: &str, ch: char) {
    let trailing = choices.dropped_blanks();
    out.raw(trailing);
    let at = out.line_break();
    out.produced(ch, at);
    out.raw(indent);
    let leading = choices.dropped_blanks();
    out.raw(leading);
}

/// Writes `count` line feeds as `count + 1` line breaks. The first break is
/// dropped and each later one becomes a line feed.
fn newlines(out: &mut Out, choices: &mut Choices, indent: &str, count: usize) {
    let trailing = choices.dropped_blanks();
    out.raw(trailing);
    out.line_break();
    for _ in 0..count {
        out.raw(&Choices::spaces(choices.pick(3)));
        let at = out.line_break();
        out.produced('\n', at);
    }
    out.raw(indent);
    let leading = choices.dropped_blanks();
    out.raw(leading);
}

fn write_plain(words: &[(String, Gap)], line_break: &'static str, choices: Vec<u8>) -> Written {
    let mut choices = Choices::new(choices);
    let mut out = Out::new(line_break);
    let indent = Choices::spaces(1 + choices.pick(4));
    out.raw("k: ");
    let start = out.yaml.len();
    for (i, (word, gap)) in words.iter().enumerate() {
        for (j, ch) in word.chars().enumerate() {
            // A plain scalar must not start with an indicator.
            out.copy(if i == 0 && j == 0 { 'p' } else { ch });
        }
        if i + 1 == words.len() {
            break;
        }
        match gap {
            Gap::Blanks(blanks) if blanks == " " && choices.coin() => {
                fold(&mut out, &mut choices, &indent, ' ');
            }
            Gap::Blanks(blanks) => blanks.chars().for_each(|ch| out.copy(ch)),
            Gap::Newlines(count) => newlines(&mut out, &mut choices, &indent, *count),
        }
    }
    out.line_break();
    out.raw("z: 1");
    out.line_break();
    out.finish(start, true)
}

fn write_single(
    lead: Option<&Gap>,
    words: &[(String, Gap)],
    trail: bool,
    line_break: &'static str,
    choices: Vec<u8>,
) -> Written {
    let mut choices = Choices::new(choices);
    let mut out = Out::new(line_break);
    let indent = Choices::spaces(1 + choices.pick(4));
    out.raw("k: ");
    let start = out.yaml.len();
    out.raw("'");
    let mut write_gap = |out: &mut Out, gap: &Gap| match gap {
        Gap::Blanks(blanks) if blanks == " " && choices.coin() => {
            fold(out, &mut choices, &indent, ' ');
        }
        Gap::Blanks(blanks) => blanks.chars().for_each(|ch| out.copy(ch)),
        Gap::Newlines(count) => newlines(out, &mut choices, &indent, *count),
    };
    if let Some(gap) = lead {
        write_gap(&mut out, gap);
    }
    for (i, (word, gap)) in words.iter().enumerate() {
        for ch in word.chars() {
            if ch == '\'' {
                let at = out.yaml.len();
                out.raw("''");
                out.produced('\'', at);
            } else {
                out.copy(ch);
            }
        }
        if trail || i + 1 < words.len() {
            write_gap(&mut out, gap);
        }
    }
    out.raw("'");
    out.line_break();
    out.raw("z: 1");
    out.line_break();
    out.finish(start, true)
}

/// Returns the ways to escape `ch` in a double-quoted scalar.
fn escapes(ch: char, choices: &mut Choices) -> Vec<String> {
    let named: &[&str] = match ch {
        '\0' => &["\\0"],
        '\x07' => &["\\a"],
        '\x08' => &["\\b"],
        '\t' => &["\\t", "\\\t"],
        '\n' => &["\\n"],
        '\x0b' => &["\\v"],
        '\x0c' => &["\\f"],
        '\r' => &["\\r"],
        '\x1b' => &["\\e"],
        ' ' => &["\\ "],
        '"' => &["\\\""],
        '/' => &["\\/"],
        '\\' => &["\\\\"],
        '\u{85}' => &["\\N"],
        '\u{a0}' => &["\\_"],
        '\u{2028}' => &["\\L"],
        '\u{2029}' => &["\\P"],
        _ => &[],
    };
    let mut forms: Vec<String> = named.iter().map(|&form| form.to_owned()).collect();
    let code = u32::from(ch);
    let upper = choices.coin();
    let hex = |prefix: &str, width: usize| {
        let digits = format!("{code:0width$x}");
        let digits = if upper {
            digits.to_ascii_uppercase()
        } else {
            digits
        };
        format!("\\{prefix}{digits}")
    };
    if code <= 0xff {
        forms.push(hex("x", 2));
    }
    if code <= 0xffff {
        forms.push(hex("u", 4));
    }
    forms.push(hex("U", 8));
    forms
}

/// Whether `ch` cannot appear unescaped in a double-quoted scalar.
fn must_escape(ch: char) -> bool {
    (ch.is_control() && ch != '\t') || matches!(ch, '"' | '\\' | '\u{2028}' | '\u{2029}')
}

fn write_double(content: &[char], line_break: &'static str, choices: Vec<u8>) -> Written {
    let mut choices = Choices::new(choices);
    let mut out = Out::new(line_break);
    let indent = Choices::spaces(1 + choices.pick(4));
    out.raw("k: ");
    let start = out.yaml.len();
    out.raw("\"");
    // A blank written right after a line break would be dropped.
    let mut after_break = false;
    // A line break written right after a blank would drop the blank.
    let mut after_blank = false;
    let escape = |out: &mut Out, choices: &mut Choices, ch: char| {
        let forms = escapes(ch, choices);
        let at = out.yaml.len();
        out.raw(&forms[choices.pick(forms.len())]);
        out.produced(ch, at);
    };
    let mut i = 0;
    while let Some(&ch) = content.get(i) {
        if choices.pick(8) == 0 {
            // An escaped line break, dropped with the next indentation.
            out.raw("\\");
            out.line_break();
            out.raw(&indent);
            out.raw(&Choices::spaces(choices.pick(2)));
            after_break = true;
            after_blank = false;
        }
        if ch == '\n' {
            let count = content[i..].iter().take_while(|&&c| c == '\n').count();
            match choices.pick(3) {
                1 if !after_break && !after_blank => {
                    newlines(&mut out, &mut choices, &indent, count);
                    after_break = true;
                }
                2 if !after_break => {
                    out.raw("\\");
                    out.line_break();
                    for _ in 0..count {
                        out.raw(&Choices::spaces(choices.pick(3)));
                        let at = out.line_break();
                        out.produced('\n', at);
                    }
                    out.raw(&indent);
                    after_break = true;
                }
                _ => {
                    for _ in 0..count {
                        escape(&mut out, &mut choices, '\n');
                    }
                    after_break = false;
                }
            }
            after_blank = false;
            i += count;
            continue;
        }
        let blank = ch == ' ' || ch == '\t';
        if blank && after_break {
            escape(&mut out, &mut choices, ch);
            after_blank = false;
        } else if ch == ' ' && !after_blank && choices.pick(3) == 0 {
            fold(&mut out, &mut choices, &indent, ' ');
            after_break = true;
            after_blank = false;
            i += 1;
            continue;
        } else if must_escape(ch) || choices.pick(4) == 0 {
            escape(&mut out, &mut choices, ch);
            after_blank = false;
        } else {
            out.copy(ch);
            after_blank = blank;
        }
        after_break = false;
        i += 1;
    }
    out.raw("\"");
    out.line_break();
    out.raw("z: 1");
    out.line_break();
    out.finish(start, true)
}

/// Block scalar content: its lines, the empty lines after them, and the
/// header and layout decisions.
#[derive(Debug, Clone)]
struct Block {
    lines: Vec<String>,
    trailing: usize,
    chomping: usize,
    indicator: bool,
    /// 0 to 5, see [`CONTEXTS`].
    context: usize,
    extra_indent: usize,
    /// End the file without a final line break.
    eof: bool,
    line_break: &'static str,
    choices: Vec<u8>,
}

/// What comes before the block scalar header, the indentation of the
/// enclosing block collection, and the sibling that follows the scalar.
const CONTEXTS: [(&str, i64, &str); 6] = [
    ("", -1, ""),
    ("--- ", -1, ""),
    ("k: ", 0, "z: 1"),
    ("- ", 0, "- 1"),
    ("- k: ", 2, "  z: 1"),
    ("a:\n  k: ", 2, "  z: 1"),
];

fn block() -> impl Strategy<Value = Block> {
    (
        prop::collection::vec(
            prop::collection::vec(select(BLOCK), 0..8)
                .prop_map(|chars| chars.into_iter().collect::<String>()),
            1..6,
        ),
        0usize..3,
        0usize..3,
        any::<bool>(),
        0usize..6,
        0usize..3,
        any::<bool>(),
        line_break(),
        choices(),
    )
        .prop_map(
            |(
                lines,
                trailing,
                chomping,
                indicator,
                context,
                extra_indent,
                eof,
                line_break,
                choices,
            )| Block {
                lines,
                trailing,
                chomping,
                indicator,
                context,
                extra_indent,
                eof,
                line_break,
                choices,
            },
        )
}

fn write_block(block: &Block, literal: bool) -> Written {
    let mut choices = Choices::new(block.choices.clone());
    let mut lines = block.lines.clone();
    // The last line has content, so the empty lines after it are the
    // trailing ones that chomping applies to.
    if lines.last().is_none_or(String::is_empty) {
        lines.push("x".to_owned());
    }
    let first = lines.iter().find(|line| !line.is_empty());
    // Without an indicator, the first line with content sets the
    // indentation, so it must not start with a blank.
    let indicator = block.indicator || first.is_some_and(|line| line.starts_with([' ', '\t']));
    let (prefix, parent, sibling) = CONTEXTS[block.context];
    let eof = block.eof && block.trailing == 0;
    let indent = usize::try_from(parent + 1).unwrap_or(0).max(1) + block.extra_indent;
    let mut out = Out::new(block.line_break);
    out.raw(&prefix.replace('\n', block.line_break));
    let start = out.yaml.len();
    out.raw(if literal { "|" } else { ">" });
    let chomp = ["", "-", "+"][block.chomping];
    let digit = if indicator {
        let base = usize::try_from(parent).unwrap_or(0);
        (indent - base).to_string()
    } else {
        String::new()
    };
    if choices.coin() {
        out.raw(chomp);
        out.raw(&digit);
    } else {
        out.raw(&digit);
        out.raw(chomp);
    }
    if choices.coin() {
        out.raw(" # header");
    }
    out.line_break();
    let mut breaks = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            out.raw(&Choices::spaces(choices.pick(indent + 1)));
        } else {
            out.raw(&Choices::spaces(indent));
            line.chars().for_each(|ch| out.copy(ch));
        }
        if eof && i + 1 == lines.len() {
            breaks.push(None);
        } else {
            breaks.push(Some(out.line_break()));
        }
        if i + 1 < lines.len()
            && let Some(at) = breaks[i]
        {
            out.produced('\n', at);
        }
    }
    let mut trailing = Vec::new();
    for _ in 0..block.trailing {
        out.raw(&Choices::spaces(choices.pick(indent + 1)));
        trailing.push(out.line_break());
    }
    if block.chomping != 1
        && let Some(Some(at)) = breaks.last()
    {
        out.produced('\n', *at);
    }
    if block.chomping == 2 {
        for at in trailing {
            out.produced('\n', at);
        }
    }
    if !eof && !sibling.is_empty() {
        out.raw(sibling);
        out.line_break();
    }
    out.finish(start, literal)
}

fn is_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Parses `written` and checks the source map of its scalar.
fn check(written: &Written) -> Result<(), TestCaseError> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", written.yaml.as_str()).unwrap();
    let text = file.text();
    let parsed = parse(&file);
    prop_assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let document = parsed.document.unwrap();
    let node = document
        .nodes()
        .map(|(_, node)| node)
        .find(|node| {
            matches!(node.kind, NodeKind::Scalar(_))
                && usize::try_from(node.span.start) == Ok(written.start)
        })
        .unwrap();
    let NodeKind::Scalar(scalar) = &node.kind else {
        unreachable!();
    };
    let style = match text[written.start..].chars().next() {
        Some('\'') => matches!(scalar.style, ScalarStyle::SingleQuoted),
        Some('"') => matches!(scalar.style, ScalarStyle::DoubleQuoted),
        Some('|') => matches!(scalar.style, ScalarStyle::Literal(_)),
        Some('>') => matches!(scalar.style, ScalarStyle::Folded(_)),
        _ => matches!(scalar.style, ScalarStyle::Plain),
    };
    prop_assert!(style, "unexpected style {:?}", scalar.style);
    if let Some(value) = &written.value {
        prop_assert_eq!(&scalar.value, value);
    }
    let map = node.source_map(&file).unwrap();
    prop_assert!(map.is_exact());
    let (start, end) = (node.span.start, node.span.end);
    let mut previous = start;
    for (i, (offset, ch)) in scalar.value.char_indices().enumerate() {
        let mapped = map.source_offset(offset);
        prop_assert!(start <= mapped && mapped < end, "{offset} maps to {mapped}");
        let at = usize::try_from(mapped).unwrap();
        prop_assert!(text.is_char_boundary(at), "{offset} maps to {mapped}");
        prop_assert!(
            mapped >= previous,
            "{offset} maps to {mapped} before {previous}"
        );
        previous = mapped;
        let source = text[at..].chars().next().unwrap();
        if let Some(expected) = &written.expected {
            let (expected, copied) = expected[i];
            prop_assert_eq!(at, expected, "decoded offset {}", offset);
            if copied {
                prop_assert_eq!(source, ch, "decoded offset {}", offset);
            }
        } else {
            let folded = is_break(source) && (ch == ' ' || ch == '\n');
            prop_assert!(source == ch || folded, "{offset} maps to {source:?}");
        }
        let span = map.source_span(offset..offset + ch.len_utf8());
        prop_assert_eq!(span.start, mapped);
        prop_assert!(span.start < span.end && span.end <= end);
    }
    let last = map.source_offset(scalar.value.len());
    prop_assert!(previous <= last && last <= end);
    Ok(())
}

proptest! {
    #[test]
    fn plain_scalars_map_exactly(
        words in prop::collection::vec((word(PLAIN), gap()), 1..6),
        line_break in line_break(),
        choices in choices(),
    ) {
        let written = write_plain(&words, line_break, choices);
        check(&written)?;
    }

    #[test]
    fn single_quoted_scalars_map_exactly(
        lead in prop::option::of(gap()),
        words in prop::collection::vec((word(QUOTED), gap()), 0..6),
        trail in any::<bool>(),
        line_break in line_break(),
        choices in choices(),
    ) {
        let written = write_single(lead.as_ref(), &words, trail, line_break, choices);
        check(&written)?;
    }

    #[test]
    fn double_quoted_scalars_map_exactly(
        content in prop::collection::vec(select(DOUBLE), 0..24),
        line_break in line_break(),
        choices in choices(),
    ) {
        let written = write_double(&content, line_break, choices);
        check(&written)?;
    }

    #[test]
    fn literal_block_scalars_map_exactly(block in block()) {
        let written = write_block(&block, true);
        check(&written)?;
    }

    #[test]
    fn folded_block_scalars_map_exactly(block in block()) {
        let written = write_block(&block, false);
        check(&written)?;
    }
}
