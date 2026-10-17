//! The rule pages in `docs/rules/` (ADR 0008, ADR 0029): every example runs as a test, every
//! registered rule has a page, and the rules index in `docs/rules/README.md` matches the registry.
//! `docs/rules/README.md` describes the page format that this test reads.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use workflint_diagnostics::{
    Category, Color, FileId, RuleMeta, Severity, SourceFile, render_oneline, sort_and_dedup,
};
use workflint_rules::registry;

/// The prefix of every `docs_url`. The page name follows it.
const DOCS_URL_PREFIX: &str = "https://github.com/stokehold/workflint/blob/main/docs/rules/";

/// The path that the diagnostics of an example name.
const EXAMPLE_PATH: &str = "test.yaml";

/// The comments around the rules index in `docs/rules/README.md`.
const INDEX_START: &str = "<!-- rules-index:start -->";
const INDEX_END: &str = "<!-- rules-index:end -->";

/// The rules that need no page yet: the `workflint-expr` rules, whose pages M4.11 (#148) writes.
/// M4.11 removes this exemption.
fn exempt(id: &str) -> bool {
    workflint_expr::rules::ALL.iter().any(|rule| rule.id == id)
}

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

/// Reads every page in `docs/rules/`, keyed by rule ID, without `README.md`.
fn read_pages() -> BTreeMap<String, String> {
    let mut pages = BTreeMap::new();
    for entry in fs::read_dir(docs_dir().join("rules")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if name == "README.md" {
            continue;
        }
        let Some(id) = name.strip_suffix(".md") else {
            panic!("docs/rules/{name} is not a Markdown page");
        };
        pages.insert(id.to_owned(), fs::read_to_string(&path).unwrap());
    }
    pages
}

/// A failing example: a workflow and the one-line diagnostics it gives.
#[derive(Debug)]
struct Failing {
    workflow: String,
    expected: Vec<String>,
}

/// A rule page, as `docs/rules/README.md` describes it.
#[derive(Debug, Default)]
struct Page {
    title: String,
    kind: String,
    category: String,
    severity: String,
    description: String,
    /// The `##` headings in page order.
    headings: Vec<String>,
    failing: Vec<Failing>,
    passing: Vec<String>,
    /// The rule IDs of the pages that this page links, such as `unexpected-key` for
    /// `[unexpected-key](unexpected-key.md)`.
    linked_rules: Vec<String>,
    /// The anchors of the `docs/differences.md` entries that this page links.
    differences: Vec<String>,
}

/// The section of the page that a line is in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Head,
    Failing,
    Passing,
    Other,
}

/// Splits a Markdown table row into its trimmed cells.
fn cells(row: &str) -> Option<Vec<&str>> {
    let inner = row.trim().strip_prefix('|')?.strip_suffix('|')?;
    Some(inner.split('|').map(str::trim).collect())
}

/// Returns the targets of the Markdown links `](target)` in `line`.
fn link_targets(line: &str) -> impl Iterator<Item = &str> {
    line.split("](")
        .skip(1)
        .filter_map(|rest| rest.split_once(')').map(|(target, _)| target))
}

/// Parses a page. The error says what does not follow the format.
fn parse_page(text: &str) -> Result<Page, String> {
    let mut page = Page::default();
    let mut lines = text.lines();
    lines
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .ok_or("the page does not start with `# <rule-id>`")?
        .clone_into(&mut page.title);

    let mut section = Section::Head;
    let mut head: Vec<&str> = Vec::new();
    // The YAML block of a failing example that waits for its `text` block.
    let mut pending: Option<String> = None;
    while let Some(line) = lines.next() {
        if let Some(heading) = line.strip_prefix("## ") {
            if pending.is_some() {
                return Err("a failing example has no `text` block".to_owned());
            }
            page.headings.push(heading.to_owned());
            section = if heading.starts_with("Failing example") {
                Section::Failing
            } else if heading.starts_with("Passing example") {
                Section::Passing
            } else {
                Section::Other
            };
            continue;
        }
        for target in link_targets(line) {
            if let Some(anchor) = target.strip_prefix("../differences.md#") {
                page.differences.push(anchor.to_owned());
            } else if let Some(id) = target.strip_suffix(".md")
                && !id.contains('/')
                && id != "README"
            {
                page.linked_rules.push(id.to_owned());
            }
        }
        if let Some(lang) = line.strip_prefix("```") {
            let mut block = String::new();
            loop {
                let Some(inner) = lines.next() else {
                    return Err("a code block is not closed".to_owned());
                };
                if inner == "```" {
                    break;
                }
                block.push_str(inner);
                block.push('\n');
            }
            match (section, lang, pending.take()) {
                (Section::Failing, "yaml", None) => pending = Some(block),
                (Section::Failing, "text", Some(workflow)) => page.failing.push(Failing {
                    workflow,
                    expected: block
                        .lines()
                        .filter(|line| !line.is_empty())
                        .map(str::to_owned)
                        .collect(),
                }),
                (Section::Passing, "yaml", None) => page.passing.push(block),
                (Section::Other, _, None) => {}
                (Section::Head, _, _) => {
                    return Err("a code block comes before the first `##` section".to_owned());
                }
                (Section::Failing, _, _) => {
                    return Err(
                        "a failing example must be a `yaml` block followed by a `text` block"
                            .to_owned(),
                    );
                }
                (Section::Passing, _, _) => {
                    return Err("a passing example must be one `yaml` block".to_owned());
                }
                (Section::Other, _, Some(_)) => unreachable!("a heading clears `pending`"),
            }
            continue;
        }
        if section == Section::Head {
            head.push(line);
        }
    }
    if pending.is_some() {
        return Err("a failing example has no `text` block".to_owned());
    }

    parse_head(&head, &mut page)?;
    Ok(page)
}

/// Reads the lines between the title and the first `##` heading: the metadata table, then the
/// description on one line.
fn parse_head(head: &[&str], page: &mut Page) -> Result<(), String> {
    let mut head = head.iter().filter(|line| !line.trim().is_empty());
    let header = head.next().and_then(|line| cells(line));
    let separator = head.next().and_then(|line| cells(line));
    let values = head.next().and_then(|line| cells(line));
    let (Some(["Kind", "Category", "Default severity"]), Some(_), Some([kind, category, severity])) =
        (header.as_deref(), separator, values.as_deref())
    else {
        return Err(
            "the title is not followed by the table `| Kind | Category | Default severity |`"
                .to_owned(),
        );
    };
    kind.trim_matches('`').clone_into(&mut page.kind);
    (*category).clone_into(&mut page.category);
    (*severity).clone_into(&mut page.severity);
    let description = head.next().ok_or("the page has no description")?;
    (*description).clone_into(&mut page.description);
    Ok(())
}

/// Lints `workflow` as the binary does, without the file system: the YAML layer, the workflow
/// parser, and the rules. Returns the diagnostics in the one-line format.
fn lint(workflow: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), EXAMPLE_PATH, workflow).unwrap();
    let parsed = workflint_yaml::parse(&file);
    let mut diagnostics = parsed.diagnostics;
    if let Some(document) = &parsed.document {
        let syntax = workflint_syntax::parse(&file, document);
        diagnostics.extend(syntax.diagnostics);
        diagnostics.extend(workflint_rules::run(&file, &syntax.workflow));
    }
    sort_and_dedup(&file, &mut diagnostics);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Returns the rule ID in the brackets at the end of a one-line diagnostic.
fn rule_of(line: &str) -> Option<&str> {
    line.strip_suffix(']')?.rsplit_once(" [").map(|(_, id)| id)
}

/// Checks the page of `id` against its registry entry, and runs its examples. Returns one message
/// per problem.
fn check_page(id: &str, text: &str, rule: Option<&RuleMeta>) -> Vec<String> {
    let page = match parse_page(text) {
        Ok(page) => page,
        Err(err) => return vec![format!("{id}: {err}")],
    };
    let mut problems = Vec::new();
    let mut problem = |message: String| problems.push(format!("{id}: {message}"));
    if page.title != id {
        problem(format!("the title is {:?}, not the rule ID", page.title));
    }
    if let Some(rule) = rule {
        let expected = [
            ("kind", rule.kind, &page.kind),
            ("category", rule.category.as_str(), &page.category),
            (
                "default severity",
                rule.default_severity.as_str(),
                &page.severity,
            ),
        ];
        for (what, registered, written) in expected {
            if registered != written {
                problem(format!(
                    "the {what} is {written:?}, the registry says {registered:?}"
                ));
            }
        }
        let description = format!("{}.", rule.description);
        if page.description != description {
            problem(format!(
                "the description is {:?}, the registry says {description:?}",
                page.description
            ));
        }
    }
    if !page.headings.iter().any(|heading| heading == "Why") {
        problem("no `## Why` section".to_owned());
    }
    if page.failing.is_empty() {
        problem("no failing example".to_owned());
    }
    if page.passing.is_empty() {
        problem("no passing example".to_owned());
    }
    for (index, example) in page.failing.iter().enumerate() {
        let n = index + 1;
        let actual = lint(&example.workflow);
        if actual != example.expected {
            problem(format!(
                "failing example {n} gives\n{}\nbut the page lists\n{}",
                actual.join("\n"),
                example.expected.join("\n")
            ));
        }
        if !actual.iter().any(|line| rule_of(line) == Some(id)) {
            problem(format!("failing example {n} gives no diagnostic of {id}"));
        }
        for line in &actual {
            let named = rule_of(line)
                .is_some_and(|other| other == id || page.linked_rules.iter().any(|l| l == other));
            if !named {
                problem(format!(
                    "failing example {n} gives a diagnostic of a rule the page does not link: {line}"
                ));
            }
        }
    }
    for (index, workflow) in page.passing.iter().enumerate() {
        let actual = lint(workflow);
        if !actual.is_empty() {
            problem(format!(
                "passing example {} gives\n{}",
                index + 1,
                actual.join("\n")
            ));
        }
    }
    let rules_dir = docs_dir().join("rules");
    for linked in &page.linked_rules {
        if !rules_dir.join(format!("{linked}.md")).is_file() {
            problem(format!("the link to {linked}.md names no page"));
        }
    }
    let differences = fs::read_to_string(docs_dir().join("differences.md")).unwrap();
    for anchor in &page.differences {
        if !differences.contains(&format!("<a id=\"{anchor}\"></a>")) {
            problem(format!("docs/differences.md has no anchor {anchor:?}"));
        }
    }
    problems
}

/// Checks that every rule in `rules` has a page in `pages` unless `exempt`, that every page
/// belongs to a rule, and that each `docs_url` names its page. Returns one message per problem.
fn check_coverage(
    rules: &[&RuleMeta],
    pages: &BTreeMap<String, String>,
    exempt: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut problems = Vec::new();
    for rule in rules {
        if !pages.contains_key(rule.id) && !exempt(rule.id) {
            problems.push(format!("{} has no page docs/rules/{}.md", rule.id, rule.id));
        }
        let url = format!("{DOCS_URL_PREFIX}{}.md", rule.id);
        if rule.docs_url != url {
            problems.push(format!(
                "{}: docs_url is {:?}, not {url:?}",
                rule.id, rule.docs_url
            ));
        }
    }
    for id in pages.keys() {
        if !rules.iter().any(|rule| rule.id == id) {
            problems.push(format!("docs/rules/{id}.md belongs to no registered rule"));
        }
    }
    problems
}

/// Renders the rules index: one table row per rule in `rules`, with a link to the page when
/// `pages` has one.
fn render_index(rules: &[&RuleMeta], pages: &BTreeMap<String, String>) -> String {
    let mut index = String::from(
        "| Rule | Kind | Category | Default severity | Description |\n\
         |---|---|---|---|---|\n",
    );
    for rule in rules {
        let name = if pages.contains_key(rule.id) {
            format!("[`{0}`]({0}.md)", rule.id)
        } else {
            format!("`{}`", rule.id)
        };
        writeln!(
            index,
            "| {name} | `{}` | {} | {} | {} |",
            rule.kind,
            rule.category.as_str(),
            rule.default_severity.as_str(),
            rule.description.replace('|', "\\|"),
        )
        .unwrap();
    }
    index
}

/// Returns the text between the index comments of `readme`.
fn written_index(readme: &str) -> Option<&str> {
    let (_, rest) = readme.split_once(INDEX_START)?;
    let (index, _) = rest.split_once(INDEX_END)?;
    Some(index.trim_start_matches('\n'))
}

#[test]
fn every_page_follows_the_format_and_its_examples_give_the_listed_diagnostics() {
    let problems: Vec<_> = read_pages()
        .iter()
        .flat_map(|(id, text)| check_page(id, text, registry::find(id)))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

#[test]
fn every_registered_rule_has_a_page_and_every_page_a_registered_rule() {
    let problems = check_coverage(registry::rules(), &read_pages(), exempt);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_rules_index_matches_the_registry() {
    let readme = fs::read_to_string(docs_dir().join("rules/README.md")).unwrap();
    let expected = render_index(registry::rules(), &read_pages());
    assert!(
        written_index(&readme) == Some(expected.as_str()),
        "the rules index in docs/rules/README.md must be, between {INDEX_START} and \
         {INDEX_END}:\n\n{expected}"
    );
}

/// The `unexpected-key` line that [`unexpected_key_page`] gives for the `default` key.
const DEFAULT_KEY: &str = "test.yaml:5:5: unexpected key \"default\" for \"job\" section. \
    expected one of \"concurrency\", \"container\", \"continue-on-error\", \"defaults\", \
    \"env\", \"environment\", \"if\", \"name\", \"needs\", \"outputs\", \"permissions\", \
    \"runs-on\", \"secrets\", \"services\", \"snapshot\", \"steps\", \"strategy\", \
    \"timeout-minutes\", \"uses\", \"with\" [unexpected-key]";

/// A page of `unexpected-key` whose failing example is `workflow` and lists `expected`, with
/// `prose` after the description.
fn unexpected_key_page(workflow: &str, expected: &str, prose: &str) -> String {
    format!(
        "\
# unexpected-key

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A mapping has a key that its section does not define.

{prose}

## Why

GitHub rejects a workflow with an unknown key.

## Failing example

```yaml
{workflow}```

```text
{expected}
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```
"
    )
}

/// A workflow whose job has the unknown key `default` at 5:5.
const DEFAULT_KEY_WORKFLOW: &str = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    default: {}
    steps:
      - run: echo
";

/// A workflow whose job has the unknown key `step` at 5:5 and so no `steps`.
const STEP_KEY_WORKFLOW: &str = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    step:
      - run: echo
";

/// The diagnostics of [`STEP_KEY_WORKFLOW`].
const STEP_KEY: &str = "\
test.yaml:3:3: \"steps\" section is missing in job \"test\" [missing-required-key]
test.yaml:5:5: unexpected key \"step\" for \"job\" section. expected one of \"concurrency\", \
\"container\", \"continue-on-error\", \"defaults\", \"env\", \"environment\", \"if\", \"name\", \
\"needs\", \"outputs\", \"permissions\", \"runs-on\", \"secrets\", \"services\", \"snapshot\", \
\"steps\", \"strategy\", \"timeout-minutes\", \"uses\", \"with\" [unexpected-key]";

fn check_unexpected_key_page(page: &str) -> Vec<String> {
    check_page("unexpected-key", page, registry::find("unexpected-key"))
}

#[test]
fn a_page_whose_listed_diagnostics_are_right_passes() {
    let page = unexpected_key_page(DEFAULT_KEY_WORKFLOW, DEFAULT_KEY, "");
    assert_eq!(check_unexpected_key_page(&page), Vec::<String>::new());
}

#[test]
fn a_page_whose_listed_diagnostics_are_wrong_fails() {
    let wrong = DEFAULT_KEY.replace("5:5", "5:6");
    let page = unexpected_key_page(DEFAULT_KEY_WORKFLOW, &wrong, "");
    let problems = check_unexpected_key_page(&page);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].starts_with("unexpected-key: failing example 1 gives\n"),
        "{problems:#?}"
    );
}

#[test]
fn a_failing_example_may_give_diagnostics_only_of_rules_the_page_links() {
    let page = unexpected_key_page(STEP_KEY_WORKFLOW, STEP_KEY, "");
    let problems = check_unexpected_key_page(&page);
    assert_eq!(
        problems,
        [format!(
            "unexpected-key: failing example 1 gives a diagnostic of a rule the page does not \
             link: {}",
            STEP_KEY.lines().next().unwrap()
        )]
    );
    let linked = "Without `steps`, [missing-required-key](missing-required-key.md) reports too.";
    let page = unexpected_key_page(STEP_KEY_WORKFLOW, STEP_KEY, linked);
    assert_eq!(check_unexpected_key_page(&page), Vec::<String>::new());
}

#[test]
fn a_failing_example_must_give_a_diagnostic_of_the_page_rule() {
    let workflow = DEFAULT_KEY_WORKFLOW.replace("    default: {}\n", "");
    let page = unexpected_key_page(&workflow, "", "");
    assert_eq!(
        check_unexpected_key_page(&page),
        ["unexpected-key: failing example 1 gives no diagnostic of unexpected-key"]
    );
}

#[test]
fn a_passing_example_with_a_diagnostic_fails() {
    let page = unexpected_key_page(DEFAULT_KEY_WORKFLOW, DEFAULT_KEY, "").replace(
        "    steps:\n      - run: echo\n```\n",
        "    stepz: {}\n```\n",
    );
    let problems = check_unexpected_key_page(&page);
    assert!(
        problems
            .iter()
            .any(|problem| problem.starts_with("unexpected-key: passing example 1 gives\n")),
        "{problems:#?}"
    );
}

#[test]
fn a_page_without_a_passing_example_fails() {
    let page = unexpected_key_page(DEFAULT_KEY_WORKFLOW, DEFAULT_KEY, "");
    let page = page.split("## Passing example").next().unwrap();
    assert_eq!(
        check_unexpected_key_page(page),
        ["unexpected-key: no passing example"]
    );
}

#[test]
fn a_page_whose_metadata_differs_from_the_registry_fails() {
    let page = unexpected_key_page(DEFAULT_KEY_WORKFLOW, DEFAULT_KEY, "")
        .replace("| error |", "| warning |")
        .replace("does not define.", "does not know.");
    assert_eq!(
        check_unexpected_key_page(&page),
        [
            "unexpected-key: the default severity is \"warning\", the registry says \"error\"",
            "unexpected-key: the description is \"A mapping has a key that its section does not \
             know.\", the registry says \"A mapping has a key that its section does not define.\"",
        ]
    );
}

#[test]
fn a_link_to_a_missing_differences_entry_fails() {
    let prose = "It differs from actionlint: [no entry](../differences.md#no-such-entry).";
    let page = unexpected_key_page(DEFAULT_KEY_WORKFLOW, DEFAULT_KEY, prose);
    assert_eq!(
        check_unexpected_key_page(&page),
        ["unexpected-key: docs/differences.md has no anchor \"no-such-entry\""]
    );
}

static NEW_RULE: RuleMeta = RuleMeta {
    id: "new-rule",
    description: "A rule without a page",
    kind: "syntax-check",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/new-rule.md",
};

#[test]
fn the_coverage_check_fails_when_a_registry_entry_has_no_page() {
    let mut rules = registry::rules().to_vec();
    rules.push(&NEW_RULE);
    let problems = check_coverage(&rules, &read_pages(), exempt);
    assert_eq!(problems, ["new-rule has no page docs/rules/new-rule.md"]);
}

#[test]
fn the_coverage_check_fails_when_a_page_has_no_registry_entry() {
    let mut pages = read_pages();
    pages.insert("old-rule".to_owned(), String::new());
    let problems = check_coverage(registry::rules(), &pages, exempt);
    assert_eq!(
        problems,
        ["docs/rules/old-rule.md belongs to no registered rule"]
    );
}

#[test]
fn the_coverage_check_fails_when_a_docs_url_names_another_page() {
    static WRONG_URL: RuleMeta = RuleMeta {
        docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/other.md",
        ..NEW_RULE
    };
    let mut pages = BTreeMap::new();
    pages.insert("new-rule".to_owned(), String::new());
    let problems = check_coverage(&[&WRONG_URL], &pages, |_| false);
    assert_eq!(
        problems,
        [format!(
            "new-rule: docs_url is {:?}, not {:?}",
            WRONG_URL.docs_url, NEW_RULE.docs_url
        )]
    );
}

#[test]
fn the_rules_index_links_only_the_rules_that_have_a_page() {
    let mut pages = BTreeMap::new();
    pages.insert("new-rule".to_owned(), String::new());
    assert_eq!(
        render_index(&[&NEW_RULE], &pages),
        "| Rule | Kind | Category | Default severity | Description |\n\
         |---|---|---|---|---|\n\
         | [`new-rule`](new-rule.md) | `syntax-check` | correctness | error | A rule without a page |\n"
    );
    assert!(render_index(&[&NEW_RULE], &BTreeMap::new()).contains("\n| `new-rule` |"));
}
