//! Checks that every published crate carries the repository's `LICENSE` and
//! `NOTICE`, so the MIT notice of the code ported from actionlint travels
//! with each package (ADR 0017).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The directories under `crates/` whose manifest does not set
/// `publish = false`, sorted.
fn published_crates() -> Vec<PathBuf> {
    let mut crates: Vec<PathBuf> = fs::read_dir(repository().join("crates"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("Cargo.toml").is_file())
        .filter(|path| {
            let manifest: toml::Table =
                toml::from_str(&fs::read_to_string(path.join("Cargo.toml")).unwrap()).unwrap();
            manifest["package"]
                .get("publish")
                .and_then(toml::Value::as_bool)
                != Some(false)
        })
        .collect();
    crates.sort();
    crates
}

#[test]
fn every_published_crate_includes_the_license_and_the_notice() {
    let crates = published_crates();
    assert!(crates.len() >= 6, "found only {crates:?}");
    for path in crates {
        let manifest: toml::Table =
            toml::from_str(&fs::read_to_string(path.join("Cargo.toml")).unwrap()).unwrap();
        // Without `include`, cargo packages every file in the directory.
        if let Some(include) = manifest["package"].get("include") {
            let include: Vec<&str> = include
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect();
            for file in ["/LICENSE", "/NOTICE"] {
                assert!(
                    include.contains(&file),
                    "{} does not include {file}",
                    path.display()
                );
            }
        }
        for file in ["LICENSE", "NOTICE"] {
            assert!(path.join(file).exists(), "{} has no {file}", path.display());
        }
    }
}

/// Each crate's files are links to the repository's. A checkout without
/// symbolic links, as Git on Windows makes by default, has a text file with
/// the target instead, so the check runs on Unix only.
#[cfg(unix)]
#[test]
fn the_license_and_the_notice_of_each_crate_match_the_root_files() {
    for path in published_crates() {
        for file in ["LICENSE", "NOTICE"] {
            assert_eq!(
                fs::read_to_string(path.join(file)).unwrap(),
                fs::read_to_string(repository().join(file)).unwrap(),
                "{} differs from /{file}",
                path.join(file).display()
            );
        }
    }
}
