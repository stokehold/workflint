#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile};

use crate::testing::render;
use crate::{Container, Credentials, Field, Job, Services, Str, Value, Workflow, parse};

/// Parses the workflow `text` and returns its diagnostics as [`render`]
/// prints them.
fn check(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    render(&file, parse(&file, &document).diagnostics)
}

/// Parses the workflow `text`, asserts that its diagnostics are `want`, and
/// runs `f` on its workflow.
fn parsed(text: &str, want: &[&str], f: impl FnOnce(&Workflow<'_>)) {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    assert_eq!(render(&file, parsed.diagnostics), want);
    f(&parsed.workflow);
}

/// A workflow with the one job `test`, whose body is `body`, indented by
/// four spaces.
fn workflow(body: &str) -> String {
    let mut text = "on: push\njobs:\n  test:\n    runs-on: x\n    steps: [{run: a}]\n".to_owned();
    for line in body.lines() {
        text.push_str("    ");
        text.push_str(line);
        text.push('\n');
    }
    text
}

fn job<'w, 'a>(workflow: &'w Workflow<'a>) -> &'w Job<'a> {
    &workflow.jobs.value().unwrap().get("test").unwrap().value
}

fn container<'w, 'a>(workflow: &'w Workflow<'a>) -> &'w Container<'a> {
    job(workflow).container.value().unwrap()
}

fn services<'w, 'a>(workflow: &'w Workflow<'a>) -> &'w Services<'a> {
    job(workflow).services.literal().unwrap()
}

fn string<'a>(field: &Field<'a, Str<'a>>) -> Option<&'a str> {
    field.value().map(Str::value)
}

fn strings(field: &Field<'_, std::sync::Arc<[Str<'_>]>>) -> Vec<String> {
    field
        .value()
        .unwrap()
        .iter()
        .map(|s| s.value().to_owned())
        .collect()
}

fn credentials<'w, 'a>(container: &'w Container<'a>) -> &'w Credentials<'a> {
    container.credentials.literal().unwrap()
}

const CONTAINER_KEYS: &str = r#""credentials", "env", "image", "options", "ports", "volumes""#;

const SERVICE_KEYS: &str =
    r#""command", "credentials", "entrypoint", "env", "image", "options", "ports", "volumes""#;

#[test]
fn a_scalar_container_is_the_image_with_no_key() {
    parsed(&workflow("container: node:24"), &[], |workflow| {
        let field = &job(workflow).container;
        assert_eq!(field.key().map(Str::value), Some("container"));
        let image = &container(workflow).image;
        assert_eq!(string(image), Some("node:24"));
        assert_eq!(image.key(), None);
    });
}

#[test]
fn a_null_container_is_the_image_null_and_an_empty_one_is_reported() {
    // `null` is the string `null`, as `run: null` is, so only a container
    // with no text at all is an empty image.
    parsed(&workflow("container: null"), &[], |workflow| {
        assert_eq!(string(&container(workflow).image), Some("null"));
    });
    assert_eq!(
        check(&workflow("container:")),
        ["6:15: string should not be empty [empty-string]"]
    );
    assert_eq!(
        check(&workflow("container: ''")),
        ["6:16: string should not be empty [empty-string]"]
    );
}

#[test]
fn a_mapping_container_has_ports_and_volumes_in_separate_fields() {
    let body = "\
container:
  image: ghcr.io/o/test
  env:
    FOO: foo
  ports:
    - 8080
    - 8081
  volumes:
    - /foo:/bar
  options: ''
  credentials:
    username: user
    password: ${{ secrets.PASS }}
";
    parsed(&workflow(body), &[], |workflow| {
        let container = container(workflow);
        assert_eq!(string(&container.image), Some("ghcr.io/o/test"));
        assert_eq!(container.image.key().map(Str::value), Some("image"));
        let env = container.env.literal().unwrap();
        assert_eq!(env.get("foo").unwrap().value.unwrap().value(), "foo");
        assert_eq!(strings(&container.ports), ["8080", "8081"]);
        assert_eq!(container.ports.key().map(Str::value), Some("ports"));
        assert_eq!(strings(&container.volumes), ["/foo:/bar"]);
        assert_eq!(container.volumes.key().map(Str::value), Some("volumes"));
        assert_eq!(string(&container.options), Some(""));
        let credentials = credentials(container);
        assert_eq!(string(&credentials.username), Some("user"));
        assert_eq!(string(&credentials.password), Some("${{ secrets.PASS }}"));
        assert_eq!(
            container.credentials.key().map(Str::value),
            Some("credentials")
        );
        assert!(!container.command.is_present());
        assert!(!container.entrypoint.is_present());
    });
}

#[test]
fn ports_and_volumes_may_be_empty_sequences_but_not_scalars() {
    parsed(
        &workflow("container: {image: x, ports: [], volumes: []}"),
        &[],
        |workflow| {
            assert_eq!(strings(&container(workflow).ports), Vec::<String>::new());
            assert_eq!(strings(&container(workflow).volumes), Vec::<String>::new());
        },
    );
    let body = "\
container:
  image: x
  ports: 8080
  volumes: '/foo:/bar'
  options: [a]
";
    parsed(
        &workflow(body),
        &[
            r#"8:14: "ports" section must be sequence node but got scalar node with "!!int" tag [unexpected-node-kind]"#,
            r#"9:16: "volumes" section must be sequence node but got scalar node with "!!str" tag [unexpected-node-kind]"#,
            r#"10:16: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
        ],
        |workflow| {
            let container = container(workflow);
            assert!(matches!(container.ports, Field::Invalid { .. }));
            assert!(matches!(container.volumes, Field::Invalid { .. }));
            assert!(matches!(container.options, Field::Invalid { .. }));
        },
    );
}

#[test]
fn a_container_without_an_image_is_reported_at_its_key() {
    assert_eq!(
        check(&workflow("container:\n  env:\n    Foo: bar")),
        [r#"6:5: "image" is missing in "container" section [missing-required-key]"#]
    );
    assert_eq!(
        check(&workflow("container: {}")),
        [
            r#"6:5: "image" is missing in "container" section [missing-required-key]"#,
            r#"6:16: "container" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
        ]
    );
    // An empty image is written, so it is not missing.
    assert_eq!(
        check(&workflow("container: {image: ''}")),
        ["6:24: string should not be empty [empty-string]"]
    );
}

#[test]
fn a_sequence_container_is_invalid_and_has_no_image() {
    parsed(
        &workflow("container: [x]"),
        &[
            r#"6:5: "image" is missing in "container" section [missing-required-key]"#,
            r#"6:16: "container" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
        ],
        |workflow| {
            assert!(matches!(job(workflow).container, Field::Invalid { .. }));
        },
    );
}

#[test]
fn command_and_entrypoint_on_a_job_container_are_unexpected() {
    let body = "\
container:
  image: x
  entrypoint: bash
  command: -lc 'echo hello'
  Image: y
";
    parsed(
        &workflow(body),
        &[
            &format!(
                r#"8:7: unexpected key "entrypoint" for "container" section. expected one of {CONTAINER_KEYS} [unexpected-key]"#
            ),
            &format!(
                r#"9:7: unexpected key "command" for "container" section. expected one of {CONTAINER_KEYS} [unexpected-key]"#
            ),
            &format!(
                r#"10:7: unexpected key "Image" for "container" section. expected one of {CONTAINER_KEYS} [unexpected-key]"#
            ),
        ],
        |workflow| {
            let container = container(workflow);
            assert!(!container.command.is_present());
            assert!(!container.entrypoint.is_present());
        },
    );
}

#[test]
fn a_duplicate_container_key_is_reported() {
    assert_eq!(
        check(&workflow("container:\n  image: x\n  image: y")),
        [
            r#"8:7: key "image" is duplicated in "container" section. previously defined at line:7,col:7 [duplicate-key]"#
        ]
    );
}

#[test]
fn a_service_has_command_and_entrypoint() {
    let body = "\
services:
  redis:
    image: ghcr.io/o/redis
    entrypoint: redis-server
    command: --save 60 1
    ports: [6379]
    volumes: [/data:/data]
  postgres: postgres:17
";
    parsed(&workflow(body), &[], |workflow| {
        let services = services(workflow);
        let ids: Vec<_> = services.iter().map(|e| e.key.value()).collect();
        assert_eq!(ids, ["redis", "postgres"]);
        assert_eq!(
            job(workflow).services.key().map(Str::value),
            Some("services")
        );

        let redis = &services.get("redis").unwrap().value;
        assert_eq!(string(&redis.image), Some("ghcr.io/o/redis"));
        assert_eq!(string(&redis.entrypoint), Some("redis-server"));
        assert_eq!(redis.entrypoint.key().map(Str::value), Some("entrypoint"));
        assert_eq!(string(&redis.command), Some("--save 60 1"));
        assert_eq!(strings(&redis.ports), ["6379"]);
        assert_eq!(strings(&redis.volumes), ["/data:/data"]);

        let postgres = &services.get("postgres").unwrap().value;
        assert_eq!(string(&postgres.image), Some("postgres:17"));
        assert_eq!(postgres.image.key(), None);
    });
}

#[test]
fn command_and_entrypoint_of_a_service_may_be_empty() {
    parsed(
        &workflow("services:\n  redis: {image: x, command: '', entrypoint: ''}"),
        &[],
        |workflow| {
            let redis = &services(workflow).get("redis").unwrap().value;
            assert_eq!(string(&redis.command), Some(""));
            assert_eq!(string(&redis.entrypoint), Some(""));
        },
    );
}

#[test]
fn service_errors_name_the_services_section() {
    let body = "\
services:
  redis:
    env:
      CONFIG: foo
  db: {}
  cache: {image: x, foo: bar}
";
    assert_eq!(
        check(&workflow(body)),
        [
            r#"7:7: "image" is missing in "services" section [missing-required-key]"#,
            r#"10:7: "image" is missing in "services" section [missing-required-key]"#,
            r#"10:11: "services" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            &format!(
                r#"11:25: unexpected key "foo" for "services" section. expected one of {SERVICE_KEYS} [unexpected-key]"#
            ),
        ]
    );
}

#[test]
fn service_ids_are_case_insensitive() {
    let body = "\
services:
  redis: x
  Redis: y
";
    parsed(
        &workflow(body),
        &[
            r#"8:7: key "Redis" is duplicated in "services" section. previously defined at line:7,col:7. note that this key is case insensitive [duplicate-key]"#,
        ],
        |workflow| {
            let services = services(workflow);
            assert_eq!(services.len(), 1);
            assert_eq!(
                string(&services.get("REDIS").unwrap().value.image),
                Some("x")
            );
        },
    );
}

#[test]
fn services_may_be_one_expression() {
    parsed(
        &workflow("services: ${{ fromJSON('{}') }}"),
        &[],
        |workflow| {
            let services = &job(workflow).services;
            assert_eq!(services.key().map(Str::value), Some("services"));
            assert_eq!(
                services.expression().unwrap().source.value(),
                "${{ fromJSON('{}') }}"
            );
        },
    );
}

#[test]
fn services_must_be_a_mapping_with_entries() {
    parsed(
        &workflow("services: redis"),
        &[
            r#"6:15: "services" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
        ],
        |workflow| {
            assert!(matches!(job(workflow).services, Value::Invalid { .. }));
        },
    );
    assert_eq!(
        check(&workflow("services: {}")),
        [
            r#"6:15: "services" section should not be empty. please remove this section if it's unnecessary [empty-section]"#
        ]
    );
    assert_eq!(
        check(&workflow("services:")),
        [
            r#"6:14: "services" section should not be empty. please remove this section if it's unnecessary [empty-section]"#
        ]
    );
}

#[test]
fn credentials_may_be_one_expression() {
    let body = "\
container:
  image: x
  credentials: ${{ fromJSON('{}') }}
services:
  redis:
    image: x
    credentials: ${{ secrets.CREDS }}
";
    parsed(&workflow(body), &[], |workflow| {
        let credentials = &container(workflow).credentials;
        assert_eq!(credentials.key().map(Str::value), Some("credentials"));
        assert_eq!(
            credentials.expression().unwrap().source.value(),
            "${{ fromJSON('{}') }}"
        );
        let redis = &services(workflow).get("redis").unwrap().value;
        assert_eq!(
            redis.credentials.expression().unwrap().source.value(),
            "${{ secrets.CREDS }}"
        );
    });
}

#[test]
fn credentials_without_a_username_or_password_are_reported_and_dropped() {
    let both = r#"both "username" and "password" must be specified in "credentials" section [incomplete-credentials]"#;
    let cases: [(&str, &[String]); 5] = [
        (
            "{username: u}",
            &[format!("8:7: {both}")],
        ),
        (
            "{password: p}",
            &[format!("8:7: {both}")],
        ),
        (
            "",
            &[
                format!("8:7: {both}"),
                r#"8:19: "credentials" section should not be empty. please remove this section if it's unnecessary [empty-section]"#.to_owned(),
            ],
        ),
        (
            "{}",
            &[
                format!("8:7: {both}"),
                r#"8:20: "credentials" section should not be empty. please remove this section if it's unnecessary [empty-section]"#.to_owned(),
            ],
        ),
        (
            "'user:pass'",
            &[
                format!("8:7: {both}"),
                r#"8:20: "credentials" section is scalar node but mapping node is expected [unexpected-node-kind]"#.to_owned(),
            ],
        ),
    ];
    for (value, want) in cases {
        let text = workflow(&format!("container:\n  image: x\n  credentials: {value}"));
        let want: Vec<&str> = want.iter().map(String::as_str).collect();
        parsed(&text, &want, |workflow| {
            let credentials = &container(workflow).credentials;
            assert!(
                matches!(credentials, Value::Invalid { .. }),
                "{value:?}: {credentials:?}"
            );
            assert_eq!(credentials.key().map(Str::value), Some("credentials"));
        });
    }
}

#[test]
fn credentials_of_a_service_are_reported_at_their_key() {
    let body = "\
services:
  redis:
    image: x
    credentials:
      username: foo
";
    assert_eq!(
        check(&workflow(body)),
        [
            r#"9:9: both "username" and "password" must be specified in "credentials" section [incomplete-credentials]"#
        ]
    );
}

#[test]
fn credentials_keys_are_checked_and_an_invalid_value_counts_as_written() {
    let body = "\
container:
  image: x
  credentials:
    username: ''
    password: [p]
    foo1: bar
    Username: u
";
    parsed(
        &workflow(body),
        &[
            "9:19: string should not be empty [empty-string]",
            r#"10:19: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"11:9: unexpected key "foo1" for "credentials" section. expected one of "password", "username" [unexpected-key]"#,
            r#"12:9: unexpected key "Username" for "credentials" section. expected one of "password", "username" [unexpected-key]"#,
        ],
        |workflow| {
            let credentials = credentials(container(workflow));
            assert!(matches!(credentials.username, Field::Invalid { .. }));
            assert!(matches!(credentials.password, Field::Invalid { .. }));
        },
    );
}

#[test]
fn a_container_env_that_is_not_one_expression_is_reported() {
    assert_eq!(
        check(&workflow("container: {image: x, env: 'FOO=BAR'}")),
        [
            r#"6:32: expecting a single ${{...}} expression or mapping value for "env" section, but found plain text node [expected-single-expression]"#
        ]
    );
}
