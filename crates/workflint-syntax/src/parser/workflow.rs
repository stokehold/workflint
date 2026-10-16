//! The workflow root, as the reference's `parse` at `parse.go:1499-1557`.

use workflint_diagnostics::Span;

use super::{KeySpec, Label, MAX_YAML_NODES, MapEntry, Parser, Section, yaml_node_count};
use crate::rules::{EMPTY_WORKFLOW, MISSING_REQUIRED_KEY, TOO_MANY_YAML_NODES};
use crate::{Field, Value, Workflow};

/// The keys of the workflow root.
struct Root;

impl Section for Root {
    type Ast<'a> = Workflow<'a>;
}

static ROOT_KEYS: [KeySpec<Root>; 8] = [
    KeySpec {
        name: "name",
        parse: |p, w, e| w.name = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "run-name",
        parse: |p, w, e| w.run_name = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "on",
        parse: parse_on,
    },
    KeySpec {
        name: "permissions",
        parse: |p, w, e| w.permissions = p.parse_permissions(e.key, e.value),
    },
    KeySpec {
        name: "env",
        parse: |p, w, e| w.env = p.parse_env(e.key, e.value),
    },
    KeySpec {
        name: "defaults",
        parse: |p, w, e| w.defaults = p.parse_defaults(e.key, e.value),
    },
    KeySpec {
        name: "concurrency",
        parse: |p, w, e| w.concurrency = p.parse_concurrency(e.key, e.value),
    },
    KeySpec {
        name: "jobs",
        parse: |p, w, e| w.jobs = p.parse_jobs(e.key, e.value),
    },
];

fn parse_on<'a>(p: &mut Parser<'a, '_>, workflow: &mut Workflow<'a>, entry: &MapEntry<'a>) {
    workflow.on = p.parse_events(entry.key.string, entry.value);
}

impl<'a> Parser<'a, '_> {
    /// Parses the workflow root.
    ///
    /// A file with no document reports `workflow is empty`. A workflow with
    /// more than [`MAX_YAML_NODES`] YAML nodes, aliases expanded, reports
    /// that GitHub rejects it, and nothing more is parsed. Otherwise the
    /// root must be a mapping with the keys of [`ROOT_KEYS`], `on` must be
    /// valid, and `jobs` must be present. The reference reports a missing `on` or
    /// `jobs` at its document node, which starts at the first directive,
    /// the `---`, or the root node, whichever comes first.
    pub(super) fn workflow(&mut self) -> Workflow<'a> {
        let document = self.document;
        let start = document.start();
        let root = document.root().and_then(|id| self.node(id));
        let end = root.map_or(start, |root| root.span.end.max(start));
        let span = Span::new(document.file(), start, end);
        let mut workflow = Workflow {
            span,
            name: Field::Absent,
            run_name: Field::Absent,
            on: Field::Absent,
            permissions: Field::Absent,
            env: Value::Absent,
            defaults: Field::Absent,
            concurrency: Field::Absent,
            jobs: Field::Absent,
        };
        let Some(root) = root else {
            self.error(&EMPTY_WORKFLOW, span, "workflow is empty".to_owned());
            return workflow;
        };
        // GitHub rejects the file, and walking it could take time
        // exponential in its size.
        if yaml_node_count(document, MAX_YAML_NODES + 1) > MAX_YAML_NODES {
            self.error(
                &TOO_MANY_YAML_NODES,
                span,
                format!(
                    "workflow has more than {MAX_YAML_NODES} YAML nodes when its aliases are \
                     expanded. GitHub rejects it with \"Maximum YAML nodes exceeded\""
                ),
            );
            return workflow;
        }
        let entries = self.parse_mapping(Label::Section("workflow"), root, false, true);
        self.parse_keys(entries, "workflow", &ROOT_KEYS, &mut workflow);
        // An invalid `on` is the reference's nil `On`, so it is missing too.
        if !workflow.on.is_valid() {
            self.error(
                &MISSING_REQUIRED_KEY,
                span,
                "\"on\" section is missing in workflow".to_owned(),
            );
        }
        if !workflow.jobs.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                span,
                "\"jobs\" section is missing in workflow".to_owned(),
            );
        }
        workflow
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use workflint_diagnostics::{FileId, SourceFile};

    use crate::testing::{laughs, render};
    use crate::{Permissions, Str, Template, parse};

    fn check(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        render(&file, parse(&file, &document).diagnostics)
    }

    #[test]
    fn a_file_with_no_document_is_an_empty_workflow() {
        for text in ["", "# only a comment\n", "\n\n"] {
            assert_eq!(
                check(text),
                ["1:1: workflow is empty [empty-workflow]"],
                "{text:?}"
            );
        }
    }

    #[test]
    fn missing_on_and_jobs_are_reported_where_the_document_starts() {
        let cases: [(&str, &[&str]); 5] = [
            (
                "# issue #232\non: push\n",
                &[r#"2:1: "jobs" section is missing in workflow [missing-required-key]"#],
            ),
            (
                "# c\n\n---\n\non: push\n",
                &[r#"3:1: "jobs" section is missing in workflow [missing-required-key]"#],
            ),
            (
                "%YAML 1.1\n---\njobs: {test: {runs-on: x, steps: [{run: a}]}}\n",
                &[r#"1:1: "on" section is missing in workflow [missing-required-key]"#],
            ),
            (
                "  on: push\n",
                &[r#"1:3: "jobs" section is missing in workflow [missing-required-key]"#],
            ),
            (
                "name: x\n",
                &[
                    r#"1:1: "jobs" section is missing in workflow [missing-required-key]"#,
                    r#"1:1: "on" section is missing in workflow [missing-required-key]"#,
                ],
            ),
        ];
        for (text, want) in cases {
            assert_eq!(check(text), want, "{text:?}");
        }
    }

    #[test]
    fn a_root_that_is_not_a_mapping_misses_on_and_jobs() {
        assert_eq!(
            check("---\n"),
            [
                r#"1:1: "jobs" section is missing in workflow [missing-required-key]"#,
                r#"1:1: "on" section is missing in workflow [missing-required-key]"#,
                r#"2:1: "workflow" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            ]
        );
        assert_eq!(
            check("[]\n"),
            [
                r#"1:1: "jobs" section is missing in workflow [missing-required-key]"#,
                r#"1:1: "on" section is missing in workflow [missing-required-key]"#,
                r#"1:1: "workflow" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            ]
        );
    }

    /// A root sequence of `n` scalars has `n + 6` events.
    fn sequence_of(n: usize) -> String {
        format!("[{}]\n", vec!["a"; n].join(", "))
    }

    #[test]
    fn a_workflow_over_github_s_node_limit_is_rejected_and_not_parsed() {
        assert_eq!(
            check(&sequence_of(49_995)),
            [
                r#"1:1: workflow has more than 50000 YAML nodes when its aliases are expanded. GitHub rejects it with "Maximum YAML nodes exceeded" [too-many-yaml-nodes]"#
            ]
        );
    }

    #[test]
    fn a_workflow_at_github_s_node_limit_is_parsed() {
        assert_eq!(
            check(&sequence_of(49_994)),
            [
                r#"1:1: "jobs" section is missing in workflow [missing-required-key]"#,
                r#"1:1: "on" section is missing in workflow [missing-required-key]"#,
                r#"1:1: "workflow" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            ]
        );
    }

    #[test]
    fn a_billion_laughs_is_over_the_node_limit() {
        let text = laughs(9, "''");
        let found = check(&text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].ends_with("[too-many-yaml-nodes]"), "{found:?}");
    }

    #[test]
    fn root_keys_are_case_sensitive_and_listed_when_unexpected() {
        assert_eq!(
            check("on: push\njobs: {test: {runs-on: x, steps: [{run: a}]}}\nName: x\n"),
            [
                r#"3:1: unexpected key "Name" for "workflow" section. expected one of "concurrency", "defaults", "env", "jobs", "name", "on", "permissions", "run-name" [unexpected-key]"#
            ]
        );
    }

    #[test]
    fn a_duplicate_root_key_is_reported_and_merge_keys_are_skipped() {
        assert_eq!(
            check(
                "on: push\njobs: {test: {runs-on: x, steps: [{run: a}]}}\n\"<<\": x\n<<: y\non: pull\n"
            ),
            [
                r#"3:1: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
                r#"4:1: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
                r#"5:1: key "on" is duplicated in "workflow" section. previously defined at line:1,col:1 [duplicate-key]"#,
            ]
        );
    }

    #[test]
    fn name_may_be_empty_and_run_name_may_not() {
        assert_eq!(
            check("on: push\njobs: {test: {runs-on: x, steps: [{run: a}]}}\nname:\nrun-name:\n"),
            ["4:10: string should not be empty [empty-string]"]
        );
    }

    #[test]
    fn name_and_run_name_are_parsed() {
        let text = "name: CI\nrun-name: Deploy ${{ inputs.env }}\non: push\njobs: {test: {runs-on: x, steps: [{run: a}]}}\n";
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = parse(&file, &document);

        let workflow = parsed.workflow;
        assert_eq!(workflow.name.value().map(Template::value), Some("CI"));
        assert_eq!(workflow.name.key().map(Str::value), Some("name"));
        assert_eq!(
            workflow.run_name.value().map(Template::value),
            Some("Deploy ${{ inputs.env }}")
        );
        assert!(workflow.on.is_present());
        assert!(workflow.jobs.is_present());
        assert_eq!(parsed.diagnostics, []);
    }

    #[test]
    fn the_shared_sections_are_parsed_at_the_root() {
        let text = "\
on: push
permissions: read-all
env:
  CI: 'true'
defaults:
  run:
    shell: bash
concurrency: ci
jobs: {test: {runs-on: x, steps: [{run: a}]}}
";
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = parse(&file, &document);
        assert_eq!(parsed.diagnostics, []);

        let workflow = parsed.workflow;
        assert!(matches!(
            workflow.permissions.value(),
            Some(Permissions::All(all)) if all.value() == "read-all"
        ));
        assert_eq!(
            workflow.permissions.key().map(Str::value),
            Some("permissions")
        );
        let env = workflow.env.literal().unwrap();
        assert_eq!(
            env.get("ci")
                .and_then(|var| var.value.value.as_ref())
                .map(Template::value),
            Some("true")
        );
        let run = workflow.defaults.value().unwrap().run.value().unwrap();
        assert_eq!(run.shell.value().map(Template::value), Some("bash"));
        let concurrency = workflow.concurrency.value().unwrap();
        assert_eq!(concurrency.group.value().map(Template::value), Some("ci"));
        assert_eq!(concurrency.group.key(), None);
    }

    #[test]
    fn errors_in_the_shared_sections_are_reported_at_the_root() {
        assert_eq!(
            check(
                "on: push
jobs: {test: {runs-on: x, steps: [{run: a}]}}
permissions:
env:
defaults:
concurrency:
  cancel-in-progress: true
"
            ),
            [
                "3:13: string should not be empty [empty-string]",
                r#"4:5: expecting a single ${{...}} expression or mapping value for "env" section, but found plain text node [expected-single-expression]"#,
                r#"5:10: "defaults" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                r#"5:10: "defaults" section should have "run" section [missing-required-key]"#,
                r#"6:1: group name is missing in "concurrency" section [missing-required-key]"#,
            ]
        );
    }
}
