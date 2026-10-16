//! Builds the tree for every YAML file in `.reference/`.
//!
//! The test needs the checkout that `cargo xtask reference` creates, so it is
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

#[test]
#[ignore = "needs .reference/ from cargo xtask reference"]
fn every_reference_file_parses_without_panicking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.reference");
    let mut files = Vec::new();
    yaml_files(&root.join("actionlint/testdata"), &mut files);
    yaml_files(&root.join("realworld"), &mut files);
    assert!(
        files.len() > 1_000,
        "found only {} files under {}",
        files.len(),
        root.display()
    );

    let mut with_document = 0;
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let file = SourceFile::new(FileId::new(0), path.display().to_string(), text).unwrap();
        let parsed = parse(&file);
        if let Some(document) = &parsed.document {
            with_document += 1;
            // Every span lies inside the file and on character boundaries.
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
    eprintln!(
        "parsed {} files, {with_document} with a document",
        files.len()
    );
}
