//! Expands a short fuzz input into a deeply nested one, so that the fuzz
//! targets reach the parser's depth limit of 10,000 levels. libFuzzer keeps
//! its inputs to a few kilobytes, and one level takes at least one byte.
//!
//! The `syntax`, `yaml_events`, and `yaml_tree` fuzz targets and
//! `fuzz_regressions.rs` include this file with `#[path]`.

use std::borrow::Cow;

/// The first byte of an input to expand. It never starts a UTF-8 file, so
/// the seed workflows reach the checks unchanged.
const MARKER: u8 = 0xC0;

/// Separates the parts of an input to expand. It never occurs in UTF-8.
const SEPARATOR: u8 = 0xC1;

/// The most repetitions, one more than the parser's depth limit.
const MAX_DEPTH: usize = 10_001;

/// The most bytes an input expands to, so that each run stays fast.
const MAX_LEN: usize = 1 << 20;

/// Expands `data` if it starts with `MARKER`, and returns it unchanged
/// otherwise.
///
/// After the marker come two bytes with the depth, big-endian, and then the
/// opening text, the middle, and the closing text, separated by
/// `SEPARATOR`. The result is the opening text repeated depth times, the
/// middle, and the closing text repeated depth times. `C0 27 11 '[' C1 C1
/// ']'` gives 10,001 `[` and 10,001 `]`. The depth is at most `MAX_DEPTH`,
/// and less if the result would be longer than `MAX_LEN`.
pub(crate) fn expand(data: &[u8]) -> Cow<'_, [u8]> {
    let Some((&MARKER, rest)) = data.split_first() else {
        return Cow::Borrowed(data);
    };
    let (depth, parts) = match rest {
        [high, low, parts @ ..] => (usize::from(u16::from_be_bytes([*high, *low])), parts),
        _ => (0, &[][..]),
    };
    let mut parts = parts.splitn(3, |&byte| byte == SEPARATOR);
    let open = parts.next().unwrap_or_default();
    let middle = parts.next().unwrap_or_default();
    let close = parts.next().unwrap_or_default();
    let per_level = (open.len() + close.len()).max(1);
    let depth = depth
        .min(MAX_DEPTH)
        .min(MAX_LEN.saturating_sub(middle.len()) / per_level);
    let mut out = Vec::with_capacity((open.len() + close.len()) * depth + middle.len());
    for _ in 0..depth {
        out.extend_from_slice(open);
    }
    out.extend_from_slice(middle);
    for _ in 0..depth {
        out.extend_from_slice(close);
    }
    Cow::Owned(out)
}
