//! Fixtures and the command line that lints them. `tests/conformance/README.md`
//! documents the same contract for the CLI.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// The kind of fixture directory, which decides how the fixture is linted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    /// `err/<name>.yaml` with `err/<name>.out`.
    Err,
    /// `examples/<name>.yaml` with `examples/<name>.out`.
    Examples,
    /// `ok/<name>.yaml`, which must produce no errors.
    Ok,
    /// `projects/<name>/` with `projects/<name>.out`.
    Projects,
}

impl Suite {
    /// The directory name of the suite.
    #[must_use]
    pub fn directory(self) -> &'static str {
        match self {
            Self::Err => "err",
            Self::Examples => "examples",
            Self::Ok => "ok",
            Self::Projects => "projects",
        }
    }

    fn from_directory(name: &str) -> Option<Self> {
        [Self::Err, Self::Examples, Self::Ok, Self::Projects]
            .into_iter()
            .find(|suite| suite.directory() == name)
    }
}

/// Where a fixture comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The reference's vendored test data in `tests/conformance/actionlint/testdata/`.
    Actionlint,
    /// workflint's own fixtures in `tests/conformance/workflint/`.
    Workflint,
}

impl Origin {
    /// The directory that holds the origin's suites.
    #[must_use]
    pub fn root(self) -> PathBuf {
        let conformance = super::conformance_dir();
        match self {
            Self::Actionlint => conformance.join("actionlint").join("testdata"),
            Self::Workflint => conformance.join("workflint"),
        }
    }
}

/// One fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    /// The key used in `expected-failures.toml` and `allowed-differences.toml`.
    /// It matches the fixture names in `docs/parity-checklist.md`: `err/name`,
    /// `examples/name`, `ok/name.yaml` and `projects/name`, with a
    /// `workflint/` prefix for workflint's own fixtures.
    pub id: String,
    /// The suite the fixture belongs to.
    pub suite: Suite,
    /// The workflow file, or the project directory.
    pub source: PathBuf,
    /// The `.out` file. `ok` fixtures have none.
    pub expected: Option<PathBuf>,
}

impl Fixture {
    /// The fixture whose test file is `path`: the `.yaml` file for `err`,
    /// `examples` and `ok`, and the `.out` file for `projects`. The suite is
    /// the name of the directory that holds `path`.
    ///
    /// # Panics
    ///
    /// Panics if the parent directory is not a suite name.
    pub fn new(origin: Origin, path: &Path) -> Self {
        let directory = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let suite = Suite::from_directory(directory)
            .unwrap_or_else(|| panic!("{} is not in a fixture suite directory", path.display()));
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_else(|| panic!("{} has no UTF-8 file name", path.display()));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(stem);
        let name = if suite == Suite::Ok { file_name } else { stem };
        let prefix = match origin {
            Origin::Actionlint => "",
            Origin::Workflint => "workflint/",
        };
        let (source, expected) = match suite {
            Suite::Err | Suite::Examples => (path.to_path_buf(), Some(path.with_extension("out"))),
            Suite::Ok => (path.to_path_buf(), None),
            Suite::Projects => (path.with_extension(""), Some(path.to_path_buf())),
        };
        Self {
            id: format!("{prefix}{}/{name}", suite.directory()),
            suite,
            source,
            expected,
        }
    }

    /// The file name that decides whether shellcheck and pyflakes run.
    fn name(&self) -> String {
        self.source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The lines of the `.out` file, or none for an `ok` fixture.
    ///
    /// # Panics
    ///
    /// Panics if the `.out` file cannot be read.
    pub fn expected_lines(&self) -> Vec<String> {
        self.expected.as_ref().map_or_else(Vec::new, |path| {
            let text = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
            super::matcher::expected_lines(&text)
        })
    }

    /// Which external tools the fixture runs with, as the reference's tests
    /// decide:
    ///
    /// - `err` and `examples` enable a tool only if the file name contains
    ///   the tool's name (`linter_test.go:182`).
    /// - `ok` enables each tool that is installed (`linter_test.go:48`).
    /// - `projects` enable neither (`linter_test.go:325`).
    ///
    /// # Errors
    ///
    /// Returns a skip message if the file name names a tool that is not
    /// installed.
    pub fn tools(&self, installed: &Tools) -> Result<Tools, String> {
        if let Some(tool) = self.missing_tool(installed) {
            return Err(format!(
                "skipped {} because {tool:?} is not installed",
                self.id
            ));
        }
        let name = self.name();
        let enabled = |tool: &str, path: &Option<PathBuf>| match self.suite {
            Suite::Err | Suite::Examples if name.contains(tool) => path.clone(),
            Suite::Ok => path.clone(),
            _ => None,
        };
        Ok(Tools {
            shellcheck: enabled("shellcheck", &installed.shellcheck),
            pyflakes: enabled("pyflakes", &installed.pyflakes),
        })
    }

    /// The tool that the fixture's file name names but that is not in
    /// `installed`, if any. The runner skips such a fixture.
    #[must_use]
    pub fn missing_tool(&self, installed: &Tools) -> Option<&'static str> {
        let name = self.name();
        [
            ("shellcheck", &installed.shellcheck),
            ("pyflakes", &installed.pyflakes),
        ]
        .into_iter()
        .find(|(tool, path)| self.suite != Suite::Projects && name.contains(tool) && path.is_none())
        .map(|(tool, _)| tool)
    }

    /// Copies the fixture into a new temporary project and returns the
    /// command line that lints it. See [`Invocation`].
    ///
    /// # Errors
    ///
    /// Returns the I/O error from creating or copying files.
    pub fn prepare(&self, tools: &Tools) -> io::Result<Invocation> {
        let temp = tempfile::Builder::new()
            .prefix("workflint-conformance-")
            .tempdir()?;
        let project = temp.path().join("project");
        let empty_config = temp.path().join("config.yaml");
        fs::write(&empty_config, "")?;

        let (config, files) = match self.suite {
            Suite::Err | Suite::Examples | Suite::Ok => {
                fs::create_dir(&project)?;
                if let Some(github) = self.source.parent().map(|dir| dir.join(".github"))
                    && github.is_dir()
                {
                    copy_dir(&github, &project.join(".github"))?;
                }
                fs::copy(&self.source, project.join("test.yaml"))?;
                (empty_config, vec![OsString::from("test.yaml")])
            }
            Suite::Projects => {
                copy_dir(&self.source, &project)?;
                let config = project.join("actionlint.yaml");
                let config = if config.is_file() {
                    config
                } else {
                    empty_config
                };
                (config, workflow_files(&project)?)
            }
        };
        fs::create_dir_all(project.join(".git"))?;
        fs::create_dir_all(project.join(".github").join("workflows"))?;

        let tool = |flag: &str, path: &Option<PathBuf>| {
            let mut arg = OsString::from(flag);
            if let Some(path) = path {
                arg.push(path);
            }
            arg
        };
        let mut args = vec![
            OsString::from("-oneline"),
            OsString::from("-no-color"),
            tool("-shellcheck=", &tools.shellcheck),
            tool("-pyflakes=", &tools.pyflakes),
            OsString::from("-config-file"),
            config.into_os_string(),
        ];
        args.extend(files);
        Ok(Invocation {
            cwd: project,
            args,
            _temp: temp,
        })
    }
}

/// The external tools found on `PATH`, or enabled for one fixture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tools {
    /// The shellcheck executable, if there is one.
    pub shellcheck: Option<PathBuf>,
    /// The pyflakes executable, if there is one.
    pub pyflakes: Option<PathBuf>,
}

impl Tools {
    /// Looks up `shellcheck` and `pyflakes` on `PATH`.
    #[must_use]
    pub fn installed() -> Self {
        Self {
            shellcheck: find_on_path("shellcheck"),
            pyflakes: find_on_path("pyflakes"),
        }
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        let extensions =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
        extensions
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("{program}{}", extension.to_ascii_lowercase()))
            .collect()
    } else {
        vec![program.to_owned()]
    };
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// A prepared fixture: run the binary with `args` in `cwd`. The temporary
/// directory lives as long as this value.
#[derive(Debug)]
pub struct Invocation {
    /// The temporary project, which has `.git` and `.github/workflows`
    /// directories so that both workflint and the reference take it as the
    /// project root.
    pub cwd: PathBuf,
    /// The command-line arguments for the binary.
    pub args: Vec<OsString>,
    _temp: TempDir,
}

/// The `.yml` and `.yaml` files under `workflows/`, in the order of Go's
/// `filepath.Walk`, relative to `project` with `/` separators. The reference
/// lints them with `LintDir` (`linter_test.go:339`).
fn workflow_files(project: &Path) -> io::Result<Vec<OsString>> {
    let mut files = Vec::new();
    collect_workflows(project, Path::new("workflows"), &mut files)?;
    Ok(files)
}

fn collect_workflows(project: &Path, relative: &Path, files: &mut Vec<OsString>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(project.join(relative))?.collect::<io::Result<_>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = relative.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            collect_workflows(project, &path, files)?;
        } else {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".yml") || name.ends_with(".yaml") {
                files.push(OsString::from(path.to_string_lossy().replace('\\', "/")));
            }
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Every fixture of `origin`, found the way the conformance target finds
/// them.
///
/// # Errors
///
/// See [`discover_in`].
pub fn discover(origin: Origin) -> Result<Vec<Fixture>, String> {
    discover_in(origin, &origin.root())
}

/// Every fixture of `origin` in the suite directories under `root`. A suite
/// directory that does not exist has no fixtures.
///
/// # Errors
///
/// Returns a message that names the directory if a suite directory exists
/// but cannot be read.
pub fn discover_in(origin: Origin, root: &Path) -> Result<Vec<Fixture>, String> {
    let mut fixtures = Vec::new();
    for suite in [Suite::Err, Suite::Examples, Suite::Ok, Suite::Projects] {
        let directory = root.join(suite.directory());
        let cannot_read =
            |error: io::Error| format!("cannot read {}: {error}", directory.display());
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(cannot_read(error)),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let path = entry.map_err(cannot_read)?.path();
            if path.is_file() && is_test_file(suite, &path) {
                paths.push(path);
            }
        }
        paths.sort();
        fixtures.extend(paths.iter().map(|path| Fixture::new(origin, path)));
    }
    Ok(fixtures)
}

fn is_test_file(suite: Suite, path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str());
    match suite {
        Suite::Projects => extension == Some("out"),
        _ => matches!(extension, Some("yaml" | "yml")),
    }
}

/// Every fixture of both origins.
///
/// # Errors
///
/// See [`discover_in`].
pub fn all() -> Result<Vec<Fixture>, String> {
    let mut fixtures = discover(Origin::Actionlint)?;
    fixtures.extend(discover(Origin::Workflint)?);
    Ok(fixtures)
}

/// The IDs of every fixture of both origins.
///
/// # Errors
///
/// See [`discover_in`].
pub fn all_ids() -> Result<BTreeSet<String>, String> {
    Ok(all()?.into_iter().map(|fixture| fixture.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(origin: Origin, path: &str) -> Fixture {
        Fixture::new(origin, &origin.root().join(path))
    }

    #[test]
    fn ids_follow_the_checklist_names() {
        assert_eq!(fixture(Origin::Actionlint, "err/a.yaml").id, "err/a");
        assert_eq!(
            fixture(Origin::Actionlint, "examples/b.yaml").id,
            "examples/b"
        );
        assert_eq!(fixture(Origin::Actionlint, "ok/c.yaml").id, "ok/c.yaml");
        assert_eq!(
            fixture(Origin::Actionlint, "projects/d.out").id,
            "projects/d"
        );
        assert_eq!(
            fixture(Origin::Workflint, "err/e.yaml").id,
            "workflint/err/e"
        );
    }

    #[test]
    fn a_project_fixture_is_its_directory_with_its_out_file() {
        let project = fixture(Origin::Actionlint, "projects/issue173.out");
        assert_eq!(project.suite, Suite::Projects);
        assert!(project.source.ends_with("projects/issue173"));
        assert!(project.expected.unwrap().ends_with("projects/issue173.out"));
    }

    // Windows may report a file in place of a directory as not found.
    #[cfg(unix)]
    #[test]
    fn discovery_fails_on_a_suite_directory_it_cannot_read() {
        let root = tempfile::tempdir().unwrap();
        // A file where the `err` directory should be cannot be listed.
        fs::write(root.path().join("err"), "").unwrap();
        let error = discover_in(Origin::Workflint, root.path()).unwrap_err();
        assert!(error.starts_with("cannot read "), "{error}");
        assert!(error.contains("err"), "{error}");
    }

    #[test]
    fn discovery_skips_a_missing_suite_directory() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("ok")).unwrap();
        fs::write(root.path().join("ok").join("a.yaml"), "").unwrap();
        let fixtures = discover_in(Origin::Workflint, root.path()).unwrap();
        let ids: Vec<&str> = fixtures.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["workflint/ok/a.yaml"]);
    }

    #[test]
    fn the_reference_has_the_fixture_counts_of_its_commit() {
        let fixtures = discover(Origin::Actionlint).unwrap();
        let count = |suite| fixtures.iter().filter(|f| f.suite == suite).count();
        assert_eq!(count(Suite::Err), 95);
        assert_eq!(count(Suite::Examples), 51);
        assert_eq!(count(Suite::Ok), 84);
        assert_eq!(count(Suite::Projects), 30);
    }

    fn installed() -> Tools {
        Tools {
            shellcheck: Some(PathBuf::from("/bin/shellcheck")),
            pyflakes: Some(PathBuf::from("/bin/pyflakes")),
        }
    }

    #[test]
    fn err_fixtures_enable_only_the_tool_in_their_name() {
        let tools = fixture(Origin::Actionlint, "err/shellcheck_x.yaml")
            .tools(&installed())
            .unwrap();
        assert_eq!(tools.shellcheck, Some(PathBuf::from("/bin/shellcheck")));
        assert_eq!(tools.pyflakes, None);
        let none = fixture(Origin::Actionlint, "examples/plain.yaml")
            .tools(&installed())
            .unwrap();
        assert_eq!(none, Tools::default());
    }

    #[test]
    fn ok_fixtures_enable_every_installed_tool() {
        let tools = fixture(Origin::Actionlint, "ok/plain.yaml")
            .tools(&installed())
            .unwrap();
        assert_eq!(tools, installed());
    }

    #[test]
    fn project_fixtures_enable_no_tool() {
        let tools = fixture(Origin::Actionlint, "projects/shellcheck.out")
            .tools(&installed())
            .unwrap();
        assert_eq!(tools, Tools::default());
    }

    #[test]
    fn a_fixture_named_after_a_missing_tool_is_skipped() {
        let error = fixture(Origin::Actionlint, "ok/pyflakes_x.yaml")
            .tools(&Tools::default())
            .unwrap_err();
        assert!(error.contains("\"pyflakes\" is not installed"), "{error}");
    }

    #[test]
    fn a_single_file_fixture_is_linted_as_test_yaml_with_an_empty_config() {
        let fixture = fixture(Origin::Actionlint, "examples/local_action_inputs.yaml");
        let invocation = fixture.prepare(&Tools::default()).unwrap();
        let args: Vec<_> = invocation
            .args
            .iter()
            .map(|a| a.to_string_lossy())
            .collect();
        assert_eq!(
            args[..4],
            ["-oneline", "-no-color", "-shellcheck=", "-pyflakes="]
        );
        assert_eq!(args[4], "-config-file");
        assert_eq!(fs::read(&args[5][..]).unwrap(), b"");
        assert_eq!(args[6], "test.yaml");
        assert_eq!(args.len(), 7);
        let cwd = &invocation.cwd;
        assert_eq!(
            fs::read(cwd.join("test.yaml")).unwrap(),
            fs::read(&fixture.source).unwrap()
        );
        assert!(cwd.join(".github/actions/my-action/action.yaml").is_file());
        assert!(cwd.join(".git").is_dir());
        assert!(cwd.join(".github/workflows").is_dir());
    }

    #[test]
    fn a_project_fixture_lints_its_workflows_with_its_config() {
        let fixture = fixture(Origin::Actionlint, "projects/paths_config.out");
        let invocation = fixture.prepare(&Tools::default()).unwrap();
        let args: Vec<_> = invocation
            .args
            .iter()
            .map(|a| a.to_string_lossy())
            .collect();
        assert!(args[5].ends_with("actionlint.yaml"), "{args:?}");
        assert!(Path::new(&args[5][..]).starts_with(&invocation.cwd));
        assert_eq!(
            args[6..],
            [
                "workflows/bar.yaml",
                "workflows/foo.yaml",
                "workflows/nested/piyo.yaml"
            ]
        );
    }

    #[test]
    fn a_project_without_a_config_gets_the_empty_config() {
        let fixture = fixture(Origin::Actionlint, "projects/issue173.out");
        let invocation = fixture.prepare(&Tools::default()).unwrap();
        let config = &invocation.args[5];
        assert_eq!(fs::read(config).unwrap(), b"");
        assert!(!Path::new(config).starts_with(&invocation.cwd));
    }

    #[test]
    fn enabled_tools_are_passed_by_path() {
        let fixture = fixture(
            Origin::Actionlint,
            "err/shellcheck_default_shell_detection.yaml",
        );
        let invocation = fixture.prepare(&installed()).unwrap();
        assert_eq!(invocation.args[2], "-shellcheck=/bin/shellcheck");
        assert_eq!(invocation.args[3], "-pyflakes=/bin/pyflakes");
    }
}
