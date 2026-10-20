//! `cargo xtask fuzz-seeds`: writes seed inputs into the corpus directory of
//! each fuzz target in `fuzz/`, so that `cargo fuzz run` starts from real
//! workflows instead of an empty input.
//!
//! The seeds stay out of git. They are copies of files that git already has.
//! The targets that read workflows get the YAML files of the vendored
//! reference test data, every workflow in its corpus archive, the inputs of
//! the YAML test suite, and the fuzz regression inputs of `workflint-yaml`,
//! `workflint-syntax`, and `workflint-diagnostics`.
//! The targets that read the text after `${{` get each line of the
//! reference's `expressions.txt` followed by `}}`, the text after each `${{`
//! in the YAML files of the vendored test data, and the fuzz regression inputs
//! of `workflint-expr`. The target that reads a JSON literal gets the value of
//! each `fromJSON('…')` string literal in `expressions.txt` and in the YAML
//! files of the vendored test data, and the JSON fuzz regression inputs of
//! `workflint-expr`.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

/// The fuzz targets in `fuzz/fuzz_targets/` that read workflows. Each gets
/// every workflow seed.
pub(crate) const TARGETS: &[&str] = &[
    "glob",
    "lint",
    "metadata",
    "render",
    "syntax",
    "yaml_events",
    "yaml_tree",
];

/// The fuzz targets in `fuzz/fuzz_targets/` that read the text after `${{`.
/// Each gets every expression seed.
pub(crate) const EXPRESSION_TARGETS: &[&str] = &["expr_check", "expr_lex", "expr_parse"];

/// The fuzz targets in `fuzz/fuzz_targets/` that read the text of a JSON
/// literal. Each gets every JSON seed.
pub(crate) const JSON_TARGETS: &[&str] = &["expr_json"];

/// The vendored reference test data, relative to the repository root.
const FIXTURES: &str = "tests/conformance/actionlint/testdata";

/// The reference's corpus archive, relative to the repository root.
const ARCHIVE: &str = "tests/conformance/actionlint/testdata/realworld/dataset.zip";

/// The YAML test suite cases, relative to the repository root.
const YAML_TEST_SUITE: &str = "crates/workflint-yaml/tests/yaml-test-suite";

/// The fuzz regression inputs of the targets that read workflows, relative
/// to the repository root, each with the prefix of its seed names.
const REGRESSIONS: &[(&str, &str)] = &[
    ("regression", "crates/workflint-yaml/tests/fuzz-regressions"),
    (
        "syntax-regression",
        "crates/workflint-syntax/tests/fuzz-regressions",
    ),
    (
        "render-regression",
        "crates/workflint-diagnostics/tests/fuzz-regressions",
    ),
];

/// The reference's real-world expressions, one per line, relative to the
/// repository root.
const EXPRESSIONS: &str = "tests/conformance/actionlint/testdata/bench/expressions.txt";

/// The fuzz regression inputs of the expression targets, relative to the
/// repository root.
const EXPRESSION_REGRESSIONS: &str = "crates/workflint-expr/tests/fuzz-regressions";

/// The fuzz regression inputs of the JSON target, relative to the repository
/// root.
const JSON_REGRESSIONS: &str = "crates/workflint-expr/tests/json-fuzz-regressions";

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
/// of every target in [`TARGETS`], [`EXPRESSION_TARGETS`], and
/// [`JSON_TARGETS`].
pub(crate) fn run(root: &Path) -> Result<()> {
    let seeds = collect(root)?;
    let expressions = collect_expressions(root)?;
    let json = collect_json(root)?;
    for (targets, seeds) in [
        (TARGETS, &seeds),
        (EXPRESSION_TARGETS, &expressions),
        (JSON_TARGETS, &json),
    ] {
        for target in targets {
            let dir = root.join(CORPUS).join(target);
            write(&dir, seeds)?;
            println!(
                "fuzz-seeds: wrote {} seeds to {CORPUS}/{target}",
                seeds.len()
            );
        }
    }
    Ok(())
}

/// Reads the workflow seeds from the sources under `root`, in a fixed order.
/// Fails if a source is missing or two seeds would get the same file name.
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
    for (prefix, dir) in REGRESSIONS {
        let dir = root.join(dir);
        for path in files_under(&dir)? {
            seeds.push(seed_from_file(prefix, &dir, &path)?);
        }
    }
    check_unique(&seeds)?;
    Ok(seeds)
}

/// Reads the expression seeds from the sources under `root`, in a fixed
/// order. Fails if a source is missing or two seeds would get the same file
/// name.
fn collect_expressions(root: &Path) -> Result<Vec<Seed>> {
    let path = root.join(EXPRESSIONS);
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut seeds: Vec<Seed> = text
        .lines()
        .enumerate()
        .map(|(index, line)| Seed {
            name: format!("expression-{:04}", index + 1),
            contents: format!("{line}}}}}").into_bytes(),
        })
        .collect();
    let fixtures = root.join(FIXTURES);
    for path in files_under(&fixtures)? {
        if !is_yaml(&path) {
            continue;
        }
        let file = seed_from_file("fixture", &fixtures, &path)?;
        let text = String::from_utf8_lossy(&file.contents);
        for (index, expression) in expressions_in(&text).into_iter().enumerate() {
            seeds.push(Seed {
                name: format!("{}-{}", file.name, index + 1),
                contents: expression.as_bytes().to_vec(),
            });
        }
    }
    for path in files_under(&root.join(EXPRESSION_REGRESSIONS))? {
        seeds.push(seed_from_file(
            "regression",
            &root.join(EXPRESSION_REGRESSIONS),
            &path,
        )?);
    }
    check_unique(&seeds)?;
    Ok(seeds)
}

/// Reads the JSON seeds from the sources under `root`, in a fixed order: the
/// `fromJSON('…')` literals of `expressions.txt` and of the YAML files of the
/// vendored test data, and the JSON fuzz regression inputs. Fails if a source
/// is missing or two seeds would get the same file name.
fn collect_json(root: &Path) -> Result<Vec<Seed>> {
    let path = root.join(EXPRESSIONS);
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut seeds: Vec<Seed> = json_literals_in(&text)
        .into_iter()
        .enumerate()
        .map(|(index, literal)| Seed {
            name: format!("json-expression-{:04}", index + 1),
            contents: literal.into_bytes(),
        })
        .collect();
    let fixtures = root.join(FIXTURES);
    for path in files_under(&fixtures)? {
        if !is_yaml(&path) {
            continue;
        }
        let file = seed_from_file("json-fixture", &fixtures, &path)?;
        let text = String::from_utf8_lossy(&file.contents);
        for (index, literal) in json_literals_in(&text).into_iter().enumerate() {
            seeds.push(Seed {
                name: format!("{}-{}", file.name, index + 1),
                contents: literal.into_bytes(),
            });
        }
    }
    for path in files_under(&root.join(JSON_REGRESSIONS))? {
        seeds.push(seed_from_file(
            "regression",
            &root.join(JSON_REGRESSIONS),
            &path,
        )?);
    }
    check_unique(&seeds)?;
    Ok(seeds)
}

/// The value of each `fromJSON('…')` string literal in `text`, with each `''`
/// turned into `'`. The function name matches in any case. The tests of
/// `workflint-expr` in `real_world.rs` cut the literals the same way.
fn json_literals_in(text: &str) -> Vec<String> {
    const CALL: &str = "fromjson('";
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut rest = 0;
    while let Some(start) = lower[rest..].find(CALL) {
        let mut literal = String::new();
        let mut chars = text[rest + start + CALL.len()..].char_indices().peekable();
        let mut end = None;
        while let Some((i, c)) = chars.next() {
            if c != '\'' {
                literal.push(c);
            } else if chars.next_if(|(_, c)| *c == '\'').is_some() {
                literal.push('\'');
            } else {
                end = Some(rest + start + CALL.len() + i + 1);
                break;
            }
        }
        let Some(end) = end else {
            break;
        };
        found.push(literal);
        rest = end;
    }
    found
}

/// Fails if two seeds have the same file name.
fn check_unique(seeds: &[Seed]) -> Result<()> {
    let mut names: Vec<&str> = seeds.iter().map(|seed| seed.name.as_str()).collect();
    names.sort_unstable();
    if let Some([name, _]) = names.array_windows().find(|[a, b]| a == b) {
        bail!("two seeds are named {name}");
    }
    Ok(())
}

/// The text after each `${{` in `text`, up to the end of the line that has
/// the next `}}`, or of the same line if no `}}` follows. The tests of
/// `workflint-expr` in `real_world.rs` cut the fixtures the same way.
fn expressions_in(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        let after = &rest[start + 3..];
        let close = after.find("}}").unwrap_or(0);
        let end = after[close..]
            .find('\n')
            .map_or(after.len(), |newline| close + newline + 1);
        found.push(&after[..end]);
        rest = after;
    }
    found
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
        let [(_, yaml), (_, syntax), (_, render)] = REGRESSIONS else {
            panic!("three regression directories");
        };
        put(root, &format!("{yaml}/tab-after-key"), "a:\t\n");
        put(root, &format!("{syntax}/metadata/merge"), "<<: {}\n");
        put(root, &format!("{render}/empty"), "");
        put(
            root,
            &format!("{FIXTURES}/c.yml"),
            "x: ${{ a }} b\ny: ${{ c\n",
        );
        put(
            root,
            &format!("{FIXTURES}/d.yml"),
            "x: ${{ fromJSON('[1]') && FromJson('{\"a\":''b''}') }}\n",
        );
        put(root, EXPRESSIONS, "a.b\n!c\nfromJSON('null')\n");
        put(root, &format!("{EXPRESSION_REGRESSIONS}/empty"), "");
        put(root, &format!("{JSON_REGRESSIONS}/trailing-comma"), "[1,]");
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
            ("fixture-c.yml", "x: ${{ a }} b\ny: ${{ c\n"),
            (
                "fixture-d.yml",
                "x: ${{ fromJSON('[1]') && FromJson('{\"a\":''b''}') }}\n",
            ),
            ("fixture-examples-a.yaml", "on: push\n"),
            ("realworld-x_y_.github_workflows_ci.yml", "jobs: {}\n"),
            ("regression-tab-after-key", "a:\t\n"),
            ("render-regression-empty", ""),
            ("syntax-regression-metadata-merge", "<<: {}\n"),
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

    fn expected_expressions() -> Vec<(String, String)> {
        [
            ("expression-0001", "a.b}}"),
            ("expression-0002", "!c}}"),
            ("expression-0003", "fromJSON('null')}}"),
            ("fixture-c.yml-1", " a }} b\n"),
            ("fixture-c.yml-2", " c\n"),
            (
                "fixture-d.yml-1",
                " fromJSON('[1]') && FromJson('{\"a\":''b''}') }}\n",
            ),
            ("regression-empty", ""),
        ]
        .map(|(name, contents)| (name.to_owned(), contents.to_owned()))
        .to_vec()
    }

    #[test]
    fn every_expression_target_gets_the_expressions_the_fixture_expressions_and_the_regressions() {
        let dir = repository();
        run(dir.path()).unwrap();
        for target in EXPRESSION_TARGETS {
            assert_eq!(
                corpus(dir.path(), target),
                expected_expressions(),
                "{target}"
            );
        }
    }

    #[test]
    fn every_json_target_gets_the_json_literals_and_the_json_regressions() {
        let dir = repository();
        run(dir.path()).unwrap();
        let expected: Vec<(String, String)> = [
            ("json-expression-0001", "null"),
            ("json-fixture-d.yml-1", "[1]"),
            ("json-fixture-d.yml-2", "{\"a\":'b'}"),
            ("regression-trailing-comma", "[1,]"),
        ]
        .map(|(name, contents)| (name.to_owned(), contents.to_owned()))
        .to_vec();
        for target in JSON_TARGETS {
            assert_eq!(corpus(dir.path(), target), expected, "{target}");
        }
    }

    #[test]
    fn a_json_literal_ends_at_a_quote_that_is_not_doubled() {
        let text =
            "fromJSON('{\"a\":''b''}') && FROMJSON( 'x') || fromjson('[1]') && fromJSON('open";
        assert_eq!(json_literals_in(text), ["{\"a\":'b'}", "[1]"]);
    }

    #[test]
    fn an_expression_seed_ends_at_the_line_with_the_next_closing_braces() {
        let text = "a: ${{ x }} b\nc: ${{ 'y\n' }} d\ne: ${{ z";
        assert_eq!(expressions_in(text), [" x }} b\n", " 'y\n' }} d\n", " z"]);
    }

    #[test]
    fn missing_expressions_file_is_an_error() {
        let dir = repository();
        fs::remove_file(dir.path().join(EXPRESSIONS)).unwrap();
        let error = run(dir.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("expressions.txt"),
            "{error:#}"
        );
    }

    /// Every fuzz target in `fuzz/Cargo.toml` gets seeds and runs in the
    /// nightly matrix of `.github/workflows/fuzz.yml`, and no other target
    /// does.
    #[test]
    fn the_target_lists_match_the_fuzz_workspace_and_the_fuzz_workflow() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).unwrap();
        let mut bins: Vec<&str> = manifest
            .lines()
            .filter_map(|line| line.strip_prefix("name = \""))
            .filter_map(|name| name.strip_suffix('"'))
            .filter(|name| *name != "workflint-fuzz")
            .collect();
        bins.sort_unstable();
        let mut targets: Vec<&str> = TARGETS
            .iter()
            .chain(EXPRESSION_TARGETS)
            .chain(JSON_TARGETS)
            .copied()
            .collect();
        targets.sort_unstable();
        assert_eq!(bins, targets);

        let workflow = fs::read_to_string(root.join(".github/workflows/fuzz.yml")).unwrap();
        let matrix = workflow
            .lines()
            .find_map(|line| line.trim().strip_prefix("target: ["))
            .and_then(|list| list.strip_suffix(']'))
            .unwrap();
        let mut matrix: Vec<&str> = matrix.split(", ").collect();
        matrix.sort_unstable();
        assert_eq!(matrix, targets);
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
