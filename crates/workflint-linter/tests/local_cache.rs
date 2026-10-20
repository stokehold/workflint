//! The project root and the per-project cache of local actions and local reusable workflows, on
//! temporary directories.
//!
//! The expected behaviour is the reference's `findProject` (`project.go:24`),
//! `LocalActionsCache.FindMetadata` (`action_metadata.go:242`), and
//! `LocalReusableWorkflowCache` (`reusable_workflow.go:207-345`).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;
use workflint_diagnostics::quote::quote;
use workflint_linter::{LocalCache, ProjectRoots};
use workflint_rules::local::{LocalAction, LocalMetadata, LocalMetadataError, Lookup};
use workflint_syntax::{ReusableWorkflowMetadata, ReusableWorkflowSecret};

const ACTION: &str = "name: Greet
inputs:
  who:
    required: true
runs:
  using: node24
  main: index.js
";

const REUSABLE: &str = "on:
  workflow_call:
    secrets:
      token:
        required: true
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";

/// A temporary project: a directory with `.git` and `.github/workflows`.
struct Project {
    dir: TempDir,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        Self { dir }
    }

    /// The project root, cleaned as the linter cleans paths.
    fn root(&self) -> PathBuf {
        workflint_linter::path::clean(self.dir.path())
    }

    /// Writes `contents` to `path`, relative to the root, creating its directories.
    fn write(&self, path: &str, contents: impl AsRef<[u8]>) -> &Self {
        let path = self.dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        self
    }

    /// Creates the cache of the project.
    fn cache(&self) -> LocalCache {
        LocalCache::new(self.root())
    }
}

/// The cache's view for linting `.github/workflows/ci.yaml` in the project of `cache`.
fn linting_ci(cache: &LocalCache) -> impl LocalMetadata + '_ {
    cache.for_file(&cache.root().join(".github/workflows/ci.yaml"))
}

fn found<T: std::fmt::Debug>(lookup: Lookup<T>) -> (Arc<T>, bool) {
    match lookup {
        Lookup::Found { metadata, first } => (metadata, first),
        other => panic!("expected metadata, got {other:?}"),
    }
}

fn error<T: std::fmt::Debug>(lookup: Lookup<T>) -> LocalMetadataError {
    match lookup {
        Lookup::Error(err) => err,
        other => panic!("expected an error, got {other:?}"),
    }
}

fn find_project_root(path: &Path, cwd: &Path) -> Option<PathBuf> {
    ProjectRoots::default().find(path, cwd)
}

fn quoted(path: &Path) -> String {
    quote(&path.to_string_lossy())
}

// The project root.

#[test]
fn the_project_root_is_the_nearest_ancestor_with_workflows_and_git() {
    let project = Project::new();
    project.write("sub/.github/workflows/x.yaml", "");
    let root = project.root();
    let cwd = Path::new("/");
    assert_eq!(
        find_project_root(&root.join(".github/workflows/ci.yaml"), cwd),
        Some(root.clone())
    );
    // `sub` has `.github/workflows` but no `.git`.
    assert_eq!(
        find_project_root(&root.join("sub/.github/workflows/x.yaml"), cwd),
        Some(root.clone())
    );
}

#[test]
fn a_git_file_marks_a_project_root_as_a_git_directory_does() {
    let project = Project::new();
    project.write("sub/.git", "gitdir: ../.git/worktrees/sub\n");
    project.write("sub/.github/workflows/x.yaml", "");
    let root = project.root();
    assert_eq!(
        find_project_root(&root.join("sub/.github/workflows/x.yaml"), Path::new("/")),
        Some(root.join("sub"))
    );
}

#[test]
fn a_relative_path_is_found_from_the_working_directory() {
    let project = Project::new();
    let root = project.root();
    assert_eq!(
        find_project_root(Path::new("./.github/workflows/ci.yaml"), &root),
        Some(root.clone())
    );
    assert_eq!(
        find_project_root(Path::new("../workflows/ci.yaml"), &root.join(".github/x")),
        Some(root)
    );
}

#[test]
fn a_file_outside_any_project_has_no_root() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
    let root = workflint_linter::path::clean(dir.path());
    // Neither `dir` nor, presumably, any ancestor of the temporary directory is a project.
    let found = find_project_root(&root.join(".github/workflows/ci.yaml"), Path::new("/"));
    assert!(
        found.as_ref().is_none_or(|found| !found.starts_with(&root)),
        "{found:?}"
    );
}

#[test]
fn a_workflows_file_is_not_a_workflows_directory() {
    let project = Project::new();
    project.write("sub/.git/HEAD", "");
    project.write("sub/.github/workflows", "");
    let root = project.root();
    assert_eq!(
        find_project_root(&root.join("sub/x.yaml"), Path::new("/")),
        Some(root)
    );
}

#[test]
fn files_in_one_directory_share_the_first_answer() {
    let dir = tempfile::tempdir().unwrap();
    let root = workflint_linter::path::clean(dir.path());
    let cwd = Path::new("/");
    let mut roots = ProjectRoots::default();
    let before = roots.find(&root.join("a/x.yaml"), cwd);
    assert!(
        before
            .as_ref()
            .is_none_or(|found| !found.starts_with(&root))
    );
    // The directory becomes a project after the first lookup. The roots read the file system
    // once per directory, so a later file in it gets the first answer.
    fs::create_dir_all(root.join("a/.github/workflows")).unwrap();
    fs::create_dir_all(root.join("a/.git")).unwrap();
    assert_eq!(roots.find(&root.join("a/y.yaml"), cwd), before);
    assert_eq!(
        ProjectRoots::default().find(&root.join("a/y.yaml"), cwd),
        Some(root.join("a"))
    );
}

#[test]
fn a_directory_shares_the_answer_of_its_ancestors() {
    let project = Project::new();
    let root = project.root();
    let cwd = Path::new("/");
    let mut roots = ProjectRoots::default();
    assert_eq!(
        roots.find(&root.join(".github/workflows/ci.yaml"), cwd),
        Some(root.clone())
    );
    assert_eq!(
        roots.find(&root.join("sub/dir/x.yaml"), cwd),
        Some(root.clone())
    );
    // A nested project that appears later is not seen, because `sub` was answered from its
    // parent.
    project.write("sub/.git/HEAD", "");
    project.write("sub/.github/workflows/x.yaml", "");
    assert_eq!(roots.find(&root.join("sub/y.yaml"), cwd), Some(root));
}

// Local actions.

#[test]
fn a_local_action_is_decoded_with_its_directory_and_file() {
    let project = Project::new();
    project.write(".github/actions/greet/action.yml", ACTION);
    let cache = project.cache();
    let (action, first) = found(linting_ci(&cache).action("./.github/actions/greet"));
    assert!(first);
    assert_eq!(action.metadata.name, "Greet");
    assert!(action.metadata.inputs["who"].required);
    assert_eq!(action.dir, project.root().join(".github/actions/greet"));
    assert_eq!(
        action.file,
        project.root().join(".github/actions/greet/action.yml")
    );
}

#[test]
fn only_the_first_lookup_of_an_action_in_the_run_is_first() {
    let project = Project::new();
    project.write("greet/action.yml", ACTION);
    let cache = project.cache();
    let first_file = cache.for_file(&project.root().join(".github/workflows/a.yaml"));
    let second_file = cache.for_file(&project.root().join(".github/workflows/b.yaml"));
    let (one, first) = found(first_file.action("./greet"));
    assert!(first);
    let (two, first) = found(first_file.action("./greet"));
    assert!(!first);
    let (three, first) = found(second_file.action("./greet"));
    assert!(!first);
    assert!(Arc::ptr_eq(&one, &two) && Arc::ptr_eq(&two, &three));
}

#[test]
fn the_raw_spec_is_the_cache_key() {
    let project = Project::new();
    project.write("greet/action.yml", ACTION);
    let cache = project.cache();
    let local = linting_ci(&cache);
    let (one, first) = found(local.action("./greet"));
    assert!(first);
    // The same directory under another spec is read again.
    let (two, first) = found(local.action("./greet/"));
    assert!(first);
    assert!(!Arc::ptr_eq(&one, &two));
    assert_eq!(one.dir, two.dir);
}

#[test]
fn action_yaml_wins_over_action_yml() {
    let project = Project::new();
    project.write("greet/action.yaml", "name: from yaml\n");
    project.write("greet/action.yml", "name: from yml\n");
    let cache = project.cache();
    let (action, _) = found(linting_ci(&cache).action("./greet"));
    assert_eq!(action.metadata.name, "from yaml");
    assert_eq!(action.file, project.root().join("greet/action.yaml"));
}

#[test]
fn an_unreadable_action_yaml_falls_back_to_action_yml() {
    let project = Project::new();
    fs::create_dir_all(project.root().join("greet/action.yaml")).unwrap();
    project.write("greet/action.yml", "name: from yml\n");
    let cache = project.cache();
    let (action, _) = found(linting_ci(&cache).action("./greet"));
    assert_eq!(action.metadata.name, "from yml");
}

#[test]
fn a_missing_action_is_not_an_error() {
    let project = Project::new();
    project.write("empty/README.md", "");
    let cache = project.cache();
    let local = linting_ci(&cache);
    assert_eq!(local.action("./empty"), Lookup::None);
    assert_eq!(local.action("./empty"), Lookup::None);
    assert_eq!(local.action("./does-not-exist"), Lookup::None);
}

#[test]
fn an_action_spec_that_is_not_local_finds_nothing() {
    let project = Project::new();
    project.write("greet/action.yml", ACTION);
    let cache = project.cache();
    let local = linting_ci(&cache);
    assert_eq!(local.action("greet"), Lookup::None);
    assert_eq!(local.action("actions/checkout@v5"), Lookup::None);
    assert_eq!(local.action("docker://alpine"), Lookup::None);
}

#[test]
fn a_parse_error_is_returned_on_the_first_lookup_only() {
    let project = Project::new();
    project.write("broken/action.yml", "outputs: x\n");
    let cache = project.cache();
    let local = linting_ci(&cache);
    let err = error(local.action("./broken"));
    assert_eq!(
        err.to_string(),
        format!(
            "could not parse action metadata in {}: line 1: yaml: outputs must be mapping node but scalar node was found at line:1, col:10",
            quoted(&project.root().join("broken"))
        )
    );
    assert_eq!(local.action("./broken"), Lookup::None);
    let other = cache.for_file(&project.root().join(".github/workflows/other.yaml"));
    assert_eq!(other.action("./broken"), Lookup::None);
}

#[test]
fn an_action_file_that_does_not_decode_is_a_yaml_error() {
    let project = Project::new();
    project.write("binary/action.yml", b"name: \xFF\n");
    let cache = project.cache();
    let err = error(linting_ci(&cache).action("./binary"));
    assert_eq!(
        err.to_string(),
        format!(
            "could not parse action metadata in {}: yaml: invalid leading UTF-8 octet",
            quoted(&project.root().join("binary"))
        )
    );
}

#[test]
fn the_action_directory_is_cleaned_as_go_joins_paths() {
    let project = Project::new();
    project.write("greet/action.yml", ACTION);
    let cache = project.cache();
    let (action, _) = found(linting_ci(&cache).action("./x/../greet/."));
    assert_eq!(action.dir, project.root().join("greet"));
}

#[test]
fn files_in_the_action_directory_are_found() {
    let project = Project::new();
    project.write("greet/action.yml", ACTION);
    project.write("greet/dist/index.js", "");
    let cache = project.cache();
    let local = linting_ci(&cache);
    let (action, _) = found(local.action("./greet"));
    assert!(local.action_file_exists(&action, "dist/index.js"));
    assert!(local.action_file_exists(&action, "./dist/../action.yml"));
    assert!(local.action_file_exists(&action, "dist"));
    assert!(!local.action_file_exists(&action, "index.js"));
}

#[test]
fn a_file_is_looked_up_in_the_directory_of_the_action_given() {
    let project = Project::new();
    project.write("other/main.js", "");
    let cache = project.cache();
    let action = LocalAction {
        metadata: workflint_syntax::ActionMetadata::default(),
        dir: project.root().join("other"),
        file: project.root().join("other/action.yml"),
    };
    assert!(linting_ci(&cache).action_file_exists(&action, "main.js"));
}

// Local reusable workflows.

#[test]
fn a_local_reusable_workflow_is_decoded_from_its_file() {
    let project = Project::new();
    project.write(".github/workflows/reusable.yaml", REUSABLE);
    let cache = project.cache();
    let local = linting_ci(&cache);
    let (metadata, first) = found(local.reusable_workflow("./.github/workflows/reusable.yaml"));
    assert!(first);
    assert!(metadata.secrets["token"].required);
    let (again, first) = found(local.reusable_workflow("./.github/workflows/reusable.yaml"));
    assert!(!first);
    assert!(Arc::ptr_eq(&metadata, &again));
}

#[test]
fn a_missing_reusable_workflow_is_an_error_on_the_first_lookup_only() {
    let project = Project::new();
    let cache = project.cache();
    let local = linting_ci(&cache);
    let spec = "./.github/workflows/missing.yaml";
    let err = error(local.reusable_workflow(spec));
    // Go's message for a missing file: its own on Unix, the OS's on Windows.
    let missing = if cfg!(windows) {
        "The system cannot find the file specified."
    } else {
        "no such file or directory"
    };
    let path = project
        .root()
        .join(".github")
        .join("workflows")
        .join("missing.yaml");
    assert_eq!(
        err.to_string(),
        format!(
            "could not read reusable workflow file for \"{spec}\": open {}: {missing}",
            path.display()
        )
    );
    assert_eq!(local.reusable_workflow(spec), Lookup::None);
}

#[test]
fn an_invalid_reusable_workflow_is_an_error_on_the_first_lookup_only() {
    let project = Project::new();
    project.write(".github/workflows/push.yaml", "on: push\n");
    let cache = project.cache();
    let local = linting_ci(&cache);
    let spec = "./.github/workflows/push.yaml";
    let err = error(local.reusable_workflow(spec));
    assert_eq!(
        err.to_string(),
        format!(
            r#"error while parsing reusable workflow "{spec}": "workflow_call" event trigger is not found in "on:" at line:1, column:5"#
        )
    );
    assert_eq!(local.reusable_workflow(spec), Lookup::None);
}

#[test]
fn a_reusable_workflow_spec_with_an_expression_or_not_local_finds_nothing() {
    let project = Project::new();
    project.write(".github/workflows/reusable.yaml", REUSABLE);
    let cache = project.cache();
    let local = linting_ci(&cache);
    assert_eq!(
        local.reusable_workflow("./.github/workflows/${{ inputs.name }}.yaml"),
        Lookup::None
    );
    assert_eq!(
        local.reusable_workflow("owner/repo/.github/workflows/reusable.yaml@v1"),
        Lookup::None
    );
}

/// The metadata that `reusable.yaml` registers in the tests: one optional secret.
fn registered() -> ReusableWorkflowMetadata {
    let mut metadata = ReusableWorkflowMetadata::default();
    metadata.secrets.insert(
        "registered".to_owned(),
        ReusableWorkflowSecret {
            name: "registered".to_owned(),
            required: false,
        },
    );
    metadata
}

#[test]
fn a_registered_workflow_is_found_without_reading_its_file() {
    let project = Project::new();
    // The file does not exist, so a lookup that read it would fail.
    let cache = project.cache();
    cache
        .for_file(&project.root().join(".github/workflows/reusable.yaml"))
        .register_workflow_call(registered());
    let local = linting_ci(&cache);
    let (metadata, first) = found(local.reusable_workflow("./.github/workflows/reusable.yaml"));
    assert!(!first);
    assert_eq!(*metadata, registered());
}

#[test]
fn registration_keeps_an_earlier_lookup() {
    let project = Project::new();
    project.write(".github/workflows/reusable.yaml", REUSABLE);
    let cache = project.cache();
    let spec = "./.github/workflows/reusable.yaml";
    let (read, _) = found(linting_ci(&cache).reusable_workflow(spec));
    cache
        .for_file(&project.root().join(".github/workflows/reusable.yaml"))
        .register_workflow_call(registered());
    let (metadata, _) = found(linting_ci(&cache).reusable_workflow(spec));
    assert!(Arc::ptr_eq(&read, &metadata));
}

#[test]
fn registration_keeps_an_earlier_registration() {
    let project = Project::new();
    let cache = project.cache();
    let reusable = cache.for_file(&project.root().join(".github/workflows/reusable.yaml"));
    reusable.register_workflow_call(registered());
    reusable.register_workflow_call(ReusableWorkflowMetadata::default());
    let (metadata, _) =
        found(linting_ci(&cache).reusable_workflow("./.github/workflows/reusable.yaml"));
    assert_eq!(*metadata, registered());
}

#[test]
fn a_workflow_is_registered_under_its_path_from_the_project_root() {
    let project = Project::new();
    let cache = project.cache();
    cache
        .for_file(&project.root().join("sub/./dir/../reusable.yaml"))
        .register_workflow_call(registered());
    let local = linting_ci(&cache);
    found(local.reusable_workflow("./sub/reusable.yaml"));
}

#[test]
fn a_workflow_outside_the_project_root_is_not_registered() {
    let project = Project::new();
    let cache = project.cache();
    let outside = project.root().join("../repo-other/reusable.yaml");
    cache
        .for_file(&outside)
        .register_workflow_call(registered());
    let local = linting_ci(&cache);
    assert!(matches!(
        local.reusable_workflow("./../repo-other/reusable.yaml"),
        Lookup::Error(_)
    ));
}

#[test]
fn a_recorded_invalid_workflow_is_not_read() {
    let project = Project::new();
    // The file exists, but the reference never reads a spec it found invalid.
    project.write(".github/workflows/reusable.yaml@main", REUSABLE);
    let cache = project.cache();
    let spec = "./.github/workflows/reusable.yaml@main";
    linting_ci(&cache).record_invalid_workflow(spec);
    assert_eq!(linting_ci(&cache).reusable_workflow(spec), Lookup::None);
}

#[test]
fn recording_an_invalid_workflow_keeps_an_earlier_lookup() {
    let project = Project::new();
    project.write(".github/workflows/reusable.yaml", REUSABLE);
    let cache = project.cache();
    let spec = "./.github/workflows/reusable.yaml";
    let (read, _) = found(linting_ci(&cache).reusable_workflow(spec));
    linting_ci(&cache).record_invalid_workflow(spec);
    let (metadata, _) = found(linting_ci(&cache).reusable_workflow(spec));
    assert!(Arc::ptr_eq(&read, &metadata));
}
