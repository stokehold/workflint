use crate::FileId;

/// A byte range in one source file.
///
/// `start` and `end` are byte offsets into [`SourceFile::text`](crate::SourceFile::text), and
/// [`SourceFile::line_col`](crate::SourceFile::line_col) turns them into lines and columns.
/// `start` should be at most `end`. The renderers draw a span whose `start` is after its `end`
/// as an empty span at `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// The file the range is in.
    pub file: FileId,
    /// The offset of the first byte.
    pub start: u32,
    /// The offset one past the last byte.
    pub end: u32,
}

impl Span {
    /// Creates a span from `start` to `end` in `file`.
    #[must_use]
    pub const fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }
}
