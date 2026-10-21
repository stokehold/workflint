//! The config file: decoding with the reference's error texts, the lookup in a project root,
//! unknown keys, `paths` ignores, and `-init-config`. The cases of `config_test.go` and of
//! `testdata/config/` in the reference are ported here.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workflint_linter::config::{Config, ConfigKind, DEFAULT_CONFIG, init_config};
use workflint_rules::PythonLinter;

/// Parses `text` as an actionlint config file named `c.yaml`.
fn parse(text: &str) -> Config {
    Config::parse(Path::new("c.yaml"), ConfigKind::Actionlint, text).unwrap()
}

/// The error of parsing `text` as an actionlint config file named `c.yaml`, without the prefix
/// that names the file.
fn parse_error(text: &str) -> String {
    let err = Config::parse(Path::new("c.yaml"), ConfigKind::Actionlint, text)
        .unwrap_err()
        .to_string();
    let Some(message) = err.strip_prefix("could not parse config file \"c.yaml\": ") else {
        panic!("the error does not name the file once: {err}");
    };
    message.to_owned()
}

/// A temporary project: a directory with `.git` and `.github/workflows`.
fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
    dir
}

fn write(dir: &Path, path: &str, contents: &str) -> PathBuf {
    // Joined part by part, so that the path has the separators of the OS.
    let path = path
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn self_hosted_runner_labels_are_read_and_null_is_the_empty_list() {
    // `TestConfigParseSelfHostedRunnerOK`.
    let cases: [(&str, &[&str]); 5] = [
        ("", &[]),
        ("self-hosted-runner:\n", &[]),
        ("self-hosted-runner:\n  labels:", &[]),
        ("self-hosted-runner:\n  labels: []", &[]),
        ("self-hosted-runner:\n  labels: [foo, bar]", &["foo", "bar"]),
    ];
    for (text, labels) in cases {
        assert_eq!(parse(text).rules().runner_labels, labels, "{text:?}");
    }
}

#[test]
fn config_variables_null_turns_the_check_off_and_an_empty_list_allows_none() {
    assert_eq!(parse("").rules().config_variables, None);
    assert_eq!(parse("config-variables:\n").rules().config_variables, None);
    assert_eq!(
        parse("config-variables: null\n").rules().config_variables,
        None
    );
    assert_eq!(
        parse("config-variables: []\n").rules().config_variables,
        Some(Vec::new())
    );
    assert_eq!(
        parse("config-variables: [WOO, FOO]\n")
            .rules()
            .config_variables,
        Some(vec!["WOO".to_owned(), "FOO".to_owned()])
    );
}

#[test]
fn an_empty_or_comment_only_file_gives_the_defaults() {
    for text in ["", "\n", "# only a comment\n", "---\n"] {
        let config = parse(text);
        assert_eq!(config.rules().runner_labels, Vec::<String>::new());
        assert_eq!(config.rules().config_variables, None);
        assert!(!config.is_ignored(".github/workflows/a.yaml", "anything"));
        assert_eq!(config.warnings(), []);
    }
}

#[test]
fn decoding_errors_have_the_texts_of_the_reference() {
    // `TestConfigParseError`, and the texts of the reference's `ParseConfig` for more inputs.
    let cases = [
        (
            "self-hosted-runner: 42\n",
            r#"yaml: unmarshal errors:   line 1: cannot unmarshal !!int `42` into struct { Labels []string "yaml:\"labels\"" }"#,
        ),
        (
            "self-hosted-runner: !!str 42\n",
            r#"yaml: unmarshal errors:   line 1: cannot unmarshal !!str `42` into struct { Labels []string "yaml:\"labels\"" }"#,
        ),
        (
            "self-hosted-runner: [a]\n",
            r#"yaml: unmarshal errors:   line 1: cannot unmarshal !!seq into struct { Labels []string "yaml:\"labels\"" }"#,
        ),
        (
            "paths:\n  foo:\n    ignore: foo+\n",
            r#"yaml: unmarshal errors:   line 3: yaml: "ignore" must be a sequence node at line:3,col:13"#,
        ),
        (
            "paths:\n  foo.{txt,xml:\n",
            r#"invalid glob pattern "foo.{txt,xml" in "paths""#,
        ),
        (
            "paths:\n  \"[\":\n    ignore: [x]\n",
            r#"invalid glob pattern "[" in "paths""#,
        ),
        (
            "self-hosted-runner:\n  labels: foo\n",
            "yaml: unmarshal errors:   line 2: cannot unmarshal !!str `foo` into []string",
        ),
        (
            "self-hosted-runner:\n  labels: [a, {b: c}, ~, 1]\n",
            "yaml: unmarshal errors:   line 2: cannot unmarshal !!map into string",
        ),
        (
            "config-variables: {a: b}\n",
            "yaml: unmarshal errors:   line 1: cannot unmarshal !!map into []string",
        ),
        (
            "paths: [a]\n",
            "yaml: unmarshal errors:   line 1: cannot unmarshal !!seq into map[string]actionlint.PathConfig",
        ),
        (
            "paths: 1\n",
            "yaml: unmarshal errors:   line 1: cannot unmarshal !!int `1` into map[string]actionlint.PathConfig",
        ),
        (
            "paths:\n  a: 1\n",
            "yaml: unmarshal errors:   line 2: cannot unmarshal !!int `1` into actionlint.PathConfig",
        ),
        (
            "paths:\n  a:\n  a:\n",
            r#"yaml: unmarshal errors:   line 3: mapping key "a" already defined at line 2"#,
        ),
        (
            "paths:\n  a:\n    ignore: [x]\n    ignore: [y]\n",
            r#"yaml: unmarshal errors:   line 4: mapping key "ignore" already defined at line 3"#,
        ),
        (
            "[1]\n",
            "yaml: unmarshal errors:   line 1: cannot unmarshal !!seq into actionlint.Config",
        ),
        (
            "foo\n",
            "yaml: unmarshal errors:   line 1: cannot unmarshal !!str `foo` into actionlint.Config",
        ),
        (
            "self-hosted-runner: 42\nconfig-variables: x\n",
            r#"yaml: unmarshal errors:   line 1: cannot unmarshal !!int `42` into struct { Labels []string "yaml:\"labels\"" }   line 2: cannot unmarshal !!str `x` into []string"#,
        ),
        ("a: [\n", "yaml: line 1: did not find expected node content"),
    ];
    for (text, expected) in cases {
        assert_eq!(parse_error(text), expected, "{text:?}");
    }
}

#[test]
fn an_invalid_ignore_regex_names_the_entry_and_its_position() {
    // The reference says `at line3,col:13` and points at the list
    // (docs/differences.md#config-error-messages).
    assert_eq!(
        parse_error("paths:\n  foo:\n    ignore: ['(foo']\n"),
        "yaml: unmarshal errors:   line 3: invalid regular expression \"(foo\" in \"ignore\" at \
         line:3,col:14: error parsing regexp: missing closing ): `(foo`"
    );
    assert_eq!(
        parse_error("paths:\n  foo:\n    ignore:\n      - x\n      - (y\n      - (z\n"),
        "yaml: unmarshal errors:   line 5: invalid regular expression \"(y\" in \"ignore\" at \
         line:5,col:9: error parsing regexp: missing closing ): `(y`"
    );
}

#[test]
fn an_ignore_entry_that_is_a_collection_or_null_is_an_error() {
    // The reference compiles the empty regex for these, which ignores every error
    // (docs/differences.md#config-ignore-entries).
    assert_eq!(
        parse_error("paths:\n  a:\n    ignore: [{}]\n"),
        "yaml: unmarshal errors:   line 3: \"ignore\" entry must be a string but mapping node \
         was found at line:3,col:14"
    );
    assert_eq!(
        parse_error("paths:\n  a:\n    ignore: [[x]]\n"),
        "yaml: unmarshal errors:   line 3: \"ignore\" entry must be a string but sequence node \
         was found at line:3,col:14"
    );
    assert_eq!(
        parse_error("paths:\n  a:\n    ignore:\n      -\n"),
        "yaml: unmarshal errors:   line 4: \"ignore\" entry must be a string but null was found \
         at line:4,col:8"
    );
}

#[test]
fn an_ignore_entry_that_is_a_number_or_an_alias_of_a_string_is_its_text() {
    let config = parse("x: &x foo\npaths:\n  a:\n    ignore: [42, *x]\n");
    assert!(config.is_ignored("a", "error 42"));
    // The reference compiles the alias's name, `x`, instead of its value.
    assert!(config.is_ignored("a", "foo"));
    assert!(!config.is_ignored("a", "x"));
}

#[test]
fn an_ignore_pattern_matches_the_message_unanchored() {
    // `TestConfigPathConfigIgnores`.
    let cases = [
        ("", false),
        ("ignore: []", false),
        ("ignore: ['(is )+']", true),
        ("ignore: ['does not match', '(is )+']", true),
        ("ignore: ['does not match', 'does not match 2']", false),
    ];
    for (entry, ignored) in cases {
        let text = format!("paths:\n  a:\n    {entry}\n");
        assert_eq!(
            parse(&text).is_ignored("a", "this is test"),
            ignored,
            "{entry:?}"
        );
    }
}

#[test]
fn every_paths_glob_that_matches_the_path_applies() {
    // `TestConfigIgnoreErrors`.
    let config = parse(
        "paths:
  .github/workflows/**/*.yaml:
    ignore: [xxx]
  .github/workflows/*.yaml:
    ignore: [yyy]
  .github/workflows/a/*.yaml:
    ignore: [zzz]
  .github/workflows/*/b.yaml:
    ignore: [uuu]
  .github/workflows/a/b.yaml:
    ignore: [vvv]
  .github/workflows/**/x.yaml:
    ignore: [www]
  .github/workflows/**/*.{yml,yaml}:
    ignore: [ttt]
",
    );
    let cases = [
        ("foo.yaml", "xxx", false),
        (".github/workflows/a.yaml", "xxx", true),
        (".github/workflows/a/b.yaml", "xxx", true),
        (".github/workflows/a/b/c/d/e/f/g/h.yaml", "xxx", true),
        (".github/workflows/a.yaml", "yyy", true),
        (".github/workflows/a/b.yaml", "yyy", false),
        (".github/workflows/a/b.yaml", "zzz", true),
        (".github/workflows/a/a.yaml", "zzz", true),
        (".github/workflows/b/b.yaml", "zzz", false),
        (".github/workflows/a/b.yaml", "uuu", true),
        (".github/workflows/b/b.yaml", "uuu", true),
        (".github/workflows/a/a.yaml", "uuu", false),
        (".github/workflows/a/b.yaml", "vvv", true),
        (".github/workflows/b/b.yaml", "vvv", false),
        (".github/workflows/a/a.yaml", "vvv", false),
        (".github/workflows/x.yaml", "www", true),
        (".github/workflows/a/x.yaml", "www", true),
        (".github/workflows/a/b/x.yaml", "www", true),
        (".github/workflows/a/b/c/x.yaml", "www", true),
        (".github/workflows/a/b.yaml", "this is not ignored", false),
        (".github/workflows/a.yml", "xxx", false),
        (".github/workflows/a.yml", "ttt", true),
    ];
    for (path, message, ignored) in cases {
        assert_eq!(
            config.is_ignored(path, message),
            ignored,
            "{message:?} at {path}"
        );
    }
}

#[test]
fn python_linter_takes_each_of_its_values_and_defaults_to_auto() {
    let cases = [
        ("", PythonLinter::Auto),
        ("python:\n", PythonLinter::Auto),
        ("python:\n  linter:\n", PythonLinter::Auto),
        ("python:\n  linter: auto\n", PythonLinter::Auto),
        ("python:\n  linter: ruff\n", PythonLinter::Ruff),
        ("python:\n  linter: pyflakes\n", PythonLinter::Pyflakes),
        ("python:\n  linter: 'off'\n", PythonLinter::Off),
    ];
    for (text, linter) in cases {
        assert_eq!(parse(text).rules().python_linter, linter, "{text:?}");
    }
}

#[test]
fn python_linter_in_an_actionlint_file_is_read_without_a_warning() {
    let config = parse("python:\n  linter: ruff\n");
    assert_eq!(config.rules().python_linter, PythonLinter::Ruff);
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
}

#[test]
fn an_invalid_python_linter_is_an_error_that_lists_the_values() {
    let expected = |found: &str, line: usize| {
        format!(
            "yaml: unmarshal errors:   line {line}: \"python.linter\" must be \"auto\", \"ruff\", \
             \"pyflakes\", or \"off\", but {found} was found at line:{line},col:11"
        )
    };
    assert_eq!(
        parse_error("python:\n  linter: black\n"),
        expected("\"black\"", 2)
    );
    // Case counts, as it does for every value of the config file.
    assert_eq!(
        parse_error("python:\n  linter: Ruff\n"),
        expected("\"Ruff\"", 2)
    );
    // YAML 1.1 reads an unquoted `off` as a boolean, but go-yaml v3 does not.
    assert_eq!(
        parse("python:\n  linter: off\n").rules().python_linter,
        PythonLinter::Off
    );
    assert_eq!(
        parse_error("python:\n  linter: [ruff]\n"),
        expected("sequence node", 2)
    );
    assert_eq!(
        parse_error("python: ruff\n"),
        "yaml: unmarshal errors:   line 1: cannot unmarshal !!str `ruff` into workflint.PythonConfig"
    );
}

#[test]
fn an_unknown_key_under_python_is_a_warning_or_an_error() {
    let text = "python:\n  lint: ruff\n";
    let warnings: Vec<String> = parse(text)
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        warnings,
        [r#"config file "c.yaml": unknown key "lint" in "python" at line:2,col:3 is ignored"#]
    );
    let err = Config::parse(Path::new("workflint.yaml"), ConfigKind::Workflint, text)
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        r#"could not parse config file "workflint.yaml": unknown key "lint" in "python" at line:2,col:3"#
    );
}

#[test]
fn an_unknown_key_in_an_actionlint_file_is_a_warning() {
    let text = "self-hosted-runners:\n  labels: [a]\nself-hosted-runner:\n  lables: [b]\npaths:\n  a:\n    ignored: [x]\n";
    let config = Config::parse(Path::new("c.yaml"), ConfigKind::Actionlint, text).unwrap();
    let warnings: Vec<String> = config.warnings().iter().map(ToString::to_string).collect();
    assert_eq!(
        warnings,
        [
            r#"config file "c.yaml": unknown key "self-hosted-runners" at line:1,col:1 is ignored"#,
            r#"config file "c.yaml": unknown key "lables" in "self-hosted-runner" at line:4,col:3 is ignored"#,
            r#"config file "c.yaml": unknown key "ignored" in "paths" entry "a" at line:7,col:5 is ignored"#,
        ]
    );
}

#[test]
fn an_unknown_key_in_a_workflint_file_is_an_error() {
    let text = "config-variables: []\nself-hosted-runner:\n  lables: [b]\n";
    let err = Config::parse(Path::new("workflint.yaml"), ConfigKind::Workflint, text)
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        r#"could not parse config file "workflint.yaml": unknown key "lables" in "self-hosted-runner" at line:3,col:3"#
    );
}

#[test]
fn keys_merged_with_a_merge_key_are_checked_too() {
    let text = "base: &base\n  labels: [a]\n  extra: 1\nself-hosted-runner:\n  <<: *base\n";
    let config = Config::parse(Path::new("c.yaml"), ConfigKind::Actionlint, text).unwrap();
    assert_eq!(config.rules().runner_labels, ["a"]);
    let warnings: Vec<String> = config.warnings().iter().map(ToString::to_string).collect();
    assert_eq!(
        warnings,
        [
            r#"config file "c.yaml": unknown key "base" at line:1,col:1 is ignored"#,
            r#"config file "c.yaml": unknown key "extra" in "self-hosted-runner" at line:3,col:3 is ignored"#,
        ]
    );
}

#[test]
fn the_kind_of_a_config_file_comes_from_its_name() {
    assert_eq!(
        ConfigKind::of_path(Path::new(".github/workflint.yaml")),
        ConfigKind::Workflint
    );
    assert_eq!(
        ConfigKind::of_path(Path::new("workflint.yml")),
        ConfigKind::Workflint
    );
    assert_eq!(
        ConfigKind::of_path(Path::new(".github/actionlint.yaml")),
        ConfigKind::Actionlint
    );
    assert_eq!(
        ConfigKind::of_path(Path::new("ci/lint.yaml")),
        ConfigKind::Actionlint
    );
}

#[test]
fn reading_a_file_names_it_in_the_error() {
    // `TestConfigReadFileOK`, `TestConfigReadFileReadError`, `TestConfigReadFileParseError`.
    let dir = tempfile::tempdir().unwrap();
    let ok = write(
        dir.path(),
        "ok.yml",
        "self-hosted-runner:\n  labels:\n    - foo\n    - bar\n",
    );
    assert_eq!(
        Config::read(&ok).unwrap().rules().runner_labels,
        ["foo", "bar"]
    );
    assert_eq!(Config::read(&ok).unwrap().path(), ok);

    let missing = dir.path().join("does-not-exist.yml");
    let err = Config::read(&missing).unwrap_err().to_string();
    let prefix = format!(
        "could not read config file {:?}: ",
        missing.display().to_string()
    );
    assert!(err.starts_with(&prefix), "{err}");

    let broken = write(dir.path(), "broken.yml", "self-hosted-runner: 42\n");
    let err = Config::read(&broken).unwrap_err().to_string();
    let prefix = format!(
        "could not parse config file {:?}: ",
        broken.display().to_string()
    );
    assert!(err.starts_with(&prefix), "{err}");
}

#[test]
fn a_utf16_config_file_is_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(
        "self-hosted-runner:\n  labels: [gpu]\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    let path = dir.path().join("c.yaml");
    fs::write(&path, bytes).unwrap();
    assert_eq!(Config::read(&path).unwrap().rules().runner_labels, ["gpu"]);
}

#[test]
fn the_lookup_takes_the_first_file_in_the_order_of_adr_0010() {
    let names = [
        "workflint.yaml",
        "workflint.yml",
        "actionlint.yaml",
        "actionlint.yml",
    ];
    for (index, name) in names.iter().enumerate() {
        let dir = project();
        // This file and every later one, each with its own label.
        for later in &names[index..] {
            let text = format!("self-hosted-runner:\n  labels: [{later}]\n");
            write(dir.path(), &format!(".github/{later}"), &text);
        }
        let config = Config::find(dir.path()).unwrap().unwrap();
        assert_eq!(config.rules().runner_labels, [*name]);
        assert_eq!(config.path(), dir.path().join(".github").join(name));
        assert_eq!(config.kind(), ConfigKind::of_path(Path::new(name)));
    }
}

#[test]
fn a_project_without_a_config_file_has_none() {
    // `testdata/config/projects/none`.
    let dir = project();
    assert!(Config::find(dir.path()).unwrap().is_none());
}

#[test]
fn a_broken_config_file_in_the_project_is_an_error_naming_it_once() {
    // `testdata/config/projects/err`. The reference names the file twice
    // (docs/differences.md#config-error-messages).
    let dir = project();
    let path = write(
        dir.path(),
        ".github/actionlint.yaml",
        "self-hosted-runner: 42\n",
    );
    let err = Config::find(dir.path()).unwrap_err().to_string();
    assert_eq!(
        err,
        format!(
            "could not parse config file {:?}: yaml: unmarshal errors:   line 1: cannot \
             unmarshal !!int `42` into struct {{ Labels []string \"yaml:\\\"labels\\\"\" }}",
            path.display().to_string()
        )
    );
}

#[test]
fn a_config_path_that_cannot_be_read_is_an_error_not_a_missing_file() {
    let dir = project();
    fs::create_dir(dir.path().join(".github/workflint.yaml")).unwrap();
    write(dir.path(), ".github/actionlint.yaml", "");
    let err = Config::find(dir.path()).unwrap_err().to_string();
    assert!(err.starts_with("could not read config file "), "{err}");
}

#[test]
fn the_default_config_is_the_reference_template_and_gives_the_defaults() {
    // `TestConfigGenerateDefaultConfigFileOK`.
    assert!(DEFAULT_CONFIG.starts_with("self-hosted-runner:\n  # Labels of self-hosted runner"));
    let config = Config::parse(
        Path::new("workflint.yaml"),
        ConfigKind::Workflint,
        DEFAULT_CONFIG,
    )
    .unwrap();
    assert_eq!(config.rules().runner_labels, Vec::<String>::new());
    assert_eq!(config.rules().config_variables, None);
    assert_eq!(config.warnings(), []);
}

#[test]
fn init_config_writes_workflint_yaml_in_the_project_of_a_subdirectory() {
    let dir = project();
    let sub = dir.path().join("src/deep");
    fs::create_dir_all(&sub).unwrap();
    let path = init_config(&sub).unwrap();
    assert_eq!(path, dir.path().join(".github").join("workflint.yaml"));
    assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777 & !0o644, 0, "{mode:o}");
    }
}

#[test]
fn init_config_refuses_to_overwrite_any_config_file() {
    // `TestLinterGenerateConfigAlreadyExists` for `projects/ok` and `projects/yml`.
    for name in [
        "workflint.yaml",
        "workflint.yml",
        "actionlint.yaml",
        "actionlint.yml",
    ] {
        let dir = project();
        let existing = write(
            dir.path(),
            &format!(".github/{name}"),
            "config-variables: []\n",
        );
        let err = init_config(dir.path()).unwrap_err().to_string();
        assert_eq!(
            err,
            format!(
                "config file already exists at {:?}",
                existing.display().to_string()
            )
        );
        assert_eq!(
            fs::read_to_string(&existing).unwrap(),
            "config-variables: []\n"
        );
        if name != "workflint.yaml" {
            assert!(!dir.path().join(".github/workflint.yaml").exists());
        }
    }
}

#[test]
fn init_config_outside_a_project_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = init_config(dir.path()).unwrap_err().to_string();
    assert_eq!(
        err,
        "project is not found. check current project is initialized as Git repository and \
         \".github/workflows\" directory exists"
    );
}
