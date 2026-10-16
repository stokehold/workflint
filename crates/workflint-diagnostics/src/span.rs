use crate::FileId;

/// A byte range in one source file.
///
/// `start` is at most `end`, and both are byte offsets into [`SourceFile::text`](crate::SourceFile::text).
/// [`SourceFile::line_col`](crate::SourceFile::line_col) turns them into lines and columns.
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
