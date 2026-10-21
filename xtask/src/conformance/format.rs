//! The cases of the reference's `testdata/format/`: `test.yaml` linted with a `-format`
//! template, whose output must be the content of `test.json`, `test.jsonl`, `test.md`, or
//! `test.sarif`. `tests/conformance/README.md` documents them.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::allow_list::Entry;
use super::fixture::{Invocation, Origin};

/// One case: the template and the file with the output it must print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    /// The ID that `allowed-differences.toml` and `docs/parity-checklist.md` use, such as
    /// `format/test.json`.
    pub id: &'static str,
    /// The name of the file in `testdata/format/` with the expected output.
    pub expected: &'static str,
    /// The value of `-format`.
    template: Template,
    /// Whether the output is compared as JSON, which ignores the layout and the order of keys.
    json: bool,
}

/// Where the value of `-format` comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Template {
    /// The value itself.
    Text(&'static str),
    /// A file in `testdata/format/`.
    File(&'static str),
}

/// The cases of `TestLinterFormatErrorMessageOK` and `TestLinterFormatErrorMessageInSARIF` in
/// the reference's `linter_test.go`, with its templates. A `\n` in them is a backslash and an
/// `n`, which `-format` unescapes.
pub const CASES: [Case; 4] = [
    Case {
        id: "format/test.json",
        expected: "test.json",
        template: Template::Text("{{json .}}"),
        json: false,
    },
    Case {
        id: "format/test.jsonl",
        expected: "test.jsonl",
        template: Template::Text("{{range $err := .}}{{json $err}}{{end}}"),
        json: false,
    },
    Case {
        id: "format/test.md",
        expected: "test.md",
        template: Template::Text(
            r"{{range $ := .}}### Error at line {{$.Line}}, col {{$.Column}} of `{{$.Filepath}}`\n\n{{$.Message}}\n\n```\n{{$.Snippet}}\n```\n\n{{end}}",
        ),
        json: false,
    },
    Case {
        id: "format/test.sarif",
        expected: "test.sarif",
        template: Template::File("sarif_template.txt"),
        json: true,
    },
];

/// The case whose expected output is the file `path`, if there is one.
#[must_use]
pub fn case(path: &Path) -> Option<&'static Case> {
    let name = path.file_name()?.to_str()?;
    CASES.iter().find(|case| case.expected == name)
}

/// The directory of the cases, `testdata/format/` of the vendored reference.
fn directory() -> PathBuf {
    Origin::Actionlint.root().join("format")
}

impl Case {
    /// The expected output.
    ///
    /// # Errors
    ///
    /// Returns a message if the file cannot be read.
    pub fn expected_output(&self) -> Result<String, String> {
        read(&directory().join(self.expected))
    }

    /// Prepares a temporary directory with `testdata/format/test.yaml` and an empty config
    /// file, and the command line that lints the workflow with the case's template, as the
    /// commands in the reference's `testdata/format/README.md` do. The tools are off, and the
    /// config file keeps out the config of the directories above.
    ///
    /// # Errors
    ///
    /// Returns a message if a file cannot be read or written.
    pub fn prepare(&self) -> Result<Invocation, String> {
        let template = match self.template {
            Template::Text(text) => text.to_owned(),
            Template::File(name) => read(&directory().join(name))?,
        };
        let temp = tempfile::tempdir().map_err(|error| format!("creating a directory: {error}"))?;
        let cwd = temp.path().to_path_buf();
        let workflow = Path::new("testdata").join("format").join("test.yaml");
        let config = cwd.join("config.yaml");
        let written: io::Result<()> = (|| {
            fs::create_dir_all(cwd.join("testdata").join("format"))?;
            fs::copy(directory().join("test.yaml"), cwd.join(&workflow))?;
            fs::write(&config, "")
        })();
        written.map_err(|error| format!("preparing {}: {error}", self.id))?;
        let args: Vec<OsString> = vec![
            "-shellcheck=".into(),
            "-pyflakes=".into(),
            "-config-file".into(),
            config.into_os_string(),
            "-format".into(),
            template.into(),
            // `/` separators, as in the reference's output on every system.
            "testdata/format/test.yaml".into(),
        ];
        Ok(Invocation::new(temp, cwd, args))
    }

    /// Compares `actual`, the output of the binary, with `expected`, the expected output after
    /// the allow-list entries.
    ///
    /// The SARIF case compares the JSON values and ignores `runs[0].tool.driver.version`, which
    /// is `(devel)` in the expected file and the version of the binary in its output.
    ///
    /// # Errors
    ///
    /// Returns a message with both outputs if they differ.
    pub fn compare(&self, expected: &str, actual: &str) -> Result<(), String> {
        let same = if self.json {
            let parse = |text: &str, what: &str| {
                serde_json::from_str::<serde_json::Value>(text)
                    .map_err(|error| format!("{} {what} is not JSON: {error}", self.id))
            };
            let mut want = parse(expected, "expected output")?;
            let mut have = parse(actual, "output")?;
            for value in [&mut want, &mut have] {
                if let Some(version) = value.pointer_mut("/runs/0/tool/driver/version") {
                    *version = serde_json::Value::Null;
                }
            }
            want == have
        } else {
            expected == actual
        };
        if same {
            Ok(())
        } else {
            Err(format!(
                "the output differs from {}.\nexpected:\n{expected}\nactual:\n{actual}",
                self.expected
            ))
        }
    }
}

/// Applies `entries` to `expected`, the expected output of a case, line by line: an entry
/// replaces the first line that is its `upstream` line with its `workflint` line, or removes it
/// if it has none.
///
/// # Errors
///
/// Returns a message if an entry has no `upstream`, or if its `upstream` line is not in
/// `expected`.
pub fn apply(expected: &str, entries: &[&Entry]) -> Result<String, String> {
    let mut lines: Vec<&str> = expected.split('\n').collect();
    for entry in entries {
        let Some(upstream) = entry.upstream.as_deref() else {
            return Err(format!(
                "{}: an entry for a format case needs the `upstream` line it replaces",
                entry.describe()
            ));
        };
        let index = lines
            .iter()
            .position(|line| *line == upstream)
            .ok_or_else(|| {
                format!(
                    "{}: `upstream` matches no line of the expected output: {upstream:?}",
                    entry.describe()
                )
            })?;
        match entry.workflint.as_deref() {
            Some(workflint) => lines[index] = workflint,
            None => {
                lines.remove(index);
            }
        }
    }
    Ok(lines.join("\n"))
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("reading {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(upstream: Option<&str>, workflint: Option<&str>) -> Entry {
        Entry {
            anchor: "column-units".into(),
            fixture: "format/test.md".into(),
            upstream: upstream.map(str::to_owned),
            workflint: workflint.map(str::to_owned),
        }
    }

    #[test]
    fn every_expected_file_has_a_case() {
        for name in ["test.json", "test.jsonl", "test.md", "test.sarif"] {
            assert_eq!(
                case(&directory().join(name)).map(|case| case.id),
                Some(format!("format/{name}").as_str())
            );
        }
        assert_eq!(case(&directory().join("test.yaml")), None);
    }

    #[test]
    fn an_entry_replaces_or_removes_the_first_matching_line() {
        let replace = entry(Some("b"), Some("B"));
        let remove = entry(Some("c"), None);
        assert_eq!(
            apply("a\nb\nc\nb\n", &[&replace, &remove]).as_deref(),
            Ok("a\nB\nb\n")
        );
    }

    #[test]
    fn an_entry_without_a_matching_line_or_without_upstream_fails() {
        assert!(apply("a\n", &[&entry(Some("b"), Some("B"))]).is_err());
        assert!(apply("a\n", &[&entry(None, Some("B"))]).is_err());
    }

    #[test]
    fn json_cases_ignore_the_layout_and_the_version() {
        let sarif = CASES[3];
        let expected =
            "{\"runs\": [{\"tool\": {\"driver\": {\"version\": \"(devel)\", \"a\": 1}}}]}";
        let actual = "{\"runs\":[{\"tool\":{\"driver\":{\"a\":1,\"version\":\"0.1.0\"}}}]}";
        assert_eq!(sarif.compare(expected, actual), Ok(()));
        assert!(sarif.compare(expected, &actual.replace('1', "2")).is_err());
        assert!(CASES[0].compare("[]\n", "[]").is_err());
    }
}
