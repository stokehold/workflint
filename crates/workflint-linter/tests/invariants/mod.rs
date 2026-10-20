//! The checks of the `lint` fuzz target, which `tests/fuzz_regressions.rs`
//! also runs on the files in `tests/fuzz-regressions/`.

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use workflint_diagnostics::{Color, render_default, render_oneline};
use workflint_linter::{Input, Linter};

/// The path of the linted workflow in the project.
pub(crate) const WORKFLOW: &str = ".github/workflows/fuzz.yaml";

/// Makes `root` a project with a local action, `./action`, so that a
/// `uses: ./action` in the input finds metadata.
pub(crate) fn write_project(root: &Path) {
    for dir in [".git", ".github/workflows", "action"] {
        fs::create_dir_all(root.join(dir)).expect("the project can be created");
    }
    fs::write(
        root.join("action/action.yml"),
        "name: a\ndescription: a\nruns:\n  using: node24\n  main: index.js\n",
    )
    .expect("the action can be written");
}

/// Writes `data` as a workflow of the project at `root`, which
/// [`write_project`] made, and lints it with the driver.
///
/// The lint must give no `internal-error` diagnostic, every span lies inside
/// the text, and both text formats render the diagnostics.
pub(crate) fn check_lint(root: &Path, data: &[u8]) {
    fs::write(root.join(WORKFLOW), data).expect("the workflow can be written");
    let linter = Linter::new(root.to_path_buf());
    let files = linter
        .lint(&[Input::file(WORKFLOW)])
        .expect("the workflow can be read");
    for linted in &files {
        assert!(!linted.has_internal_error(), "{:?}", linted.diagnostics);
        for diagnostic in &linted.diagnostics {
            let span = diagnostic.span;
            assert!(
                span.start <= span.end && span.end as usize <= linted.file.text().len(),
                "{diagnostic:?}"
            );
        }
        let mut out = Vec::new();
        render_default(&mut out, &linted.file, &linted.diagnostics, Color::On)
            .expect("rendering to memory succeeds");
        render_oneline(&mut out, &linted.file, &linted.diagnostics, Color::Off)
            .expect("rendering to memory succeeds");
    }
}
