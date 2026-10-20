//! The visitor calls each hook of each rule in the reference's order (`pass.go:50`).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use workflint_diagnostics::{FileId, SourceFile, Span};
use workflint_rules::kind::{self, Kind};
use workflint_rules::{Context, Rule, visit};
use workflint_syntax::{Job, KeyedEntry, Step, Workflow};

/// A rule that appends `<name> <hook> <node>` to a shared log for each hook.
struct Recorder {
    name: &'static str,
    log: Rc<RefCell<Vec<String>>>,
}

impl Recorder {
    fn record(&self, hook: &str, node: &str) {
        self.log
            .borrow_mut()
            .push(format!("{} {hook} {node}", self.name));
    }
}

impl<'a> Rule<'a> for Recorder {
    fn kind(&self) -> &'static Kind {
        &kind::MATRIX
    }

    fn workflow_pre(&mut self, _workflow: &'a Workflow<'a>, _cx: &mut Context<'_>) {
        self.record("workflow-pre", "-");
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.record("job-pre", job.key.value());
    }

    fn step(&mut self, step: &'a Step<'a>, _cx: &mut Context<'_>) {
        let name = step.name.value().map_or("-", |name| name.value());
        self.record("step", name);
    }

    fn job_post(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.record("job-post", job.key.value());
    }

    fn workflow_post(&mut self, _workflow: &'a Workflow<'a>, _cx: &mut Context<'_>) {
        self.record("workflow-post", "-");
    }
}

/// Parses `text` as a workflow, runs two recorders over it, and returns the log.
fn hooks(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut rules: Vec<Box<dyn Rule<'_> + '_>> = vec![
        Box::new(Recorder {
            name: "first",
            log: Rc::clone(&log),
        }),
        Box::new(Recorder {
            name: "second",
            log: Rc::clone(&log),
        }),
    ];
    let mut cx = Context::new(&file);
    visit(&parsed.workflow, &mut rules, &mut cx);
    log.take()
}

#[test]
fn hooks_run_for_every_rule_in_registration_order_and_jobs_in_source_order() {
    let log = hooks(
        "\
on: push
jobs:
  zeta:
    runs-on: ubuntu-latest
    steps:
      - name: z1
        run: echo
      - name: z2
        run: echo
  alpha:
    runs-on: ubuntu-latest
    steps:
      - name: a1
        run: echo
      - name: a2
        run: echo
",
    );
    assert_eq!(
        log,
        [
            "first workflow-pre -",
            "second workflow-pre -",
            "first job-pre zeta",
            "second job-pre zeta",
            "first step z1",
            "second step z1",
            "first step z2",
            "second step z2",
            "first job-post zeta",
            "second job-post zeta",
            "first job-pre alpha",
            "second job-pre alpha",
            "first step a1",
            "second step a1",
            "first step a2",
            "second step a2",
            "first job-post alpha",
            "second job-post alpha",
            "first workflow-post -",
            "second workflow-post -",
        ]
    );
}

#[test]
fn invalid_jobs_and_steps_are_not_visited() {
    let log = hooks(
        "\
on: push
jobs:
  only:
    runs-on: ubuntu-latest
    steps: 42
",
    );
    assert_eq!(
        log,
        [
            "first workflow-pre -",
            "second workflow-pre -",
            "first job-pre only",
            "second job-pre only",
            "first job-post only",
            "second job-post only",
            "first workflow-post -",
            "second workflow-post -",
        ]
    );
    let log = hooks("on: push\njobs: 42\n");
    assert_eq!(
        log,
        [
            "first workflow-pre -",
            "second workflow-pre -",
            "first workflow-post -",
            "second workflow-post -",
        ]
    );
}

#[test]
fn a_report_through_the_context_names_its_rule_and_file() {
    let file = SourceFile::new(FileId::new(3), "test.yaml", "on: push\n").unwrap();
    let mut cx = Context::new(&file);
    assert_eq!(cx.file().path(), "test.yaml");
    let span = Span::new(FileId::new(3), 0, 2);
    cx.report(&workflint_syntax::rules::UNEXPECTED_KEY, span, "message");
    let diagnostics = cx.into_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule.id, "unexpected-key");
    assert_eq!(diagnostics[0].span, span);
    assert_eq!(diagnostics[0].message, "message");
}
