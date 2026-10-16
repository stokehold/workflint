//! `cargo xtask reference`: recreates the parts of `.reference/` that the
//! conformance tooling needs. ADR 0004 defines the reference.
//!
//! The pins live here so that other subcommands reuse them, and each step is a
//! function that they can call. The command touches only
//! `.reference/actionlint`, `.reference/bin`, and `.reference/realworld`.

mod binary;
mod checkout;
mod corpus;
mod go;

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// The repository of the reference.
pub(crate) const REPOSITORY_URL: &str = "https://github.com/rhysd/actionlint.git";

/// The reference commit (ADR 0004).
pub(crate) const COMMIT: &str = "011a6d15e749bb3f2d771eed9c7aa0e7e3e10ee7";

/// The file that pins the Go version, relative to the repository root.
/// `actions/setup-go` reads it through `go-version-file`.
pub(crate) const GO_VERSION_FILE: &str = ".go-version";

/// What a step did.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Outcome {
    /// The result did not exist, and the step made it.
    Created,
    /// The result existed but was stale, and the step replaced it.
    Updated,
    /// The result was already up to date, and the step changed nothing.
    UpToDate,
}

/// The paths under `.reference/` that the command manages.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    root: PathBuf,
}

impl Layout {
    /// The layout of this repository.
    pub(crate) fn repository() -> Self {
        Self::at(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
    }

    /// The layout of the repository at `root`.
    pub(crate) fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// The actionlint checkout, `.reference/actionlint`.
    pub(crate) fn checkout(&self) -> PathBuf {
        self.root.join(".reference").join("actionlint")
    }

    /// The reference binary, `.reference/bin/actionlint`, with `.exe` on
    /// Windows.
    pub(crate) fn binary(&self) -> PathBuf {
        self.root
            .join(".reference")
            .join("bin")
            .join(format!("actionlint{}", env::consts::EXE_SUFFIX))
    }

    /// The unpacked corpus, `.reference/realworld`.
    pub(crate) fn corpus(&self) -> PathBuf {
        self.root.join(".reference").join("realworld")
    }

    /// The corpus archive inside the checkout.
    pub(crate) fn corpus_archive(&self) -> PathBuf {
        self.checkout()
            .join("testdata")
            .join("realworld")
            .join("dataset.zip")
    }

    /// The file that pins the Go version.
    pub(crate) fn go_version_file(&self) -> PathBuf {
        self.root.join(GO_VERSION_FILE)
    }

    /// `path` relative to the repository root, for messages.
    fn display<'a>(&self, path: &'a Path) -> std::path::Display<'a> {
        path.strip_prefix(&self.root).unwrap_or(path).display()
    }
}

/// Step 1: makes `.reference/actionlint` a clean checkout of [`COMMIT`].
pub(crate) fn ensure_checkout(layout: &Layout) -> Result<Outcome> {
    checkout::ensure(&layout.checkout(), REPOSITORY_URL, COMMIT)
}

/// Step 2: builds `.reference/bin/actionlint` from the checkout with the
/// pinned Go version. Call [`ensure_checkout`] first.
pub(crate) fn ensure_binary(layout: &Layout) -> Result<Outcome> {
    let go_version = go::read_pinned_version(&layout.go_version_file())?;
    binary::ensure(
        &layout.checkout(),
        &layout.binary(),
        COMMIT,
        &OsString::from("go"),
        &go_version,
    )
}

/// Step 3: unpacks the corpus into `.reference/realworld`. Call
/// [`ensure_checkout`] first. Returns the number of files in the corpus too.
pub(crate) fn ensure_corpus(layout: &Layout) -> Result<(Outcome, usize)> {
    corpus::ensure(&layout.corpus_archive(), &layout.corpus())
}

/// Runs the three steps and prints one line for each.
pub(crate) fn run(layout: &Layout) -> Result<()> {
    let short = &COMMIT[..7];

    let checkout = layout.checkout();
    let path = layout.display(&checkout);
    match ensure_checkout(layout)? {
        Outcome::Created => println!("checkout: cloned {path} at {short}"),
        Outcome::Updated => println!("checkout: moved {path} to {short}"),
        Outcome::UpToDate => println!("checkout: {path} is up to date at {short}"),
    }

    let binary = layout.binary();
    let path = layout.display(&binary);
    match ensure_binary(layout)? {
        Outcome::Created | Outcome::Updated => println!("binary: built {path}"),
        Outcome::UpToDate => println!("binary: {path} is up to date"),
    }

    let corpus = layout.corpus();
    let path = layout.display(&corpus);
    match ensure_corpus(layout)? {
        (Outcome::Created | Outcome::Updated, files) => {
            println!("corpus: unpacked {files} files into {path}");
        }
        (Outcome::UpToDate, files) => {
            println!("corpus: {path} is up to date with {files} files");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_go_version_file_holds_a_version() {
        let version = go::read_pinned_version(&Layout::repository().go_version_file()).unwrap();
        assert!(version.starts_with("go1."), "{version}");
    }

    #[test]
    fn layout_stays_inside_the_reference_directory() {
        let layout = Layout::at(Path::new("repo"));
        let reference = Path::new("repo").join(".reference");
        assert_eq!(layout.checkout(), reference.join("actionlint"));
        assert_eq!(layout.corpus(), reference.join("realworld"));
        assert!(layout.binary().starts_with(reference.join("bin")));
    }

    #[test]
    fn commit_is_a_full_hash() {
        assert_eq!(COMMIT.len(), 40);
        assert!(COMMIT.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
