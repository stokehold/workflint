//! Runs `workflint completion <shell>` and loads each script in its shell, where the shell is
//! installed, to check what it completes.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Output};

const SHELLS: [&str; 4] = ["bash", "zsh", "fish", "powershell"];

/// No completion.
const NOTHING: [&str; 0] = [];

fn workflint(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_workflint"))
        .args(args)
        .output()
        .unwrap()
}

/// The script that `workflint completion <shell>` prints.
fn script(shell: &str) -> String {
    let output = workflint(&["completion", shell]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Whether `program` runs. A test that needs it is skipped when it does not.
fn installed(program: &str, version_arg: &str) -> bool {
    let found = Command::new(program)
        .arg(version_arg)
        .output()
        .is_ok_and(|output| output.status.success());
    if !found {
        eprintln!("skipped: {program} is not installed");
    }
    found
}

/// Writes `script` to a file named `name` in a new temporary directory, with the files
/// `ci.yaml`, `notes.txt`, and `sub/` that file completion can offer.
fn workspace(name: &str, script: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), script).unwrap();
    std::fs::write(dir.path().join("ci.yaml"), "").unwrap();
    std::fs::write(dir.path().join("notes.txt"), "").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    dir
}

fn lines(output: &Output) -> Vec<String> {
    assert!(output.status.success(), "{output:?}");
    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    lines.sort();
    lines
}

#[test]
fn completion_prints_a_script_for_each_shell() {
    for shell in SHELLS {
        let script = script(shell);
        assert!(script.contains("workflint"), "{shell}:\n{script}");
        assert!(script.contains("oneline"), "{shell}:\n{script}");
    }
}

#[test]
fn completion_without_one_known_shell_is_an_invalid_command_line() {
    for args in [
        &["completion"][..],
        &["completion", "tcsh"],
        &["completion", "bash", "zsh"],
    ] {
        let output = workflint(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.starts_with(
                "completion takes one argument, the name of a shell: bash, zsh, fish, or \
                 powershell\nUsage: workflint"
            ),
            "{args:?}: {stderr}"
        );
    }
}

#[test]
fn completion_help_prints_the_usage() {
    let output = workflint(&["completion", "-h"]);
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("workflint completion bash"), "{stderr}");
}

/// Splits `line` into words as bash fills `COMP_WORDS` for the cursor at the end: at
/// whitespace, with `=` and `:` as words of their own.
#[cfg(unix)]
fn bash_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    for part in line.split_whitespace() {
        let mut word = String::new();
        for ch in part.chars() {
            if ch == '=' || ch == ':' {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                words.push(ch.to_string());
            } else {
                word.push(ch);
            }
        }
        if !word.is_empty() {
            words.push(word);
        }
    }
    if line.ends_with(char::is_whitespace) {
        words.push(String::new());
    }
    words
}

/// The words that the bash script offers for `line`, with the cursor at the end, as bash calls
/// the completion function.
#[cfg(unix)]
fn bash_complete(dir: &Path, line: &str) -> Vec<String> {
    let driver = r#"
        source ./workflint.bash || exit 1
        COMP_LINE=$1
        COMP_POINT=${#COMP_LINE}
        shift
        COMP_WORDS=("$@")
        COMP_CWORD=$(( $# - 1 ))
        _workflint || exit 1
        if (( ${#COMPREPLY[@]} )); then printf '%s\n' "${COMPREPLY[@]}"; fi
    "#;
    let output = Command::new("bash")
        .args(["--norc", "--noprofile", "-c", driver, "bash", line])
        .args(bash_words(line))
        .current_dir(dir)
        .output()
        .unwrap();
    lines(&output)
}

#[cfg(unix)]
#[test]
fn the_bash_script_loads_and_registers_the_function() {
    if !installed("bash", "--version") {
        return;
    }
    let dir = workspace("workflint.bash", &script("bash"));
    let output = Command::new("bash")
        .args([
            "--norc",
            "--noprofile",
            "-c",
            "source ./workflint.bash && complete -p workflint",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        lines(&output),
        ["complete -o filenames -F _workflint workflint"]
    );
}

#[cfg(unix)]
#[test]
fn the_bash_script_completes_single_dash_flags_and_their_values() {
    if !installed("bash", "--version") {
        return;
    }
    let dir = workspace("workflint.bash", &script("bash"));
    let dir = dir.path();
    assert_eq!(bash_complete(dir, "workflint -one"), ["-oneline"]);
    assert_eq!(bash_complete(dir, "workflint --one"), ["--oneline"]);
    assert_eq!(
        bash_complete(dir, "workflint -shellcheck"),
        ["-shellcheck", "-shellcheck-cache"]
    );
    assert_eq!(bash_complete(dir, "workflint check -sil"), ["-silent"]);
    assert_eq!(
        bash_complete(dir, "workflint -format "),
        ["github", "json", "jsonl", "sarif", "summary"]
    );
    assert_eq!(
        bash_complete(dir, "workflint -format=js"),
        ["json", "jsonl"]
    );
    assert_eq!(
        bash_complete(dir, "workflint --format="),
        ["github", "json", "jsonl", "sarif", "summary"]
    );
    assert_eq!(bash_complete(dir, "workflint -oneline=f"), ["false"]);
    assert_eq!(
        bash_complete(dir, "workflint -config-file n"),
        ["notes.txt"]
    );
    assert_eq!(bash_complete(dir, "workflint -config=n"), ["notes.txt"]);
    assert_eq!(bash_complete(dir, "workflint -ignore "), NOTHING);
    assert_eq!(
        bash_complete(dir, "workflint -format json "),
        ["ci.yaml", "sub"]
    );
}

#[cfg(unix)]
#[test]
fn the_bash_script_offers_no_flag_after_the_flags_end() {
    if !installed("bash", "--version") {
        return;
    }
    let dir = workspace("workflint.bash", &script("bash"));
    let dir = dir.path();
    // Go's flag package ends the flags at the first argument that is not a flag, so this
    // `-o` would be a file name.
    assert_eq!(bash_complete(dir, "workflint ci.yaml -o"), NOTHING);
    assert_eq!(bash_complete(dir, "workflint -- -o"), NOTHING);
    assert!(bash_complete(dir, "workflint -shellcheck= -o").contains(&"-oneline".to_owned()));
    assert_eq!(bash_complete(dir, "workflint -shellcheck= x -o"), NOTHING);
    assert_eq!(bash_complete(dir, "workflint ci.yaml "), ["ci.yaml", "sub"]);
}

#[cfg(unix)]
#[test]
fn the_bash_script_completes_the_commands_and_the_shells() {
    if !installed("bash", "--version") {
        return;
    }
    let dir = workspace("workflint.bash", &script("bash"));
    let dir = dir.path();
    assert_eq!(
        bash_complete(dir, "workflint c"),
        ["check", "ci.yaml", "completion"]
    );
    assert_eq!(
        bash_complete(dir, "workflint completion "),
        ["bash", "fish", "powershell", "zsh"]
    );
    assert_eq!(bash_complete(dir, "workflint completion bash "), NOTHING);
    assert_eq!(bash_complete(dir, "workflint check c"), ["ci.yaml"]);
}

#[cfg(unix)]
#[test]
fn the_zsh_script_loads_and_registers_the_function() {
    if !installed("zsh", "--version") {
        return;
    }
    let dir = workspace("_workflint", &script("zsh"));
    let output = Command::new("zsh")
        .args([
            "-f",
            "-c",
            "fpath=($PWD $fpath); autoload -Uz compinit; compinit -u -D; \
             print -r -- $_comps[workflint]",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(lines(&output), ["_workflint"]);
}

/// The completions that the zsh script offers for `line`, with the cursor at the end. zsh runs
/// its completion system in a pseudo-terminal, where a wrapper of `compadd` prints each match
/// after a marker.
#[cfg(unix)]
fn zsh_complete(dir: &Path, line: &str) -> Vec<String> {
    let driver = r#"
        zmodload zsh/zpty || exit 1
        # Fail instead of waiting forever for the end marker.
        { sleep 30; kill $$ } >/dev/null 2>&1 &!
        local watchdog=$!
        zpty z zsh -f -i
        zpty -w z "fpath=($PWD \$fpath); autoload -Uz compinit; compinit -u -D"
        zpty -w z "bindkey '^I' complete-word"
        zpty -w z "_wf_end() { print -r -- '<e''nd>' }; comppostfuncs=(_wf_end exit)"
        zpty -w z 'compadd() {
            if [[ ${@[1,(i)(-|--)]} == *-(O|A|D)\ * ]]; then builtin compadd "$@"; return; fi
            local -a hits; builtin compadd -A hits "$@"
            local h; for h in $hits; do print -r -- "<m""atch>$IPREFIX$h"; done
            builtin compadd "$@"
        }'
        zpty -w z "$1"$'\t'
        local out line
        zpty -r -m z out '*<end>*'
        zpty -d z
        for line in ${(f)out//$'\r'/}; do
            [[ $line == *'<match>'* ]] && print -r -- ${line##*'<match>'}
        done
        kill $watchdog
        exit 0
    "#;
    let output = Command::new("zsh")
        .args(["-f", "-c", driver, "zsh", line])
        .current_dir(dir)
        .output()
        .unwrap();
    lines(&output)
}

#[cfg(unix)]
#[test]
fn the_zsh_script_completes_single_dash_flags_and_their_values() {
    if !installed("zsh", "--version") {
        return;
    }
    let dir = workspace("_workflint", &script("zsh"));
    let dir = dir.path();
    assert_eq!(zsh_complete(dir, "workflint -onel"), ["-oneline"]);
    assert_eq!(zsh_complete(dir, "workflint check -sil"), ["-silent"]);
    assert_eq!(
        zsh_complete(dir, "workflint -format "),
        ["github", "json", "jsonl", "sarif", "summary"]
    );
    assert_eq!(
        zsh_complete(dir, "workflint -format=js"),
        ["-format=json", "-format=jsonl"]
    );
    assert_eq!(
        zsh_complete(dir, "workflint c"),
        ["check", "ci.yaml", "completion"]
    );
    assert_eq!(
        zsh_complete(dir, "workflint completion "),
        ["bash", "fish", "powershell", "zsh"]
    );
    assert_eq!(zsh_complete(dir, "workflint ci.yaml -o"), NOTHING);
}

/// The completions that fish offers for `line`, without their descriptions.
fn fish_complete(dir: &Path, line: &str) -> Vec<String> {
    let output = Command::new("fish")
        .args([
            "--no-config",
            "-c",
            "source ./workflint.fish; and complete -C $argv[1]",
            line,
        ])
        .current_dir(dir)
        .output()
        .unwrap();
    lines(&output)
        .into_iter()
        .map(|line| line.split('\t').next().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn the_fish_script_completes_single_dash_flags_and_their_values() {
    if !installed("fish", "--version") {
        return;
    }
    let dir = workspace("workflint.fish", &script("fish"));
    let dir = dir.path();
    assert_eq!(fish_complete(dir, "workflint -onel"), ["-oneline"]);
    assert_eq!(fish_complete(dir, "workflint check -sil"), ["-silent"]);
    assert_eq!(
        fish_complete(dir, "workflint -format "),
        ["github", "json", "jsonl", "sarif", "summary"]
    );
    assert_eq!(
        fish_complete(dir, "workflint -format=js"),
        ["-format=json", "-format=jsonl"]
    );
    assert_eq!(
        fish_complete(dir, "workflint completion "),
        ["bash", "fish", "powershell", "zsh"]
    );
    assert_eq!(fish_complete(dir, "workflint ci.yaml -o"), NOTHING);
    assert_eq!(
        fish_complete(dir, "workflint c"),
        ["check", "ci.yaml", "completion"]
    );
}

/// The completions that PowerShell offers for `line`, with the cursor at the end.
fn powershell_complete(dir: &Path, line: &str) -> Vec<String> {
    let driver = "param($line) . ./workflint.ps1; \
                  (TabExpansion2 -inputScript $line -cursorColumn $line.Length).CompletionMatches \
                  | ForEach-Object { $_.CompletionText }";
    let output = Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!("& {{ {driver} }} '{}'", line.replace('\'', "''")))
        .current_dir(dir)
        .output()
        .unwrap();
    lines(&output)
}

#[test]
fn the_powershell_script_completes_single_dash_flags_and_their_values() {
    if !installed("pwsh", "-Version") {
        return;
    }
    let dir = workspace("workflint.ps1", &script("powershell"));
    let dir = dir.path();
    assert_eq!(powershell_complete(dir, "workflint -onel"), ["-oneline"]);
    assert_eq!(
        powershell_complete(dir, "workflint check -sil"),
        ["-silent"]
    );
    assert_eq!(
        powershell_complete(dir, "workflint -format "),
        ["github", "json", "jsonl", "sarif", "summary"]
    );
    assert_eq!(
        powershell_complete(dir, "workflint -format=js"),
        ["-format=json", "-format=jsonl"]
    );
    assert_eq!(
        powershell_complete(dir, "workflint completion "),
        ["bash", "fish", "powershell", "zsh"]
    );
    assert!(!powershell_complete(dir, "workflint ci.yaml -o").contains(&"-oneline".to_owned()));
}
