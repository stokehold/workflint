#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

use super::RunnerLabel;
use crate::{Context, Rule, visit};

/// Lints `text` with only the `runner-label` rule, configured with the label patterns `known`,
/// and returns its diagnostics in the one-line format.
fn lint(text: &str, known: &[&str]) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let rule = RunnerLabel::new(known.iter().map(|label| (*label).to_owned()).collect());
    let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(rule)];
    let mut cx = Context::new(&file);
    visit(&parsed.workflow, &mut rules, &mut cx);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &cx.into_diagnostics(), Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A YAML single-quoted scalar.
fn single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Returns a workflow whose job runs on `labels` with the matrix row `os` set to `matrix`, if
/// it is not `None`.
fn workflow(labels: &[&str], matrix: Option<&[&str]>) -> String {
    let labels: Vec<_> = labels.iter().map(|label| single_quoted(label)).collect();
    let strategy = matrix.map_or_else(String::new, |matrix| {
        let values: Vec<_> = matrix.iter().map(|value| single_quoted(value)).collect();
        format!(
            "    strategy:\n      matrix:\n        os: [{}]\n",
            values.join(", ")
        )
    });
    format!(
        "on: push\njobs:\n  test:\n{strategy}    runs-on: [{}]\n    steps:\n      - run: echo\n",
        labels.join(", ")
    )
}

/// One case of `TestRuleRunnerLabelCheckLabels` in `rule_runner_label_test.go`: the labels, the
/// values of the matrix row `os`, the configured label patterns, and a part of each message.
struct Case {
    what: &'static str,
    labels: &'static [&'static str],
    matrix: Option<&'static [&'static str]>,
    known: &'static [&'static str],
    errs: &'static [&'static str],
}

const fn ok(what: &'static str, labels: &'static [&'static str]) -> Case {
    Case {
        what,
        labels,
        matrix: None,
        known: &[],
        errs: &[],
    }
}

const REFERENCE_CASES: &[Case] = &[
    ok("simple GH-hosted Linux runner label", &["ubuntu-24.04"]),
    ok("simple GH-hosted Windows runner label", &["windows-latest"]),
    ok("simple GH-hosted macOS runner label", &["macos-14"]),
    ok("simple GH-hosted runner label in upper case", &["macOS-14"]),
    ok("self-hosted Linux runner", &["self-hosted", "linux"]),
    ok(
        "self-hosted all Linux runner labels",
        &["self-hosted", "linux", "ubuntu-24.04", "ubuntu-latest"],
    ),
    Case {
        what: "self-hosted all macOS runner labels",
        labels: &["self-hosted", "macOS", "macOS-latest", "macOS-15"],
        matrix: None,
        known: &[],
        // The reference accepts the labels. GitHub has moved the `-latest` label to a newer
        // image (`docs/differences.md#platform-data-updates`).
        errs: &[r#"label "macOS-15" conflicts with label "macOS-latest""#],
    },
    Case {
        what: "self-hosted all Windows runner labels",
        labels: &["self-hosted", "windows", "windows-latest", "windows-2022"],
        matrix: None,
        known: &[],
        // The reference accepts the labels. GitHub has moved the `-latest` label to a newer
        // image (`docs/differences.md#platform-data-updates`).
        errs: &[r#"label "windows-2022" conflicts with label "windows-latest""#],
    },
    ok(
        "self-hosted Linux runner in upper case",
        &["SELF-HOSTED", "LINUX"],
    ),
    ok(
        "self-hosted macOS runner",
        &["self-hosted", "macos", "arm64"],
    ),
    ok(
        "self-hosted runner with GH-hosted runner label",
        &["self-hosted", "ubuntu-24.04"],
    ),
    ok("larger Ubuntu runner", &["ubuntu-latest-16-cores"]),
    ok(
        "larger Ubuntu runner with other labels",
        &["ubuntu-latest", "ubuntu-latest-16-cores"],
    ),
    ok("larger Windows runner", &["windows-latest-8-cores"]),
    ok(
        "multiple labels for GH-hosted Linux runner",
        &["ubuntu-latest", "ubuntu-24.04"],
    ),
    Case {
        what: "multiple labels for GH-hosted Windows runner",
        labels: &["windows-latest", "windows-2022"],
        matrix: None,
        known: &[],
        // The reference accepts the labels. GitHub has moved the `-latest` label to a newer
        // image (`docs/differences.md#platform-data-updates`).
        errs: &[r#"label "windows-2022" conflicts with label "windows-latest""#],
    },
    Case {
        what: "multiple labels for GH-hosted macOS runner",
        labels: &["macos-latest", "macos-15"],
        matrix: None,
        known: &[],
        // The reference accepts the labels. GitHub has moved the `-latest` label to a newer
        // image (`docs/differences.md#platform-data-updates`).
        errs: &[r#"label "macos-15" conflicts with label "macos-latest""#],
    },
    Case {
        what: "user-defined labels",
        labels: &["self-hosted", "foo", "bar", "linux"],
        matrix: None,
        known: &["foo", "bar"],
        errs: &[],
    },
    Case {
        what: "user-defined labels with patterns",
        labels: &[
            "self-hosted",
            "INSTANCE_TYPE=m6a.large",
            "some-base-prefix:size=large&cpu=8",
        ],
        matrix: None,
        known: &["INSTANCE_TYPE=*", "some-base-prefix:size=*&cpu=?"],
        errs: &[],
    },
    Case {
        what: "some character is escaped in user-defined label patterns",
        labels: &["linux-[arch]"],
        matrix: None,
        known: &[r"*-\[*]"],
        errs: &[],
    },
    Case {
        what: "matrix",
        labels: &["${{matrix.os}}"],
        matrix: Some(&["ubuntu-latest", "windows-latest"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "matrix at second label",
        labels: &["self-hosted", "${{matrix.os}}"],
        matrix: Some(&["linux", "windows"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "matrix at first label",
        labels: &["${{matrix.os}}", "ubuntu-24.04"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "user-defined label with matrix",
        labels: &["self-hosted", "${{matrix.os}}"],
        matrix: Some(&["foo", "bar"]),
        known: &["foo", "bar"],
        errs: &[],
    },
    Case {
        what: "cannot check label: prefix",
        labels: &["foo-${{matrix.os}}"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        // The reference skips the label. workflint evaluates it
        // (`docs/differences.md#runs-on-expressions`).
        errs: &[r#"label "foo-ubuntu-latest" is unknown"#],
    },
    Case {
        what: "cannot check label: suffix",
        labels: &["${{matrix.os}}-bar"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        // The reference skips the label. workflint evaluates it
        // (`docs/differences.md#runs-on-expressions`).
        errs: &[r#"label "ubuntu-latest-bar" is unknown"#],
    },
    Case {
        what: "cannot check label: not a matrix",
        labels: &["${{fromJSON(env.FOO).os}}"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "cannot check label: not a matrix 2",
        labels: &["${{env.os}}"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "cannot check label: matrix object",
        labels: &["${{matrix}}"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "cannot parse expression",
        labels: &["${{}}"],
        matrix: Some(&["ubuntu-latest"]),
        known: &[],
        errs: &[],
    },
    Case {
        what: "give up checking matrix value containing expression",
        labels: &["${{matrix.os}}"],
        matrix: Some(&["ubuntu-latest", "${{env.OS}}"]),
        known: &[],
        errs: &[],
    },
    ok("use matrix value but no matrix exist", &["${{matrix.os}}"]),
    ok("ubuntu-24.04", &["ubuntu-24.04"]),
    Case {
        what: "undefined label",
        labels: &["linux-latest"],
        matrix: None,
        known: &[],
        errs: &[r#""linux-latest" is unknown"#],
    },
    Case {
        what: "undefined self-hosted label",
        labels: &["self-hosted", "foo"],
        matrix: None,
        known: &[],
        errs: &[r#""foo" is unknown"#],
    },
    Case {
        what: "GH-hosted runner labels conflict",
        labels: &["ubuntu-latest", "windows-latest"],
        matrix: None,
        known: &[],
        errs: &[r#"label "windows-latest" conflicts with label "ubuntu-latest""#],
    },
    Case {
        what: "self-hosted runner labels conflict",
        labels: &["self-hosted", "windows", "linux"],
        matrix: None,
        known: &[],
        errs: &[r#"label "linux" conflicts with label "windows""#],
    },
    Case {
        what: "self-hosted runner labels conflict with GH-hosted runner label",
        labels: &["self-hosted", "windows", "macOS-latest"],
        matrix: None,
        known: &[],
        errs: &[r#"label "macOS-latest" conflicts with label "windows""#],
    },
    Case {
        what: "GH-hosted labels multiple conflicts",
        labels: &["ubuntu-latest", "windows-latest", "macos-latest"],
        matrix: None,
        known: &[],
        errs: &[
            r#"label "windows-latest" conflicts with label "ubuntu-latest""#,
            r#"label "macos-latest" conflicts with label "ubuntu-latest""#,
        ],
    },
    Case {
        what: "GH-hosted labels conflict mixed with self-hosted runner labels",
        labels: &[
            "self-hosted",
            "ubuntu-latest",
            "x64",
            "windows-latest",
            "foo",
        ],
        matrix: None,
        known: &["foo"],
        errs: &[r#"label "windows-latest" conflicts with label "ubuntu-latest""#],
    },
    Case {
        what: "GH-hosted labels conflict ignore case",
        labels: &["macOS-latest", "Windows-latest"],
        matrix: None,
        known: &[],
        errs: &[r#"label "Windows-latest" conflicts with label "macOS-latest""#],
    },
    Case {
        what: "GH-hosted labels conflict with matrix at second label",
        labels: &["ubuntu-latest", "${{matrix.os}}"],
        matrix: Some(&["windows-latest", "macos-latest"]),
        known: &[],
        errs: &[
            r#"label "windows-latest" conflicts with label "ubuntu-latest""#,
            r#"label "macos-latest" conflicts with label "ubuntu-latest""#,
        ],
    },
    Case {
        what: "GH-hosted labels conflict with matrix at first label",
        labels: &["${{matrix.os}}", "ubuntu-latest"],
        matrix: Some(&["windows-latest", "macos-latest"]),
        known: &[],
        // The reference names a random partner. The earliest is `windows-latest`.
        errs: &[r#"label "ubuntu-latest" conflicts with label "windows-latest""#],
    },
    Case {
        what: "GH-hosted labels conflicts with multiple matrixes",
        labels: &["${{matrix.os}}", "${{matrix.os}}"],
        matrix: Some(&["windows-latest", "macos-latest"]),
        known: &[],
        errs: &[
            r#"label "windows-latest" conflicts with label "macos-latest""#,
            r#"label "macos-latest" conflicts with label "windows-latest""#,
        ],
    },
    Case {
        what: "Linux labels version conflict",
        labels: &["ubuntu-latest", "ubuntu-22.04"],
        matrix: None,
        known: &[],
        errs: &[r#"label "ubuntu-22.04" conflicts with label "ubuntu-latest""#],
    },
    Case {
        what: "macOS labels version conflict",
        labels: &["macos-14", "macos-15"],
        matrix: None,
        known: &[],
        errs: &[r#"label "macos-15" conflicts with label "macos-14""#],
    },
    Case {
        what: "Windows labels version conflict",
        labels: &["windows-2025", "windows-2022"],
        matrix: None,
        known: &[],
        errs: &[r#"label "windows-2022" conflicts with label "windows-2025""#],
    },
    Case {
        what: "Linux labels architecture conflict",
        labels: &["ubuntu-24.04", "ubuntu-22.04"],
        matrix: None,
        known: &[],
        errs: &[r#"label "ubuntu-22.04" conflicts with label "ubuntu-24.04""#],
    },
    Case {
        what: "macOS labels architecture conflict",
        labels: &["macos-15", "macos-15-intel"],
        matrix: None,
        known: &[],
        errs: &[r#"label "macos-15-intel" conflicts with label "macos-15""#],
    },
    Case {
        what: "macOS 26 labels architecture conflict",
        labels: &["macos-26", "macos-26-intel"],
        matrix: None,
        known: &[],
        errs: &[r#"label "macos-26-intel" conflicts with label "macos-26""#],
    },
    Case {
        what: "Windows labels architecture conflict",
        labels: &["windows-2025", "windows-11-arm"],
        matrix: None,
        known: &[],
        errs: &[r#"label "windows-11-arm" conflicts with label "windows-2025""#],
    },
    Case {
        what: "macOS XL and normal labels conflict",
        labels: &["macos-26-xlarge", "macos-26"],
        matrix: None,
        known: &[],
        errs: &[r#"label "macos-26" conflicts with label "macos-26-xlarge""#],
    },
    Case {
        what: "larger runner labels conflict",
        labels: &["ubuntu-latest-16-cores", "windows-latest-8-cores"],
        matrix: None,
        known: &[],
        errs: &[r#"label "windows-latest-8-cores" conflicts with label "ubuntu-latest-16-cores""#],
    },
    Case {
        what: "user-defined labels with invalid glob pattern",
        labels: &["self-hosted", "INSTANCE_TYPE=m6a.large"],
        matrix: None,
        known: &["INSTANCE_TYPE=["],
        errs: &[
            r#"label pattern "INSTANCE_TYPE=[" is an invalid glob. kindly check list of labels in actionlint.yaml config file: syntax error in pattern"#,
        ],
    },
];

#[test]
fn the_cases_of_the_reference_tests_give_the_reference_messages() {
    for case in REFERENCE_CASES {
        let have = lint(&workflow(case.labels, case.matrix), case.known);
        assert_eq!(have.len(), case.errs.len(), "{}: {have:#?}", case.what);
        for (have, want) in have.iter().zip(case.errs) {
            assert!(
                have.contains(want),
                "{}: {have:?} lacks {want:?}",
                case.what
            );
        }
    }
}

#[test]
fn a_job_without_runs_on_gives_nothing() {
    let text = "on: push\njobs:\n  test:\n    uses: ./.github/workflows/x.yml\n";
    assert_eq!(lint(text, &[]), Vec::<String>::new());
}

#[test]
fn the_partner_named_in_a_conflict_is_the_earliest_conflicting_label() {
    // `windows` conflicts with both `ubuntu-latest` and `macos-latest`. The reference iterates a
    // Go map and names either one.
    let text = workflow(&["ubuntu-latest", "macos-latest", "windows"], None);
    for _ in 0..20 {
        assert_eq!(
            lint(&text, &[]),
            [
                "test.yaml:4:32: label \"macos-latest\" conflicts with label \"ubuntu-latest\" defined at line:4,col:15. note: to run your job on each workers, use matrix [runner-label-conflict]",
                "test.yaml:4:48: label \"windows\" conflicts with label \"ubuntu-latest\" defined at line:4,col:15. note: to run your job on each workers, use matrix [runner-label-conflict]",
            ]
        );
    }
}

#[test]
fn a_label_with_the_images_of_an_earlier_one_does_not_become_the_partner() {
    // `ubuntu-24.04` has the images of `ubuntu-latest`, so only `ubuntu-latest` is kept, as in
    // the reference.
    let text = workflow(&["ubuntu-latest", "ubuntu-24.04", "windows-latest"], None);
    let have = lint(&text, &[]);
    assert_eq!(have.len(), 1, "{have:#?}");
    assert!(have[0].contains(r#"conflicts with label "ubuntu-latest" defined at line:4,col:15"#));
}

#[test]
fn a_label_that_conflicts_is_not_kept_as_a_partner() {
    // `windows-latest` conflicts with `ubuntu-latest`, so `windows-2022` conflicts only with
    // `ubuntu-latest`.
    let text = workflow(&["ubuntu-latest", "windows-latest", "windows-2022"], None);
    let have = lint(&text, &[]);
    assert_eq!(have.len(), 2, "{have:#?}");
    assert!(have[1].contains(r#"label "windows-2022" conflicts with label "ubuntu-latest""#));
}

#[test]
fn an_unknown_label_lists_the_configured_patterns_last() {
    let have = lint(&workflow(&["bar"], None), &["foo-*"]);
    assert_eq!(have.len(), 1, "{have:#?}");
    assert!(
        have[0].contains(r#""linux", "macos", "windows", "foo-*". if it is a custom label"#),
        "{have:?}"
    );
    assert!(have[0].starts_with(
        r#"test.yaml:4:15: label "bar" is unknown. available labels are "windows-latest", "#
    ));
    assert!(have[0].ends_with("[unknown-runner-label]"));
}

#[test]
fn an_invalid_pattern_is_reported_at_each_label_that_reaches_it() {
    // The labels before the invalid pattern stop at a match, as in the reference.
    let have = lint(
        &workflow(&["foo", "bar", "baz"], None),
        &["foo", "[", "baz"],
    );
    assert_eq!(
        have,
        [
            r#"test.yaml:4:22: label pattern "[" is an invalid glob. kindly check list of labels in actionlint.yaml config file: syntax error in pattern [invalid-runner-label-pattern]"#,
            r#"test.yaml:4:29: label pattern "[" is an invalid glob. kindly check list of labels in actionlint.yaml config file: syntax error in pattern [invalid-runner-label-pattern]"#,
        ]
    );
}

#[test]
fn configured_patterns_match_the_whole_label_and_compare_case() {
    assert_eq!(lint(&workflow(&["gpu-a100"], None), &["gpu-*"]).len(), 0);
    assert_eq!(lint(&workflow(&["my-gpu-a100"], None), &["gpu-*"]).len(), 1);
    assert_eq!(lint(&workflow(&["GPU-a100"], None), &["gpu-*"]).len(), 1);
    assert_eq!(lint(&workflow(&["gpu/a100"], None), &["gpu*"]).len(), 1);
}

#[test]
fn matrix_values_are_reported_at_the_value() {
    let text = "on: push\njobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-oldest]\n        include:\n          - os: macos-oldest\n          - os: ${{ env.OS }}\n          - os: [x]\n    runs-on: ${{ matrix.os }}\n    steps:\n      - run: echo\n";
    let have = lint(text, &[]);
    assert_eq!(have.len(), 2, "{have:#?}");
    assert!(have[0].starts_with(r#"test.yaml:6:14: label "ubuntu-oldest" is unknown"#));
    assert!(have[1].starts_with(r#"test.yaml:8:17: label "macos-oldest" is unknown"#));
}

#[test]
fn matrix_values_of_a_whole_runs_on_expression_are_checked_for_conflicts_with_each_other_first() {
    // `runs-on: ${{ matrix.os }}` is the reference's `LabelsExpr`, checked as one of several
    // labels. Its values are checked against the labels before it, which are none, so they do
    // not conflict.
    let text = "on: push\njobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, windows-latest]\n    runs-on:\n      labels: ${{ matrix.os }}\n    steps:\n      - run: echo\n";
    assert_eq!(lint(text, &[]), Vec::<String>::new());
}

#[test]
fn matrix_property_names_compare_without_case() {
    let text = "on: push\njobs:\n  test:\n    strategy:\n      matrix:\n        OS: [ubuntu-oldest]\n    runs-on: ${{ matrix.Os }}\n    steps:\n      - run: echo\n";
    assert_eq!(lint(text, &[]).len(), 1);
}

#[test]
fn invalid_labels_keep_their_slot_and_are_skipped() {
    // Two slots, so the reference takes the path for several labels. The invalid one, which
    // the parser reported, gives nothing here.
    let text = "on: push\njobs:\n  test:\n    runs-on: ['', ubuntu-latest, windows-latest]\n    steps:\n      - run: echo\n";
    let have = lint(text, &[]);
    assert_eq!(have.len(), 1, "{have:#?}");
    assert!(have[0].contains(r#"label "windows-latest" conflicts with label "ubuntu-latest""#));
    let text = "on: push\njobs:\n  test:\n    runs-on: ['']\n    steps:\n      - run: echo\n";
    assert_eq!(lint(text, &[]), Vec::<String>::new());
}

/// Returns a workflow whose job has the matrix `matrix`, YAML indented under `matrix:`, if it is
/// not empty, and runs on `runs_on`, written as the YAML value of `runs-on`.
fn job(matrix: &str, runs_on: &str) -> String {
    let strategy = if matrix.is_empty() {
        String::new()
    } else {
        let rows: String = matrix
            .lines()
            .flat_map(|line| {
                [
                    "        ", line, "
",
                ]
            })
            .collect();
        format!("    strategy:\n      matrix:\n{rows}")
    };
    format!(
        "on: push\njobs:\n  test:\n{strategy}    runs-on: {runs_on}\n    steps:\n      - run: echo\n"
    )
}

/// Lints `text` and shortens each diagnostic to its position and the text before `. available`
/// or `defined at`.
fn short(text: &str) -> Vec<String> {
    lint(text, &[])
        .iter()
        .map(|line| {
            let line = line.strip_prefix("test.yaml:").unwrap_or(line);
            let end = line
                .find(". available")
                .or_else(|| line.find(" defined at"))
                .unwrap_or(line.len());
            line[..end].to_owned()
        })
        .collect()
}

#[test]
fn a_string_literal_label_is_checked_at_the_label() {
    assert_eq!(
        short(&job("", "${{ 'linux-latest' }}")),
        [r#"4:14: label "linux-latest" is unknown"#]
    );
}

#[test]
fn a_template_that_mixes_text_and_matrix_values_is_checked_for_each_value() {
    assert_eq!(
        short(&job("os: [ubuntu, linux]", "${{ matrix.os }}-latest")),
        [r#"7:14: label "linux-latest" is unknown"#]
    );
}

#[test]
fn format_with_matrix_values_is_checked_for_each_value() {
    assert_eq!(
        short(&job(
            "os: [ubuntu, linux]",
            "${{ format('{0}-latest', matrix.os) }}"
        )),
        [r#"7:14: label "linux-latest" is unknown"#]
    );
    // `{{` and `}}` are braces, and an argument may be used twice.
    assert_eq!(
        short(&job("", "${{ format('{{{0}}}-{0}', 'x') }}")),
        [r#"4:14: label "{x}-x" is unknown"#]
    );
}

#[test]
fn format_with_an_invalid_format_string_is_skipped() {
    for format in ["'{0'", "'{1}'", "'}'", "'{x}'", "'{-1}'", "env.FORMAT"] {
        let runs_on = format!("${{{{ format({format}, 'linux-latest') }}}}");
        assert_eq!(short(&job("os: ['{0}']", &runs_on)), Vec::<String>::new());
    }
}

#[test]
fn and_or_give_the_value_of_each_branch() {
    // The condition is unknown, so both values are checked.
    assert_eq!(
        short(&job(
            "",
            "${{ github.repository == 'a/b' && 'self-hosted' || 'ubuntu-20.04' }}"
        )),
        [r#"4:14: label "ubuntu-20.04" is unknown"#]
    );
    // GitHub's truthiness: `''` and `0` are false, so `||` gives the other operand.
    assert_eq!(
        short(&job("", "${{ '' || 0 || 'linux-latest' }}")),
        [r#"4:14: label "linux-latest" is unknown"#]
    );
    assert_eq!(
        short(&job("", "${{ 'x' && 'linux-latest' }}")),
        [r#"4:14: label "linux-latest" is unknown"#]
    );
}

#[test]
fn case_gives_each_value_whose_predicate_is_not_known_to_be_false() {
    // The predicate is unknown, so the label is the value or the default.
    assert_eq!(
        short(&job(
            "",
            "${{ case(github.ref == 'a', 'ubuntu-latest', 'ubuntu-latst') }}"
        )),
        [r#"4:14: label "ubuntu-latst" is unknown"#]
    );
    assert_eq!(
        short(&job(
            "",
            "${{ case(github.ref == 'a', 'ubuntu-latst', 'ubuntu-latest') }}"
        )),
        [r#"4:14: label "ubuntu-latst" is unknown"#]
    );
    // A false predicate skips its value, and a true one ends the call.
    assert_eq!(
        short(&job(
            "",
            "${{ case(false, 'linux-a', github.ref == 'a', 'linux-b', true, 'linux-c', 'linux-d') }}"
        )),
        [
            r#"4:14: label "linux-b" is unknown"#,
            r#"4:14: label "linux-c" is unknown"#,
        ]
    );
}

#[test]
fn case_predicates_on_the_matrix_are_evaluated_for_each_combination() {
    assert_eq!(
        short(&job(
            "os: [ubuntu, windows]",
            "${{ case(matrix.os == 'windows', 'windows-latest', format('{0}-latst', matrix.os)) }}"
        )),
        [r#"7:14: label "ubuntu-latst" is unknown"#]
    );
}

#[test]
fn case_that_github_rejects_is_skipped() {
    // A predicate that is not a boolean, a missing default, and too few arguments.
    for runs_on in [
        "${{ case('x', 'linux-a', 'linux-b') }}",
        "${{ case(github.ref == 'a', 'linux-a', true, 'linux-b') }}",
        "${{ case(true, 'linux-a') }}",
    ] {
        assert_eq!(short(&job("", runs_on)), Vec::<String>::new(), "{runs_on}");
    }
}

#[test]
fn a_condition_on_the_matrix_is_evaluated_for_each_combination() {
    // In the `windows` combination the condition is true, so `windows` is never a label.
    assert_eq!(
        short(&job(
            "os: [ubuntu-latest, windows]",
            "${{ matrix.os == 'windows' && 'windows-latest' || matrix.os }}"
        )),
        Vec::<String>::new()
    );
    assert_eq!(
        short(&job(
            "os: [ubuntu-latest, windows]",
            "${{ !(matrix.os != 'Windows') && 'windows-oldest' || matrix.os }}"
        )),
        [r#"7:14: label "windows-oldest" is unknown"#]
    );
}

#[test]
fn a_label_with_a_value_that_cannot_be_worked_out_is_skipped() {
    for runs_on in [
        "${{ github.event.inputs.platform || 'ubuntu' }}-latest",
        "${{ fromJSON(needs.a.outputs.runner) }}",
        "${{ inputs.os }}",
        "${{ matrix.os }}-latest",
        "${{ format('{0}-{1}', 'linux', env.ARCH) }}",
        "${{ matrix.os < 'x' && 'linux-latest' }}",
    ] {
        assert_eq!(short(&job("", runs_on)), Vec::<String>::new(), "{runs_on}");
    }
    // A matrix value that is an expression, a mapping, or a sequence is not known.
    for row in [
        "os: ['${{ env.OS }}']",
        "os: [{name: linux}]",
        "os: [[linux]]",
        "os: ${{ fromJSON(env.OS) }}",
    ] {
        assert_eq!(
            short(&job(row, "${{ matrix.os }}-latest")),
            Vec::<String>::new(),
            "{row}"
        );
    }
    // A whole matrix written as one expression.
    let text = job("", "${{ matrix.os }}-latest").replace(
        "    runs-on",
        "    strategy:\n      matrix: ${{ fromJSON(env.M) }}\n    runs-on",
    );
    assert_eq!(short(&text), Vec::<String>::new());
}

#[test]
fn include_values_are_not_combined_with_the_values_of_other_include_items() {
    let matrix = "include:\n  - {os: ubuntu, ver: '24.04'}\n  - {os: macos, ver: '15'}";
    assert_eq!(
        short(&job(matrix, "${{ matrix.os }}-${{ matrix.ver }}")),
        Vec::<String>::new()
    );
}

#[test]
fn include_extends_the_combinations_whose_row_values_it_matches() {
    // `{os: ubuntu, arch: arm}` extends the `ubuntu` combination, and `{os: windows}` matches
    // no row value, so it is a combination of its own without `arch`, which is null.
    let matrix = "os: [ubuntu, macos]\ninclude:\n  - {os: ubuntu, arch: '-arm'}\n  - {os: windows}";
    assert_eq!(
        short(&job(matrix, "${{ matrix.os }}${{ matrix.arch }}-x")),
        [
            r#"10:14: label "ubuntu-arm-x" is unknown"#,
            r#"10:14: label "macos-x" is unknown"#,
            r#"10:14: label "windows-x" is unknown"#,
        ]
    );
}

#[test]
fn excluded_combinations_are_not_checked() {
    let matrix = "os: [ubuntu, macos]\nver: ['24.04', '15']\nexclude:\n  - {os: ubuntu, ver: '15'}\n  - {os: macos, ver: '24.04'}";
    assert_eq!(
        short(&job(matrix, "${{ matrix.os }}-${{ matrix.ver }}")),
        Vec::<String>::new()
    );
}

#[test]
fn numbers_booleans_and_null_become_text_as_github_prints_them() {
    // `3.10` is the number 3.1.
    assert_eq!(
        short(&job("v: [13, 3.10]", "macos-${{ matrix.v }}")),
        [
            r#"7:14: label "macos-13" is unknown"#,
            r#"7:14: label "macos-3.1" is unknown"#,
        ]
    );
    assert_eq!(
        short(&job("", "x-${{ true }}-${{ null }}-${{ 0x10 }}")),
        [r#"4:14: label "x-true--16" is unknown"#]
    );
}

#[test]
fn each_value_of_a_label_is_reported_once() {
    assert_eq!(
        short(&job(
            "os: [linux, linux2]\nver: [1, 2]",
            "${{ format('{0}-latest', matrix.ver) }}"
        )),
        [
            r#"8:14: label "1-latest" is unknown"#,
            r#"8:14: label "2-latest" is unknown"#,
        ]
    );
}

#[test]
fn values_of_an_evaluated_label_conflict_with_earlier_labels_but_not_each_other() {
    let text = job(
        "os: [ubuntu, macos]",
        "[windows-latest, '${{ matrix.os }}-latest']",
    );
    assert_eq!(
        short(&text),
        [
            r#"7:31: label "ubuntu-latest" conflicts with label "windows-latest""#,
            r#"7:31: label "macos-latest" conflicts with label "windows-latest""#,
        ]
    );
    let text = job(
        "",
        "${{ github.ref == 'x' && 'windows-latest' || 'ubuntu-latest' }}",
    );
    assert_eq!(short(&text), Vec::<String>::new());
}

#[test]
fn a_matrix_with_more_combinations_than_the_limit_is_not_evaluated() {
    let values =
        |n: usize| -> String { (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ") };
    // 16 × 16 = 256 combinations, the limit.
    let matrix = format!("a: [{}]\nb: [{}]", values(16), values(16));
    assert_eq!(short(&job(&matrix, "x-${{ matrix.a }}")).len(), 16);
    // 16 × 17 = 272.
    let matrix = format!("a: [{}]\nb: [{}]", values(16), values(17));
    assert_eq!(
        short(&job(&matrix, "x-${{ matrix.a }}")),
        Vec::<String>::new()
    );
    // A label that does not use the matrix is checked whatever its size.
    assert_eq!(short(&job(&matrix, "${{ 'x' }}")).len(), 1);
}

#[test]
fn a_label_with_more_values_than_the_limit_is_not_evaluated() {
    // Each `||` with an unknown condition doubles the values: 2^8 = 256 is the limit.
    let branch = |i: usize| format!("format('{{0}}', env.A{i} && '1' || '0')");
    let label = |n: usize| {
        let args: Vec<_> = (0..n).map(branch).collect();
        let format: Vec<_> = (0..n).map(|i| format!("{{{i}}}")).collect();
        let format = format.concat();
        format!("x-${{{{ format('{format}', {}) }}}}", args.join(", "))
    };
    assert_eq!(short(&job("", &label(8))).len(), 256);
    assert_eq!(short(&job("", &label(9))), Vec::<String>::new());
}

#[test]
fn labels_of_runner_providers_are_known() {
    for label in [
        "blacksmith-4vcpu-ubuntu-2404",
        "depot-windows-2022-8",
        "nscloud-macos-tahoe-arm64-6x14",
        "namespace-profile-default",
        "runs-on=1234/runner=2cpu-linux-x64",
        "ubicloud-standard-8-arm",
        "warp-ubuntu-latest-x64-4x",
    ] {
        assert_eq!(
            lint(&workflow(&[label], None), &[]),
            Vec::<String>::new(),
            "{label}"
        );
    }
    // The array form of RunsOn puts each `key=value` in a label of its own.
    let labels = [
        "runs-on=${{ github.run_id }}",
        "cpu=4",
        "image=ubuntu22-full-x64",
    ];
    assert_eq!(lint(&workflow(&labels, None), &[]), Vec::<String>::new());
}

#[test]
fn a_provider_label_conflicts_with_no_label() {
    let labels = ["depot-ubuntu-24.04", "windows-latest"];
    assert_eq!(lint(&workflow(&labels, None), &[]), Vec::<String>::new());
}

#[test]
fn a_custom_label_that_names_a_system_is_still_unknown() {
    assert_eq!(
        short(&workflow(
            &["self-hosted", "self-32vcpu-windows-2022"],
            None
        )),
        ["4:30: label \"self-32vcpu-windows-2022\" is unknown"]
    );
}

#[test]
fn a_provider_label_that_breaks_its_pattern_is_unknown() {
    assert_eq!(
        short(&workflow(&["depot-ubuntu-2404"], None)),
        ["4:15: label \"depot-ubuntu-2404\" is unknown"]
    );
}
