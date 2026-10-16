//! `cargo xtask yaml-parity`: compares the tree that `workflint-yaml` builds
//! for each file with the `yaml.Node` tree that go-yaml decodes, as the
//! reference does in `Parse` (`parse.go`). ADR 0005 requires the same
//! positions.
//!
//! `xtask/goyaml-dump` prints the go-yaml tree and [`dump`] prints the
//! `workflint-yaml` tree in the same format. [`compare`] decides which
//! differences count.

mod compare;
mod dump;
mod files;
mod goyaml;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::reference::{Layout, corpus, go};
use crate::vendor_testdata;

/// The number of files in the corpus archive.
const CORPUS_FILES: usize = 1_503;

/// The inputs that the vendored test data lacks, relative to the repository
/// root.
const EXTRA_INPUTS: &str = "tests/yaml-parity";

/// Compares the trees of `paths`, or of the default inputs if `paths` is
/// empty, prints a report, and fails if a file differs.
pub(crate) fn run(layout: &Layout, paths: &[PathBuf]) -> Result<()> {
    let (files, corpus) = if paths.is_empty() {
        let (files, corpus) = default_inputs(layout)?;
        (files, Some(corpus))
    } else {
        (files::collect(paths)?, None)
    };
    if files.is_empty() {
        bail!("found no files to compare");
    }

    let work = layout.root().join("target").join("yaml-parity");
    let binary = work.join(format!("goyaml-dump{}", std::env::consts::EXE_SUFFIX));
    let go_version = go::read_pinned_version(&layout.go_version_file())?;
    goyaml::build(
        &OsString::from("go"),
        &layout.root().join("xtask").join("goyaml-dump"),
        &binary,
        &go_version,
    )?;
    let go_dumps = goyaml::run(&binary, &files)?;

    let mut report = compare::Report::default();
    let mut corpus_verdicts = BTreeMap::new();
    for (file, go_dump) in files.iter().zip(&go_dumps) {
        let bytes = fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
        let verdict = report.add(
            &layout.display(file).to_string(),
            go_dump,
            &dump::dump(&bytes),
        );
        let corpus_name = corpus
            .as_deref()
            .and_then(|corpus| file.strip_prefix(corpus).ok());
        if let Some(name) = corpus_name {
            corpus_verdicts.insert(name.to_string_lossy().into_owned(), verdict);
        }
    }
    print!("{}", report.render());
    match report.differing() {
        0 => {}
        1 => bail!("1 file differs"),
        n => bail!("{n} files differ"),
    }
    if corpus.is_some() {
        check_panicked_in_research(&corpus_verdicts)?;
    }
    Ok(())
}

/// The vendored test data, the corpus unpacked from its vendored archive
/// into `target/yaml-parity/realworld`, and [`EXTRA_INPUTS`]. Returns the
/// directory of the corpus too.
fn default_inputs(layout: &Layout) -> Result<(Vec<PathBuf>, PathBuf)> {
    let testdata = layout
        .root()
        .join(vendor_testdata::DESTINATION)
        .join("testdata");
    let archive = testdata.join("realworld").join("dataset.zip");
    let corpus = layout
        .root()
        .join("target")
        .join("yaml-parity")
        .join("realworld");
    if let Some(parent) = corpus.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let count = unpack_corpus(&archive, &corpus)?;
    if count != CORPUS_FILES {
        bail!(
            "{} holds {count} files, but {CORPUS_FILES} were expected",
            archive.display()
        );
    }
    let extra = layout.root().join(EXTRA_INPUTS);
    let inputs = files::collect(&[testdata, corpus.clone(), extra])?;
    let in_corpus = inputs
        .iter()
        .filter(|path| path.starts_with(&corpus))
        .count();
    if in_corpus != CORPUS_FILES {
        bail!(
            "{} holds {in_corpus} YAML files, but {CORPUS_FILES} were expected",
            corpus.display()
        );
    }
    Ok((inputs, corpus))
}

/// Unpacks `archive` into `target` and returns the number of files.
///
/// `target` holds only a copy of the archive, so a copy that does not match
/// the archive is removed and unpacked again. A CI build cache can restore
/// the directory without its files.
fn unpack_corpus(archive: &Path, target: &Path) -> Result<usize> {
    if let Ok((_, count)) = corpus::ensure(archive, target) {
        return Ok(count);
    }
    if target.exists() {
        fs::remove_dir_all(target)
            .with_context(|| format!("cannot remove {}", target.display()))?;
    }
    let (_, count) = corpus::ensure(archive, target)?;
    Ok(count)
}

/// The corpus files on which libyaml-safer 0.3.0 panicked in
/// `docs/research/01-parsing-ast.md`, section 9.10. Each ends in a literal
/// block scalar without a trailing newline.
const PANICKED_IN_RESEARCH: [&str; 9] = [
    "3b1b_manim_.github_workflows_publish.yml",
    "ReactiveCocoa_ReactiveCocoa_.github_workflows_master.yml",
    "alibaba_arthas_.github_workflows_test.yaml",
    "beego_beego_.github_workflows_changelog.yml",
    "felixrieseberg_windows95_.github_workflows_build.yml",
    "huggingface_transformers_.github_workflows_stale.yml",
    "jorgebucaran_hyperapp_.github_workflows_ci.yml",
    "libgdx_libgdx_.github_workflows_build-pullrequest.yml",
    "libgdx_libgdx_.github_workflows_build-snapshot.yml",
];

/// Fails unless each file in [`PANICKED_IN_RESEARCH`] has the verdict
/// [`Verdict::Identical`](compare::Verdict::Identical) in `corpus`, which maps
/// corpus file names to their verdicts.
fn check_panicked_in_research(corpus: &BTreeMap<String, compare::Verdict>) -> Result<()> {
    for name in PANICKED_IN_RESEARCH {
        match corpus.get(name) {
            Some(compare::Verdict::Identical) => {}
            Some(_) => bail!("{name} panicked libyaml-safer 0.3.0 and must now be identical"),
            None => bail!("{name} panicked libyaml-safer 0.3.0 but was not compared"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;
    use compare::Verdict;

    fn write_archive(path: &Path, files: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, contents) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn a_stale_corpus_in_target_is_unpacked_again() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("dataset.zip");
        write_archive(
            &archive,
            &[("a.yml", "on: push\n"), ("b.yml", "on: pull_request\n")],
        );
        let target = dir.path().join("realworld");
        // A build cache can restore the directory without its files.
        fs::create_dir(&target).unwrap();
        fs::write(target.join("stale.yml"), "x").unwrap();

        assert_eq!(unpack_corpus(&archive, &target).unwrap(), 2);
        assert_eq!(
            fs::read_to_string(target.join("b.yml")).unwrap(),
            "on: pull_request\n"
        );
        assert!(!target.join("stale.yml").exists());
    }

    fn all_identical() -> BTreeMap<String, Verdict> {
        PANICKED_IN_RESEARCH
            .iter()
            .map(|name| ((*name).to_owned(), Verdict::Identical))
            .collect()
    }

    #[test]
    fn files_that_panicked_in_the_research_pass_when_identical() {
        check_panicked_in_research(&all_identical()).unwrap();
    }

    #[test]
    fn a_file_that_panicked_in_the_research_must_not_fail_in_both() {
        let mut corpus = all_identical();
        corpus.insert(PANICKED_IN_RESEARCH[3].to_owned(), Verdict::BothFailed);
        let error = check_panicked_in_research(&corpus).unwrap_err();
        assert!(
            format!("{error:#}").contains(PANICKED_IN_RESEARCH[3]),
            "{error:#}"
        );
    }

    #[test]
    fn a_file_that_panicked_in_the_research_must_be_compared() {
        let mut corpus = all_identical();
        corpus.remove(PANICKED_IN_RESEARCH[0]);
        let error = check_panicked_in_research(&corpus).unwrap_err();
        assert!(
            format!("{error:#}").contains(PANICKED_IN_RESEARCH[0]),
            "{error:#}"
        );
    }
}
