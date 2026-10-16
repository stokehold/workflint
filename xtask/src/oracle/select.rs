//! Chooses the fixtures and corpus files that the path arguments name.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};
use xtask::conformance::fixture::{Fixture, Suite};

/// The corpus files to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Corpus {
    None,
    All,
    /// File names in the corpus.
    Names(BTreeSet<String>),
}

/// What to compare.
#[derive(Debug)]
pub(crate) struct Selection {
    pub(crate) fixtures: Vec<Fixture>,
    pub(crate) corpus: Corpus,
}

/// Where the inputs live. Every path is absolute or relative to the same
/// directory, as `paths` are.
#[derive(Debug)]
pub(crate) struct Places<'a> {
    /// Every vendored fixture.
    pub(crate) fixtures: Vec<Fixture>,
    /// The vendored corpus archive.
    pub(crate) archive: &'a Path,
    /// The unpacked corpus, `.reference/realworld`.
    pub(crate) corpus: &'a Path,
}

/// Selects everything when `paths` is empty. Otherwise each path selects:
///
/// - the fixtures whose file, `.out` file, or project directory is the path
///   or lies under it, and the project that holds the path;
/// - the whole corpus if the archive or `.reference/realworld` lies under
///   the path;
/// - the corpus file with the path's name if the path lies under
///   `.reference/realworld`.
///
/// # Errors
///
/// Fails if a path selects nothing.
pub(crate) fn select(places: Places<'_>, paths: &[PathBuf]) -> Result<Selection> {
    if paths.is_empty() {
        return Ok(Selection {
            fixtures: places.fixtures,
            corpus: Corpus::All,
        });
    }
    let mut chosen = BTreeSet::new();
    let mut corpus = Corpus::None;
    for path in paths {
        let mut matched = false;
        for (index, fixture) in places.fixtures.iter().enumerate() {
            if names(fixture, path) {
                chosen.insert(index);
                matched = true;
            }
        }
        if places.archive.starts_with(path) || places.corpus.starts_with(path) {
            corpus = Corpus::All;
            matched = true;
        } else if let Ok(rest) = path.strip_prefix(places.corpus)
            && let Some(Component::Normal(name)) = rest.components().next()
        {
            let name = name.to_string_lossy().into_owned();
            match &mut corpus {
                Corpus::All => {}
                Corpus::Names(names) => {
                    names.insert(name);
                }
                Corpus::None => corpus = Corpus::Names(BTreeSet::from([name])),
            }
            matched = true;
        }
        if !matched {
            bail!(
                "{} is not a vendored fixture, a directory of fixtures, or part of the corpus",
                path.display()
            );
        }
    }
    let fixtures = places
        .fixtures
        .into_iter()
        .enumerate()
        .filter(|(index, _)| chosen.contains(index))
        .map(|(_, fixture)| fixture)
        .collect();
    Ok(Selection { fixtures, corpus })
}

/// Whether `path` names `fixture`.
fn names(fixture: &Fixture, path: &Path) -> bool {
    fixture.source.starts_with(path)
        || fixture
            .expected
            .as_ref()
            .is_some_and(|expected| expected.starts_with(path))
        || (fixture.suite == Suite::Projects && path.starts_with(&fixture.source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use xtask::conformance::fixture::{Origin, discover};

    fn testdata() -> PathBuf {
        Origin::Actionlint.root()
    }

    fn archive() -> PathBuf {
        testdata().join("realworld").join("dataset.zip")
    }

    fn corpus() -> PathBuf {
        PathBuf::from("/repo/.reference/realworld")
    }

    fn select_paths(paths: &[PathBuf]) -> Result<Selection> {
        let archive = archive();
        let corpus = corpus();
        select(
            Places {
                fixtures: discover(Origin::Actionlint).unwrap(),
                archive: &archive,
                corpus: &corpus,
            },
            paths,
        )
    }

    fn ids(selection: &Selection) -> Vec<&str> {
        selection
            .fixtures
            .iter()
            .map(|fixture| fixture.id.as_str())
            .collect()
    }

    #[test]
    fn no_paths_select_every_fixture_and_the_whole_corpus() {
        let selection = select_paths(&[]).unwrap();
        assert_eq!(selection.fixtures.len(), 260);
        assert_eq!(selection.corpus, Corpus::All);
    }

    #[test]
    fn a_fixture_file_selects_only_that_fixture() {
        let selection = select_paths(&[testdata().join("err/one_error.yaml")]).unwrap();
        assert_eq!(ids(&selection), ["err/one_error"]);
        assert_eq!(selection.corpus, Corpus::None);
    }

    #[test]
    fn an_out_file_selects_its_fixture() {
        let selection = select_paths(&[testdata().join("projects/issue173.out")]).unwrap();
        assert_eq!(ids(&selection), ["projects/issue173"]);
    }

    #[test]
    fn a_project_directory_or_a_file_in_it_selects_the_project() {
        let project = testdata().join("projects/paths_config");
        for path in [project.clone(), project.join("workflows/foo.yaml")] {
            let selection = select_paths(&[path]).unwrap();
            assert_eq!(ids(&selection), ["projects/paths_config"]);
        }
    }

    #[test]
    fn a_suite_directory_selects_its_fixtures() {
        let selection = select_paths(&[testdata().join("examples")]).unwrap();
        assert_eq!(selection.fixtures.len(), 51);
        assert_eq!(selection.corpus, Corpus::None);
    }

    #[test]
    fn the_archive_selects_the_whole_corpus() {
        let selection = select_paths(&[archive()]).unwrap();
        assert_eq!(ids(&selection), Vec::<&str>::new());
        assert_eq!(selection.corpus, Corpus::All);
    }

    #[test]
    fn a_file_in_the_unpacked_corpus_selects_that_file() {
        let selection = select_paths(&[
            corpus().join("a_b_.github_workflows_ci.yml"),
            corpus().join("c_d_.github_workflows_x.yaml"),
        ])
        .unwrap();
        assert_eq!(
            selection.corpus,
            Corpus::Names(BTreeSet::from([
                "a_b_.github_workflows_ci.yml".to_owned(),
                "c_d_.github_workflows_x.yaml".to_owned()
            ]))
        );
        assert_eq!(ids(&selection), Vec::<&str>::new());
    }

    #[test]
    fn a_path_that_selects_fixtures_again_is_accepted() {
        let file = testdata().join("err/one_error.yaml");
        let selection = select_paths(&[testdata().join("err"), file]).unwrap();
        assert_eq!(selection.fixtures.len(), 95);
    }

    #[test]
    fn the_test_data_directory_selects_everything() {
        let selection = select_paths(&[testdata()]).unwrap();
        assert_eq!(selection.fixtures.len(), 260);
        assert_eq!(selection.corpus, Corpus::All);
    }

    #[test]
    fn a_path_that_selects_nothing_is_an_error() {
        let path = testdata().join("config");
        let error = select_paths(&[path]).unwrap_err();
        assert!(format!("{error:#}").contains("config"), "{error:#}");
    }
}
