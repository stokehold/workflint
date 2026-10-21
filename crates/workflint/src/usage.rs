//! The text of `-help`: a header, then the flags.

use crate::flags;
use crate::version;

/// The text of `-help`, which an invalid command line also prints after its error. The
/// documentation links point at the files of the release for a release build, and at `main`
/// otherwise.
pub(crate) fn text() -> String {
    let docs = format!(
        "https://github.com/stokehold/workflint/blob/{}/docs",
        version::docs_ref()
    );
    let mut text = format!(
        "Usage: workflint [check] [FLAGS] [FILES...] [-]

  workflint is a linter for GitHub Actions workflow files, compatible with
  actionlint.

  To check all YAML files in the current repository, run workflint without
  arguments. It finds the nearest '.github/workflows' directory:

    $ workflint

  To check specific files, pass their paths as arguments:

    $ workflint file1.yaml file2.yaml

  To check text that is not saved in a file, such as the output of another
  command, pass - as the only argument. workflint reads it from stdin:

    $ workflint -

  To print errors as JSON, use the -format flag. It formats error messages
  with Go template syntax:

    $ workflint -format '{{{{json .}}}}'

  This version does not support the -format flag yet.

Documents:

  - Command line: {docs}/cli.md
  - Rules:        {docs}/rules/README.md
  - Differences:  {docs}/differences.md

Flags:
"
    );
    flags::write_defaults(&mut text);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_comes_before_the_flags() {
        let text = text();
        assert!(text.starts_with("Usage: workflint [check] [FLAGS] [FILES...] [-]\n"));
        assert!(text.contains("\nFlags:\n  -color\n"), "{text}");
    }

    #[test]
    fn documentation_links_point_at_main_outside_a_release_build() {
        assert!(
            text().contains("https://github.com/stokehold/workflint/blob/main/docs/cli.md"),
            "{}",
            text()
        );
    }
}
