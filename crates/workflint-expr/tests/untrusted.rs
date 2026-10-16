//! Ports of the untrusted-input tests in the reference's `expr_insecure_test.go`, plus the cases
//! where workflint differs and the tests of the check inside [`TypeEnv::check`].
//!
//! The modules `detect_untrusted_value`, `all_untrusted_values_at_once`, `init_state`,
//! `no_untrusted_value`, `customized_untrusted_input_mapping`,
//! `detect_untrusted_object_filtering`, and `no_untrusted_object_filtering` port the reference's
//! `TestExprInsecure*` tests of the same names. The reference walks the tree with
//! `VisitExprNode`, as [`find_untrusted_inputs`] does. Its tests check that each message
//! contains a path. These tests compare whole messages, the rule, and the offset of the
//! variable that each error is reported at.

#![allow(clippy::expect_used, clippy::panic)]

use workflint_data::{UNTRUSTED_INPUTS, UntrustedInput};
use workflint_diagnostics::quote::{quote, sorted_quotes};
use workflint_expr::rules::{UNDEFINED_FUNCTION, UNTRUSTED_INPUT, UNTRUSTED_OBJECT_FILTER};
use workflint_expr::{Expr, ExprError, TypeEnv, find_untrusted_inputs, parse_expression};

const SEE: &str = "see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details";

/// Parses `src`, which ends with `}}`.
fn parse(src: &str) -> Expr {
    match parse_expression(src) {
        Ok((expr, _)) => expr,
        Err(err) => panic!("parsing {src:?} failed at {}: {err}", err.start),
    }
}

/// The message about one untrusted value.
fn untrusted(path: &str) -> String {
    format!(
        "{} is potentially untrusted. avoid using it directly in inline scripts. instead, pass it through an environment variable. {SEE}",
        quote(path)
    )
}

/// The message about an object filter that extracts several untrusted values.
fn filtered(paths: &[&str]) -> String {
    format!(
        "object filter extracts potentially untrusted properties {}. avoid using the value directly in inline scripts. instead, pass the value through an environment variable. {SEE}",
        sorted_quotes(paths)
    )
}

/// Every `body` one level below `github.event`.
const BODIES: [&str; 6] = [
    "github.event.comment.body",
    "github.event.discussion.body",
    "github.event.issue.body",
    "github.event.pull_request.body",
    "github.event.review.body",
    "github.event.review_comment.body",
];

/// An error: the rule ID, the byte offset, and the message.
type Error = (&'static str, u32, String);

fn simplify(errors: &[ExprError]) -> Vec<Error> {
    errors
        .iter()
        .map(|err| (err.rule.id, err.start, err.message.clone()))
        .collect()
}

/// The errors of `input` with `roots`, found by walking the whole tree.
fn found_with(roots: &'static [UntrustedInput], input: &str) -> Vec<Error> {
    let src = format!("{input}}}}}");
    simplify(&find_untrusted_inputs(&parse(&src), roots))
}

fn found(input: &str) -> Vec<Error> {
    found_with(UNTRUSTED_INPUTS, input)
}

/// The byte offset of the `n`th `github` in `input`, in any case, counting from 0.
fn nth_github(input: &str, n: usize) -> u32 {
    let Some((offset, _)) = input.to_ascii_lowercase().match_indices("github").nth(n) else {
        panic!("{input:?} has no github number {n}");
    };
    u32::try_from(offset).expect("a short input")
}

/// The expected error for a message at the `n`th `github` of `input`. The rule follows from
/// the message.
fn at(input: &str, n: usize, message: String) -> Error {
    let rule = if message.starts_with("object filter") {
        &UNTRUSTED_OBJECT_FILTER
    } else {
        &UNTRUSTED_INPUT
    };
    (rule.id, nth_github(input, n), message)
}

/// Asserts that `input` has one error, about `message`, at its first `github`.
fn assert_one(input: &str, message: String) {
    assert_eq!(found(input), [at(input, 0, message)], "{input}");
}

/// `TestExprInsecureDetectUntrustedValue`.
mod detect_untrusted_value {
    use super::*;

    /// The forms of `path` that the reference's test builds: the path itself, the path with
    /// each property as a literal index, and, for a path with `*`, both again with `[0]` for
    /// each `*`.
    fn forms(path: &str) -> Vec<String> {
        let mut props = path.split('.');
        let root = props.next().unwrap_or_default();
        let props: Vec<&str> = props.collect();
        let bracketed = |star: &str| {
            let mut out = root.to_owned();
            for prop in &props {
                if *prop == "*" {
                    out.push_str(star);
                } else {
                    out.push_str("['");
                    out.push_str(prop);
                    out.push_str("']");
                }
            }
            out
        };
        let mut forms = vec![path.to_owned(), bracketed(".*")];
        if path.contains(".*") {
            forms.push(path.replace(".*", "[0]"));
            forms.push(bracketed("[0]"));
        }
        forms
    }

    macro_rules! paths {
        ($($name:ident => $path:literal,)*) => {$(
            #[test]
            fn $name() {
                for input in forms($path) {
                    assert_one(&input, untrusted($path));
                }
            }
        )*};
    }

    paths! {
        github_event_issue_title => "github.event.issue.title",
        github_event_issue_body => "github.event.issue.body",
        github_event_pull_request_title => "github.event.pull_request.title",
        github_event_pull_request_body => "github.event.pull_request.body",
        github_event_comment_body => "github.event.comment.body",
        github_event_review_body => "github.event.review.body",
        github_event_review_comment_body => "github.event.review_comment.body",
        github_event_pages_page_name => "github.event.pages.*.page_name",
        github_event_commits_message => "github.event.commits.*.message",
        github_event_head_commit_message => "github.event.head_commit.message",
        github_event_head_commit_author_email => "github.event.head_commit.author.email",
        github_event_head_commit_author_name => "github.event.head_commit.author.name",
        github_event_commits_author_email => "github.event.commits.*.author.email",
        github_event_commits_author_name => "github.event.commits.*.author.name",
        github_event_pull_request_head_ref => "github.event.pull_request.head.ref",
        github_event_pull_request_head_label => "github.event.pull_request.head.label",
        github_event_pull_request_head_repo_default_branch => "github.event.pull_request.head.repo.default_branch",
        github_event_discussion_title => "github.event.discussion.title",
        github_event_discussion_body => "github.event.discussion.body",
        github_head_ref => "github.head_ref",
    }

    #[test]
    fn forms_of_a_path_with_an_array() {
        assert_eq!(
            forms("github.event.pages.*.page_name"),
            [
                "github.event.pages.*.page_name",
                "github['event']['pages'].*['page_name']",
                "github.event.pages[0].page_name",
                "github['event']['pages'][0]['page_name']",
            ]
        );
    }

    #[test]
    fn two_untrusted_values_in_a_logical_operator() {
        let input = "github.event.issue.body || github.event.issue.title";
        assert_eq!(
            found(input),
            [
                at(input, 0, untrusted("github.event.issue.body")),
                at(input, 1, untrusted("github.event.issue.title")),
            ]
        );
    }

    #[test]
    fn untrusted_values_in_indices_of_a_trusted_value() {
        let input = "matrix.foo[github.event.issue.title].bar[github.event.issue.body]";
        assert_eq!(
            found(input),
            [
                at(input, 1, untrusted("github.event.issue.body")),
                at(input, 0, untrusted("github.event.issue.title")),
            ]
        );
    }

    #[test]
    fn untrusted_value_in_the_index_of_an_untrusted_value() {
        let input = "github.event.pages[github.event.issue.title].page_name";
        assert_eq!(
            found(input),
            [
                at(input, 1, untrusted("github.event.issue.title")),
                at(input, 0, untrusted("github.event.pages.*.page_name")),
            ]
        );
    }

    #[test]
    fn untrusted_value_in_a_nested_index_of_an_untrusted_value() {
        let input = "github.event.pages[foo[github.event.issue.title]].page_name";
        assert_eq!(
            found(input),
            [
                at(input, 1, untrusted("github.event.issue.title")),
                at(input, 0, untrusted("github.event.pages.*.page_name")),
            ]
        );
    }

    #[test]
    fn untrusted_values_in_indices_of_a_string() {
        let input = "github.event.issue.body[github.event.issue.title][github.head_ref]";
        assert_eq!(
            found(input),
            [
                at(input, 2, untrusted("github.head_ref")),
                at(input, 1, untrusted("github.event.issue.title")),
            ]
        );
    }

    #[test]
    fn call_in_an_index() {
        let input = "github.event.pages[format('0')].page_name";
        assert_one(input, untrusted("github.event.pages.*.page_name"));
    }

    #[test]
    fn trusted_value_in_an_index() {
        let input = "github.event.pages[matrix.page_num].page_name";
        assert_one(input, untrusted("github.event.pages.*.page_name"));
    }

    #[test]
    fn untrusted_values_nested_three_deep() {
        let input = "github.event.pages[github.event.commits[github.event.issue.title].author.name].page_name";
        assert_eq!(
            found(input),
            [
                at(input, 2, untrusted("github.event.issue.title")),
                at(input, 1, untrusted("github.event.commits.*.author.name")),
                at(input, 0, untrusted("github.event.pages.*.page_name")),
            ]
        );
    }

    #[test]
    fn untrusted_value_in_a_call_in_an_index() {
        let input = "github.event.pages[format('{0}', github.event.issue.title)].page_name";
        assert_eq!(
            found(input),
            [
                at(input, 1, untrusted("github.event.issue.title")),
                at(input, 0, untrusted("github.event.pages.*.page_name")),
            ]
        );
    }

    #[test]
    fn untrusted_values_in_arguments() {
        let input = "format('{0} {1}', github.event.pages.*.page_name, github.event.issue.title)";
        assert_eq!(
            found(input),
            [
                at(input, 0, untrusted("github.event.pages.*.page_name")),
                at(input, 1, untrusted("github.event.issue.title")),
            ]
        );
    }

    #[test]
    fn object_filters_in_arguments() {
        let input = "format('{0} {1}', github.event.*.body, github.event.*.*)";
        assert_eq!(
            found(input),
            [
                at(input, 0, filtered(&BODIES)),
                at(
                    input,
                    1,
                    filtered(&[
                        "github.event.comment.body",
                        "github.event.discussion.body",
                        "github.event.discussion.title",
                        "github.event.head_commit.message",
                        "github.event.issue.body",
                        "github.event.issue.title",
                        "github.event.pull_request.body",
                        "github.event.pull_request.title",
                        "github.event.review.body",
                        "github.event.review_comment.body",
                    ])
                ),
            ]
        );
    }

    #[test]
    fn contains_before_an_untrusted_value() {
        let input = "contains(github.event.issue.body, 'foo') || github.event.issue.title";
        assert_eq!(
            found(input),
            [at(input, 1, untrusted("github.event.issue.title"))]
        );
    }

    #[test]
    fn contains_after_an_untrusted_value() {
        let input = "github.event.issue.title && contains(github.event.issue.body, 'foo')";
        assert_one(input, untrusted("github.event.issue.title"));
    }

    #[test]
    fn contains_between_untrusted_values() {
        let input = "format('{0}{1}{2}', github.event.issue.title, contains(github.event.issue.body, 'foo'), github.event.issue.title)";
        assert_eq!(
            found(input),
            [
                at(input, 0, untrusted("github.event.issue.title")),
                at(input, 2, untrusted("github.event.issue.title")),
            ]
        );
    }
}

/// `TestExprInsecureAllUntrustedValuesAtOnce`.
mod all_untrusted_values_at_once {
    use super::*;

    #[test]
    fn every_untrusted_value_as_an_argument() {
        let mut args = Vec::new();
        let mut want = Vec::new();
        for leaf in [
            "github.event.issue.title",
            "github.event.issue.body",
            "github.event.pull_request.title",
            "github.event.pull_request.body",
            "github.event.comment.body",
            "github.event.review.body",
            "github.event.review_comment.body",
            "github.event.pages.*.page_name",
            "github.event.commits.*.message",
            "github.event.head_commit.message",
            "github.event.head_commit.author.email",
            "github.event.head_commit.author.name",
            "github.event.commits.*.author.email",
            "github.event.commits.*.author.name",
            "github.event.pull_request.head.ref",
            "github.event.pull_request.head.label",
            "github.event.pull_request.head.repo.default_branch",
            "github.event.discussion.title",
            "github.event.discussion.body",
            "github.head_ref",
        ] {
            args.push(leaf.to_owned());
            want.push(untrusted(leaf));
            if leaf.contains(".*") {
                args.push(leaf.replace(".*", "[0]"));
                want.push(untrusted(leaf));
            }
        }
        let input = format!("someFunc({})", args.join(", "));
        let want: Vec<Error> = want
            .into_iter()
            .enumerate()
            .map(|(i, message)| at(&input, i, message))
            .collect();
        assert_eq!(want.len(), 24);
        assert_eq!(found(&input), want);
    }
}

/// `TestExprInsecureInitState`. The reference resets its checker between expressions. Here each
/// check starts empty.
mod init_state {
    use super::*;

    #[test]
    fn each_check_reports_only_its_own_expression() {
        let mut env = TypeEnv::new();
        env.set_untrusted_inputs(UNTRUSTED_INPUTS);
        for _ in 0..2 {
            let (_, errors) = env.check(&parse("github.event.issue.title }}"));
            assert_eq!(
                simplify(&errors),
                [(UNTRUSTED_INPUT.id, 0, untrusted("github.event.issue.title"))]
            );
        }
    }
}

/// `TestExprInsecureNoUntrustedValue`.
mod no_untrusted_value {
    use super::*;

    macro_rules! cases {
        ($($name:ident => $input:literal,)*) => {$(
            #[test]
            fn $name() {
                assert_eq!(found($input), [], "{}", $input);
            }
        )*};
    }

    cases! {
        number => "0",
        bool => "true",
        null => "null",
        string_with_an_untrusted_path => "'github.event.issue.title'",
        matrix_property => "matrix.foo",
        untrusted_path_below_matrix => "matrix.github.event.issue.title",
        event_path_below_matrix => "matrix.event.issue.title",
        github => "github",
        github_event => "github.event",
        github_event_issue => "github.event.issue",
        unknown_property_in_place_of_an_array => "github.event.commits.foo.message",
        array_element => "github.event.commits[0]",
        filtered_array => "github.event.commits.*",
        unknown_property_of_an_array_element => "github.event.commits.*.foo",
        unknown_event => "github.event.foo.body",
        dynamic_index_of_github => "github[x].issue.title",
        dynamic_index_of_github_event => "github.event[foo].title",
        index_of_an_object => "github.event.issue[0].title",
        untrusted_path_below_another_variable => "foo(github.event, pull_request.body)",
        unknown_property_of_github => "foo(github.event, github.pull_request.body)",
        untrusted_path_below_a_call => "foo(github.event, bar().pull_request.body)",
        dynamic_index_of_an_object => "github.event[pull_request.body]",
        dynamic_index_of_github_with_a_path => "github[event.pull_request].body",
        object_as_an_index => "github[github.event.pull_request].body",
        object_in_the_index_of_a_trusted_value => "matrix.foo[github.event.pages].page_name",
        property_of_an_untrusted_value => "github.event.issue.body.foo.bar",
        index_of_an_untrusted_value => "github.event.issue.body[0]",
        untrusted_values_in_contains => "contains(github.event.issue.body, github.event.issue.title)",
        untrusted_value_in_starts_with => "startsWith(github.event.comment.body, 'LGTM')",
        untrusted_values_in_ends_with => "endsWith(github.event.pull_request.title, github.event.issue.title)",
        contains_in_contains => "contains(contains(github.event.issue.body, github.event.issue.title), github.event.issue.title)",
        from_json_in_contains => "contains(fromJSON(github.event.issue.body), github.event.issue.title)",
        contains_in_from_json => "fromJSON(contains(github.event.issue.body, github.event.issue.title))",
    }
}

/// `TestExprInsecureCustomizedUntrustedInputMapping`.
mod customized_untrusted_input_mapping {
    use super::*;

    const fn node(name: &'static str, children: &'static [UntrustedInput]) -> UntrustedInput {
        UntrustedInput::new(name, children)
    }

    static FOO: [UntrustedInput; 1] = [node("foo", &[])];
    static FOO_BAR_PIYO: [UntrustedInput; 1] = [node("foo", &[node("bar", &[node("piyo", &[])])])];
    static GITHUB_FOO_ARRAY: [UntrustedInput; 1] =
        [node("github", &[node("foo", &[node("*", &[])])])];

    fn assert_one_with(roots: &'static [UntrustedInput], input: &str, path: &str) {
        assert_eq!(
            found_with(roots, input),
            [(UNTRUSTED_INPUT.id, 0, untrusted(path))],
            "{input}"
        );
    }

    #[test]
    fn root_without_children() {
        assert_one_with(&FOO, "foo", "foo");
    }

    #[test]
    fn nested_properties() {
        assert_one_with(&FOO_BAR_PIYO, "foo.bar.piyo", "foo.bar.piyo");
    }

    #[test]
    fn index_of_an_array() {
        assert_one_with(&GITHUB_FOO_ARRAY, "github.foo[0]", "github.foo.*");
    }

    #[test]
    fn filtered_array() {
        assert_one_with(&GITHUB_FOO_ARRAY, "github.foo.*", "github.foo.*");
    }

    #[test]
    fn object_filter() {
        assert_one_with(&FOO_BAR_PIYO, "foo.*.piyo", "foo.bar.piyo");
    }

    #[test]
    fn index_after_an_object_filter() {
        assert_one_with(&FOO_BAR_PIYO, "foo.*.piyo[0]", "foo.bar.piyo");
    }

    #[test]
    fn builtin_paths_are_not_reported_with_other_roots() {
        assert_eq!(found_with(&FOO, "github.event.issue.title"), []);
    }
}

/// `TestExprInsecureDetectUntrustedObjectFiltering`.
mod detect_untrusted_object_filtering {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $input:literal => $message:expr,)*) => {$(
            #[test]
            fn $name() {
                assert_one($input, $message);
            }
        )*};
    }

    cases! {
        bodies_of_event_objects: "github.event.*.body" => filtered(&BODIES),
        index_after_bodies_of_event_objects: "github.event.*.body[0]" => filtered(&BODIES),
        emails_two_levels_down: "github.event.*.*.email" => untrusted("github.event.head_commit.author.email"),
        index_after_emails_two_levels_down: "github.event.*.*.email[0]" => untrusted("github.event.head_commit.author.email"),
        literal_index_before_a_filter: "github['event'].*.body" => filtered(&BODIES),
        literal_index_after_two_filters: "github.event.*.*['email']" => untrusted("github.event.head_commit.author.email"),
        index_after_a_literal_index_after_two_filters: "github.event.*.*['email'][0]" => untrusted("github.event.head_commit.author.email"),
        author_email_of_event_objects: "github.event.*.author.email" => untrusted("github.event.head_commit.author.email"),
        literal_author_email_of_event_objects: "github.event.*['author']['email']" => untrusted("github.event.head_commit.author.email"),
        messages_two_levels_down: "github.event.*.*.message" => untrusted("github.event.commits.*.message"),
        everything_three_levels_down: "github.event.*.*.*" => filtered(&[
            "github.event.commits.*.message",
            "github.event.head_commit.author.email",
            "github.event.head_commit.author.name",
            "github.event.pages.*.page_name",
            "github.event.pull_request.head.label",
            "github.event.pull_request.head.ref",
        ]),
        properties_of_github: "github.*" => untrusted("github.head_ref"),
        index_after_properties_of_github: "github.*[0]" => untrusted("github.head_ref"),
        bodies_two_levels_below_github: "github.*.*.body" => filtered(&BODIES),
        commit_messages_below_github: "github.*.commits.*.message" => untrusted("github.event.commits.*.message"),
    }

    /// The second half of the reference's test checks every case with one checker. Here one
    /// environment checks them all.
    #[test]
    fn one_environment_checks_every_case() {
        let mut env = TypeEnv::new();
        env.set_untrusted_inputs(UNTRUSTED_INPUTS);
        for input in [
            "github.event.*.body",
            "github.event.*.*.email",
            "github.*",
            "github.*.commits.*.message",
        ] {
            let src = format!("{input}}}}}");
            let (_, errors) = env.check(&parse(&src));
            let untrusted: Vec<&ExprError> = errors
                .iter()
                .filter(|err| err.rule.id.starts_with("untrusted-"))
                .collect();
            assert_eq!(untrusted.len(), 1, "{input}: {errors:?}");
        }
    }
}

/// `TestExprInsecureNoUntrustedObjectFiltering`.
mod no_untrusted_object_filtering {
    use super::*;

    macro_rules! cases {
        ($($name:ident => $input:literal,)*) => {$(
            #[test]
            fn $name() {
                assert_eq!(found($input), [], "{}", $input);
            }
        )*};
    }

    cases! {
        unknown_property_of_github_properties => "github.*.foo",
        unknown_literal_property_of_github_properties => "github.*['foo']",
        property_of_bodies => "github.event.*.body.foo",
        filter_of_bodies => "github.event.*.body.*",
        unknown_property_two_levels_down => "github.*.*.foo",
        unknown_property_of_commits => "github.*.commits.*.foo",
        unknown_property_of_literal_commits => "github.*['commits'].*.foo",
        property_of_commit_messages => "github.*.commits.*.message.foo",
        filter_of_another_variable => "a.*",
    }
}

/// Where workflint differs from the reference.
mod differences {
    use super::*;

    /// `docs/differences.md#literal-index-case`. The reference misses the first input.
    #[test]
    fn literal_index_keys_match_without_regard_to_case() {
        assert_one(
            "github['EVENT']['ISSUE']['TITLE']",
            untrusted("github.event.issue.title"),
        );
        assert_one(
            "github['Event'].Pages[0]['Page_Name']",
            untrusted("github.event.pages.*.page_name"),
        );
    }

    /// The reference lowercases variable and property names too, so this is not a difference.
    #[test]
    fn names_match_without_regard_to_case() {
        assert_one(
            "GitHub.Event.Issue.Title",
            untrusted("github.event.issue.title"),
        );
    }
}

/// The check inside [`TypeEnv::check`], as the reference's `Check` runs it.
mod type_env {
    use super::*;

    fn checked(env: &TypeEnv, input: &str) -> Vec<Error> {
        let src = format!("{input}}}}}");
        let (_, errors) = env.check(&parse(&src));
        simplify(&errors)
    }

    fn env() -> TypeEnv {
        let mut env = TypeEnv::new();
        env.set_untrusted_inputs(UNTRUSTED_INPUTS);
        env
    }

    #[test]
    fn check_is_off_by_default() {
        assert_eq!(checked(&TypeEnv::new(), "github.event.issue.title"), []);
    }

    #[test]
    fn untrusted_inputs_come_after_the_semantic_errors() {
        let input = "github.event.issue.title == foo";
        let errors = checked(&env(), input);
        let rules: Vec<&str> = errors.iter().map(|(rule, _, _)| *rule).collect();
        assert_eq!(rules, ["undefined-variable", UNTRUSTED_INPUT.id]);
        assert_eq!(
            errors.get(1),
            Some(&at(input, 0, untrusted("github.event.issue.title")))
        );
    }

    #[test]
    fn object_filter_has_its_own_rule() {
        let input = "github.event.*.body";
        assert_eq!(checked(&env(), input), [at(input, 0, filtered(&BODIES))]);
    }

    /// The reference's `Check` does not visit the arguments of a function that is not built in,
    /// so it does not report untrusted values in them.
    #[test]
    fn arguments_of_an_undefined_function_are_not_checked() {
        let errors = checked(&env(), "someFunc(github.event.issue.title)");
        let rules: Vec<&str> = errors.iter().map(|(rule, _, _)| *rule).collect();
        assert_eq!(rules, [UNDEFINED_FUNCTION.id]);
    }

    #[test]
    fn untrusted_value_in_a_narrowed_operand_is_reported() {
        let input = "!(github.event.issue.title || github.head_ref) && 'x'";
        assert_eq!(
            checked(&env(), input),
            [
                at(input, 0, untrusted("github.event.issue.title")),
                at(input, 1, untrusted("github.head_ref")),
            ]
        );
    }

    #[test]
    fn walk_and_check_find_the_same_errors() {
        for input in [
            "github.event.pages[github.event.commits[github.event.issue.title].author.name].page_name",
            "format('{0} {1}', github.event.*.body, github.event.*.*)",
            "contains(github.event.issue.body, 'foo') || github.event.issue.title",
            "github['EVENT']['ISSUE']['TITLE']",
        ] {
            assert_eq!(checked(&env(), input), found(input), "{input}");
        }
    }
}

/// The byte ranges of the errors.
mod error_ranges {
    use super::*;

    fn ranges(input: &str) -> Vec<(u32, u32)> {
        let src = format!("{input}}}}}");
        find_untrusted_inputs(&parse(&src), UNTRUSTED_INPUTS)
            .iter()
            .map(|err| (err.start, err.end))
            .collect()
    }

    #[test]
    fn error_covers_the_path_from_its_variable() {
        // `github.event.issue.title` is bytes 7..31.
        assert_eq!(ranges("format(github.event.issue.title)"), [(7, 31)]);
    }

    #[test]
    fn error_covers_indices_and_filters() {
        let input = "github.event.pages[github.event.issue.title].page_name";
        let len = u32::try_from(input.len()).expect("a short input");
        assert_eq!(ranges(input), [(19, 43), (0, len)]);
        assert_eq!(ranges("github.event.*.body[0] == 1"), [(0, 22)]);
    }

    #[test]
    fn parenthesized_variable_starts_the_error() {
        assert_eq!(ranges("(github).head_ref"), [(1, 17)]);
    }
}
