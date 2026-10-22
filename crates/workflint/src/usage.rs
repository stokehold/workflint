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

  A directory checks the YAML files under it, and workflint expands a quoted
  glob itself:

    $ workflint .github 'docs/**/*.yaml'

  To check text that is not saved in a file, such as the output of another
  command, pass - as the only argument. workflint reads it from stdin:

    $ workflint -

  To print errors as JSON, pass json or jsonl to the -format flag. The
  flag also takes a Go template, as in actionlint:

    $ workflint -format json
    $ workflint -format '{{{{json .}}}}'

  In GitHub Actions, -format github prints annotations, and -output writes
  more formats to files in the same run:

    $ workflint -format github -output sarif=workflint.sarif

Documents:

  - Command line: {docs}/cli.md
  - Config file:  {docs}/config.md
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
