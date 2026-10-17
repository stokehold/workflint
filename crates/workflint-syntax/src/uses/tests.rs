#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile, Span};

use super::*;
use crate::Str;
use crate::testing::{document, node_at};

/// Classifies a step's `uses` value. Each part's span is its byte range in
/// the value.
fn step(value: &str) -> StepUsesKind<'_> {
    classify_step(value, span)
}

/// Classifies a job's `uses` value. Each part's span is its byte range in
/// the value.
fn job(value: &str) -> JobUsesKind<'_> {
    classify_job(value, span)
}

fn span(start: usize, end: usize) -> Span {
    Span::new(
        FileId::new(0),
        u32::try_from(start).unwrap(),
        u32::try_from(end).unwrap(),
    )
}

/// A part with the text `value` at byte `start` of the `uses` value.
fn part(value: &str, start: usize) -> UsesPart<'_> {
    UsesPart {
        value,
        span: span(start, start + value.len()),
    }
}

fn repository<'a>(
    owner: (&'a str, usize),
    repo: (&'a str, usize),
    path: Option<(&'a str, usize)>,
    git_ref: (&'a str, usize),
) -> StepUsesKind<'a> {
    StepUsesKind::Repository {
        owner: part(owner.0, owner.1),
        repo: part(repo.0, repo.1),
        path: path.map(|(value, start)| part(value, start)),
        git_ref: part(git_ref.0, git_ref.1),
    }
}

fn workflow<'a>(
    owner: (&'a str, usize),
    repo: (&'a str, usize),
    path: (&'a str, usize),
    git_ref: (&'a str, usize),
) -> JobUsesKind<'a> {
    JobUsesKind::Repository {
        owner: part(owner.0, owner.1),
        repo: part(repo.0, repo.1),
        path: part(path.0, path.1),
        git_ref: part(git_ref.0, git_ref.1),
    }
}

#[test]
fn a_step_uses_value_starting_with_dot_slash_is_a_local_action() {
    assert_eq!(
        step("./.github/actions/build"),
        StepUsesKind::Local {
            path: part("./.github/actions/build", 0)
        }
    );
    // The reference does not look for a ref in a local action.
    assert_eq!(
        step("./a@v1"),
        StepUsesKind::Local {
            path: part("./a@v1", 0)
        }
    );
}

#[test]
fn a_docker_action_splits_the_image_and_the_tag_at_the_first_colon() {
    assert_eq!(
        step("docker://alpine:3.20"),
        StepUsesKind::Docker {
            image: part("alpine", 9),
            tag: Some(part("3.20", 16)),
        }
    );
    assert_eq!(
        step("docker://alpine"),
        StepUsesKind::Docker {
            image: part("alpine", 9),
            tag: None,
        }
    );
    assert_eq!(
        step("docker://"),
        StepUsesKind::Docker {
            image: part("", 9),
            tag: None,
        }
    );
}

#[test]
fn a_docker_action_with_a_colon_and_nothing_after_it_has_an_empty_tag() {
    // `examples/invalid_action_format`: "tag of Docker action should not be
    // empty".
    assert_eq!(
        step("docker://image:"),
        StepUsesKind::Docker {
            image: part("image", 9),
            tag: Some(part("", 15)),
        }
    );
}

#[test]
fn a_registry_port_is_read_as_the_start_of_the_docker_tag() {
    // `checkDockerAction` splits at the first colon after `docker://`.
    assert_eq!(
        step("docker://localhost:5000/app:1"),
        StepUsesKind::Docker {
            image: part("localhost", 9),
            tag: Some(part("5000/app:1", 19)),
        }
    );
}

#[test]
fn a_repository_action_has_an_owner_a_repo_and_a_ref() {
    assert_eq!(
        step("actions/checkout@v6"),
        repository(("actions", 0), ("checkout", 8), None, ("v6", 17))
    );
}

#[test]
fn a_repository_action_path_keeps_every_slash_after_the_repo() {
    assert_eq!(
        step("github/codeql-action/upload-sarif/sub@v3"),
        repository(
            ("github", 0),
            ("codeql-action", 7),
            Some(("upload-sarif/sub", 21)),
            ("v3", 38),
        )
    );
    // A slash after the repo gives an empty path.
    assert_eq!(
        step("a/b/@v1"),
        repository(("a", 0), ("b", 2), Some(("", 4)), ("v1", 5))
    );
}

#[test]
fn a_repository_action_ref_starts_after_the_first_at_sign() {
    assert_eq!(
        step("a/b@c@d"),
        repository(("a", 0), ("b", 2), None, ("c@d", 4))
    );
    // The owner is read from the text before the first `@`, so an `@` in
    // the owner hides the owner.
    assert_eq!(
        step("o@x/r/p@v"),
        StepUsesKind::Invalid(InvalidAction::MissingOwner)
    );
}

#[test]
fn a_step_uses_value_without_an_at_sign_is_missing_its_ref() {
    // `examples/invalid_action_format`.
    assert_eq!(
        step("actions/checkout"),
        StepUsesKind::Invalid(InvalidAction::MissingRef)
    );
    // A local action path without `./`.
    assert_eq!(
        step(".github/my-actions/do-something"),
        StepUsesKind::Invalid(InvalidAction::MissingRef)
    );
    assert_eq!(step(""), StepUsesKind::Invalid(InvalidAction::MissingRef));
}

#[test]
fn a_step_uses_value_without_a_slash_before_the_ref_is_missing_its_owner() {
    // `examples/invalid_action_format`.
    assert_eq!(
        step("checkout@v2"),
        StepUsesKind::Invalid(InvalidAction::MissingOwner)
    );
    assert_eq!(
        step("@"),
        StepUsesKind::Invalid(InvalidAction::MissingOwner)
    );
}

#[test]
fn a_repository_action_with_an_empty_owner_repo_or_ref_is_invalid() {
    for value in ["/b@v1", "a/@v1", "a/b@", "/@", "a//c@v1"] {
        assert_eq!(
            step(value),
            StepUsesKind::Invalid(InvalidAction::EmptyPart),
            "{value:?}"
        );
    }
}

#[test]
fn a_step_uses_value_with_an_expression_is_not_classified() {
    assert_eq!(step("${{ matrix.action }}"), StepUsesKind::Expression);
    assert_eq!(step("./${{ inputs.dir }}"), StepUsesKind::Expression);
    assert_eq!(step("docker://${{ env.IMAGE }}"), StepUsesKind::Expression);
    // `ContainsExpression` needs `${{` before the first `}}`.
    assert_eq!(
        step("}}${{"),
        StepUsesKind::Invalid(InvalidAction::MissingRef)
    );
}

#[test]
fn each_invalid_action_reason_is_the_text_of_the_reference() {
    assert_eq!(InvalidAction::MissingRef.reason(), "ref is missing");
    assert_eq!(InvalidAction::MissingOwner.reason(), "owner is missing");
    assert_eq!(
        InvalidAction::EmptyPart.reason(),
        "owner and repo and ref should not be empty"
    );
}

#[test]
fn a_job_uses_value_starting_with_dot_slash_without_a_ref_is_a_local_workflow() {
    // `examples/workflow_call_jobs`.
    assert_eq!(
        job("./.github/workflows/not-existing.yml"),
        JobUsesKind::Local {
            path: part("./.github/workflows/not-existing.yml", 0)
        }
    );
}

#[test]
fn a_local_workflow_with_a_ref_is_invalid() {
    // `examples/workflow_call_jobs` and `err/workflow_call_job`.
    for value in [
        "./.github/workflows/ci.yml@main",
        "./foo/bar/workflow.yml@main",
    ] {
        assert_eq!(
            job(value),
            JobUsesKind::Invalid(InvalidWorkflow::LocalWithRef),
            "{value:?}"
        );
    }
}

#[test]
fn an_at_sign_right_after_dot_slash_is_part_of_a_local_workflow_path() {
    // `isWorkflowCallUsesLocalFormat` rejects an `@` only at an index above
    // zero.
    assert_eq!(
        job("./@main"),
        JobUsesKind::Local {
            path: part("./@main", 0)
        }
    );
}

#[test]
fn dot_slash_alone_is_not_a_local_workflow() {
    assert_eq!(
        job("./"),
        JobUsesKind::Invalid(InvalidWorkflow::LocalWithoutPath)
    );
}

#[test]
fn a_repository_workflow_has_an_owner_a_repo_a_path_and_a_ref() {
    // `examples/workflow_call_jobs`.
    assert_eq!(
        job("owner/repo/path/to/workflow.yml@v1"),
        workflow(
            ("owner", 0),
            ("repo", 6),
            ("path/to/workflow.yml", 11),
            ("v1", 32)
        )
    );
    // `err/workflow_call_job`, a path without a directory.
    assert_eq!(
        job("org/repo/workflow.yml@v1"),
        workflow(("org", 0), ("repo", 4), ("workflow.yml", 9), ("v1", 22))
    );
    assert_eq!(
        job("o/r/w.yml@a@b"),
        workflow(("o", 0), ("r", 2), ("w.yml", 4), ("a@b", 10))
    );
}

#[test]
fn a_job_uses_value_starting_with_a_dot_but_not_dot_slash_is_invalid() {
    for value in [".github/workflows/ci.yml", "../ci.yml", ".o/r/w.yml@v1"] {
        assert_eq!(
            job(value),
            JobUsesKind::Invalid(InvalidWorkflow::DotPrefix),
            "{value:?}"
        );
    }
}

#[test]
fn a_job_uses_value_names_the_first_missing_part_of_the_repository_format() {
    // From `err/workflow_call_job` and `err/upper_case_duplicate_keys`, then
    // empty parts and values made of separators.
    let cases = [
        ("/foo/bar/workflow.yml@main", InvalidWorkflow::MissingOwner),
        ("", InvalidWorkflow::MissingOwner),
        ("@", InvalidWorkflow::MissingOwner),
        ("/", InvalidWorkflow::MissingOwner),
        ("o//w.yml@v1", InvalidWorkflow::MissingRepo),
        // With one `/`, the text after it is read as the repo.
        ("foo/workflow.yml@main", InvalidWorkflow::MissingPath),
        ("owner/repo@main", InvalidWorkflow::MissingPath),
        ("o/r/@v1", InvalidWorkflow::MissingPath),
        ("foo/bar/workflow.yml", InvalidWorkflow::MissingRef),
        ("o/r/w.yml@", InvalidWorkflow::MissingRef),
    ];
    for (value, reason) in cases {
        assert_eq!(job(value), JobUsesKind::Invalid(reason), "{value:?}");
    }
}

#[test]
fn a_job_uses_value_with_an_expression_is_not_classified() {
    assert_eq!(job("${{ inputs.org }}/r/w.yml@v1"), JobUsesKind::Expression);
    assert_eq!(job("./${{ inputs.path }}"), JobUsesKind::Expression);
}

/// Parses `text`, a mapping with a `uses` key, and returns the classified
/// value with the file.
fn parsed_step(text: &str, check: impl FnOnce(&SourceFile, StepUses<'_>)) {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = document(text);
    let source = Str::new(node_at(&document, &["uses"])).unwrap();
    check(&file, StepUses::new(Template::new(source), &file));
}

fn text_at<'f>(file: &'f SourceFile, part: &UsesPart<'_>) -> &'f str {
    &file.text()[part.span.start as usize..part.span.end as usize]
}

#[test]
fn a_part_span_points_into_the_source_file() {
    parsed_step("uses: \"actions/\\x63heckout@v6\"\n", |file, uses| {
        assert_eq!(uses.value(), "actions/checkout@v6");
        assert_eq!(uses.span(), uses.template.source.span());
        let StepUsesKind::Repository {
            owner,
            repo,
            git_ref,
            ..
        } = uses.kind
        else {
            panic!("{:?}", uses.kind);
        };
        assert_eq!(text_at(file, &owner), "actions");
        // The escape `\x63` decodes to `c`, so the repo's span covers it.
        assert_eq!(repo.value, "checkout");
        assert_eq!(text_at(file, &repo), "\\x63heckout");
        assert_eq!(text_at(file, &git_ref), "v6");
    });
}

#[test]
fn a_job_uses_part_span_points_into_the_source_file() {
    let text = "uses: 'o/r/.github/workflows/ci.yml@v1'\n";
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = document(text);
    let source = Str::new(node_at(&document, &["uses"])).unwrap();
    let uses = JobUses::new(Template::new(source), &file);
    let JobUsesKind::Repository { path, git_ref, .. } = uses.kind else {
        panic!("{:?}", uses.kind);
    };
    assert_eq!(text_at(&file, &path), ".github/workflows/ci.yml");
    assert_eq!(text_at(&file, &git_ref), "v1");
}

/// Returns the byte range of `part` in `value`, of which it is a slice.
fn range_in(value: &str, part: &UsesPart<'_>) -> std::ops::Range<usize> {
    let start = part.value.as_ptr() as usize - value.as_ptr() as usize;
    start..start + part.value.len()
}

#[test]
fn part_spans_match_the_source_map_in_every_scalar_style() {
    let cases = [
        "uses: o/r/p@v1\n",
        "uses: &a o/r/p@v1\n",
        "uses: !!str o/r/p@v1\n",
        "uses: 'o/r/p@v1'\n",
        "uses: \"o/r/p@v1\"\n",
        "uses: 'o''r/p@v1'\n",
        "uses: \"o/\\u0072/p@v1\"\n",
        "uses: \"o/\\\\/p@v1\"\n",
        "uses: o/r/p\n  @v1\n",
        "uses: 'o/r/p\n\n  @v1'\n",
        "uses: >-\n  o/r/p@v1\n",
        "uses: |-\n  o/r/p@v1\n",
        "uses: docker://é:ü\n",
    ];
    for text in cases {
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = document(text);
        let source = Str::new(node_at(&document, &["uses"])).unwrap();
        let uses = StepUses::new(Template::new(source), &file);
        let parts: Vec<UsesPart<'_>> = match uses.kind {
            StepUsesKind::Repository {
                owner,
                repo,
                path,
                git_ref,
            } => [Some(owner), Some(repo), path, Some(git_ref)]
                .into_iter()
                .flatten()
                .collect(),
            StepUsesKind::Docker { image, tag } => {
                [Some(image), tag].into_iter().flatten().collect()
            }
            kind => panic!("{text:?}: {kind:?}"),
        };
        for part in parts {
            let expected = source
                .source_span(&file, range_in(source.value(), &part))
                .unwrap();
            assert_eq!(part.span, expected, "{text:?}: {part:?}");
        }
    }
}
