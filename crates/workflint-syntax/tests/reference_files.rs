//! Runs the check of the `syntax` fuzz target, which includes the check of
//! the `metadata` target, on every YAML file of the reference.
//!
//! The first test reads the fixtures vendored in
//! `tests/conformance/actionlint/testdata/`. The second reads the corpus in
//! `.reference/realworld/`, which `cargo xtask reference` creates, so it is
//! ignored by default. Run it with
//! `cargo nextest run -p workflint-syntax --run-ignored all -E 'binary(reference_files)'`.

mod invariants;

use std::path::{Path, PathBuf};

fn yaml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            yaml_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "yaml" || ext == "yml")
        {
            out.push(path);
        }
    }
}

/// Runs the check on every YAML file under `dir`. Returns the number of
/// files.
fn parse_all(dir: &Path) -> usize {
    let mut files = Vec::new();
    yaml_files(dir, &mut files);
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        // The check's messages do not name the file, so print it first. The
        // test runner shows the output of a failed test.
        println!("{}", path.display());
        invariants::check_parse(&String::from_utf8_lossy(&bytes));
    }
    files.len()
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_reference_fixture_passes_the_fuzz_check() {
    let count = parse_all(&repository().join("tests/conformance/actionlint/testdata"));
    assert!(count > 300, "found only {count} fixtures");
}

#[test]
#[ignore = "needs .reference/realworld from cargo xtask reference"]
fn every_corpus_file_passes_the_fuzz_check() {
    let count = parse_all(&repository().join(".reference/realworld"));
    assert!(count > 1000, "found only {count} corpus files");
}
