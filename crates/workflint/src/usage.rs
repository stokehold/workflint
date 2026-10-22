//! The text of `-help`: a header, then the flags. The man page has the same header text.

use std::fmt::Write as _;

use crate::flags;
use crate::version;

/// The command lines of `-help`.
pub(crate) const SYNOPSIS: [&str; 2] = [
    "workflint [check] [FLAGS] [FILES...] [-]",
    "workflint completion bash|zsh|fish|powershell",
];

/// A part of the description in the header of `-help`.
pub(crate) enum Block {
    /// A paragraph, as lines of at most 76 characters.
    Text(&'static str),
    /// Command lines, each run after `$ `.
    Example(&'static [&'static str]),
}

/// The description in the header of `-help`.
pub(crate) const DESCRIPTION: [Block; 15] = [
    Block::Text(
        "workflint is a linter for GitHub Actions workflow files, compatible with\n\
         actionlint.",
    ),
    Block::Text(
        "To check all YAML files in the current repository, run workflint without\n\
         arguments. It finds the nearest '.github/workflows' directory:",
    ),
    Block::Example(&["workflint"]),
    Block::Text("To check specific files, pass their paths as arguments:"),
    Block::Example(&["workflint file1.yaml file2.yaml"]),
    Block::Text(
        "A directory checks the YAML files under it, and workflint expands a quoted\n\
         glob itself:",
    ),
    Block::Example(&["workflint .github 'docs/**/*.yaml'"]),
    Block::Text(
        "To check text that is not saved in a file, such as the output of another\n\
         command, pass - as the only argument. workflint reads it from stdin:",
    ),
    Block::Example(&["workflint -"]),
    Block::Text(
        "To print errors as JSON, pass json or jsonl to the -format flag. The\n\
         flag also takes a Go template, as in actionlint:",
    ),
    Block::Example(&["workflint -format json", "workflint -format '{{json .}}'"]),
    Block::Text(
        "In GitHub Actions, -format github prints annotations, and -output writes\n\
         more formats to files in the same run:",
    ),
    Block::Example(&["workflint -format github -output sarif=workflint.sarif"]),
    Block::Text(
        "To complete flags in bash, zsh, fish, or PowerShell, load the script that\n\
         workflint completion prints for the shell:",
    ),
    Block::Example(&["eval \"$(workflint completion bash)\""]),
];

/// The documents that `-help` links to, with their paths under `docs/`.
pub(crate) const DOCUMENTS: [(&str, &str); 4] = [
    ("Command line", "cli.md"),
    ("Config file", "config.md"),
    ("Rules", "rules/README.md"),
    ("Differences", "differences.md"),
];

/// The URL of the `docs` directory at the Git ref `git_ref`.
pub(crate) fn docs_url(git_ref: &str) -> String {
    format!("https://github.com/stokehold/workflint/blob/{git_ref}/docs")
}

/// The text of `-help`, which an invalid command line also prints after its error. The
/// documentation links point at the files of the release for a release build, and at `main`
/// otherwise.
pub(crate) fn text() -> String {
    let mut text = format!("Usage: {}\n", SYNOPSIS[0]);
    for line in &SYNOPSIS[1..] {
        text.push_str("       ");
        text.push_str(line);
        text.push('\n');
    }
    for block in &DESCRIPTION {
        text.push('\n');
        match block {
            Block::Text(paragraph) => {
                for line in paragraph.lines() {
                    text.push_str("  ");
                    text.push_str(line);
                    text.push('\n');
                }
            }
            Block::Example(commands) => {
                for command in *commands {
                    text.push_str("    $ ");
                    text.push_str(command);
                    text.push('\n');
                }
            }
        }
    }
    text.push_str("\nDocuments:\n\n");
    let docs = docs_url(&version::docs_ref());
    let width = DOCUMENTS
        .iter()
        .map(|(title, _)| title.len())
        .max()
        .unwrap_or_default();
    for (title, path) in DOCUMENTS {
        let pad = width - title.len();
        let _ = writeln!(text, "  - {title}: {:pad$}{docs}/{path}", "");
    }
    text.push_str("\nFlags:\n");
    flags::write_defaults(&mut text);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_comes_before_the_flags() {
        let text = text();
        assert!(text.starts_with(
            "Usage: workflint [check] [FLAGS] [FILES...] [-]\n       \
             workflint completion bash|zsh|fish|powershell\n\n  workflint is a linter"
        ));
        assert!(text.contains("\nFlags:\n  -color\n"), "{text}");
    }

    #[test]
    fn paragraphs_are_indented_and_examples_start_with_a_prompt() {
        let text = text();
        assert!(
            text.contains(
                "\n\n  To check specific files, pass their paths as arguments:\n\n    \
                 $ workflint file1.yaml file2.yaml\n\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("    $ workflint -format json\n    $ workflint -format '{{json .}}'\n"),
            "{text}"
        );
    }

    #[test]
    fn paragraph_lines_fit_in_80_columns() {
        for block in &DESCRIPTION {
            if let Block::Text(paragraph) = block {
                for line in paragraph.lines() {
                    assert!(line.len() <= 76, "{line}");
                }
            }
        }
    }

    #[test]
    fn documentation_links_point_at_main_outside_a_release_build() {
        let text = text();
        assert!(
            text.contains(
                "\n  - Command line: https://github.com/stokehold/workflint/blob/main/docs/cli.md\n  \
                 - Config file:  https://github.com/stokehold/workflint/blob/main/docs/config.md\n"
            ),
            "{text}"
        );
    }
}
