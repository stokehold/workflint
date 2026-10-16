//! Collections keyed by name, in source order.

use std::borrow::Cow;
use std::sync::Arc;

use workflint_diagnostics::Span;
use workflint_yaml::simple_lowercase;

use crate::Str;

/// A collection keyed by case-insensitive names, such as jobs, `env`,
/// `with`, `outputs`, `secrets`, `services`, matrix rows, permission
/// scopes, and inputs.
///
/// Entries stay in source order. Each one keeps its key as written and the
/// folded key, the key lowercased as Go's `strings.ToLower` does
/// ([`simple_lowercase`]), which lookups compare. [`get`](Self::get) returns
/// the first entry with a name, so a duplicate the parser has reported
/// never shadows the entry before it.
///
/// Cloning is cheap: clones share their entries. The parser can reuse the
/// collection it built for an anchored mapping at every alias of it, which
/// keeps the AST linear in the size of the input.
#[derive(Debug)]
pub struct Keyed<'a, T> {
    inner: Arc<Inner<'a, T>>,
}

#[derive(Debug)]
struct Inner<'a, T> {
    entries: Vec<KeyedEntry<'a, T>>,
    /// Indexes into `entries`, sorted by folded key and then by index.
    sorted: Vec<u32>,
}

impl<'a, T> Keyed<'a, T> {
    /// Collects `entries`, keeping their order.
    #[must_use]
    pub fn new(entries: Vec<KeyedEntry<'a, T>>) -> Self {
        let mut sorted: Vec<u32> = (0..entries.len())
            .map_while(|index| u32::try_from(index).ok())
            .collect();
        // Stable, so equal folded keys stay in source order.
        sorted.sort_by(|&a, &b| folded_at(&entries, a).cmp(folded_at(&entries, b)));
        Self {
            inner: Arc::new(Inner { entries, sorted }),
        }
    }

    /// Returns the first entry whose folded key equals `name` folded.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&KeyedEntry<'a, T>> {
        let name = simple_lowercase(name);
        let entries = &self.inner.entries;
        let sorted = &self.inner.sorted;
        let first = sorted.partition_point(|&index| folded_at(entries, index) < &*name);
        let &index = sorted.get(first)?;
        let entry = entries.get(usize::try_from(index).ok()?)?;
        (entry.folded == name).then_some(entry)
    }

    /// Returns whether an entry has the folded key of `name`.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Returns the entries in source order.
    #[must_use]
    pub fn entries(&self) -> &[KeyedEntry<'a, T>] {
        &self.inner.entries
    }

    /// Iterates over the entries in source order.
    pub fn iter(&self) -> std::slice::Iter<'_, KeyedEntry<'a, T>> {
        self.inner.entries.iter()
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }

    /// Returns whether the collection has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }
}

fn folded_at<'e, T>(entries: &'e [KeyedEntry<'_, T>], index: u32) -> &'e str {
    usize::try_from(index)
        .ok()
        .and_then(|index| entries.get(index))
        .map_or("", |entry| &entry.folded)
}

impl<T> Clone for Keyed<'_, T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Default for Keyed<'_, T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<T: PartialEq> PartialEq for Keyed<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.entries() == other.entries()
    }
}

impl<'k, 'a, T> IntoIterator for &'k Keyed<'a, T> {
    type Item = &'k KeyedEntry<'a, T>;
    type IntoIter = std::slice::Iter<'k, KeyedEntry<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// One entry of a [`Keyed`] collection.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedEntry<'a, T> {
    /// The key as written. Its span is the key position the reference
    /// reports for the entry, such as the job ID of a job.
    pub key: Str<'a>,
    /// The span of the value node.
    pub span: Span,
    /// The value.
    pub value: T,
    folded: Cow<'a, str>,
}

impl<'a, T> KeyedEntry<'a, T> {
    /// Creates the entry of `value` under `key`, whose value node spans
    /// `span`.
    #[must_use]
    pub fn new(key: Str<'a>, span: Span, value: T) -> Self {
        Self {
            key,
            span,
            value,
            folded: simple_lowercase(key.value()),
        }
    }

    /// Returns the folded key, which lookups compare. The reference uses it
    /// as the key of its Go map, and for `workflow_call` inputs as the
    /// input's `ID`.
    #[must_use]
    pub fn folded(&self) -> &str {
        &self.folded
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::testing::{document, root_entries};

    fn keyed<'a>(entries: &[(Str<'a>, Str<'a>)]) -> Keyed<'a, Str<'a>> {
        Keyed::new(
            entries
                .iter()
                .map(|&(key, value)| KeyedEntry::new(key, value.span(), value))
                .collect(),
        )
    }

    #[test]
    fn lookup_by_name_ignores_case_and_returns_the_first_entry() {
        let document = document("Foo: first\nFOO: second\nbar: third\n");
        let env = keyed(&root_entries(&document));

        for name in ["foo", "FOO", "Foo", "fOO"] {
            let entry = env.get(name).unwrap();
            assert_eq!(entry.key.value(), "Foo");
            assert_eq!(entry.value.value(), "first");
        }
        assert_eq!(env.get("bar").unwrap().value.value(), "third");
        assert!(env.get("baz").is_none());
        assert!(env.contains_key("BAR"));
    }

    #[test]
    fn entries_keep_the_original_spelling_and_the_folded_key() {
        let document = document("GITHUB_TOKEN: x\nlower: y\n");
        let env = keyed(&root_entries(&document));

        let keys: Vec<_> = env.iter().map(|e| (e.key.value(), e.folded())).collect();
        assert_eq!(keys, [("GITHUB_TOKEN", "github_token"), ("lower", "lower")]);
    }

    #[test]
    fn iteration_follows_source_order() {
        let document = document("zeta: 1\nAlpha: 2\nmid: 3\nbeta: 4\n");
        let rows = keyed(&root_entries(&document));

        let keys: Vec<_> = rows.iter().map(|e| e.key.value()).collect();
        assert_eq!(keys, ["zeta", "Alpha", "mid", "beta"]);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows.entries()[2].key.value(), "mid");
    }

    #[test]
    fn duplicates_are_kept_in_source_order() {
        let document = document("a: 1\nB: 2\nA: 3\nb: 4\n");
        let keyed = keyed(&root_entries(&document));

        let values: Vec<_> = keyed.iter().map(|e| e.value.value()).collect();
        assert_eq!(values, ["1", "2", "3", "4"]);
        assert_eq!(keyed.get("a").unwrap().value.value(), "1");
        assert_eq!(keyed.get("b").unwrap().value.value(), "2");
    }

    #[test]
    fn lookup_folds_with_go_simple_case_mapping() {
        // U+0130 lowercases to a plain `i` in Go's `strings.ToLower`.
        let document = document("\u{130}D: x\n");
        let keyed = keyed(&root_entries(&document));

        assert_eq!(keyed.entries()[0].folded(), "id");
        assert_eq!(keyed.get("id").unwrap().key.value(), "\u{130}D");
    }

    #[test]
    fn an_empty_collection_finds_nothing() {
        let keyed: Keyed<'_, Str<'_>> = Keyed::default();
        assert!(keyed.is_empty());
        assert!(keyed.get("a").is_none());
    }

    #[test]
    fn a_clone_shares_the_entries() {
        let document = document("a: 1\nb: 2\n");
        let keyed = keyed(&root_entries(&document));
        let clone = keyed.clone();

        assert!(std::ptr::eq(keyed.entries(), clone.entries()));
    }
}
