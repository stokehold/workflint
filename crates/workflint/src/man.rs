//! The man page, `docs/workflint.1`, from the header of `-help` and the flags of [`FLAGS`].
//!
//! The page is checked in. A test fails when it differs from [`page`], and rewrites it when
//! `WORKFLINT_UPDATE_MAN` is set.

use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;

use crate::flags::FLAGS;
use crate::usage::{Block, DESCRIPTION, DOCUMENTS, SYNOPSIS, docs_url};

/// The exit codes and what they mean, as in `docs/cli.md`.
const EXIT_STATUS: [(&str, &str); 4] = [
    (
        "0",
        "No errors, also when -ignore or the config dropped every error. -h, -help, -version, \
         completion, and a successful -init-config exit with 0 too.",
    ),
    ("1", "At least one error."),
    ("2", "The command line is invalid."),
    (
        "3",
        "A fatal error, such as a file or a config file that cannot be read, no workflow file \
         to lint, an invalid -ignore pattern or -format template, or a shellcheck that \
         -shellcheck names and that is not found.",
    ),
];

/// The environment variables that change what workflint does, as in `docs/cli.md`.
const ENVIRONMENT: [(&str, &str); 6] = [
    (
        "NO_COLOR",
        "When not empty, turns color off unless -color is given.",
    ),
    ("TERM", "dumb turns color off unless -color is given."),
    ("WORKFLINT_INSTALLED_FROM", "The second line of -version."),
    (
        "RAYON_NUM_THREADS",
        "The number of files linted at once. The output is the same for every value.",
    ),
    ("PATH", "Where -shellcheck looks up a bare name."),
    (
        "SHELLCHECK_OPTS",
        "Options that shellcheck reads. workflint passes its environment on to shellcheck.",
    ),
];

/// Escapes `text` for roff: backslashes, dashes, and a `.` or `'` that starts a line.
fn escape(text: &str) -> String {
    let text = text.replace('\\', r"\e").replace('-', r"\-");
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if line.starts_with(['.', '\'']) {
                format!(r"\&{line}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    lines.join("\n")
}

/// The man page in roff, with the `man` macros.
pub(crate) fn page() -> String {
    let mut page = String::from(
        ".\\\" Generated from the flags of workflint. Do not edit: run\n\
         .\\\" WORKFLINT_UPDATE_MAN=1 cargo nextest run -p workflint man:: to update it.\n\
         .TH WORKFLINT 1 \"\" \"workflint\" \"User Commands\"\n\
         .nh\n\
         .ad l\n\
         .SH NAME\n\
         workflint \\- linter for GitHub Actions workflow files\n\
         .SH SYNOPSIS\n",
    );
    for (i, line) in SYNOPSIS.iter().enumerate() {
        if i > 0 {
            page.push_str(".br\n");
        }
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        let _ = writeln!(page, ".B {command}\n{}", escape(rest));
    }
    page.push_str(".SH DESCRIPTION\n");
    for block in &DESCRIPTION {
        match block {
            Block::Text(paragraph) => {
                let _ = writeln!(page, ".PP\n{}", escape(paragraph));
            }
            Block::Example(commands) => {
                page.push_str(".PP\n.RS 4\n.nf\n");
                for command in *commands {
                    let _ = writeln!(page, "$ {}", escape(command));
                }
                page.push_str(".fi\n.RE\n");
            }
        }
    }
    page.push_str(".SH OPTIONS\n");
    let mut flags: Vec<(&str, &str, String)> = FLAGS
        .iter()
        .map(|flag| {
            let mut text = flag.usage.to_owned();
            if let Some(default) = flag.default.filter(|default| !default.is_empty()) {
                let _ = write!(text, " (default {})", quote(default));
            }
            (flag.name, flag.value_name, text)
        })
        .collect();
    flags.push((
        "help",
        "",
        "Print the usage to stderr and exit. -h is the same flag".to_owned(),
    ));
    flags.sort_by_key(|(name, _, _)| *name);
    for (name, value_name, text) in flags {
        page.push_str(".TP\n");
        if value_name.is_empty() {
            let _ = writeln!(page, ".B \\-{}", escape(name));
        } else {
            let _ = writeln!(page, ".BI \\-{} \" {value_name}\"", escape(name));
        }
        let _ = writeln!(page, "{}", escape(&text));
    }
    page.push_str(".SH EXIT STATUS\n");
    for (code, meaning) in EXIT_STATUS {
        let _ = writeln!(page, ".TP\n.B {code}\n{}", escape(meaning));
    }
    page.push_str(".SH ENVIRONMENT\n");
    for (name, effect) in ENVIRONMENT {
        let _ = writeln!(page, ".TP\n.B {name}\n{}", escape(effect));
    }
    page.push_str(".SH SEE ALSO\n");
    let docs = docs_url("main");
    for (i, (title, path)) in DOCUMENTS.iter().enumerate() {
        if i > 0 {
            page.push_str(".br\n");
        }
        let _ = writeln!(page, "{title}: {}", escape(&format!("{docs}/{path}")));
    }
    page
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::path::Path;
    use std::process::Command;

    use super::*;

    #[test]
    fn the_checked_in_man_page_is_up_to_date() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/workflint.1");
        let page = page();
        if std::env::var_os("WORKFLINT_UPDATE_MAN").is_some() {
            std::fs::write(&path, &page).unwrap();
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in.replace("\r\n", "\n") == page,
            "docs/workflint.1 is out of date. Run \
             `WORKFLINT_UPDATE_MAN=1 cargo nextest run -p workflint man::` to update it."
        );
    }

    #[test]
    fn the_man_page_lists_every_flag_that_help_lists() {
        let page = page();
        let help = crate::usage::text();
        let flags: Vec<&str> = help
            .lines()
            .filter_map(|line| line.strip_prefix("  -"))
            .filter(|line| !line.starts_with(' '))
            .map(|line| line.split(['\t', ' ']).next().unwrap_or(line))
            .collect();
        assert_eq!(flags.len(), FLAGS.len());
        for flag in flags {
            let escaped = flag.replace('-', r"\-");
            assert!(
                page.contains(&format!(".B \\-{escaped}\n"))
                    || page.contains(&format!(".BI \\-{escaped} \"")),
                "-{flag} is not in the man page"
            );
        }
    }

    #[test]
    fn dashes_backslashes_and_leading_dots_are_escaped() {
        assert_eq!(escape(r"-a\b"), r"\-a\eb");
        assert_eq!(escape(".x\n'y\nz.'"), "\\&.x\n\\&'y\nz.'");
    }

    /// groff reads the page without a warning, where groff is installed. A URL longer than the
    /// line cannot be adjusted, so that warning is off.
    #[test]
    fn groff_reads_the_man_page_without_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflint.1");
        std::fs::write(&path, page()).unwrap();
        let Ok(output) = Command::new("groff")
            .args(["-man", "-Tutf8", "-ww", "-Wbreak", "-z"])
            .arg(&path)
            .output()
        else {
            eprintln!("skipped: groff is not installed");
            return;
        };
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    }
}
