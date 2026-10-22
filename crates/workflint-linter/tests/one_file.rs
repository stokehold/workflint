//! A run with one file does not start rayon's global thread pool. Starting it reads the CPU
//! limits and spawns a thread per CPU, which costs more than linting a small workflow (#266).
//!
//! The test is alone in its binary, because any other test that lints two files starts the pool
//! for the whole process.

#![allow(clippy::unwrap_used)]

use std::fs;

use workflint_linter::{Input, Linter};

/// A workflow that uses a broken local action, so that its lint also goes through the replay of
/// the project's cache.
const USES_BROKEN_ACTION: &str = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/broken\n";

#[test]
fn one_file_is_linted_without_starting_the_global_thread_pool() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    let workflows = dir.path().join(".github").join("workflows");
    fs::create_dir_all(&workflows).unwrap();
    let action = dir.path().join(".github").join("actions").join("broken");
    fs::create_dir_all(&action).unwrap();
    fs::write(action.join("action.yml"), "name: [").unwrap();
    fs::write(workflows.join("a.yaml"), USES_BROKEN_ACTION).unwrap();

    let linter = Linter::new(dir.path().to_path_buf());
    let files = linter
        .lint(&[Input::file(".github/workflows/a.yaml")])
        .unwrap();
    assert_eq!(files.len(), 1);
    assert_ne!(files[0].diagnostics.len(), 0);

    // Building the global pool fails once anything has started it.
    assert!(
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build_global()
            .is_ok()
    );
}
