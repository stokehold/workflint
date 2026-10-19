//! The `job-needs` rule: duplicate and unknown job IDs in `needs`, duplicate job IDs, and cycles
//! in the dependencies that `needs` makes.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_job_needs.go`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{Job, KeyedEntry, Workflow};
use workflint_yaml::simple_lowercase;

use crate::{Context, Rule};

/// A job lists the same job twice in `needs`, ignoring case.
pub(crate) static DUPLICATE_NEEDS: RuleMeta = RuleMeta {
    id: "duplicate-needs",
    description: "A job lists the same job twice in `needs`, ignoring case",
    kind: "job-needs",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-needs.md",
};

/// Two jobs have the same ID, ignoring case.
pub(crate) static DUPLICATE_JOB_ID: RuleMeta = RuleMeta {
    id: "duplicate-job-id",
    description: "Two jobs have the same ID, ignoring case",
    kind: "job-needs",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-job-id.md",
};

/// A job lists a job in `needs` that the workflow does not have.
pub(crate) static UNKNOWN_NEEDED_JOB: RuleMeta = RuleMeta {
    id: "unknown-needed-job",
    description: "A job lists a job in `needs` that the workflow does not have",
    kind: "job-needs",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-needed-job.md",
};

/// The jobs' `needs` make a cycle.
pub(crate) static CYCLIC_NEEDS: RuleMeta = RuleMeta {
    id: "cyclic-needs",
    description: "The jobs' `needs` make a cycle",
    kind: "job-needs",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/cyclic-needs.md",
};

/// A job of the dependency graph, the reference's `jobNode`.
struct Node<'a> {
    /// The folded job ID.
    id: &'a str,
    /// The folded job IDs of `needs`, without duplicates, in source order.
    needs: Vec<Cow<'a, str>>,
    /// The span of the job ID.
    span: Span,
}

/// Where the depth-first search is with a job, the reference's `nodeStatus`.
#[derive(Clone, Copy, PartialEq)]
enum Status {
    /// Not visited yet.
    New,
    /// On the path of the search.
    Active,
    /// Visited with every job it needs.
    Finished,
}

/// The reference's `RuleJobNeeds`. It collects the jobs in source order, where the reference
/// keeps them in a Go map, so the cycle it reports does not depend on map order
/// (`docs/differences.md#deterministic-order`).
#[derive(Default)]
pub(crate) struct JobNeeds<'a> {
    nodes: Vec<Node<'a>>,
    /// The index in `nodes` of each folded job ID.
    index: BTreeMap<&'a str, usize>,
}

impl<'a> Rule<'a> for JobNeeds<'a> {
    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let mut needs: Vec<Cow<'a, str>> = Vec::new();
        // An item that is not a valid string is `None`, and the parser has reported it.
        for need in job
            .value
            .needs
            .value()
            .into_iter()
            .flat_map(|needs| needs.iter().flatten())
        {
            let id = simple_lowercase(need.value());
            if needs.contains(&id) {
                cx.report(
                    &DUPLICATE_NEEDS,
                    need.span(),
                    format!(
                        "job ID {} duplicates in \"needs\" section. note that job ID is case insensitive",
                        quote(need.value())
                    ),
                );
                continue;
            }
            if !id.is_empty() {
                needs.push(id);
            }
        }

        let id = job.folded();
        if id.is_empty() {
            return;
        }
        let node = Node {
            id,
            needs,
            span: job.key.span(),
        };
        // The parser drops a job whose ID repeats one before it, so this is reached only when a
        // caller passes the same job twice. The later job replaces the earlier, as in the
        // reference.
        if let Some(&index) = self.index.get(id) {
            if let Some(previous) = self.nodes.get_mut(index) {
                cx.report(
                    &DUPLICATE_JOB_ID,
                    job.key.span(),
                    format!(
                        "job ID {} duplicates. previously defined at {}. note that job ID is case insensitive",
                        quote(job.key.value()),
                        cx.file().line_col(previous.span.start),
                    ),
                );
                *previous = node;
            }
            return;
        }
        self.index.insert(id, self.nodes.len());
        self.nodes.push(node);
    }

    fn workflow_post(&mut self, _workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        let Some(resolved) = self.resolve(cx) else {
            return;
        };
        if let Some(cycle) = first_cycle(&resolved) {
            self.report_cycle(&cycle, cx);
        }
    }
}

impl JobNeeds<'_> {
    /// Returns the indexes of the jobs that each job needs, or `None` after reporting each need
    /// that names no job of the workflow. The reference skips the search for cycles then.
    fn resolve(&self, cx: &mut Context<'_>) -> Option<Vec<Vec<usize>>> {
        let mut valid = true;
        let mut resolved = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let mut edges = Vec::with_capacity(node.needs.len());
            for need in &node.needs {
                if let Some(&index) = self.index.get(need.as_ref()) {
                    edges.push(index);
                } else {
                    cx.report(
                        &UNKNOWN_NEEDED_JOB,
                        node.span,
                        format!(
                            "job {} needs job {} which does not exist in this workflow",
                            quote(node.id),
                            quote(need)
                        ),
                    );
                    valid = false;
                }
            }
            resolved.push(edges);
        }
        valid.then_some(resolved)
    }

    /// Reports `cycle`, the indexes of its jobs in the order of their edges, starting at the job
    /// whose ID comes first in the file, as the reference does.
    fn report_cycle(&self, cycle: &[usize], cx: &mut Context<'_>) {
        let nodes: Vec<_> = cycle
            .iter()
            .filter_map(|&index| self.nodes.get(index))
            .collect();
        let Some((start, first)) = nodes
            .iter()
            .enumerate()
            .min_by_key(|(_, node)| node.span.start)
        else {
            return;
        };
        let path: Vec<_> = nodes
            .iter()
            .cycle()
            .skip(start)
            .take(nodes.len() + 1)
            .map(|node| quote(node.id))
            .collect();
        cx.report(
            &CYCLIC_NEEDS,
            first.span,
            format!(
                "cyclic dependencies in \"needs\" job configurations are detected. detected cycle is {}",
                path.join(" -> ")
            ),
        );
    }
}

/// Returns the first cycle that a depth-first search finds in `graph`, which lists the jobs that
/// each job needs, as the indexes of its jobs in the order of their edges. The search starts from
/// each job in source order and follows `needs` in source order.
///
/// The reference's `detectFirstCycle` is recursive. This search keeps its path on a heap stack,
/// so a long chain of `needs` cannot overflow the call stack. When it meets a job on its path,
/// the cycle is the path from that job on. That is the cycle the reference's `collectCycle`
/// finds: from each job on the path, the first edge in `needs` order to a job on the path leads
/// to the next job of the path, since an earlier edge to the path would have ended the search
/// there.
fn first_cycle(graph: &[Vec<usize>]) -> Option<Vec<usize>> {
    let mut status = vec![Status::New; graph.len()];
    // Each job on the path with the index of the next edge to follow from it.
    let mut path: Vec<(usize, usize)> = Vec::new();
    for root in 0..graph.len() {
        if status.get(root) != Some(&Status::New) {
            continue;
        }
        set(&mut status, root, Status::Active);
        path.push((root, 0));
        while let Some((job, next)) = path.last_mut() {
            let Some(&needed) = graph.get(*job).and_then(|edges| edges.get(*next)) else {
                set(&mut status, *job, Status::Finished);
                path.pop();
                continue;
            };
            *next += 1;
            match status.get(needed) {
                Some(Status::Active) => {
                    let from = path.iter().position(|&(job, _)| job == needed)?;
                    return Some(path.iter().skip(from).map(|&(job, _)| job).collect());
                }
                Some(Status::New) => {
                    set(&mut status, needed, Status::Active);
                    path.push((needed, 0));
                }
                Some(Status::Finished) | None => {}
            }
        }
    }
    None
}

/// Sets the status of job `index`.
fn set(status: &mut [Status], index: usize, to: Status) {
    if let Some(slot) = status.get_mut(index) {
        *slot = to;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::fmt::Write as _;

    use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

    use super::{JobNeeds, first_cycle};
    use crate::{Context, Rule, visit};

    /// Renders the diagnostics of `cx` in the one-line format.
    fn render(file: &SourceFile, cx: Context<'_>) -> Vec<String> {
        let mut out = Vec::new();
        render_oneline(&mut out, file, &cx.into_diagnostics(), Color::Off).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Lints `text` with only the `job-needs` rule and returns its diagnostics in the one-line
    /// format.
    fn lint(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(JobNeeds::default())];
        let mut cx = Context::new(&file);
        visit(&parsed.workflow, &mut rules, &mut cx);
        render(&file, cx)
    }

    /// Returns a workflow of jobs, each written as its ID and the IDs it needs.
    fn workflow(jobs: &[(&str, &[&str])]) -> String {
        let mut text = String::from("on: push\njobs:\n");
        for (id, needs) in jobs {
            writeln!(
                text,
                "  {id}:\n    needs: [{}]\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo",
                needs.join(", ")
            )
            .unwrap();
        }
        text
    }

    #[test]
    fn a_need_repeated_in_another_case_is_reported_at_the_repeat() {
        assert_eq!(
            lint(&workflow(&[("a", &["b", "B", "b"]), ("b", &[])])),
            [
                r#"test.yaml:4:16: job ID "B" duplicates in "needs" section. note that job ID is case insensitive [duplicate-needs]"#,
                r#"test.yaml:4:19: job ID "b" duplicates in "needs" section. note that job ID is case insensitive [duplicate-needs]"#,
            ]
        );
    }

    #[test]
    fn unknown_needs_are_reported_at_the_job_id_with_folded_ids() {
        assert_eq!(
            lint(&workflow(&[("Foo", &["Bar", "baz"]), ("baz", &["Qux"])])),
            [
                r#"test.yaml:3:3: job "foo" needs job "bar" which does not exist in this workflow [unknown-needed-job]"#,
                r#"test.yaml:8:3: job "baz" needs job "qux" which does not exist in this workflow [unknown-needed-job]"#,
            ]
        );
    }

    #[test]
    fn an_unknown_need_skips_the_search_for_cycles() {
        assert_eq!(
            lint(&workflow(&[("a", &["b"]), ("b", &["a", "c"])])),
            [
                r#"test.yaml:8:3: job "b" needs job "c" which does not exist in this workflow [unknown-needed-job]"#
            ]
        );
    }

    #[test]
    fn a_job_that_needs_itself_is_a_cycle() {
        assert_eq!(
            lint(&workflow(&[("a", &["A"])])),
            [
                r#"test.yaml:3:3: cyclic dependencies in "needs" job configurations are detected. detected cycle is "a" -> "a" [cyclic-needs]"#
            ]
        );
    }

    #[test]
    fn the_cycle_starts_at_the_job_written_first() {
        assert_eq!(
            lint(&workflow(&[
                ("x", &["c"]),
                ("b", &["c"]),
                ("a", &["b"]),
                ("c", &["a"]),
            ])),
            [
                r#"test.yaml:8:3: cyclic dependencies in "needs" job configurations are detected. detected cycle is "b" -> "c" -> "a" -> "b" [cyclic-needs]"#
            ]
        );
    }

    /// Two cycles, `a -> b -> a` and `c -> d -> c`. Only the one found first is reported, and
    /// the search starts from the jobs in source order.
    #[test]
    fn only_the_first_cycle_in_source_order_is_reported() {
        let jobs: [(&str, &[&str]); 4] =
            [("c", &["d"]), ("a", &["b"]), ("d", &["c"]), ("b", &["a"])];
        assert_eq!(
            lint(&workflow(&jobs)),
            [
                r#"test.yaml:3:3: cyclic dependencies in "needs" job configurations are detected. detected cycle is "c" -> "d" -> "c" [cyclic-needs]"#
            ]
        );
    }

    /// Runs the rule's hooks over the jobs of `text` in the order of `order`, the indexes of the
    /// jobs in source order, as the reference's map order may visit them. Each job keeps the
    /// position of its ID.
    fn lint_in_order(text: &str, order: &[usize]) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let jobs = parsed.workflow.jobs.value().unwrap().entries();
        let mut rule = JobNeeds::default();
        let mut cx = Context::new(&file);
        for &index in order {
            rule.job_pre(&jobs[index], &mut cx);
        }
        rule.workflow_post(&parsed.workflow, &mut cx);
        render(&file, cx)
    }

    #[test]
    fn the_cycle_does_not_depend_on_the_order_of_the_jobs() {
        let text = workflow(&[
            ("setup", &[]),
            ("a", &["setup", "b"]),
            ("c", &["d"]),
            ("d", &["a"]),
            ("b", &["c"]),
        ]);
        let expected = [
            r#"test.yaml:8:3: cyclic dependencies in "needs" job configurations are detected. detected cycle is "a" -> "b" -> "c" -> "d" -> "a" [cyclic-needs]"#,
        ];
        for order in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0], [3, 0, 4, 2, 1]] {
            assert_eq!(lint_in_order(&text, &order), expected, "{order:?}");
        }
    }

    /// The parser drops a job whose ID repeats an earlier one, so the rule sees a duplicate only
    /// when its hook runs twice for the same ID.
    #[test]
    fn a_duplicate_job_id_is_reported_at_the_later_job() {
        let text = workflow(&[("test", &[])]);
        let file = SourceFile::new(FileId::new(0), "test.yaml", &text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let job = parsed.workflow.jobs.value().unwrap().iter().next().unwrap();
        let mut rule = JobNeeds::default();
        let mut cx = Context::new(&file);
        rule.job_pre(job, &mut cx);
        rule.job_pre(job, &mut cx);
        rule.workflow_post(&parsed.workflow, &mut cx);
        assert_eq!(
            render(&file, cx),
            [
                r#"test.yaml:3:3: job ID "test" duplicates. previously defined at line:3,col:3. note that job ID is case insensitive [duplicate-job-id]"#
            ]
        );
    }

    #[test]
    fn a_long_chain_of_needs_does_not_overflow_the_stack() {
        let count = 200_000;
        let mut graph: Vec<Vec<usize>> = (1..count).map(|next| vec![next]).collect();
        graph.push(vec![0]);
        let cycle = first_cycle(&graph).unwrap();
        assert_eq!(cycle.len(), count);
        assert_eq!(cycle.first(), Some(&0));

        graph.pop();
        graph.push(Vec::new());
        assert_eq!(first_cycle(&graph), None);
    }

    #[test]
    fn the_search_finds_the_cycle_behind_finished_jobs() {
        // 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3, 3 -> 2.
        let graph = vec![vec![1, 2], vec![3], vec![3], vec![2]];
        assert_eq!(first_cycle(&graph), Some(vec![3, 2]));
    }
}
