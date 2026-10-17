//! The checks that the `syntax` and `metadata` fuzz targets in `fuzz/` run
//! on each input.
//!
//! The fuzz targets include this file with `#[path]`, and
//! `reference_files.rs` and `fuzz_regressions.rs` run it on files, so a
//! crash input found by the fuzzer fails the same check as a test. Each
//! check panics when an invariant does not hold.

#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{FileId, SourceFile, Span, sort_and_dedup};
use workflint_syntax::{
    ActionMetadata, Exec, JobUsesKind, ReusableWorkflowMetadata, StepUsesKind, Str, UsesPart,
    Workflow,
};
use workflint_yaml::simple_lowercase;

/// Parses `text` with the YAML layer and, if it gives a document, with the
/// workflow parser. Runs [`check_metadata`] first.
///
/// The parser does not panic, and every diagnostic belongs to the file, has
/// a span inside `text` on character boundaries, and converts to a line and
/// column. Sorting and deduplicating the diagnostics does not panic. Every
/// part of a classified `uses` value has a span inside the value's span.
pub(crate) fn check_parse(text: &str) {
    check_metadata(text);
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz.yml", text) else {
        return;
    };
    let Some(document) = workflint_yaml::parse(&file).document else {
        return;
    };
    let parsed = workflint_syntax::parse(&file, &document);
    let mut diagnostics = parsed.diagnostics;
    for diagnostic in &diagnostics {
        let what = format!("{:?} of {:?}", diagnostic.span, diagnostic.message);
        check_span(&file, diagnostic.span, &what);
    }
    sort_and_dedup(&file, &mut diagnostics);
    for (source, parts) in uses_parts(&parsed.workflow) {
        for part in parts {
            let what = format!("{part:?} of `uses: {}`", source.value());
            check_span(&file, part.span, &what);
            let outer = source.span();
            assert!(
                outer.start <= part.span.start && part.span.end <= outer.end,
                "{what}"
            );
        }
    }
}

/// Decodes `text` as the `action.yml` of a local action and as a local
/// reusable workflow.
///
/// Neither decoder panics, and a second decode gives the same result. An
/// error has a message. Each input, output, and secret is keyed by its
/// name in lowercase.
pub(crate) fn check_metadata(text: &str) {
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz.yml", text) else {
        return;
    };
    let action = ActionMetadata::decode(&file);
    assert_eq!(action, ActionMetadata::decode(&file));
    match &action {
        Ok(metadata) => {
            for (id, input) in &metadata.inputs {
                assert_eq!(*id, simple_lowercase(&input.name));
            }
            for (id, output) in &metadata.outputs {
                assert_eq!(*id, simple_lowercase(&output.name));
            }
        }
        Err(err) => assert!(!err.message().is_empty()),
    }
    let workflow = ReusableWorkflowMetadata::decode(&file);
    assert_eq!(workflow, ReusableWorkflowMetadata::decode(&file));
    match &workflow {
        Ok(metadata) => {
            for (id, input) in &metadata.inputs {
                assert_eq!(*id, simple_lowercase(&input.name));
            }
            for (id, output) in &metadata.outputs {
                assert_eq!(*id, simple_lowercase(&output.name));
            }
            for (id, secret) in &metadata.secrets {
                assert_eq!(*id, simple_lowercase(&secret.name));
            }
        }
        Err(err) => assert!(!err.message().is_empty()),
    }
}

/// Asserts that `span` belongs to `file`, lies inside its text on character
/// boundaries, and converts to a line and column.
fn check_span(file: &SourceFile, span: Span, what: &str) {
    let text = file.text();
    assert_eq!(span.file, file.id(), "{what}");
    assert!(span.start <= span.end, "{what}");
    let (start, end) = (
        usize::try_from(span.start).unwrap(),
        usize::try_from(span.end).unwrap(),
    );
    assert!(end <= text.len(), "{what}");
    assert!(text.is_char_boundary(start), "{what}");
    assert!(text.is_char_boundary(end), "{what}");
    let pos = file.line_col(span.start);
    assert!(pos.line >= 1 && pos.column >= 1, "{what}");
}

/// Returns the scalar and the parts of every classified `uses` value of
/// the steps and the reusable workflow calls of `workflow`.
fn uses_parts<'a>(workflow: &Workflow<'a>) -> Vec<(Str<'a>, Vec<UsesPart<'a>>)> {
    let mut out = Vec::new();
    let Some(jobs) = workflow.jobs.value() else {
        return out;
    };
    for job in jobs {
        let job = &job.value;
        for step in job.steps.value().into_iter().flat_map(|steps| steps.iter()) {
            if let Some(Exec::Action(action)) = &step.exec
                && let Some(uses) = action.uses.value()
            {
                let parts = match uses.kind {
                    StepUsesKind::Expression | StepUsesKind::Invalid(_) => vec![],
                    StepUsesKind::Local { path } => vec![path],
                    StepUsesKind::Docker { image, tag } => {
                        [Some(image), tag].into_iter().flatten().collect()
                    }
                    StepUsesKind::Repository {
                        owner,
                        repo,
                        path,
                        git_ref,
                    } => [Some(owner), Some(repo), path, Some(git_ref)]
                        .into_iter()
                        .flatten()
                        .collect(),
                };
                out.push((uses.template.source, parts));
            }
        }
        if let Some(uses) = job
            .workflow_call
            .as_ref()
            .and_then(|call| call.uses.value())
        {
            let parts = match uses.kind {
                JobUsesKind::Expression | JobUsesKind::Invalid(_) => vec![],
                JobUsesKind::Local { path } => vec![path],
                JobUsesKind::Repository {
                    owner,
                    repo,
                    path,
                    git_ref,
                } => vec![owner, repo, path, git_ref],
            };
            out.push((uses.template.source, parts));
        }
    }
    out
}
