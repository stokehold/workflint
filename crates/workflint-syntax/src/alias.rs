//! The aliases through which the parser reaches each string.

use std::collections::HashMap;
use std::sync::Arc;

use workflint_diagnostics::{Label, Span};

/// The chain of aliases through which the parser reached a node, from the
/// outermost to the innermost.
///
/// It indexes the [`AliasUses`] of the workflow. A [`Str`](crate::Str)
/// reached without an alias has no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AliasPath(u32);

/// The alias paths of a workflow: for each one, its innermost alias and the
/// path that reached that alias.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AliasUses {
    uses: Arc<[AliasUse]>,
}

/// One use of an alias, reached through the path `outer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasUse {
    /// The alias as written, such as `*build`.
    pub(crate) alias: Span,
    /// The anchor name.
    pub(crate) name: Box<str>,
    /// The anchored node.
    pub(crate) target: Span,
    /// The path that reached the alias. Its index is lower than this use's.
    pub(crate) outer: Option<AliasPath>,
}

impl AliasUses {
    /// Returns a label at each alias of `path` that leads to `span`,
    /// outermost first.
    ///
    /// The innermost aliases whose anchored nodes do not contain `span` are
    /// left out. So a diagnostic at a key, whose value the parser reached
    /// through an alias, has no label for that alias.
    #[must_use]
    pub fn labels(&self, path: Option<AliasPath>, span: Span) -> Vec<Label> {
        labels(&self.uses, path, span)
    }
}

/// Builds the labels of [`AliasUses::labels`] from `uses`.
pub(crate) fn labels(uses: &[AliasUse], path: Option<AliasPath>, span: Span) -> Vec<Label> {
    // The uses from the innermost to the outermost. Each outer path has a
    // lower index, so the walk ends.
    let mut chain = Vec::new();
    let mut next = path;
    while let Some(AliasPath(index)) = next {
        let Some(alias) = usize::try_from(index).ok().and_then(|i| uses.get(i)) else {
            break;
        };
        chain.push(alias);
        next = alias.outer.filter(|outer| outer.0 < index);
    }
    let contains = |target: Span| {
        target.file == span.file && target.start <= span.start && span.end <= target.end
    };
    let Some(innermost) = chain.iter().position(|alias| contains(alias.target)) else {
        return Vec::new();
    };
    chain[innermost..]
        .iter()
        .rev()
        .map(|alias| Label {
            span: alias.alias,
            note: format!("reached through alias *{}", alias.name),
        })
        .collect()
}

/// Interns the alias paths that the parser builds.
#[derive(Debug, Default)]
pub(crate) struct AliasTable {
    uses: Vec<AliasUse>,
    /// The path of each alias reached through each path.
    paths: HashMap<(Option<AliasPath>, u32), AliasPath>,
}

impl AliasTable {
    /// Returns the path that reaches the alias whose node has the index
    /// `node`, through `outer`. `make` describes the alias the first time.
    /// Past `u32::MAX` paths, it returns `outer`.
    pub(crate) fn path(
        &mut self,
        outer: Option<AliasPath>,
        node: u32,
        make: impl FnOnce() -> (Span, Box<str>, Span),
    ) -> Option<AliasPath> {
        if let Some(&path) = self.paths.get(&(outer, node)) {
            return Some(path);
        }
        let Ok(index) = u32::try_from(self.uses.len()) else {
            return outer;
        };
        let path = AliasPath(index);
        let (alias, name, target) = make();
        self.uses.push(AliasUse {
            alias,
            name,
            target,
            outer,
        });
        self.paths.insert((outer, node), path);
        Some(path)
    }

    /// Returns the labels of [`AliasUses::labels`] for the paths so far.
    pub(crate) fn labels(&self, path: Option<AliasPath>, span: Span) -> Vec<Label> {
        labels(&self.uses, path, span)
    }

    pub(crate) fn finish(self) -> AliasUses {
        AliasUses {
            uses: self.uses.into(),
        }
    }
}
