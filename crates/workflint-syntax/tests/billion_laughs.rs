//! A "billion laughs" workflow: nine levels of anchors, each used ten times
//! by the next, which expand to 10^9 nodes. GitHub rejects it, and the
//! parser must say so in linear time. The test puts the anchors where
//! section parsers read arbitrary YAML: matrix rows, `env`, `with`, and
//! `steps`.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use workflint_diagnostics::{FileId, SourceFile, sort_and_dedup};

fn laughs() -> String {
    let mut text = String::from(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    strategy:\n      matrix:\n",
    );
    let _ = writeln!(text, "        s0: &s0 [{}]", ["''"; 10].join(", "));
    let _ = writeln!(
        text,
        "        m0: &m0 {{{}}}",
        (0..10)
            .map(|i| format!("k{i}: ''"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for level in 1..9 {
        let below = level - 1;
        let _ = writeln!(
            text,
            "        s{level}: &s{level} [{}]",
            vec![format!("*s{below}"); 10].join(", ")
        );
        let _ = writeln!(
            text,
            "        m{level}: &m{level} {{{}}}",
            (0..10)
                .map(|i| format!("k{i}: *m{below}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    text.push_str("        all: [*s8, *m8]\n");
    text.push_str(
        "    env: *m8\n    steps:\n      - uses: actions/checkout@v6\n        with: *m8\n",
    );
    text.push_str("      - run: echo\n        env: *m8\n  bomb:\n    runs-on: ubuntu-latest\n    steps: *s8\n");
    text
}

#[test]
fn a_billion_laughs_finishes_in_under_one_second() {
    let text = laughs();
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();

    let start = Instant::now();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let mut diagnostics = workflint_syntax::parse(&file, &document).diagnostics;
    sort_and_dedup(&file, &mut diagnostics);
    let elapsed = start.elapsed();

    // One second is the bound in a release build. A debug build is about
    // ten times slower.
    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(1)
    };
    assert!(elapsed < limit, "took {elapsed:?}");
    let rules: Vec<_> = diagnostics.iter().map(|d| d.rule.id).collect();
    assert_eq!(rules, ["too-many-yaml-nodes"]);
}
