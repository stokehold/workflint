//! `cargo xtask fuzz-seeds`: writes seed inputs into the corpus directory of
//! each fuzz target in `fuzz/`, so that `cargo fuzz run` starts from real
//! workflows instead of an empty input.
//!
//! The seeds stay out of git. They are copies of files that git already has:
//! the YAML files of the vendored reference test data, every workflow in its
//! corpus archive, the inputs of the YAML test suite, and the fuzz regression
//! inputs of `workflint-yaml`.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

/// The fuzz targets in `fuzz/fuzz_targets/`. Each gets every seed.
pub(crate) const TARGETS: &[&str] = &["render", "yaml_events", "yaml_tree"];

/// The vendored reference test data, relative to the repository root.
const FIXTURES: &str = "tests/conformance/actionlint/testdata";

/// The reference's corpus archive, relative to the repository root.
const ARCHIVE: &str = "tests/conformance/actionlint/testdata/realworld/dataset.zip";

/// The YAML test suite cases, relative to the repository root.
const YAML_TEST_SUITE: &str = "crates/workflint-yaml/tests/yaml-test-suite";

/// The fuzz regression inputs, relative to the repository root.
const REGRESSIONS: &str = "crates/workflint-yaml/tests/fuzz-regressions";

/// The corpus directories, relative to the repository root. `cargo fuzz run
/// <target>` reads `fuzz/corpus/<target>/` by default.
const CORPUS: &str = "fuzz/corpus";

/// One seed input: its file name in the corpus directory and its contents.
#[derive(Debug, PartialEq, Eq)]
struct Seed {
    name: String,
    contents: Vec<u8>,
}

/// Collects the seeds under `root` and writes them into the corpus directory
/// of every target in [`TARGETS`].
pub(crate) fn run(root: &Path) -> Result<()> {
    let seeds = collect(root)?;
    for target in TARGETS {
        let dir = root.join(CORPUS).join(target);
        write(&dir, &seeds)?;
        println!(
            "fuzz-seeds: wrote {} seeds to {CORPUS}/{target}",
            seeds.len()
        );
    }
    Ok(())
}

/// Reads the seeds from the sources under `root`, in a fixed order. Fails if
/// a source is missing or two seeds would get the same file name.
fn collect(root: &Path) -> Result<Vec<Seed>> {
    let mut seeds = Vec::new();
    for path in files_under(&root.join(FIXTURES))? {
        if is_yaml(&path) {
            seeds.push(seed_from_file("fixture", &root.join(FIXTURES), &path)?);
        }
    }
    seeds.extend(archive_seeds(&root.join(ARCHIVE))?);
    for path in files_under(&root.join(YAML_TEST_SUITE))? {
        if path.file_name().is_some_and(|name| name == "in.yaml") {
            seeds.push(seed_from_file(
                "yaml-test-suite",
                &root.join(YAML_TEST_SUITE),
                &path,
            )?);
        }
    }
    for path in files_under(&root.join(REGRESSIONS))? {
        seeds.push(seed_from_file(
            "regression",
            &root.join(REGRESSIONS),
            &path,
        )?);
    }
    let mut names: Vec<&str> = seeds.iter().map(|seed| seed.name.as_str()).collect();
    names.sort_unstable();
    if let Some([name, _]) = names.array_windows().find(|[a, b]| a == b) {
        bail!("two seeds are named {name}");
    }
    Ok(seeds)
}

/// Writes every seed into `dir`, creating it if needed. Files already in
/// `dir`, such as inputs the fuzzer found, stay.
fn write(dir: &Path, seeds: &[Seed]) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    for seed in seeds {
        let path = dir.join(&seed.name);
        fs::write(&path, &seed.contents)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
}

fn is_yaml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "yaml" || ext == "yml")
}

/// Every file under `dir`, recursively, sorted by path.
fn files_under(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries =
            fs::read_dir(&dir).with_context(|| format!("cannot read {}", dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("cannot read {}", dir.display()))?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Reads `path`, which is under `base`, as a seed named after `prefix` and
/// its path relative to `base`.
fn seed_from_file(prefix: &str, base: &Path, path: &Path) -> Result<Seed> {
    let relative = path
        .strip_prefix(base)
        .with_context(|| format!("{} is not under {}", path.display(), base.display()))?;
    let parts: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    let contents = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(Seed {
        name: seed_name(prefix, &parts),
        contents,
    })
}

/// Every file in the zip archive at `path`, as seeds named `realworld-<name>`.
fn archive_seeds(path: &Path) -> Result<Vec<Seed>> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut zip =
        ZipArchive::new(file).with_context(|| format!("cannot read {}", path.display()))?;
    let mut seeds = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.into_owned();
        let parts: Vec<String> = name.split('/').map(str::to_owned).collect();
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .with_context(|| format!("cannot unpack {name} from {}", path.display()))?;
        seeds.push(Seed {
            name: seed_name("realworld", &parts),
            contents,
        });
    }
    seeds.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(seeds)
}

/// Joins `prefix` and the path components `parts` with `-` into one file
/// name. Characters other than ASCII letters, digits, `.`, `_`, and `-`
/// become `_`, so the name is valid on every OS.
fn seed_name(prefix: &str, parts: &[String]) -> String {
    let mut name = prefix.to_owned();
    for part in parts {
        name.push('-');
        name.extend(part.chars().map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        }));
    }
    name
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn put(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn write_archive(root: &Path, files: &[(&str, &str)]) {
        let path = root.join(ARCHIVE);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, contents) in files {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    /// A repository with one file of each kind that the command reads or
    /// skips.
    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, &format!("{FIXTURES}/examples/a.yaml"), "on: push\n");
        put(root, &format!("{FIXTURES}/examples/a.out"), "output\n");
        put(root, &format!("{FIXTURES}/b.yml"), "on: pull_request\n");
        write_archive(root, &[("x_y_.github_workflows_ci.yml", "jobs: {}\n")]);
        put(root, &format!("{YAML_TEST_SUITE}/229Q/in.yaml"), "- a\n");
        put(
            root,
            &format!("{YAML_TEST_SUITE}/229Q/test.event"),
            "+STR\n",
        );
        put(root, &format!("{REGRESSIONS}/tab-after-key"), "a:\t\n");
        dir
    }

    fn corpus(root: &Path, target: &str) -> Vec<(String, String)> {
        let mut files: Vec<(String, String)> = fs::read_dir(root.join(CORPUS).join(target))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().into_string().unwrap(),
                    fs::read_to_string(entry.path()).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    }

    fn expected() -> Vec<(String, String)> {
        [
            ("fixture-b.yml", "on: pull_request\n"),
            ("fixture-examples-a.yaml", "on: push\n"),
            ("realworld-x_y_.github_workflows_ci.yml", "jobs: {}\n"),
            ("regression-tab-after-key", "a:\t\n"),
            ("yaml-test-suite-229Q-in.yaml", "- a\n"),
        ]
        .map(|(name, contents)| (name.to_owned(), contents.to_owned()))
        .to_vec()
    }

    #[test]
    fn every_target_gets_the_yaml_fixtures_the_archive_the_suite_and_the_regressions() {
        let dir = repository();
        run(dir.path()).unwrap();
        for target in TARGETS {
            assert_eq!(corpus(dir.path(), target), expected(), "{target}");
        }
    }

    #[test]
    fn inputs_the_fuzzer_found_stay_in_the_corpus() {
        let dir = repository();
        put(
            dir.path(),
            &format!("{CORPUS}/yaml_tree/0123abcd"),
            "found\n",
        );
        run(dir.path()).unwrap();
        let files = corpus(dir.path(), "yaml_tree");
        assert!(files.contains(&("0123abcd".to_owned(), "found\n".to_owned())));
        assert_eq!(files.len(), expected().len() + 1);
    }

    #[test]
    fn a_second_run_writes_the_same_files() {
        let dir = repository();
        run(dir.path()).unwrap();
        run(dir.path()).unwrap();
        assert_eq!(corpus(dir.path(), "yaml_events"), expected());
    }

    #[test]
    fn missing_archive_is_an_error() {
        let dir = repository();
        fs::remove_file(dir.path().join(ARCHIVE)).unwrap();
        let error = run(dir.path()).unwrap_err();
        assert!(format!("{error:#}").contains("dataset.zip"), "{error:#}");
    }

    #[test]
    fn archive_entries_in_directories_get_flat_names() {
        let dir = repository();
        write_archive(dir.path(), &[("sub dir/ci.yml", "on: push\n")]);
        let seeds = archive_seeds(&dir.path().join(ARCHIVE)).unwrap();
        assert_eq!(
            seeds,
            [Seed {
                name: "realworld-sub_dir-ci.yml".to_owned(),
                contents: b"on: push\n".to_vec(),
            }]
        );
    }

    #[test]
    fn two_seeds_with_the_same_name_are_an_error() {
        let dir = repository();
        put(dir.path(), &format!("{FIXTURES}/examples-a.yaml"), "x\n");
        let error = collect(dir.path()).unwrap_err();
        assert!(
            format!("{error}").contains("fixture-examples-a.yaml"),
            "{error}"
        );
    }
}
