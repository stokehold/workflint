use std::collections::{BTreeMap, VecDeque};

use super::macros::{is_blank, is_blankz, is_break};
use super::parser::SimpleKey;
use super::reader::{CharBuffer, ReaderState, yaml_parser_update_buffer};
use super::token::{Token, TokenData};
use super::{
    CommentKind, Encoding, Error, ErrorKind, INPUT_BUFFER_SIZE, Mark, Result, ScalarStyle,
};

/// The most digits of a `%YAML` version number, go-yaml's
/// `max_number_length`.
const MAX_NUMBER_LENGTH: u64 = 2;

/// The deepest flow nesting, go-yaml's `max_flow_level`.
const MAX_FLOW_LEVEL: i32 = 10_000;

/// The deepest stack of block indentation levels, go-yaml's `max_indents`.
const MAX_INDENTS: usize = 10_000;

/// How many characters after its start a simple key may end, as YAML 1.2
/// and go-yaml's `simpleKeyIsValid` require.
const MAX_SIMPLE_KEY_LENGTH: u64 = 1024;

/// How far go-yaml's comment scanners look ahead, in bytes.
const COMMENT_PEEK_LIMIT: usize = 512;

/// The context of an error in a tag or a `%TAG` directive, as go-yaml's
/// `setScannerTagError`.
fn tag_context(directive: bool) -> &'static str {
    if directive {
        "while parsing a %TAG directive"
    } else {
        "while parsing a tag"
    }
}

/// A comment the scanner skipped, a workflint addition.
#[derive(Debug, PartialEq)]
pub(crate) struct Comment {
    /// Whether code precedes the comment on its line.
    pub(crate) kind: CommentKind,
    /// The position of the `#`.
    pub(crate) start_mark: Mark,
    /// The end of the comment's line, before the line break.
    pub(crate) end_mark: Mark,
}

/// A position as go-yaml records it: the mark and go-yaml's `Mark.Index`,
/// which counts characters, where `Mark::index` counts bytes.
#[derive(Clone, Copy)]
struct Position {
    mark: Mark,
    chars: u64,
}

/// A block of comment lines that go-yaml's `scanComments` records as one
/// head or foot comment. `unroll_indent` moves a block end to the start of
/// such a block, so the scanner keeps the blocks that the current
/// `scan_to_next_token` call found. Older blocks end before that scan
/// started, so go-yaml's search never reaches them.
struct CommentBlock {
    /// Where go-yaml started to scan for this block, as a character index.
    scan: i64,
    /// The character index of the block's first `#`.
    start: i64,
    /// Where the block ends, as a character index.
    end: i64,
    /// go-yaml's 1-based column of the block's first `#`.
    column: i64,
    /// The mark go-yaml gives a block end that moves to this block. Its line
    /// and column are go-yaml's, which counts the column from 1 here and can
    /// count one line too few. The index is one byte after the `#`.
    mark: Mark,
}

/// The kinds of token that go-yaml's comment scanner looks at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TailKind {
    FlowEntry,
    Value,
    BlockEntry,
    Other,
}

impl TailKind {
    fn of(data: &TokenData) -> Self {
        match data {
            TokenData::FlowEntry => Self::FlowEntry,
            TokenData::Value => Self::Value,
            TokenData::BlockEntry => Self::BlockEntry,
            _ => Self::Other,
        }
    }
}

/// Reads the scanner's buffer one UTF-8 byte at a time, as go-yaml's comment
/// scanner reads `parser.buffer[parser.buffer_pos+peek]`.
#[derive(Clone, Copy, Default)]
struct BytePeek {
    /// The character in the buffer.
    index: usize,
    /// The byte inside that character.
    byte: usize,
}

impl BytePeek {
    fn get(self, buffer: &CharBuffer) -> Option<(char, u8)> {
        let ch = *buffer.get(self.index)?;
        let mut bytes = [0; 4];
        let byte = *ch.encode_utf8(&mut bytes).as_bytes().get(self.byte)?;
        Some((ch, byte))
    }

    /// The byte here, or NUL past the end of the buffer as in go-yaml.
    fn byte(self, buffer: &CharBuffer) -> u8 {
        self.get(buffer).map_or(0, |(_, byte)| byte)
    }

    /// Whether a line break starts here, as go-yaml's `isLineBreak`.
    fn is_line_break(self, buffer: &CharBuffer) -> bool {
        self.byte == 0 && self.get(buffer).is_some_and(|(ch, _)| is_break(ch))
    }

    fn advance(&mut self, buffer: &CharBuffer) {
        let width = buffer.get(self.index).map_or(1, |ch| ch.len_utf8());
        if self.byte + 1 < width {
            self.byte += 1;
        } else {
            self.index += 1;
            self.byte = 0;
        }
    }
}

/// Given an input string, produce a stream of [`Token`]s.
///
/// The parser takes the tokens with `peek` and `skip_token`.
pub(crate) struct Scanner<'input> {
    /// The whole input, for the reader and for looking back at the line
    /// before a comment.
    pub(crate) source: &'input str,
    /// Where the reader is in `source`.
    pub(crate) reader: ReaderState,
    /// The working buffer.
    ///
    /// This always contains valid UTF-8.
    pub(crate) buffer: CharBuffer,
    /// Whether go-yaml's buffer starts with a byte order mark, which its
    /// `isBOM` checks instead of the current character.
    pub(crate) buffer_starts_with_bom: bool,
    /// The mark of the current position.
    pub(crate) mark: Mark,
    /// go-yaml's `Mark.Index` of the current position, which counts
    /// characters.
    chars: u64,
    /// The line breaks skipped since the last character that is not a blank,
    /// go-yaml's `newlines`.
    newlines: u64,
    /// Have we started to scan the input stream?
    pub(crate) stream_start_produced: bool,
    /// Have we reached the end of the input stream?
    pub(crate) stream_end_produced: bool,
    /// The number of unclosed '[' and '{' indicators.
    pub(crate) flow_level: i32,
    /// The tokens queue.
    pub(crate) tokens: VecDeque<Token>,
    /// The number of tokens fetched from the queue.
    pub(crate) tokens_parsed: usize,
    /// Does the tokens queue contain a token ready for dequeueing.
    pub(crate) token_available: bool,
    /// The kinds of the last and the second-to-last token in queue order,
    /// counting tokens the parser has taken.
    tail: [TailKind; 2],
    /// The indentation levels stack.
    pub(crate) indents: Vec<i64>,
    /// The current indentation level.
    pub(crate) indent: i64,
    /// May a simple key occur at the current position?
    pub(crate) simple_key_allowed: bool,
    /// The stack of simple keys.
    pub(crate) simple_keys: Vec<SimpleKey>,
    /// The index in `simple_keys` of the simple key that starts at each
    /// token number, go-yaml's `simple_keys_by_tok`.
    simple_keys_by_token: BTreeMap<usize, usize>,
    /// The comments scanned and not yet taken by the parser, in source order.
    pub(crate) comments: VecDeque<Comment>,
    /// The comment blocks of the current `scan_to_next_token` call.
    comment_blocks: Vec<CommentBlock>,
    /// The buffers of the scalar scanners, kept between scalars.
    spare: Spare,
}

/// The buffers that the scalar scanners fill with the blanks and line breaks
/// between the parts of a scalar. The scanner keeps them between scalars, so
/// that each scalar does not allocate them again. They are empty between
/// scalars.
#[derive(Default)]
struct Spare {
    leading_break: String,
    trailing_breaks: String,
    whitespaces: String,
}

impl Spare {
    /// Empties the buffers and keeps their memory.
    fn cleared(
        mut leading_break: String,
        mut trailing_breaks: String,
        mut whitespaces: String,
    ) -> Self {
        leading_break.clear();
        trailing_breaks.clear();
        whitespaces.clear();
        Self {
            leading_break,
            trailing_breaks,
            whitespaces,
        }
    }
}

impl<'input> Scanner<'input> {
    /// Create a scanner over `input`.
    pub(crate) fn new(input: &'input str) -> Scanner<'input> {
        Self {
            source: input,
            reader: ReaderState::default(),
            buffer: CharBuffer::with_capacity(INPUT_BUFFER_SIZE),
            buffer_starts_with_bom: false,
            mark: Mark::default(),
            chars: 0,
            newlines: 0,
            stream_start_produced: false,
            stream_end_produced: false,
            flow_level: 0,
            tokens: VecDeque::with_capacity(16),
            tokens_parsed: 0,
            token_available: false,
            tail: [TailKind::Other; 2],
            indents: Vec::with_capacity(16),
            indent: 0,
            simple_key_allowed: false,
            simple_keys: Vec::with_capacity(16),
            simple_keys_by_token: BTreeMap::new(),
            comments: VecDeque::new(),
            comment_blocks: Vec::new(),
            spare: Spare::default(),
        }
    }

    #[inline]
    fn cache(&mut self, length: usize) -> Result<()> {
        if self.unread() >= length {
            Ok(())
        } else {
            yaml_parser_update_buffer(self, length)
        }
    }

    fn position(&self) -> Position {
        Position {
            mark: self.mark,
            chars: self.chars,
        }
    }

    /// Equivalent to the libyaml macro `SKIP`.
    ///
    /// Does nothing at the end of the input.
    #[inline]
    fn skip_char(&mut self) {
        if let Some(popped) = self.buffer.pop_front() {
            if !is_blank(popped) {
                self.newlines = 0;
            }
            let width = popped.len_utf8();
            self.mark.index += width as u64;
            self.mark.column += 1;
            self.chars += 1;
        }
    }

    /// Equivalent to the libyaml macro `SKIP_LINE`.
    fn skip_line_break(&mut self) {
        if let Some(front) = self.buffer.front().copied() {
            if let ('\r', Some('\n')) = (front, self.buffer.get(1).copied()) {
                self.mark.index += 2;
                self.mark.column = 0;
                self.mark.line += 1;
                self.chars += 2;
                self.newlines += 1;
                self.buffer.skip(2);
            } else if is_break(front) {
                let width = front.len_utf8();
                self.mark.index += width as u64;
                self.mark.column = 0;
                self.mark.line += 1;
                self.chars += 1;
                self.newlines += 1;
                self.buffer.pop_front();
            }
        }
    }

    /// Equivalent to the libyaml macro `READ`.
    ///
    /// Does nothing at the end of the input.
    #[inline]
    fn read_char(&mut self, string: &mut String) {
        if let Some(popped) = self.buffer.pop_front() {
            if !is_blank(popped) {
                self.newlines = 0;
            }
            string.push(popped);
            self.mark.index += popped.len_utf8() as u64;
            self.mark.column += 1;
            self.chars += 1;
        }
    }

    /// Appends the input from byte offset `start` to the current position to
    /// `string`. The scanner moved over those characters with
    /// [`skip_char`](Self::skip_char), which leaves the same state as
    /// [`read_char`](Self::read_char), so this appends what `read_char`
    /// would have appended, in one copy.
    fn push_skipped(&self, string: &mut String, start: u64) {
        let skipped = usize::try_from(start)
            .ok()
            .zip(usize::try_from(self.mark.index).ok())
            .and_then(|(start, end)| self.source.get(start..end));
        debug_assert!(skipped.is_some(), "skipped characters out of the input");
        string.push_str(skipped.unwrap_or_default());
    }

    /// Equivalent to the libyaml macro `READ_LINE`.
    ///
    /// Like `READ_LINE`, this does nothing when the input does not continue
    /// with a line break. A literal block scalar that ends the input without a
    /// final line break reaches this at the end of the input, where upstream
    /// panicked.
    fn read_line_break(&mut self, string: &mut String) {
        let Some(front) = self.buffer.front().copied() else {
            return;
        };

        if let ('\r', Some('\n')) = (front, self.buffer.get(1).copied()) {
            string.push('\n');
            self.buffer.skip(2);
            self.mark.index += 2;
            self.mark.column = 0;
            self.mark.line += 1;
            self.chars += 2;
            self.newlines += 1;
        } else if is_break(front) {
            self.buffer.pop_front();
            let char_len = front.len_utf8();
            if char_len == 3 {
                // libyaml preserves Unicode breaks in this case.
                string.push(front);
            } else {
                string.push('\n');
            }
            self.mark.index += char_len as u64;
            self.mark.column = 0;
            self.mark.line += 1;
            self.chars += 1;
            self.newlines += 1;
        }
    }

    /// Equivalent of the libyaml `PEEK_TOKEN` macro, used by the parser.
    pub(crate) fn peek(&mut self) -> Result<&Token> {
        self.peek_mut().map(|token| &*token)
    }

    /// Equivalent of the libyaml `PEEK_TOKEN` macro, used by the parser.
    pub(crate) fn peek_mut(&mut self) -> Result<&mut Token> {
        if !self.token_available {
            self.fetch_more_tokens()?;
        }
        let mark = self.mark;
        self.tokens
            .front_mut()
            .ok_or_else(|| Error::internal(ErrorKind::Scanner, mark))
    }

    /// Equivalent of the libyaml `SKIP_TOKEN` macro, used by the parser.
    pub(crate) fn skip_token(&mut self) {
        self.token_available = false;
        self.tokens_parsed = self.tokens_parsed.wrapping_add(1);
        self.stream_end_produced = matches!(
            self.tokens.pop_front(),
            Some(Token {
                data: TokenData::StreamEnd,
                ..
            })
        );
    }

    /// Adds `token` to the queue at `position`, counted from the first token
    /// the parser has not taken, or at the end for `None`, as go-yaml's
    /// `insertToken` does.
    ///
    /// A position past the end of the queue is an internal error. The
    /// callers compute positions from token numbers that a simple key
    /// recorded while its token was still in the queue, and go-yaml's
    /// `fetchMoreTokens` keeps a possible simple key's token in the queue.
    fn insert_token(&mut self, position: Option<usize>, token: Token) -> Result<()> {
        let kind = TailKind::of(&token.data);
        let len = self.tokens.len();
        match position {
            None => self.tokens.push_back(token),
            Some(position) if position <= len => self.tokens.insert(position, token),
            Some(_) => return Err(Error::internal(ErrorKind::Scanner, self.mark)),
        }
        match position {
            Some(position) if position + 1 < len => {}
            Some(position) if position + 1 == len => self.tail[1] = kind,
            _ => self.tail = [kind, self.tail[0]],
        }
        Ok(())
    }

    /// The position in the queue of the token with number `token_number`, or
    /// `None` if the parser has already taken that token. go-yaml's
    /// `insertToken` appends a token whose position is negative, so `None`
    /// tells [`Self::insert_token`] to append.
    fn queue_position(&self, token_number: usize) -> Option<usize> {
        token_number.checked_sub(self.tokens_parsed)
    }

    /// The top of the simple key stack, which `fetch_stream_start` creates and
    /// only the end of a flow collection pops.
    fn last_simple_key(&mut self) -> Result<(usize, &mut SimpleKey)> {
        let mark = self.mark;
        let index = self.simple_keys.len().wrapping_sub(1);
        self.simple_keys
            .last_mut()
            .map(|key| (index, key))
            .ok_or_else(|| Error::internal(ErrorKind::Scanner, mark))
    }

    fn set_scanner_error<T>(
        &mut self,
        context: &'static str,
        context_mark: Mark,
        problem: &'static str,
    ) -> Result<T> {
        Err(Error::scanner(context, context_mark, problem, self.mark))
    }

    /// Fetches tokens until the parser may take the first one, as go-yaml's
    /// `fetchMoreTokens` does: the queue holds at least three tokens, and the
    /// first one cannot become a simple key any more. This lookahead is why
    /// an error in a later token can come before the event of an earlier one.
    pub(crate) fn fetch_more_tokens(&mut self) -> Result<()> {
        loop {
            if self.tokens.len() > 2 {
                let Some(&index) = self.simple_keys_by_token.get(&self.tokens_parsed) else {
                    break;
                };
                if !self.simple_key_is_valid(index)? {
                    break;
                }
            }
            self.fetch_next_token()?;
        }
        self.token_available = true;
        Ok(())
    }

    fn fetch_next_token(&mut self) -> Result<()> {
        self.cache(1)?;
        if !self.stream_start_produced {
            return self.fetch_stream_start();
        }
        let scan_mark = self.position();
        self.scan_to_next_token()?;
        self.unroll_indent(self.mark.column as i64, scan_mark)?;
        self.cache(4)?;
        if IS_Z!(self.buffer) {
            return self.fetch_stream_end();
        }
        if self.mark.column == 0_u64 && CHECK!(self.buffer, '%') {
            return self.fetch_directive();
        }
        if self.mark.column == 0_u64
            && CHECK_AT!(self.buffer, '-', 0)
            && CHECK_AT!(self.buffer, '-', 1)
            && CHECK_AT!(self.buffer, '-', 2)
            && is_blankz(self.buffer.get(3).copied())
        {
            return self.fetch_document_indicator(TokenData::DocumentStart);
        }
        if self.mark.column == 0_u64
            && CHECK_AT!(self.buffer, '.', 0)
            && CHECK_AT!(self.buffer, '.', 1)
            && CHECK_AT!(self.buffer, '.', 2)
            && is_blankz(self.buffer.get(3).copied())
        {
            return self.fetch_document_indicator(TokenData::DocumentEnd);
        }
        self.fetch_content_token()?;
        // go-yaml reads a comment on the rest of the line right after each
        // token, except after a `-`.
        if self.tail[0] != TailKind::BlockEntry {
            self.scan_line_comment()?;
        }
        Ok(())
    }

    /// Fetches a token that is neither a directive, a document indicator, nor
    /// the end of the stream.
    fn fetch_content_token(&mut self) -> Result<()> {
        if CHECK!(self.buffer, '[') {
            return self.fetch_flow_collection_start(TokenData::FlowSequenceStart);
        }
        if CHECK!(self.buffer, '{') {
            return self.fetch_flow_collection_start(TokenData::FlowMappingStart);
        }
        if CHECK!(self.buffer, ']') {
            return self.fetch_flow_collection_end(TokenData::FlowSequenceEnd);
        }
        if CHECK!(self.buffer, '}') {
            return self.fetch_flow_collection_end(TokenData::FlowMappingEnd);
        }
        if CHECK!(self.buffer, ',') {
            return self.fetch_flow_entry();
        }
        if CHECK!(self.buffer, '-') && IS_BLANKZ_AT!(self.buffer, 1) {
            return self.fetch_block_entry();
        }
        if CHECK!(self.buffer, '?') && IS_BLANKZ_AT!(self.buffer, 1) {
            return self.fetch_key();
        }
        if CHECK!(self.buffer, ':') && (self.flow_level != 0 || IS_BLANKZ_AT!(self.buffer, 1)) {
            return self.fetch_value();
        }
        if CHECK!(self.buffer, '*') {
            return self.fetch_anchor(true);
        }
        if CHECK!(self.buffer, '&') {
            return self.fetch_anchor(false);
        }
        if CHECK!(self.buffer, '!') {
            return self.fetch_tag();
        }
        if CHECK!(self.buffer, '|') && self.flow_level == 0 {
            return self.fetch_block_scalar(true);
        }
        if CHECK!(self.buffer, '>') && self.flow_level == 0 {
            return self.fetch_block_scalar(false);
        }
        if CHECK!(self.buffer, '\'') {
            return self.fetch_flow_scalar(true);
        }
        if CHECK!(self.buffer, '"') {
            return self.fetch_flow_scalar(false);
        }
        if !(IS_BLANKZ!(self.buffer)
            || CHECK!(self.buffer, '-')
            || CHECK!(self.buffer, '?')
            || CHECK!(self.buffer, ':')
            || CHECK!(self.buffer, ',')
            || CHECK!(self.buffer, '[')
            || CHECK!(self.buffer, ']')
            || CHECK!(self.buffer, '{')
            || CHECK!(self.buffer, '}')
            || CHECK!(self.buffer, '#')
            || CHECK!(self.buffer, '&')
            || CHECK!(self.buffer, '*')
            || CHECK!(self.buffer, '!')
            || CHECK!(self.buffer, '|')
            || CHECK!(self.buffer, '>')
            || CHECK!(self.buffer, '\'')
            || CHECK!(self.buffer, '"')
            || CHECK!(self.buffer, '%')
            || CHECK!(self.buffer, '@')
            || CHECK!(self.buffer, '`'))
            || CHECK!(self.buffer, '-') && !IS_BLANK_AT!(self.buffer, 1)
            || (CHECK!(self.buffer, '?') || CHECK!(self.buffer, ':'))
                && !IS_BLANKZ_AT!(self.buffer, 1)
        {
            return self.fetch_plain_scalar();
        }
        self.set_scanner_error(
            "while scanning for the next token",
            self.mark,
            "found character that cannot start any token",
        )
    }

    /// Whether the simple key at `index` in the stack can still be a key, as
    /// go-yaml's `simpleKeyIsValid`. A key that is too far behind is no
    /// longer possible, and an error if it was required.
    ///
    /// go-yaml compares character indexes. A key is valid only on the line
    /// where it starts, so the columns give the same distance.
    fn simple_key_is_valid(&mut self, index: usize) -> Result<bool> {
        let mark = self.mark;
        let Some(key) = self.simple_keys.get_mut(index) else {
            return Ok(false);
        };
        if !key.possible {
            return Ok(false);
        }
        if key.mark.line < mark.line || key.mark.column + MAX_SIMPLE_KEY_LENGTH < mark.column {
            if key.required {
                let key_mark = key.mark;
                return self.set_scanner_error(
                    "while scanning a simple key",
                    key_mark,
                    "could not find expected ':'",
                );
            }
            key.possible = false;
            return Ok(false);
        }
        Ok(true)
    }

    fn save_simple_key(&mut self) -> Result<()> {
        let required = self.flow_level == 0 && self.indent == self.mark.column as i64;
        if self.simple_key_allowed {
            let simple_key = SimpleKey {
                possible: true,
                required,
                token_number: self.tokens_parsed + self.tokens.len(),
                mark: self.mark,
            };
            self.remove_simple_key()?;
            let (index, key) = self.last_simple_key()?;
            *key = simple_key;
            self.simple_keys_by_token
                .insert(simple_key.token_number, index);
        }
        Ok(())
    }

    fn remove_simple_key(&mut self) -> Result<()> {
        let (_, simple_key) = self.last_simple_key()?;
        if simple_key.possible {
            let mark = simple_key.mark;
            if simple_key.required {
                return self.set_scanner_error(
                    "while scanning a simple key",
                    mark,
                    "could not find expected ':'",
                );
            }
            simple_key.possible = false;
            let token_number = simple_key.token_number;
            self.simple_keys_by_token.remove(&token_number);
        }
        Ok(())
    }

    fn increase_flow_level(&mut self) -> Result<()> {
        let empty_simple_key = SimpleKey {
            possible: false,
            required: false,
            token_number: self.tokens_parsed + self.tokens.len(),
            mark: self.mark,
        };
        self.simple_keys.push(empty_simple_key);
        self.flow_level += 1;
        if self.flow_level > MAX_FLOW_LEVEL {
            return self.set_scanner_error(
                "while increasing flow level",
                empty_simple_key.mark,
                "exceeded max depth of 10000",
            );
        }
        Ok(())
    }

    fn decrease_flow_level(&mut self) {
        if self.flow_level != 0 {
            self.flow_level -= 1;
            // go-yaml forgets the token number of the popped level's key,
            // even when that key is no longer possible.
            if let Some(key) = self.simple_keys.pop() {
                self.simple_keys_by_token.remove(&key.token_number);
            }
        }
    }

    /// Pushes a block indentation level and inserts `data` at the token
    /// `number`, or at the end for `None`.
    fn roll_indent(
        &mut self,
        column: i64,
        number: Option<usize>,
        data: TokenData,
        mark: Mark,
    ) -> Result<()> {
        if self.flow_level != 0 {
            return Ok(());
        }
        if self.indent < column {
            self.indents.push(self.indent);
            self.indent = column;
            if self.indents.len() > MAX_INDENTS {
                let (_, key) = self.last_simple_key()?;
                let key_mark = key.mark;
                return self.set_scanner_error(
                    "while increasing indent level",
                    key_mark,
                    "exceeded max depth of 10000",
                );
            }
            let token = Token {
                data,
                start_mark: mark,
                end_mark: mark,
            };
            let position = number.and_then(|number| self.queue_position(number));
            self.insert_token(position, token)?;
        }
        Ok(())
    }

    /// Pops the indentation levels above `column` and adds a block end for
    /// each, as go-yaml's `unrollIndent` does.
    ///
    /// A block end is where scanning for the current token started, which is
    /// the end of the previous token. If a comment block that go-yaml would
    /// treat as a foot comment of the closing block starts at its
    /// indentation, the block end moves to that comment.
    fn unroll_indent(&mut self, column: i64, scan_mark: Position) -> Result<()> {
        if self.flow_level != 0 {
            return Ok(());
        }
        let mut block_mark = scan_mark.mark;
        let mut block_index = scan_mark.chars as i64 - 1;
        while self.indent > column {
            let mut stop_index = block_index;
            for block in self.comment_blocks.iter().rev() {
                if block.end < stop_index {
                    break;
                }
                if block.column == self.indent + 1 {
                    block_mark = block.mark;
                    block_index = block.start;
                }
                stop_index = block.scan;
            }
            let token = Token {
                data: TokenData::BlockEnd,
                start_mark: block_mark,
                end_mark: block_mark,
            };
            self.insert_token(None, token)?;
            // `roll_indent` pushes an entry for every indentation level above
            // -1, so the stack is empty only once `indent` is back at -1.
            let Some(indent) = self.indents.pop() else {
                break;
            };
            self.indent = indent;
        }
        Ok(())
    }

    fn fetch_stream_start(&mut self) -> Result<()> {
        let simple_key = SimpleKey {
            possible: false,
            required: false,
            token_number: 0,
            mark: Mark {
                index: 0,
                line: 0,
                column: 0,
            },
        };
        self.indent = -1;
        self.simple_keys.push(simple_key);
        self.simple_key_allowed = true;
        self.stream_start_produced = true;
        let token = Token {
            data: TokenData::StreamStart {
                encoding: Encoding::Utf8,
            },
            start_mark: self.mark,
            end_mark: self.mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_stream_end(&mut self) -> Result<()> {
        if self.mark.column != 0_u64 {
            self.mark.column = 0_u64;
            self.mark.line += 1;
        }
        self.unroll_indent(-1_i64, self.position())?;
        self.remove_simple_key()?;
        self.simple_key_allowed = false;
        let token = Token {
            data: TokenData::StreamEnd,
            start_mark: self.mark,
            end_mark: self.mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_directive(&mut self) -> Result<()> {
        self.unroll_indent(-1_i64, self.position())?;
        self.remove_simple_key()?;
        self.simple_key_allowed = false;
        let token = self.scan_directive()?;
        self.insert_token(None, token)
    }

    fn fetch_document_indicator(&mut self, data: TokenData) -> Result<()> {
        self.unroll_indent(-1_i64, self.position())?;
        self.remove_simple_key()?;
        self.simple_key_allowed = false;
        let start_mark: Mark = self.mark;
        self.skip_char();
        self.skip_char();
        self.skip_char();
        let end_mark: Mark = self.mark;

        let token = Token {
            data,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_flow_collection_start(&mut self, data: TokenData) -> Result<()> {
        self.save_simple_key()?;
        self.increase_flow_level()?;
        self.simple_key_allowed = true;
        let start_mark: Mark = self.mark;
        self.skip_char();
        let end_mark: Mark = self.mark;
        let token = Token {
            data,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_flow_collection_end(&mut self, data: TokenData) -> Result<()> {
        self.remove_simple_key()?;
        self.decrease_flow_level();
        self.simple_key_allowed = false;
        let start_mark: Mark = self.mark;
        self.skip_char();
        let end_mark: Mark = self.mark;
        let token = Token {
            data,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_flow_entry(&mut self) -> Result<()> {
        self.remove_simple_key()?;
        self.simple_key_allowed = true;
        let start_mark: Mark = self.mark;
        self.skip_char();
        let end_mark: Mark = self.mark;
        let token = Token {
            data: TokenData::FlowEntry,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_block_entry(&mut self) -> Result<()> {
        if self.flow_level == 0 {
            if !self.simple_key_allowed {
                return self.set_scanner_error(
                    "",
                    self.mark,
                    "block sequence entries are not allowed in this context",
                );
            }
            self.roll_indent(
                self.mark.column as i64,
                None,
                TokenData::BlockSequenceStart,
                self.mark,
            )?;
        }
        self.remove_simple_key()?;
        self.simple_key_allowed = true;
        let start_mark: Mark = self.mark;
        self.skip_char();
        let end_mark: Mark = self.mark;
        let token = Token {
            data: TokenData::BlockEntry,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_key(&mut self) -> Result<()> {
        if self.flow_level == 0 {
            if !self.simple_key_allowed {
                return self.set_scanner_error(
                    "",
                    self.mark,
                    "mapping keys are not allowed in this context",
                );
            }
            self.roll_indent(
                self.mark.column as i64,
                None,
                TokenData::BlockMappingStart,
                self.mark,
            )?;
        }
        self.remove_simple_key()?;
        self.simple_key_allowed = self.flow_level == 0;
        let start_mark: Mark = self.mark;
        self.skip_char();
        let end_mark: Mark = self.mark;
        let token = Token {
            data: TokenData::Key,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_value(&mut self) -> Result<()> {
        let (index, simple_key) = self.last_simple_key()?;
        let simple_key = *simple_key;
        if self.simple_key_is_valid(index)? {
            let token = Token {
                data: TokenData::Key,
                start_mark: simple_key.mark,
                end_mark: simple_key.mark,
            };
            // The parser can already have taken the key's first token when
            // an empty flow collection starts the key: closing it forgets the
            // key's token number, as in go-yaml's `decreaseFlowLevel`, so
            // `fetch_more_tokens` stops waiting for the `:`. Then the KEY
            // token, and the BLOCK-MAPPING-START token that `roll_indent`
            // adds, go to the end of the queue, as go-yaml's `insertToken`
            // does with a negative position. So `[] x: y` is the document
            // `[]`, and `- [] b: 1` fails in the parser.
            let position = self.queue_position(simple_key.token_number);
            self.insert_token(position, token)?;
            self.roll_indent(
                simple_key.mark.column as i64,
                Some(simple_key.token_number),
                TokenData::BlockMappingStart,
                simple_key.mark,
            )?;
            if let Some(key) = self.simple_keys.get_mut(index) {
                key.possible = false;
            }
            self.simple_keys_by_token.remove(&simple_key.token_number);
            self.simple_key_allowed = false;
        } else {
            if self.flow_level == 0 {
                if !self.simple_key_allowed {
                    return self.set_scanner_error(
                        "",
                        self.mark,
                        "mapping values are not allowed in this context",
                    );
                }
                self.roll_indent(
                    self.mark.column as i64,
                    None,
                    TokenData::BlockMappingStart,
                    self.mark,
                )?;
            }
            self.simple_key_allowed = self.flow_level == 0;
        }
        let start_mark: Mark = self.mark;
        self.skip_char();
        let end_mark: Mark = self.mark;
        let token = Token {
            data: TokenData::Value,
            start_mark,
            end_mark,
        };
        self.insert_token(None, token)
    }

    fn fetch_anchor(&mut self, fetch_alias_instead_of_anchor: bool) -> Result<()> {
        self.save_simple_key()?;
        self.simple_key_allowed = false;
        let token = self.scan_anchor(fetch_alias_instead_of_anchor)?;
        self.insert_token(None, token)
    }

    fn fetch_tag(&mut self) -> Result<()> {
        self.save_simple_key()?;
        self.simple_key_allowed = false;
        let token = self.scan_tag()?;
        self.insert_token(None, token)
    }

    fn fetch_block_scalar(&mut self, literal: bool) -> Result<()> {
        self.remove_simple_key()?;
        self.simple_key_allowed = true;
        let token = self.scan_block_scalar(literal)?;
        self.insert_token(None, token)
    }

    fn fetch_flow_scalar(&mut self, single: bool) -> Result<()> {
        self.save_simple_key()?;
        self.simple_key_allowed = false;
        let token = self.scan_flow_scalar(single)?;
        self.insert_token(None, token)
    }

    fn fetch_plain_scalar(&mut self) -> Result<()> {
        self.save_simple_key()?;
        self.simple_key_allowed = false;
        let token = self.scan_plain_scalar()?;
        self.insert_token(None, token)
    }

    fn scan_to_next_token(&mut self) -> Result<()> {
        let scan_mark = self.position();
        self.comment_blocks.clear();
        loop {
            self.cache(1)?;
            if self.mark.column == 0 && self.buffer_starts_with_bom {
                self.skip_char();
            }
            self.cache(1)?;
            while CHECK!(self.buffer, ' ')
                || (self.flow_level != 0 || !self.simple_key_allowed) && CHECK!(self.buffer, '\t')
            {
                self.skip_char();
                self.cache(1)?;
            }
            if CHECK!(self.buffer, '#') {
                self.scan_comments(scan_mark)?;
            }
            if !IS_BREAK!(self.buffer) {
                break;
            }
            self.cache(2)?;
            self.skip_line_break();
            if self.flow_level == 0 {
                self.simple_key_allowed = true;
            }
        }
        Ok(())
    }

    /// Skips the comment whose `#` is `seen` characters ahead, and the blanks
    /// and line breaks before it, and records the comment. This is the
    /// consuming loop of go-yaml's comment scanners.
    fn skip_to_comment_end(&mut self, seen: u64) -> Result<Mark> {
        let mut start = None;
        loop {
            self.cache(1)?;
            if IS_Z!(self.buffer) {
                break;
            }
            if IS_BREAK!(self.buffer) {
                if self.chars >= seen {
                    break;
                }
                self.cache(2)?;
                self.skip_line_break();
            } else {
                if self.chars >= seen && start.is_none() {
                    start = Some(self.mark);
                }
                self.skip_char();
            }
        }
        let start_mark = start.unwrap_or(self.mark);
        self.record_comment(start_mark)?;
        Ok(start_mark)
    }

    /// Reads the comment on the rest of the line after a token, as go-yaml's
    /// `scanLineComment`. Unlike the scan between tokens, this skips tabs
    /// before the `#` in every context.
    fn scan_line_comment(&mut self) -> Result<()> {
        if self.newlines > 0 {
            return Ok(());
        }
        for peek in 0..COMMENT_PEEK_LIMIT {
            self.cache(peek + 1)?;
            let ch = self.buffer.get(peek).copied();
            if is_blank(ch) {
                continue;
            }
            if ch == Some('#') {
                self.skip_to_comment_end(self.chars + peek as u64)?;
            }
            break;
        }
        Ok(())
    }

    /// Reads the comments that start at the current `#`, as go-yaml's
    /// `scanComments`.
    ///
    /// go-yaml looks ahead over blanks and line breaks for the next `#`, and
    /// takes every comment it finds, with the blanks before it. So a tab may
    /// come before a comment on a later line, which the scan between tokens
    /// does not allow.
    ///
    /// The function also records the comment blocks that go-yaml splits the
    /// comments into, which `unroll_indent` needs. The splitting follows
    /// go-yaml byte for byte, including its line count, which misses the
    /// line break right after a comment unless it is a `\r\n`.
    fn scan_comments(&mut self, scan_mark: Position) -> Result<()> {
        let token_kind = match self.tail {
            [TailKind::FlowEntry, before] if self.tokens_parsed + self.tokens.len() > 1 => before,
            [last, _] => last,
        };
        let indent = |scanner: &Self| scanner.indent.max(0);
        let mut next_indent = indent(self);
        let mut recent_empty = false;
        let mut first_empty = self.newlines <= 1;
        let mut line = self.mark.line as i64;
        let mut column = self.mark.column as i64;
        // go-yaml's comment text. Only whether it is empty matters here.
        let mut text = false;
        // go-yaml's `start_mark` of the current block: the character index,
        // the line, and the 1-based column of its first `#`.
        let mut start: (i64, i64, i64) = (0, 0, 0);
        let mut start_mark = self.mark;
        let mut scan = scan_mark.chars as i64;

        let newlines = self.newlines as i64;
        let mut foot_line = -1;
        if scan_mark.mark.line > 0 {
            foot_line = self.mark.line as i64 - newlines + 1;
            if newlines == 0 && self.mark.column > 1 {
                foot_line += 1;
            }
        }

        let mut peek = 0;
        let mut at = BytePeek::default();
        while peek < COMMENT_PEEK_LIMIT {
            self.cache(peek + 1)?;
            let here = self.chars as i64 + peek as i64;
            column += 1;
            let byte = at.byte(&self.buffer);
            if byte == b' ' || byte == b'\t' {
                peek += 1;
                at.advance(&self.buffer);
                continue;
            }
            let close_flow = self.flow_level > 0 && (byte == b']' || byte == b'}');
            let line_break = at.is_line_break(&self.buffer);
            if close_flow || line_break || byte == 0 {
                if (close_flow || !recent_empty)
                    && (close_flow
                        || first_empty
                            && (start.1 == foot_line && token_kind != TailKind::Value
                                || start.2 - 1 < next_indent))
                    && text
                {
                    self.comment_blocks.push(CommentBlock {
                        scan,
                        start: start.0,
                        end: here,
                        column: start.2,
                        mark: start_mark,
                    });
                    scan = here;
                    text = false;
                }
                if !line_break {
                    break;
                }
                first_empty = false;
                recent_empty = true;
                column = 0;
                line += 1;
                peek += 1;
                at.advance(&self.buffer);
                continue;
            }
            if text && column - 1 < next_indent && column != start.2 {
                self.comment_blocks.push(CommentBlock {
                    scan,
                    start: start.0,
                    end: here,
                    column: start.2,
                    mark: start_mark,
                });
                scan = here;
                text = false;
            }
            if byte != b'#' {
                break;
            }
            let new_block = !text;
            if new_block {
                start = (here, line, column);
            }
            text = true;
            recent_empty = false;

            let hash = self.skip_to_comment_end(self.chars + peek as u64)?;
            if new_block {
                start_mark = Mark {
                    index: hash.index + 1,
                    line: u64::try_from(start.1).unwrap_or(hash.line),
                    column: u64::try_from(start.2).unwrap_or(hash.column),
                };
            }

            // go-yaml resets `peek` to 0 here and its loop then adds 1, so it
            // skips the line break after the comment without counting it.
            peek = 1;
            at = BytePeek { index: 1, byte: 0 };
            column = 0;
            line = self.mark.line as i64;
            next_indent = indent(self);
        }
        if text {
            let here = self.chars as i64 + peek as i64;
            self.comment_blocks.push(CommentBlock {
                scan,
                start: start.0,
                end: here - 1,
                column: start.2,
                mark: start_mark,
            });
        }
        Ok(())
    }

    /// Records the comment from `start_mark`, at its `#`, to the current
    /// position, the end of its line. A workflint addition: libyaml skips
    /// comments without recording them.
    fn record_comment(&mut self, start_mark: Mark) -> Result<()> {
        let kind = self.comment_kind(start_mark)?;
        self.comments.push_back(Comment {
            kind,
            start_mark,
            end_mark: self.mark,
        });
        Ok(())
    }

    /// Skips the comment that starts at the current `#` and records it, a
    /// workflint addition. go-yaml drops the comment after a directive, and
    /// workflint records it too.
    ///
    /// The comment ends before the line break or at the end of the input.
    fn scan_comment(&mut self) -> Result<()> {
        let start_mark = self.mark;
        while !IS_BREAKZ!(self.buffer) {
            self.skip_char();
            self.cache(1)?;
        }
        self.record_comment(start_mark)
    }

    /// Finds whether code precedes the comment at `start_mark` on its line.
    /// Spaces, tabs, and a byte order mark are not code.
    ///
    /// This reads the source because the scanner's state cannot tell: a plain
    /// scalar consumes the line breaks and indentation after it, so the
    /// scanner can reach a `#` at the start of a line right after a token.
    fn comment_kind(&self, start_mark: Mark) -> Result<CommentKind> {
        let before = usize::try_from(start_mark.index)
            .ok()
            .and_then(|index| self.source.get(..index))
            .ok_or_else(|| Error::internal(ErrorKind::Scanner, start_mark))?;
        let code_precedes = before
            .chars()
            .rev()
            .take_while(|&ch| !is_break(ch))
            .any(|ch| !matches!(ch, ' ' | '\t' | '\u{feff}'));
        Ok(if code_precedes {
            CommentKind::Trailing
        } else {
            CommentKind::OwnLine
        })
    }

    fn scan_directive(&mut self) -> Result<Token> {
        let end_mark: Mark;
        let mut major: i32 = 0;
        let mut minor: i32 = 0;
        let start_mark: Mark = self.mark;
        self.skip_char();
        let name = self.scan_directive_name(start_mark)?;
        let token = if name == "YAML" {
            self.scan_version_directive_value(start_mark, &mut major, &mut minor)?;

            end_mark = self.mark;
            Token {
                data: TokenData::VersionDirective { major, minor },
                start_mark,
                end_mark,
            }
        } else if name == "TAG" {
            let (handle, prefix) = self.scan_tag_directive_value(start_mark)?;
            end_mark = self.mark;
            Token {
                data: TokenData::TagDirective { handle, prefix },
                start_mark,
                end_mark,
            }
        } else {
            return self.set_scanner_error(
                "while scanning a directive",
                start_mark,
                "found unknown directive name",
            );
        };
        self.cache(1)?;
        loop {
            if !IS_BLANK!(self.buffer) {
                break;
            }
            self.skip_char();
            self.cache(1)?;
        }

        if CHECK!(self.buffer, '#') {
            self.scan_comment()?;
        }

        if IS_BREAKZ!(self.buffer) {
            if IS_BREAK!(self.buffer) {
                self.cache(2)?;
                self.skip_line_break();
            }
            Ok(token)
        } else {
            self.set_scanner_error(
                "while scanning a directive",
                start_mark,
                "did not find expected comment or line break",
            )
        }
    }

    fn scan_directive_name(&mut self, start_mark: Mark) -> Result<String> {
        let mut string = String::new();
        self.cache(1)?;

        loop {
            if !IS_ALPHA!(self.buffer) {
                break;
            }
            self.read_char(&mut string);
            self.cache(1)?;
        }

        if string.is_empty() {
            self.set_scanner_error(
                "while scanning a directive",
                start_mark,
                "could not find expected directive name",
            )
        } else if !IS_BLANKZ!(self.buffer) {
            self.set_scanner_error(
                "while scanning a directive",
                start_mark,
                "found unexpected non-alphabetical character",
            )
        } else {
            Ok(string)
        }
    }

    fn scan_version_directive_value(
        &mut self,
        start_mark: Mark,
        major: &mut i32,
        minor: &mut i32,
    ) -> Result<()> {
        self.cache(1)?;
        while IS_BLANK!(self.buffer) {
            self.skip_char();
            self.cache(1)?;
        }
        self.scan_version_directive_number(start_mark, major)?;
        if !CHECK!(self.buffer, '.') {
            return self.set_scanner_error(
                "while scanning a %YAML directive",
                start_mark,
                "did not find expected digit or '.' character",
            );
        }
        self.skip_char();
        self.scan_version_directive_number(start_mark, minor)
    }

    fn scan_version_directive_number(&mut self, start_mark: Mark, number: &mut i32) -> Result<()> {
        let mut value: i32 = 0;
        let mut length = 0;
        self.cache(1)?;
        while IS_DIGIT!(self.buffer) {
            length += 1;
            if length > MAX_NUMBER_LENGTH {
                return self.set_scanner_error(
                    "while scanning a %YAML directive",
                    start_mark,
                    "found extremely long version number",
                );
            }
            value = (value * 10) + AS_DIGIT!(self.buffer) as i32;
            self.skip_char();
            self.cache(1)?;
        }
        if length == 0 {
            return self.set_scanner_error(
                "while scanning a %YAML directive",
                start_mark,
                "did not find expected version number",
            );
        }
        *number = value;
        Ok(())
    }

    // Returns (handle, prefix)
    fn scan_tag_directive_value(&mut self, start_mark: Mark) -> Result<(String, String)> {
        self.cache(1)?;

        loop {
            if IS_BLANK!(self.buffer) {
                self.skip_char();
                self.cache(1)?;
            } else {
                let handle_value = self.scan_tag_handle(true, start_mark)?;

                self.cache(1)?;

                if !IS_BLANK!(self.buffer) {
                    return self.set_scanner_error(
                        "while scanning a %TAG directive",
                        start_mark,
                        "did not find expected whitespace",
                    );
                }

                while IS_BLANK!(self.buffer) {
                    self.skip_char();
                    self.cache(1)?;
                }

                let prefix_value = self.scan_tag_uri(true, true, None, start_mark)?;
                self.cache(1)?;

                if !IS_BLANKZ!(self.buffer) {
                    return self.set_scanner_error(
                        "while scanning a %TAG directive",
                        start_mark,
                        "did not find expected whitespace or line break",
                    );
                }
                return Ok((handle_value, prefix_value));
            }
        }
    }

    /// Whether the current character may be part of an anchor or alias
    /// name, as go-yaml's `isAnchorChar`: a printable ASCII character that is
    /// not a blank, a flow indicator, or `:`. go-yaml's `isBOM` looks at the
    /// start of its buffer, so no character qualifies while the buffer
    /// starts with a byte order mark.
    fn is_anchor_char(&self) -> bool {
        self.buffer.front().is_some_and(|&ch| {
            ch.is_ascii_graphic() && !matches!(ch, '[' | ']' | '{' | '}' | ',' | ':')
        }) && !self.buffer_starts_with_bom
    }

    fn scan_anchor(&mut self, scan_alias_instead_of_anchor: bool) -> Result<Token> {
        let mut string = String::new();
        let start_mark: Mark = self.mark;
        self.skip_char();
        self.cache(1)?;

        while self.is_anchor_char() {
            self.read_char(&mut string);
            self.cache(1)?;
        }
        let end_mark: Mark = self.mark;
        if string.is_empty()
            || !(IS_BLANKZ!(self.buffer)
                || CHECK!(self.buffer, '?')
                || CHECK!(self.buffer, ':')
                || CHECK!(self.buffer, ',')
                || CHECK!(self.buffer, ']')
                || CHECK!(self.buffer, '}')
                || CHECK!(self.buffer, '%')
                || CHECK!(self.buffer, '@')
                || CHECK!(self.buffer, '`'))
        {
            self.set_scanner_error(
                if scan_alias_instead_of_anchor {
                    "while scanning an alias"
                } else {
                    "while scanning an anchor"
                },
                start_mark,
                "did not find expected alphabetic or numeric character",
            )
        } else {
            Ok(Token {
                data: if scan_alias_instead_of_anchor {
                    TokenData::Alias { value: string }
                } else {
                    TokenData::Anchor { value: string }
                },
                start_mark,
                end_mark,
            })
        }
    }

    fn scan_tag(&mut self) -> Result<Token> {
        let mut handle;
        let mut suffix;

        let start_mark: Mark = self.mark;

        self.cache(2)?;

        if CHECK_AT!(self.buffer, '<', 1) {
            handle = String::new();
            self.skip_char();
            self.skip_char();
            suffix = self.scan_tag_uri(true, false, None, start_mark)?;

            if !CHECK!(self.buffer, '>') {
                return self.set_scanner_error(
                    "while scanning a tag",
                    start_mark,
                    "did not find the expected '>'",
                );
            }
            self.skip_char();
        } else {
            handle = self.scan_tag_handle(false, start_mark)?;
            if handle.starts_with('!') && handle.len() > 1 && handle.ends_with('!') {
                suffix = self.scan_tag_uri(false, false, None, start_mark)?;
            } else {
                suffix = self.scan_tag_uri(false, false, Some(&handle), start_mark)?;
                handle = String::from("!");
                if suffix.is_empty() {
                    core::mem::swap(&mut handle, &mut suffix);
                }
            }
        }

        self.cache(1)?;
        // In a flow collection, a ',' may follow the tag, as in libyaml.
        // Upstream panicked here.
        if !IS_BLANKZ!(self.buffer) && (self.flow_level == 0 || !CHECK!(self.buffer, ',')) {
            return self.set_scanner_error(
                "while scanning a tag",
                start_mark,
                "did not find expected whitespace or line break",
            );
        }

        let end_mark: Mark = self.mark;
        Ok(Token {
            data: TokenData::Tag { handle, suffix },
            start_mark,
            end_mark,
        })
    }

    fn scan_tag_handle(&mut self, directive: bool, start_mark: Mark) -> Result<String> {
        let mut string = String::new();
        self.cache(1)?;

        if !CHECK!(self.buffer, '!') {
            return self.set_scanner_error(
                tag_context(directive),
                start_mark,
                "did not find expected '!'",
            );
        }

        self.read_char(&mut string);
        self.cache(1)?;
        loop {
            if !IS_ALPHA!(self.buffer) {
                break;
            }
            self.read_char(&mut string);
            self.cache(1)?;
        }
        if CHECK!(self.buffer, '!') {
            self.read_char(&mut string);
        } else if directive && string != "!" {
            return self.set_scanner_error(
                tag_context(directive),
                start_mark,
                "did not find expected '!'",
            );
        }
        Ok(string)
    }

    fn scan_tag_uri(
        &mut self,
        uri_char: bool,
        directive: bool,
        head: Option<&str>,
        start_mark: Mark,
    ) -> Result<String> {
        let head = head.unwrap_or("");
        let mut length = head.len();
        let mut string = String::new();

        if length > 1 {
            string = String::from(&head[1..]);
        }
        self.cache(1)?;

        while IS_ALPHA!(self.buffer)
            || CHECK!(self.buffer, ';')
            || CHECK!(self.buffer, '/')
            || CHECK!(self.buffer, '?')
            || CHECK!(self.buffer, ':')
            || CHECK!(self.buffer, '@')
            || CHECK!(self.buffer, '&')
            || CHECK!(self.buffer, '=')
            || CHECK!(self.buffer, '+')
            || CHECK!(self.buffer, '$')
            || CHECK!(self.buffer, '.')
            || CHECK!(self.buffer, '%')
            || CHECK!(self.buffer, '!')
            || CHECK!(self.buffer, '~')
            || CHECK!(self.buffer, '*')
            || CHECK!(self.buffer, '\'')
            || CHECK!(self.buffer, '(')
            || CHECK!(self.buffer, ')')
            // go-yaml takes `,`, `[`, and `]` in every tag. In a flow
            // collection, a tag ends before them, as in libyaml
            // (docs/differences.md#tag-before-flow-comma).
            || (uri_char || self.flow_level == 0)
                && (CHECK!(self.buffer, ',')
                    || CHECK!(self.buffer, '[')
                    || CHECK!(self.buffer, ']'))
        {
            if CHECK!(self.buffer, '%') {
                self.scan_uri_escapes(directive, start_mark, &mut string)?;
            } else {
                self.read_char(&mut string);
            }
            length += 1;
            self.cache(1)?;
        }
        if length == 0 {
            self.set_scanner_error(
                tag_context(directive),
                start_mark,
                "did not find expected tag URI",
            )
        } else {
            Ok(string)
        }
    }

    fn scan_uri_escapes(
        &mut self,
        directive: bool,
        start_mark: Mark,
        string: &mut String,
    ) -> Result<()> {
        let mut width: i32 = 0;
        let mut octets = Vec::with_capacity(4);
        loop {
            self.cache(3)?;
            if !(CHECK!(self.buffer, '%')
                && IS_HEX_AT!(self.buffer, 1)
                && IS_HEX_AT!(self.buffer, 2))
            {
                return self.set_scanner_error(
                    tag_context(directive),
                    start_mark,
                    "did not find URI escaped octet",
                );
            }
            let octet = ((AS_HEX_AT!(self.buffer, 1) << 4) + AS_HEX_AT!(self.buffer, 2)) as u8;
            if width == 0 {
                width = if octet & 0x80 == 0 {
                    1
                } else if octet & 0xE0 == 0xC0 {
                    2
                } else if octet & 0xF0 == 0xE0 {
                    3
                } else if octet & 0xF8 == 0xF0 {
                    4
                } else {
                    0
                };
                if width == 0 {
                    return self.set_scanner_error(
                        tag_context(directive),
                        start_mark,
                        "found an incorrect leading UTF-8 octet",
                    );
                }
            } else if octet & 0xC0 != 0x80 {
                return self.set_scanner_error(
                    tag_context(directive),
                    start_mark,
                    "found an incorrect trailing UTF-8 octet",
                );
            }
            octets.push(octet);
            self.skip_char();
            self.skip_char();
            self.skip_char();
            width -= 1;
            if width == 0 {
                break;
            }
        }
        // The octets are one UTF-8 character. go-yaml checks only the lead
        // and continuation bits, so an overlong or surrogate sequence passes
        // there and becomes U+FFFD here.
        string.push_str(&String::from_utf8_lossy(&octets));
        Ok(())
    }

    fn scan_block_scalar(&mut self, literal: bool) -> Result<Token> {
        let mut end_mark: Mark;
        let mut string = String::new();
        let Spare {
            mut leading_break,
            mut trailing_breaks,
            whitespaces,
        } = std::mem::take(&mut self.spare);
        let mut chomping: i8 = 0;
        let mut increment: i64 = 0;
        let mut indent: i64 = 0;
        let mut leading_blank = false;
        let mut trailing_blank;
        let start_mark: Mark = self.mark;
        self.skip_char();
        self.cache(1)?;

        if CHECK!(self.buffer, '+') || CHECK!(self.buffer, '-') {
            chomping = if CHECK!(self.buffer, '+') { 1 } else { -1 };
            self.skip_char();
            self.cache(1)?;
            if IS_DIGIT!(self.buffer) {
                if CHECK!(self.buffer, '0') {
                    return self.set_scanner_error(
                        "while scanning a block scalar",
                        start_mark,
                        "found an indentation indicator equal to 0",
                    );
                }
                increment = i64::from(AS_DIGIT!(self.buffer));
                self.skip_char();
            }
        } else if IS_DIGIT!(self.buffer) {
            if CHECK!(self.buffer, '0') {
                return self.set_scanner_error(
                    "while scanning a block scalar",
                    start_mark,
                    "found an indentation indicator equal to 0",
                );
            }
            increment = i64::from(AS_DIGIT!(self.buffer));
            self.skip_char();
            self.cache(1)?;
            if CHECK!(self.buffer, '+') || CHECK!(self.buffer, '-') {
                chomping = if CHECK!(self.buffer, '+') { 1 } else { -1 };
                self.skip_char();
            }
        }

        self.cache(1)?;
        loop {
            if !IS_BLANK!(self.buffer) {
                break;
            }
            self.skip_char();
            self.cache(1)?;
        }

        if CHECK!(self.buffer, '#') {
            self.scan_line_comment()?;
            while !IS_BREAKZ!(self.buffer) {
                self.skip_char();
                self.cache(1)?;
            }
        }

        if !IS_BREAKZ!(self.buffer) {
            return self.set_scanner_error(
                "while scanning a block scalar",
                start_mark,
                "did not find expected comment or line break",
            );
        }

        if IS_BREAK!(self.buffer) {
            self.cache(2)?;
            self.skip_line_break();
        }

        end_mark = self.mark;
        if increment != 0 {
            indent = if self.indent >= 0 {
                self.indent + increment
            } else {
                increment
            };
        }
        self.scan_block_scalar_breaks(
            &mut indent,
            &mut trailing_breaks,
            start_mark,
            &mut end_mark,
        )?;

        self.cache(1)?;

        loop {
            if self.mark.column as i64 != indent || IS_Z!(self.buffer) {
                break;
            }
            trailing_blank = IS_BLANK!(self.buffer);
            if !literal && leading_break.starts_with('\n') && !leading_blank && !trailing_blank {
                if trailing_breaks.is_empty() {
                    string.push(' ');
                }
                leading_break.clear();
            } else {
                string.push_str(&leading_break);
                leading_break.clear();
            }
            string.push_str(&trailing_breaks);
            trailing_breaks.clear();
            leading_blank = IS_BLANK!(self.buffer);
            let line_start = self.mark.index;
            while !IS_BREAKZ!(self.buffer) {
                self.skip_char();
                self.cache(1)?;
            }
            self.push_skipped(&mut string, line_start);
            self.cache(2)?;
            self.read_line_break(&mut leading_break);
            self.scan_block_scalar_breaks(
                &mut indent,
                &mut trailing_breaks,
                start_mark,
                &mut end_mark,
            )?;
        }

        if chomping != -1 {
            string.push_str(&leading_break);
        }

        if chomping == 1 {
            string.push_str(&trailing_breaks);
        }
        self.spare = Spare::cleared(leading_break, trailing_breaks, whitespaces);

        Ok(Token {
            data: TokenData::Scalar {
                value: string,
                style: if literal {
                    ScalarStyle::Literal
                } else {
                    ScalarStyle::Folded
                },
            },
            start_mark,
            end_mark,
        })
    }

    fn scan_block_scalar_breaks(
        &mut self,
        indent: &mut i64,
        breaks: &mut String,
        start_mark: Mark,
        end_mark: &mut Mark,
    ) -> Result<()> {
        let mut max_indent: i64 = 0;
        *end_mark = self.mark;
        loop {
            self.cache(1)?;
            while (*indent == 0 || (self.mark.column as i64) < *indent) && IS_SPACE!(self.buffer) {
                self.skip_char();
                self.cache(1)?;
            }
            if self.mark.column as i64 > max_indent {
                max_indent = self.mark.column as i64;
            }
            if (*indent == 0 || (self.mark.column as i64) < *indent) && IS_TAB!(self.buffer) {
                return self.set_scanner_error(
                    "while scanning a block scalar",
                    start_mark,
                    "found a tab character where an indentation space is expected",
                );
            }
            if !IS_BREAK!(self.buffer) {
                break;
            }
            self.cache(2)?;
            self.read_line_break(breaks);
            *end_mark = self.mark;
        }
        if *indent == 0 {
            *indent = max_indent;
            if *indent < self.indent + 1 {
                *indent = self.indent + 1;
            }
            if *indent < 1 {
                *indent = 1;
            }
        }
        Ok(())
    }

    fn scan_flow_scalar(&mut self, single: bool) -> Result<Token> {
        let mut string = String::new();
        let Spare {
            mut leading_break,
            mut trailing_breaks,
            mut whitespaces,
        } = std::mem::take(&mut self.spare);
        let mut leading_blanks;

        let start_mark: Mark = self.mark;
        self.skip_char();
        loop {
            self.cache(4)?;

            if self.mark.column == 0
                && (CHECK_AT!(self.buffer, '-', 0)
                    && CHECK_AT!(self.buffer, '-', 1)
                    && CHECK_AT!(self.buffer, '-', 2)
                    || CHECK_AT!(self.buffer, '.', 0)
                        && CHECK_AT!(self.buffer, '.', 1)
                        && CHECK_AT!(self.buffer, '.', 2))
                && IS_BLANKZ_AT!(self.buffer, 3)
            {
                return self.set_scanner_error(
                    "while scanning a quoted scalar",
                    start_mark,
                    "found unexpected document indicator",
                );
            } else if IS_Z!(self.buffer) {
                return self.set_scanner_error(
                    "while scanning a quoted scalar",
                    start_mark,
                    "found unexpected end of stream",
                );
            }
            self.cache(2)?;
            leading_blanks = false;
            while !IS_BLANKZ!(self.buffer) {
                if single && CHECK_AT!(self.buffer, '\'', 0) && CHECK_AT!(self.buffer, '\'', 1) {
                    string.push('\'');
                    self.skip_char();
                    self.skip_char();
                } else {
                    if CHECK!(self.buffer, if single { '\'' } else { '"' }) {
                        break;
                    }
                    if !single && CHECK!(self.buffer, '\\') && IS_BREAK_AT!(self.buffer, 1) {
                        self.cache(3)?;
                        self.skip_char();
                        self.skip_line_break();
                        leading_blanks = true;
                        break;
                    } else if !single && CHECK!(self.buffer, '\\') {
                        let mut code_length = 0usize;
                        // At the end of the input there is no escape
                        // character. libyaml reports it as unknown, and
                        // upstream panicked.
                        match self.buffer.get(1).copied().unwrap_or('\0') {
                            '0' => {
                                string.push('\0');
                            }
                            'a' => {
                                string.push('\x07');
                            }
                            'b' => {
                                string.push('\x08');
                            }
                            't' | '\t' => {
                                string.push('\t');
                            }
                            'n' => {
                                string.push('\n');
                            }
                            'v' => {
                                string.push('\x0B');
                            }
                            'f' => {
                                string.push('\x0C');
                            }
                            'r' => {
                                string.push('\r');
                            }
                            'e' => {
                                string.push('\x1B');
                            }
                            ' ' => {
                                string.push(' ');
                            }
                            '"' => {
                                string.push('"');
                            }
                            '\'' => {
                                string.push('\'');
                            }
                            // go-yaml rejects `\/`, which YAML 1.2 allows
                            // (docs/differences.md#yaml-solidus-escape).
                            '/' => {
                                string.push('/');
                            }
                            '\\' => {
                                string.push('\\');
                            }
                            // NEL (#x85)
                            'N' => {
                                string.push('\u{0085}');
                            }
                            // #xA0
                            '_' => {
                                string.push('\u{00a0}');
                            }
                            // LS (#x2028)
                            'L' => {
                                string.push('\u{2028}');
                            }
                            // PS (#x2029)
                            'P' => {
                                string.push('\u{2029}');
                            }
                            'x' => {
                                code_length = 2;
                            }
                            'u' => {
                                code_length = 4;
                            }
                            'U' => {
                                code_length = 8;
                            }
                            _ => {
                                return self.set_scanner_error(
                                    "while parsing a quoted scalar",
                                    start_mark,
                                    "found unknown escape character",
                                );
                            }
                        }
                        self.skip_char();
                        self.skip_char();
                        if code_length != 0 {
                            let mut value: u32 = 0;
                            let mut k = 0;
                            self.cache(code_length)?;
                            while k < code_length {
                                if !IS_HEX_AT!(self.buffer, k) {
                                    return self.set_scanner_error(
                                        "while parsing a quoted scalar",
                                        start_mark,
                                        "did not find expected hexadecimal number",
                                    );
                                }
                                value = (value << 4) + AS_HEX_AT!(self.buffer, k);
                                k += 1;
                            }
                            if let Some(ch) = char::from_u32(value) {
                                string.push(ch);
                            } else {
                                return self.set_scanner_error(
                                    "while parsing a quoted scalar",
                                    start_mark,
                                    "found invalid Unicode character escape code",
                                );
                            }

                            k = 0;
                            while k < code_length {
                                self.skip_char();
                                k += 1;
                            }
                        }
                    } else {
                        self.read_char(&mut string);
                    }
                }
                self.cache(2)?;
            }
            self.cache(1)?;
            if CHECK!(self.buffer, if single { '\'' } else { '"' }) {
                break;
            }
            self.cache(1)?;
            while IS_BLANK!(self.buffer) || IS_BREAK!(self.buffer) {
                if IS_BLANK!(self.buffer) {
                    if leading_blanks {
                        self.skip_char();
                    } else {
                        self.read_char(&mut whitespaces);
                    }
                } else {
                    self.cache(2)?;
                    if leading_blanks {
                        self.read_line_break(&mut trailing_breaks);
                    } else {
                        whitespaces.clear();
                        self.read_line_break(&mut leading_break);
                        leading_blanks = true;
                    }
                }
                self.cache(1)?;
            }
            if leading_blanks {
                if leading_break.starts_with('\n') {
                    if trailing_breaks.is_empty() {
                        string.push(' ');
                    } else {
                        string.push_str(&trailing_breaks);
                        trailing_breaks.clear();
                    }
                    leading_break.clear();
                } else {
                    string.push_str(&leading_break);
                    string.push_str(&trailing_breaks);
                    leading_break.clear();
                    trailing_breaks.clear();
                }
            } else {
                string.push_str(&whitespaces);
                whitespaces.clear();
            }
        }

        self.skip_char();
        let end_mark: Mark = self.mark;
        self.spare = Spare::cleared(leading_break, trailing_breaks, whitespaces);
        Ok(Token {
            data: TokenData::Scalar {
                value: string,
                style: if single {
                    ScalarStyle::SingleQuoted
                } else {
                    ScalarStyle::DoubleQuoted
                },
            },
            start_mark,
            end_mark,
        })
    }

    fn scan_plain_scalar(&mut self) -> Result<Token> {
        let mut end_mark: Mark;
        let mut string = String::new();
        let Spare {
            mut leading_break,
            mut trailing_breaks,
            mut whitespaces,
        } = std::mem::take(&mut self.spare);
        let mut leading_blanks = false;
        let indent: i64 = self.indent + 1;
        end_mark = self.mark;
        let start_mark: Mark = end_mark;
        loop {
            self.cache(4)?;
            if self.mark.column == 0
                && (CHECK_AT!(self.buffer, '-', 0)
                    && CHECK_AT!(self.buffer, '-', 1)
                    && CHECK_AT!(self.buffer, '-', 2)
                    || CHECK_AT!(self.buffer, '.', 0)
                        && CHECK_AT!(self.buffer, '.', 1)
                        && CHECK_AT!(self.buffer, '.', 2))
                && IS_BLANKZ_AT!(self.buffer, 3)
            {
                break;
            }
            if CHECK!(self.buffer, '#') {
                break;
            }
            // The characters of this run are copied in one piece after the
            // loop. The blanks before them are added on the first one.
            let mut run_start = None;
            while !IS_BLANKZ!(self.buffer) {
                // A `:` that no blank follows is part of the scalar, also in
                // a flow collection, as in go-yaml. libyaml rejects it before
                // a flow indicator.
                if CHECK!(self.buffer, ':') && IS_BLANKZ_AT!(self.buffer, 1)
                    || self.flow_level != 0
                        && (CHECK!(self.buffer, ',')
                            || CHECK!(self.buffer, '?') && IS_BLANKZ_AT!(self.buffer, 1)
                            || CHECK!(self.buffer, '[')
                            || CHECK!(self.buffer, ']')
                            || CHECK!(self.buffer, '{')
                            || CHECK!(self.buffer, '}'))
                {
                    break;
                }
                if leading_blanks || !whitespaces.is_empty() {
                    if leading_blanks {
                        if leading_break.starts_with('\n') {
                            if trailing_breaks.is_empty() {
                                string.push(' ');
                            } else {
                                string.push_str(&trailing_breaks);
                                trailing_breaks.clear();
                            }
                            leading_break.clear();
                        } else {
                            string.push_str(&leading_break);
                            string.push_str(&trailing_breaks);
                            leading_break.clear();
                            trailing_breaks.clear();
                        }
                        leading_blanks = false;
                    } else {
                        string.push_str(&whitespaces);
                        whitespaces.clear();
                    }
                }
                run_start.get_or_insert(self.mark.index);
                self.skip_char();
                end_mark = self.mark;
                self.cache(2)?;
            }
            if let Some(start) = run_start {
                self.push_skipped(&mut string, start);
            }
            if !(IS_BLANK!(self.buffer) || IS_BREAK!(self.buffer)) {
                break;
            }
            self.cache(1)?;

            while IS_BLANK!(self.buffer) || IS_BREAK!(self.buffer) {
                if IS_BLANK!(self.buffer) {
                    if leading_blanks && (self.mark.column as i64) < indent && IS_TAB!(self.buffer)
                    {
                        return self.set_scanner_error(
                            "while scanning a plain scalar",
                            start_mark,
                            "found a tab character that violates indentation",
                        );
                    } else if !leading_blanks {
                        self.read_char(&mut whitespaces);
                    } else {
                        self.skip_char();
                    }
                } else {
                    self.cache(2)?;

                    if leading_blanks {
                        self.read_line_break(&mut trailing_breaks);
                    } else {
                        whitespaces.clear();
                        self.read_line_break(&mut leading_break);
                        leading_blanks = true;
                    }
                }
                self.cache(1)?;
            }
            if self.flow_level == 0 && (self.mark.column as i64) < indent {
                break;
            }
        }

        if leading_blanks {
            self.simple_key_allowed = true;
        }
        self.spare = Spare::cleared(leading_break, trailing_breaks, whitespaces);

        Ok(Token {
            data: TokenData::Scalar {
                value: string,
                style: ScalarStyle::Plain,
            },
            start_mark,
            end_mark,
        })
    }
}
