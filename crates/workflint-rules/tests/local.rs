//! Rules look up local actions and local reusable workflows through the context, and
//! `MemoryProject` gives them the outcomes of the linter's cache.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use workflint_diagnostics::{FileId, SourceFile};
use workflint_rules::kind::{self, Kind};
use workflint_rules::local::{LocalMetadata, Lookup, MemoryProject, NoProject};
use workflint_rules::{Context, Rule, visit};
use workflint_syntax::{Exec, ReusableWorkflowMetadata, Step, Workflow};

const ACTION: &str = "name: Greet\nruns:\n  using: node24\n  main: index.js\n";

#[test]
fn a_memory_project_finds_an_action_and_remembers_the_first_lookup() {
    let project = MemoryProject::new()
        .with_file("greet/action.yml", ACTION)
        .with_file("greet/index.js", "");
    let Lookup::Found { metadata, first } = project.action("./greet") else {
        panic!("the action is found");
    };
    assert!(first);
    assert_eq!(metadata.metadata.name, "Greet");
    assert_eq!(metadata.dir.to_str(), Some("greet"));
    assert_eq!(metadata.file.to_str(), Some("greet/action.yml"));
    assert!(project.action_file_exists(&metadata, "index.js"));
    assert!(!project.action_file_exists(&metadata, "main.js"));
    assert!(matches!(
        project.action("./greet"),
        Lookup::Found { first: false, .. }
    ));
}

/// The same cases as `files_in_the_action_directory_are_found` in the tests of the linter's
/// cache, which asks the file system.
#[test]
fn a_memory_project_finds_files_and_directories_as_the_file_system_does() {
    let project = MemoryProject::new()
        .with_file("greet/action.yml", ACTION)
        .with_file("greet/dist/index.js", "")
        .with_file("shared/lib.js", "");
    let Lookup::Found { metadata, .. } = project.action("./x/../greet/.") else {
        panic!("the action is found");
    };
    assert_eq!(metadata.dir.to_str(), Some("greet"));
    assert!(project.action_file_exists(&metadata, "dist/index.js"));
    assert!(project.action_file_exists(&metadata, "./dist/../action.yml"));
    assert!(project.action_file_exists(&metadata, "dist"));
    assert!(project.action_file_exists(&metadata, "dist/"));
    assert!(project.action_file_exists(&metadata, "."));
    assert!(project.action_file_exists(&metadata, "../shared/lib.js"));
    assert!(!project.action_file_exists(&metadata, "index.js"));
    assert!(!project.action_file_exists(&metadata, "dis"));
}

#[test]
fn a_memory_project_prefers_action_yaml() {
    let project = MemoryProject::new()
        .with_file("greet/action.yml", "name: yml\n")
        .with_file("greet/action.yaml", "name: yaml\n");
    let Lookup::Found { metadata, .. } = project.action("./greet") else {
        panic!("the action is found");
    };
    assert_eq!(metadata.metadata.name, "yaml");
}

#[test]
fn a_memory_project_reports_an_invalid_action_once() {
    let project = MemoryProject::new().with_file("broken/action.yml", "outputs: x\n");
    let Lookup::Error(err) = project.action("./broken") else {
        panic!("the action is invalid");
    };
    assert_eq!(
        err.to_string(),
        r#"could not parse action metadata in "broken": line 1: yaml: outputs must be mapping node but scalar node was found at line:1, col:10"#
    );
    assert_eq!(project.action("./broken"), Lookup::None);
    assert_eq!(project.action("./missing"), Lookup::None);
    assert_eq!(project.action("broken"), Lookup::None);
}

#[test]
fn a_memory_project_reports_a_missing_or_invalid_workflow_once() {
    let project = MemoryProject::new().with_file(".github/workflows/push.yaml", "on: push\n");
    let Lookup::Error(err) = project.reusable_workflow("./.github/workflows/missing.yaml") else {
        panic!("the workflow is missing");
    };
    assert_eq!(
        err.to_string(),
        r#"could not read reusable workflow file for "./.github/workflows/missing.yaml": open .github/workflows/missing.yaml: no such file or directory"#
    );
    assert_eq!(
        project.reusable_workflow("./.github/workflows/missing.yaml"),
        Lookup::None
    );
    let Lookup::Error(err) = project.reusable_workflow("./.github/workflows/push.yaml") else {
        panic!("the workflow is invalid");
    };
    assert_eq!(
        err.to_string(),
        r#"error while parsing reusable workflow "./.github/workflows/push.yaml": "workflow_call" event trigger is not found in "on:" at line:1, column:5"#
    );
    assert_eq!(
        project.reusable_workflow("./.github/workflows/push.yaml"),
        Lookup::None
    );
    assert_eq!(
        project.reusable_workflow("./.github/workflows/${{ inputs.x }}.yaml"),
        Lookup::None
    );
}

#[test]
fn a_memory_project_finds_a_registered_workflow_without_its_file() {
    let project = MemoryProject::new().linting(".github/workflows/reusable.yaml");
    project.register_workflow_call(ReusableWorkflowMetadata::default());
    assert_eq!(
        project.reusable_workflow("./.github/workflows/reusable.yaml"),
        Lookup::Found {
            metadata: ReusableWorkflowMetadata::default().into(),
            first: false
        }
    );
}

#[test]
fn no_project_finds_nothing() {
    let project = MemoryProject::new().with_file("greet/action.yml", ACTION);
    let Lookup::Found { metadata, .. } = project.action("./greet") else {
        panic!("the action is found");
    };
    assert_eq!(NoProject.action("./greet"), Lookup::None);
    assert_eq!(NoProject.reusable_workflow("./x.yaml"), Lookup::None);
    assert!(!NoProject.action_file_exists(&metadata, "action.yml"));
}

/// A rule that looks up the `uses:` of each step and records what it found.
struct LookUp {
    log: Rc<RefCell<Vec<String>>>,
}

impl<'a> Rule<'a> for LookUp {
    fn kind(&self) -> &'static Kind {
        &kind::ACTION
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        let Some(Exec::Action(action)) = &step.exec else {
            return;
        };
        let Some(uses) = action.uses.value() else {
            return;
        };
        let uses = uses.value();
        let found = match cx.local_metadata().action(uses) {
            Lookup::None => "none".to_owned(),
            Lookup::Found { metadata, first } => {
                format!("{} first={first}", metadata.metadata.name)
            }
            Lookup::Error(err) => err.to_string(),
        };
        self.log.borrow_mut().push(format!("{uses}: {found}"));
    }

    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        if let Some(event) = workflow.workflow_call_event() {
            cx.local_metadata()
                .register_workflow_call(ReusableWorkflowMetadata::from_event(event));
        }
    }
}

const WORKFLOW: &str = "on: workflow_call
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: ./greet
      - uses: ./greet
      - uses: ./missing
";

/// Visits the workflow in `file` with a [`LookUp`] rule and returns its log.
fn run(file: &SourceFile, cx: &mut Context<'_>) -> Vec<String> {
    let document = workflint_yaml::parse(file).document.unwrap();
    let parsed = workflint_syntax::parse(file, &document);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut rules: Vec<Box<dyn Rule<'_> + '_>> = vec![Box::new(LookUp {
        log: Rc::clone(&log),
    })];
    visit(&parsed.workflow, &mut rules, cx);
    log.take()
}

#[test]
fn rules_look_up_local_metadata_through_the_context() {
    let file = SourceFile::new(FileId::new(0), "ci.yaml", WORKFLOW).unwrap();
    let project = MemoryProject::new()
        .with_file("greet/action.yml", ACTION)
        .linting(".github/workflows/ci.yaml");
    let mut cx = Context::with_local_metadata(&file, &project);
    assert_eq!(
        run(&file, &mut cx),
        [
            "./greet: Greet first=true",
            "./greet: Greet first=false",
            "./missing: none"
        ]
    );
    assert!(matches!(
        project.reusable_workflow("./.github/workflows/ci.yaml"),
        Lookup::Found { first: false, .. }
    ));
}

#[test]
fn a_context_without_a_project_finds_nothing() {
    let file = SourceFile::new(FileId::new(0), "ci.yaml", WORKFLOW).unwrap();
    let mut cx = Context::new(&file);
    assert_eq!(
        run(&file, &mut cx),
        ["./greet: none", "./greet: none", "./missing: none"]
    );
}
