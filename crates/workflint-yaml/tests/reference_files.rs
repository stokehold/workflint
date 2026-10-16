//! Builds the tree for every YAML file of the reference.
//!
//! The first test reads the fixtures vendored in
//! `tests/conformance/actionlint/testdata/`. The second reads the corpus in
//! `.reference/realworld/`, which `cargo xtask reference` creates, so it is
//! ignored by default. Run it with
//! `cargo nextest run -p workflint-yaml --run-ignored all reference_files`.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::parse;

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

/// Parses every YAML file under `dir` and checks that the tree's spans lie
/// on character boundaries of the file. Returns the number of files.
fn parse_all(dir: &Path) -> usize {
    let mut files = Vec::new();
    yaml_files(dir, &mut files);
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let file = SourceFile::new(FileId::new(0), path.display().to_string(), text).unwrap();
        let parsed = parse(&file);
        if let Some(document) = &parsed.document {
            for (_, node) in document.nodes() {
                let (start, end) = (node.span.start as usize, node.span.end as usize);
                assert!(
                    start <= end && file.text().get(start..end).is_some(),
                    "{}: bad span {start}..{end}",
                    path.display()
                );
            }
            let _ = document.dump(&file);
        }
    }
    files.len()
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_reference_fixture_parses_without_panicking() {
    let count = parse_all(&repository().join("tests/conformance/actionlint/testdata"));
    assert!(count > 300, "found only {count} fixtures");
}

#[test]
#[ignore = "needs .reference/ from cargo xtask reference"]
fn every_corpus_file_parses_without_panicking() {
    let count = parse_all(&repository().join(".reference/realworld"));
    assert_eq!(count, 1_503);
}
