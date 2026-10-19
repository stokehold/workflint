//! What a job's `runs-on` resolves to: a hosted, self-hosted, or unknown runner, and its
//! system when the labels name one. The `shell-name` rule picks the job's platform from it.

use std::borrow::Cow;

use workflint_data::{ResolvedRunner, resolve_runner};
use workflint_syntax::{Runner, Template};

/// Resolves the labels of `runner` with [`resolve_runner`]. The text of a whole
/// `runs-on: ${{ }}` is not known, and each `${{ }}` inside a label becomes a space.
pub(crate) fn resolve(runner: &Runner<'_>) -> ResolvedRunner {
    let Some(labels) = runner.labels.literal() else {
        return resolve_runner([None]);
    };
    let texts: Vec<Option<Cow<'_, str>>> = labels
        .iter()
        .map(|label| label.as_ref().map(label_text))
        .collect();
    resolve_runner(texts.iter().map(Option::as_deref))
}

/// Returns the text of `label` with each `${{ }}` replaced by a space.
fn label_text<'a>(label: &Template<'a>) -> Cow<'a, str> {
    let value = label.value();
    if label.placeholders().is_empty() {
        return Cow::Borrowed(value);
    }
    let mut text = String::with_capacity(value.len());
    let mut at = 0;
    for placeholder in label.placeholders() {
        let range = placeholder.range();
        text.push_str(value.get(at..range.start).unwrap_or_default());
        text.push(' ');
        at = range.end;
    }
    text.push_str(value.get(at..).unwrap_or_default());
    Cow::Owned(text)
}
