//! The `shell-name` rule: `shell` values that are not a shell of the job's platform.

#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline, sort_and_dedup};

/// Lints `workflow` as the binary does, without the file system, and returns every diagnostic in
/// the one-line format.
fn lint_all(workflow: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", workflow).unwrap();
    let parsed = workflint_yaml::parse(&file);
    let document = parsed.document.unwrap();
    let syntax = workflint_syntax::parse(&file, &document);
    let mut diagnostics = syntax.diagnostics;
    diagnostics.extend(workflint_rules::run(&file, &syntax.workflow));
    sort_and_dedup(&file, &mut diagnostics);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Lints `workflow` and returns the diagnostics of `invalid-shell-name`.
fn lint(workflow: &str) -> Vec<String> {
    lint_all(workflow)
        .into_iter()
        .filter(|line| line.ends_with("[invalid-shell-name]"))
        .collect()
}

/// Returns a workflow with one job on `runs_on` and one step with `shell: <shell>`. The shell is
/// on line 7, column 16.
fn step_shell(runs_on: &str, shell: &str) -> String {
    format!(
        "on: push\njobs:\n  test:\n    runs-on: {runs_on}\n    steps:\n      - run: echo\n        shell: {shell}\n"
    )
}

const MAC_OR_LINUX: &str = r#""bash", "pwsh", "python", "sh""#;
const WINDOWS: &str = r#""bash", "cmd", "powershell", "pwsh", "python""#;
const ANY: &str = r#""bash", "cmd", "powershell", "pwsh", "python", "sh""#;

#[test]
fn an_unknown_shell_is_reported_with_the_names_of_the_platform_and_the_custom_shell_form() {
    assert_eq!(
        lint(&step_shell("ubuntu-latest", "dash")),
        [format!(
            r#"test.yaml:7:16: shell name "dash" is invalid. available names are {MAC_OR_LINUX}. use a command with {{0}} for a custom shell, as in "dash {{0}}" [invalid-shell-name]"#
        )]
    );
    assert_eq!(
        lint(&step_shell("windows-latest", "nix develop")),
        [format!(
            r#"test.yaml:7:16: shell name "nix develop" is invalid. available names are {WINDOWS}. use a command with {{0}} for a custom shell, as in "nix develop {{0}}" [invalid-shell-name]"#
        )]
    );
}

#[test]
fn a_shell_of_another_platform_names_the_platform_and_not_the_custom_shell_form() {
    assert_eq!(
        lint(&step_shell("ubuntu-latest", "powershell")),
        [format!(
            r#"test.yaml:7:16: shell name "powershell" is invalid on macOS or Linux. available names are {MAC_OR_LINUX} [invalid-shell-name]"#
        )]
    );
    assert_eq!(
        lint(&step_shell("macos-latest", "cmd")),
        [format!(
            r#"test.yaml:7:16: shell name "cmd" is invalid on macOS or Linux. available names are {MAC_OR_LINUX} [invalid-shell-name]"#
        )]
    );
    assert_eq!(
        lint(&step_shell("windows-latest", "SH")),
        [format!(
            r#"test.yaml:7:16: shell name "SH" is invalid on Windows. available names are {WINDOWS} [invalid-shell-name]"#
        )]
    );
}

#[test]
fn every_shell_of_the_platform_passes_in_any_case() {
    for (runs_on, shells) in [
        ("ubuntu-latest", &["bash", "pwsh", "python", "sh"][..]),
        (
            "windows-latest",
            &["bash", "pwsh", "python", "cmd", "powershell"],
        ),
        (
            "self-hosted",
            &["bash", "pwsh", "python", "sh", "cmd", "powershell"],
        ),
    ] {
        for shell in shells {
            for written in [shell.to_string(), shell.to_uppercase()] {
                assert_eq!(
                    lint(&step_shell(runs_on, &written)),
                    Vec::<String>::new(),
                    "{runs_on} {written}"
                );
            }
        }
    }
}

#[test]
fn a_custom_shell_or_an_expression_is_not_checked() {
    for shell in [
        "'perl {0}'",
        "'{0}'",
        "${{ inputs.shell }}",
        "'my${{ inputs.shell }}'",
    ] {
        assert_eq!(
            lint(&step_shell("ubuntu-latest", shell)),
            Vec::<String>::new(),
            "{shell}"
        );
    }
}

#[test]
fn the_platform_comes_from_the_literal_labels() {
    for (runs_on, platform, names) in [
        ("windows", " on Windows", WINDOWS),
        ("Windows-2025", " on Windows", WINDOWS),
        ("[self-hosted, windows]", " on Windows", WINDOWS),
        ("{labels: windows-latest}", " on Windows", WINDOWS),
        ("macos", " on macOS or Linux", MAC_OR_LINUX),
        ("Linux", " on macOS or Linux", MAC_OR_LINUX),
        ("ubuntu-24.04", " on macOS or Linux", MAC_OR_LINUX),
        (
            "[self-hosted, linux, x64]",
            " on macOS or Linux",
            MAC_OR_LINUX,
        ),
    ] {
        let shell = if names == WINDOWS { "sh" } else { "cmd" };
        assert_eq!(
            lint(&step_shell(runs_on, shell)),
            [format!(
                r#"test.yaml:7:16: shell name "{shell}" is invalid{platform}. available names are {names} [invalid-shell-name]"#
            )],
            "{runs_on}"
        );
    }
}

#[test]
fn the_platform_comes_from_provider_labels_and_the_words_of_custom_labels() {
    for (runs_on, platform, names) in [
        ("depot-windows-2022", " on Windows", WINDOWS),
        (
            "blacksmith-4vcpu-ubuntu-2404",
            " on macOS or Linux",
            MAC_OR_LINUX,
        ),
        ("ubicloud-standard-2", " on macOS or Linux", MAC_OR_LINUX),
        ("warp-macos-15-arm64-6x", " on macOS or Linux", MAC_OR_LINUX),
        (
            "[self-hosted, self-32vcpu-windows-2022]",
            " on Windows",
            WINDOWS,
        ),
        (
            "[self-hosted, gpu-ubuntu22.04]",
            " on macOS or Linux",
            MAC_OR_LINUX,
        ),
        (
            "'runs-on=${{ github.run_id }}/runner=2cpu-linux-x64'",
            " on macOS or Linux",
            MAC_OR_LINUX,
        ),
        (
            "['runs-on=${{ github.run_id }}', image=windows25-full-x64]",
            " on Windows",
            WINDOWS,
        ),
    ] {
        let shell = if names == WINDOWS { "sh" } else { "cmd" };
        assert_eq!(
            lint(&step_shell(runs_on, shell)),
            [format!(
                r#"test.yaml:7:16: shell name "{shell}" is invalid{platform}. available names are {names} [invalid-shell-name]"#
            )],
            "{runs_on}"
        );
    }
}

#[test]
fn words_inside_an_expression_name_no_platform() {
    let runs_on = "\"${{ matrix.os == 'windows' && 'a' || 'b' }}\"";
    assert_eq!(lint(&step_shell(runs_on, "cmd")), Vec::<String>::new());
    assert_eq!(lint(&step_shell(runs_on, "sh")), Vec::<String>::new());
}

#[test]
fn labels_of_two_platforms_or_none_check_against_any_platform() {
    for runs_on in [
        "[ubuntu-latest, windows-latest]",
        "self-hosted",
        "${{ matrix.os }}",
        "{labels: '${{ inputs.runner }}'}",
        "{group: large}",
        "'${{ matrix.os }}-latest'",
    ] {
        assert_eq!(
            lint(&step_shell(runs_on, "cmd")),
            Vec::<String>::new(),
            "{runs_on}"
        );
        assert_eq!(
            lint(&step_shell(runs_on, "fish")),
            [format!(
                r#"test.yaml:7:16: shell name "fish" is invalid. available names are {ANY}. use a command with {{0}} for a custom shell, as in "fish {{0}}" [invalid-shell-name]"#
            )],
            "{runs_on}"
        );
    }
}

#[test]
fn the_workflow_default_is_checked_against_any_platform() {
    let text = "on: push
defaults:
  run:
    shell: powershell
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(lint(text), Vec::<String>::new());
    let text = text.replace("shell: powershell", "shell: fish");
    assert_eq!(
        lint(&text),
        [format!(
            r#"test.yaml:4:12: shell name "fish" is invalid. available names are {ANY}. use a command with {{0}} for a custom shell, as in "fish {{0}}" [invalid-shell-name]"#
        )]
    );
}

#[test]
fn the_job_default_is_checked_against_the_job_platform() {
    let text = "on: push
jobs:
  test:
    runs-on: windows-latest
    defaults:
      run:
        shell: sh
    steps:
      - run: echo
";
    assert_eq!(
        lint(text),
        [format!(
            r#"test.yaml:7:16: shell name "sh" is invalid on Windows. available names are {WINDOWS} [invalid-shell-name]"#
        )]
    );
}

#[test]
fn the_platform_of_one_job_does_not_carry_over_to_the_next() {
    let text = "on: push
jobs:
  windows:
    runs-on: windows-latest
    steps:
      - run: echo
  other:
    runs-on: self-hosted
    steps:
      - run: echo
        shell: sh
";
    assert_eq!(lint(text), Vec::<String>::new());
}

/// The reference checks a job's `defaults.run.shell` only when `runs-on` is written
/// (`rule_shell_name.go:45`). The parser reports the missing `runs-on`.
#[test]
fn the_job_default_is_not_checked_without_runs_on() {
    let text = "on: push
jobs:
  test:
    defaults:
      run:
        shell: fish
    steps:
      - run: echo
        shell: dash
";
    let lines = lint(text);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with(
        r#"test.yaml:9:16: shell name "dash" is invalid. available names are "bash", "cmd""#
    ));
}

#[test]
fn an_empty_or_non_scalar_shell_gets_only_the_parser_error() {
    let text = "on: push
defaults:
  run:
    shell: ''
jobs:
  test:
    runs-on: ubuntu-latest
    defaults:
      run:
        shell: [x]
    steps:
      - run: echo
        shell: ''
      - run: echo
        shell: [x]
";
    assert_eq!(
        lint_all(text),
        [
            "test.yaml:4:12: string should not be empty [empty-string]",
            r#"test.yaml:10:16: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            "test.yaml:13:16: string should not be empty [empty-string]",
            r#"test.yaml:15:16: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
        ]
    );
}
