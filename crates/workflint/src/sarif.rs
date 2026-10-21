//! The SARIF 2.1.0 output of `-format sarif`, which GitHub code scanning reads (ADR 0007).
//!
//! [`Results::of`] turns the diagnostics of one file into SARIF results, and [`write()`] prints
//! one log with one run: the tool, a `rules` entry for every rule it is given, and the results
//! of every file. Regions count columns in UTF-16 code units. Each result has a
//! `partialFingerprints` entry computed from the rule ID, the file path, and the text of the
//! span, so it stays the same when unrelated lines move. `docs/sarif.md` describes the output.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};

use serde::Serialize;
use sha2::{Digest, Sha256};

use workflint_diagnostics::{Category, Diagnostic, RuleMeta, Severity, SourceFile, Span};

/// The schema the log names in `$schema`.
const SCHEMA: &str =
    "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json";

/// The analysis tool that wrote the log.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tool<'a> {
    /// The name, such as `workflint`.
    pub(crate) name: &'a str,
    /// The version, such as `0.1.0`.
    pub(crate) version: &'a str,
    /// The URL of the tool's home page.
    pub(crate) information_uri: &'a str,
}

/// The SARIF results of one file.
#[derive(Debug, Clone)]
pub(crate) struct Results(Vec<Entry>);

/// One result, before [`write()`] gives it the index of its rule.
#[derive(Debug, Clone)]
struct Entry {
    rule: &'static RuleMeta,
    level: &'static str,
    message: Message,
    location: Location,
    related: Vec<RelatedLocation>,
    fingerprint: String,
}

impl Results {
    /// The results of `diagnostics`, which are in `file`, in the order of `diagnostics`.
    #[must_use]
    pub(crate) fn of(file: &SourceFile, diagnostics: &[Diagnostic]) -> Self {
        let artifact = ArtifactLocation::of(file.path());
        // The number of results so far with each fingerprint hash.
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let entries = diagnostics
            .iter()
            .map(|diagnostic| {
                let hash = fingerprint_hash(file, diagnostic);
                let occurrence = seen.entry(hash.clone()).or_default();
                *occurrence += 1;
                let fingerprint = format!("{hash}:{occurrence}");
                let text = match &diagnostic.help {
                    Some(help) => format!("{}\n\nhelp: {help}", diagnostic.message),
                    None => diagnostic.message.clone(),
                };
                Entry {
                    rule: diagnostic.rule,
                    level: level(diagnostic.severity),
                    message: Message { text },
                    location: Location::of(file, &artifact, diagnostic.span),
                    related: diagnostic
                        .labels
                        .iter()
                        .zip(1..)
                        .map(|(label, id)| RelatedLocation {
                            id,
                            location: Location::of(file, &artifact, label.span),
                            message: Message {
                                text: label.note.clone(),
                            },
                        })
                        .collect(),
                    fingerprint,
                }
            })
            .collect();
        Self(entries)
    }
}

/// Writes a SARIF log with one run to `out`: `tool` with a `rules` entry for each of `rules`,
/// in their order, and the results of `files`, in their order. The JSON is indented and ends
/// with a line break.
///
/// # Errors
///
/// Returns the error of writing to `out`.
pub(crate) fn write<W: Write + ?Sized>(
    out: &mut W,
    tool: &Tool<'_>,
    rules: &[&'static RuleMeta],
    files: &[Results],
) -> io::Result<()> {
    let results = files
        .iter()
        .flat_map(|results| &results.0)
        .map(|entry| SarifResult {
            rule_id: entry.rule.id,
            rule_index: rules.iter().position(|rule| rule.id == entry.rule.id),
            level: entry.level,
            message: &entry.message,
            locations: [&entry.location],
            related_locations: &entry.related,
            partial_fingerprints: Fingerprints {
                workflint: &entry.fingerprint,
            },
        })
        .collect();
    let log = Log {
        schema: SCHEMA,
        version: "2.1.0",
        runs: [Run {
            tool: RunTool {
                driver: Driver {
                    name: tool.name,
                    version: tool.version,
                    semantic_version: tool.version,
                    information_uri: tool.information_uri,
                    rules: rules.iter().map(|rule| Rule::of(rule)).collect(),
                },
            },
            column_kind: "utf16CodeUnits",
            results,
        }],
    };
    serde_json::to_writer_pretty(&mut *out, &log).map_err(io::Error::from)?;
    out.write_all(b"\n")
}

/// The SARIF level of `severity`.
const fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
    }
}

/// The hash part of the fingerprint of `diagnostic`: the start of the SHA-256 of its rule ID,
/// the path of `file`, and the text of its span, or the rest of its line when the span is
/// empty. Line numbers and columns do not go in, so the hash stays the same when other lines
/// move.
fn fingerprint_hash(file: &SourceFile, diagnostic: &Diagnostic) -> String {
    let mut text = span_text(file, diagnostic.span);
    if text.is_empty() {
        let start = offset(file.text(), diagnostic.span.start);
        let rest = file.text().get(start..).unwrap_or_default();
        text = rest
            .split(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}'])
            .next()
            .unwrap_or_default();
    }
    let mut hasher = Sha256::new();
    for part in [diagnostic.rule.id, file.path(), text] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    // 16 hex digits, as long as GitHub's own line hashes.
    hasher
        .finalize()
        .iter()
        .take(8)
        .fold(String::with_capacity(16), |mut hex, byte| {
            // Writing to a `String` cannot fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The text of `span` in `file`, with its ends moved to the end of the text and to the start
/// of the character they are in. A span whose start is after its end is empty.
fn span_text(file: &SourceFile, span: Span) -> &str {
    let text = file.text();
    let start = offset(text, span.start);
    let end = offset(text, span.end).max(start);
    text.get(start..end).unwrap_or_default()
}

/// `at` as an offset into `text`: at most the length of `text`, and moved to the start of the
/// character it is in.
fn offset(text: &str, at: u32) -> usize {
    text.floor_char_boundary(usize::try_from(at).unwrap_or(usize::MAX))
}

#[derive(Serialize)]
struct Log<'a> {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: [Run<'a>; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Run<'a> {
    tool: RunTool<'a>,
    column_kind: &'static str,
    results: Vec<SarifResult<'a>>,
}

#[derive(Serialize)]
struct RunTool<'a> {
    driver: Driver<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Driver<'a> {
    name: &'a str,
    version: &'a str,
    semantic_version: &'a str,
    information_uri: &'a str,
    rules: Vec<Rule>,
}

/// A `reportingDescriptor`: the ID, the description, the documentation URL, and the default
/// level of a rule. A rule outside the `correctness` category is off by default (ADR 0008).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    id: &'static str,
    short_description: Message,
    help_uri: &'static str,
    default_configuration: Configuration,
    properties: RuleProperties,
}

impl Rule {
    fn of(rule: &RuleMeta) -> Self {
        Self {
            id: rule.id,
            short_description: Message {
                text: rule.description.to_owned(),
            },
            help_uri: rule.docs_url,
            default_configuration: Configuration {
                enabled: rule.category == Category::Correctness,
                level: level(rule.default_severity),
            },
            properties: RuleProperties {
                tags: [rule.category.as_str()],
            },
        }
    }
}

#[derive(Serialize)]
struct Configuration {
    #[serde(skip_serializing_if = "is_true")]
    enabled: bool,
    level: &'static str,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde passes a reference"
)]
const fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Serialize)]
struct RuleProperties {
    tags: [&'static str; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult<'a> {
    rule_id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule_index: Option<usize>,
    level: &'static str,
    message: &'a Message,
    locations: [&'a Location; 1],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    related_locations: &'a [RelatedLocation],
    partial_fingerprints: Fingerprints<'a>,
}

/// The `partialFingerprints` of a result. The version in the key changes when the way the
/// fingerprint is computed changes.
#[derive(Serialize)]
struct Fingerprints<'a> {
    #[serde(rename = "workflint/v1")]
    workflint: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct Message {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct RelatedLocation {
    id: usize,
    #[serde(flatten)]
    location: Location,
    message: Message,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    physical_location: PhysicalLocation,
}

impl Location {
    fn of(file: &SourceFile, artifact: &ArtifactLocation, span: Span) -> Self {
        let start = file.line_col_utf16(span.start);
        let end = file.line_col_utf16(span.end.max(span.start));
        Self {
            physical_location: PhysicalLocation {
                artifact_location: artifact.clone(),
                region: Region {
                    start_line: start.line,
                    start_column: start.column,
                    end_line: end.line,
                    end_column: end.column,
                },
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalLocation {
    artifact_location: ArtifactLocation,
    region: Region,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactLocation {
    uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri_base_id: Option<&'static str>,
}

impl ArtifactLocation {
    /// The location of the file at `path`. A relative path becomes a URI reference relative to
    /// `%SRCROOT%`, the root of the sources, and an absolute path a `file` URI. A backslash
    /// separates directories, as on Windows.
    fn of(path: &str) -> Self {
        let path = path.replace('\\', "/");
        let bytes = path.as_bytes();
        let drive = matches!(bytes, [letter, b':', b'/', ..] if letter.is_ascii_alphabetic());
        if drive {
            let (letter, rest) = path.split_at(2);
            return Self {
                uri: format!("file:///{letter}{}", encode(rest)),
                uri_base_id: None,
            };
        }
        if path.starts_with("//") {
            return Self {
                uri: format!("file:{}", encode(&path)),
                uri_base_id: None,
            };
        }
        if path.starts_with('/') {
            return Self {
                uri: format!("file://{}", encode(&path)),
                uri_base_id: None,
            };
        }
        Self {
            uri: encode(&path),
            uri_base_id: Some("%SRCROOT%"),
        }
    }
}

/// Percent-encodes every byte of `path` except the unreserved characters of RFC 3986 and `/`.
fn encode(path: &str) -> String {
    let mut uri = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            // Writing to a `String` cannot fail.
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

#[cfg(test)]
mod tests {
    //! `tests/sarif-schema/sarif-2.1.0.json` is the SARIF 2.1.0 schema from microsoft/sarif-sdk
    //! at commit `aaf860dff84137c7a6346f2bd6c4a5642a33b3c5`, with its original LICENSE file.

    #![allow(clippy::unwrap_used, clippy::panic)]

    use serde_json::Value;
    use workflint_diagnostics::{FileId, Label};

    use super::*;

    #[test]
    fn encode_keeps_unreserved_characters_and_slashes() {
        assert_eq!(encode("a/B-9._~"), "a/B-9._~");
        assert_eq!(encode("a b:c%"), "a%20b%3Ac%25");
    }

    const fn rule(id: &'static str, severity: Severity, category: Category) -> RuleMeta {
        RuleMeta {
            id,
            description: "A test rule reports something",
            kind: "syntax-check",
            category,
            default_severity: severity,
            docs_url: "https://example.com/rules/test.md",
        }
    }

    static ERROR_RULE: RuleMeta = rule("error-rule", Severity::Error, Category::Correctness);
    static INFO_RULE: RuleMeta = rule("info-rule", Severity::Info, Category::Style);
    static WARNING_RULE: RuleMeta = rule("warning-rule", Severity::Warning, Category::Pedantic);
    static RULES: [&RuleMeta; 3] = [&ERROR_RULE, &INFO_RULE, &WARNING_RULE];

    const TOOL: Tool<'static> = Tool {
        name: "workflint",
        version: "1.2.3",
        information_uri: "https://github.com/stokehold/workflint",
    };

    fn file(id: u32, path: &str, text: &str) -> SourceFile {
        SourceFile::new(FileId::new(id), path, text).unwrap()
    }

    /// The span of the `n`th occurrence of `token` in `file`, counting from 0.
    fn nth_span(file: &SourceFile, token: &str, n: usize) -> Span {
        let start = file.text().match_indices(token).nth(n).unwrap().0;
        let start = u32::try_from(start).unwrap();
        let end = start + u32::try_from(token.len()).unwrap();
        Span::new(file.id(), start, end)
    }

    fn span(file: &SourceFile, token: &str) -> Span {
        nth_span(file, token, 0)
    }

    fn sarif_text(files: &[(&SourceFile, Vec<Diagnostic>)]) -> String {
        let results: Vec<Results> = files
            .iter()
            .map(|(file, diagnostics)| Results::of(file, diagnostics))
            .collect();
        let mut out = Vec::new();
        write(&mut out, &TOOL, &RULES, &results).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn sarif_json(files: &[(&SourceFile, Vec<Diagnostic>)]) -> Value {
        serde_json::from_str(&sarif_text(files)).unwrap()
    }

    fn first_result(sarif: &Value) -> &Value {
        &sarif["runs"][0]["results"][0]
    }

    fn fingerprints(sarif: &Value) -> Vec<Value> {
        sarif["runs"][0]["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|result| result["partialFingerprints"].clone())
            .collect()
    }

    #[test]
    fn zero_diagnostics_print_every_rule_and_no_results() {
        let empty = file(0, "a.yaml", "on: push\n");
        insta::assert_snapshot!(sarif_text(&[(&empty, Vec::new())]));
    }

    #[test]
    fn non_ascii_text_before_a_diagnostic_counts_columns_in_utf16_code_units() {
        // "😀" is one code point but two UTF-16 code units, so `bad` is at column 12 in SARIF and
        // at column 11 in the default format.
        let text = "on: push\njobs: 日本😀 bad\n";
        let f = file(0, ".github/workflows/ci.yaml", text);
        let d = Diagnostic::new(&ERROR_RULE, span(&f, "bad"), "the value is bad");
        assert_eq!(f.line_col(d.span.start).column, 11);
        let text = sarif_text(&[(&f, vec![d])]);
        let sarif: Value = serde_json::from_str(&text).unwrap();
        let region = &first_result(&sarif)["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startColumn"], 12);
        assert_eq!(region["endColumn"], 15);
        assert_eq!(sarif["runs"][0]["columnKind"], "utf16CodeUnits");
        insta::assert_snapshot!(text);
    }

    #[test]
    fn labels_become_related_locations_and_help_follows_the_message() {
        let text =
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n  build:\n    runs-on: x\n";
        let f = file(0, "dup.yaml", text);
        let mut d = Diagnostic::new(
            &WARNING_RULE,
            nth_span(&f, "build", 1),
            "key \"build\" is duplicated",
        )
        .with_help(Some("rename one of the jobs".to_owned()));
        d.labels.push(Label {
            span: span(&f, "build"),
            note: "first defined here".to_owned(),
        });
        insta::assert_snapshot!(sarif_text(&[(&f, vec![d])]));
    }

    #[test]
    fn severities_map_to_sarif_levels() {
        let f = file(0, "a.yaml", "a b c\n");
        let diagnostics = vec![
            Diagnostic::new(&ERROR_RULE, span(&f, "a"), "e"),
            Diagnostic::new(&WARNING_RULE, span(&f, "b"), "w"),
            Diagnostic::new(&INFO_RULE, span(&f, "c"), "i"),
        ];
        let sarif = sarif_json(&[(&f, diagnostics)]);
        let levels: Vec<&Value> = sarif["runs"][0]["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|result| &result["level"])
            .collect();
        assert_eq!(levels, ["error", "warning", "note"]);
        let defaults: Vec<&Value> = sarif["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| &rule["defaultConfiguration"]["level"])
            .collect();
        assert_eq!(defaults, ["error", "note", "warning"]);
    }

    #[test]
    fn a_fingerprint_stays_the_same_when_a_line_is_inserted_above() {
        let before = "on: push\njobs:\n  a:\n    runs-on: bad-label\n";
        let after = "# a comment\non: push\njobs:\n\n  a:\n    runs-on: bad-label\n";
        let fingerprint = |text: &str| {
            let f = file(0, "ci.yaml", text);
            let d = Diagnostic::new(&ERROR_RULE, span(&f, "bad-label"), "unknown label");
            let sarif = sarif_json(&[(&f, vec![d])]);
            let line =
                first_result(&sarif)["locations"][0]["physicalLocation"]["region"]["startLine"]
                    .clone();
            (line, fingerprints(&sarif))
        };
        let (line_before, before) = fingerprint(before);
        let (line_after, after) = fingerprint(after);
        assert_ne!(line_before, line_after);
        assert_eq!(before, after);
    }

    #[test]
    fn the_same_rule_on_the_same_text_twice_gets_two_fingerprints() {
        let f = file(0, "ci.yaml", "x: bad\ny: bad\n");
        let diagnostics = vec![
            Diagnostic::new(&ERROR_RULE, nth_span(&f, "bad", 0), "bad"),
            Diagnostic::new(&ERROR_RULE, nth_span(&f, "bad", 1), "bad"),
        ];
        let prints = fingerprints(&sarif_json(&[(&f, diagnostics)]));
        assert_ne!(prints[0], prints[1]);
    }

    #[test]
    fn a_fingerprint_depends_on_the_rule_the_file_and_the_span_text() {
        let one = |rule: &'static RuleMeta, path: &str, text: &str, token: &str| {
            let f = file(0, path, text);
            let d = Diagnostic::new(rule, span(&f, token), "message");
            fingerprints(&sarif_json(&[(&f, vec![d])]))
        };
        let base = one(&ERROR_RULE, "a.yaml", "x: tok\n", "tok");
        assert_ne!(base, one(&WARNING_RULE, "a.yaml", "x: tok\n", "tok"));
        assert_ne!(base, one(&ERROR_RULE, "b.yaml", "x: tok\n", "tok"));
        assert_ne!(base, one(&ERROR_RULE, "a.yaml", "x: tokk\n", "tokk"));
        assert_eq!(base, one(&ERROR_RULE, "a.yaml", "yy: tok\n", "tok"));
    }

    #[test]
    fn an_empty_span_is_fingerprinted_with_the_rest_of_its_line() {
        let f = file(0, "ci.yaml", "a: one\nb: two\n");
        let at = |token: &str| {
            let start = span(&f, token).start;
            Diagnostic::new(&ERROR_RULE, Span::new(f.id(), start, start), "m")
        };
        let prints = fingerprints(&sarif_json(&[(&f, vec![at("one"), at("two")])]));
        assert_ne!(prints[0], prints[1]);
        let moved = file(0, "ci.yaml", "\nc: one\n");
        let start = span(&moved, "one").start;
        let d = Diagnostic::new(&ERROR_RULE, Span::new(moved.id(), start, start), "m");
        assert_eq!(
            fingerprints(&sarif_json(&[(&moved, vec![d])]))[0],
            prints[0]
        );
    }

    #[test]
    fn paths_become_uri_references() {
        let cases = [
            ("ci.yaml", "ci.yaml", true),
            (
                ".github/workflows/my workflow#1.yaml",
                ".github/workflows/my%20workflow%231.yaml",
                true,
            ),
            ("dir\\ci.yaml", "dir/ci.yaml", true),
            ("/abs/ci.yaml", "file:///abs/ci.yaml", false),
            ("C:\\work\\ci.yaml", "file:///C:/work/ci.yaml", false),
            ("<stdin>", "%3Cstdin%3E", true),
            ("ü.yaml", "%C3%BC.yaml", true),
        ];
        for (path, uri, relative) in cases {
            let f = file(0, path, "a\n");
            let d = Diagnostic::new(&ERROR_RULE, span(&f, "a"), "m");
            let sarif = sarif_json(&[(&f, vec![d])]);
            let artifact =
                &first_result(&sarif)["locations"][0]["physicalLocation"]["artifactLocation"];
            assert_eq!(artifact["uri"], uri, "path {path}");
            assert_eq!(artifact.get("uriBaseId").is_some(), relative, "path {path}");
        }
    }

    #[test]
    fn output_validates_against_the_sarif_2_1_0_schema() {
        let schema: Value =
            serde_json::from_str(include_str!("../tests/sarif-schema/sarif-2.1.0.json")).unwrap();
        let mut schemas = boon::Schemas::new();
        let mut compiler = boon::Compiler::new();
        compiler.add_resource("sarif-2.1.0.json", schema).unwrap();
        let index = compiler.compile("sarif-2.1.0.json", &mut schemas).unwrap();

        let a = file(0, "a.yaml", "on: push\njobs: 日本😀 bad\n");
        let b = file(1, "dir/b.yaml", "x: tok\ny:\n  tok\n");
        let mut labelled = Diagnostic::new(&WARNING_RULE, nth_span(&b, "tok", 1), "twice")
            .with_help(Some("did you mean \"tick\"?".to_owned()));
        labelled.labels.push(Label {
            span: span(&b, "tok"),
            note: "first here".to_owned(),
        });
        let empty_span = Span::new(b.id(), 0, 0);
        let cases = [
            sarif_json(&[(&a, Vec::new())]),
            sarif_json(&[
                (
                    &a,
                    vec![Diagnostic::new(&ERROR_RULE, span(&a, "bad"), "bad")],
                ),
                (
                    &b,
                    vec![
                        Diagnostic::new(&INFO_RULE, empty_span, "at the start"),
                        labelled,
                    ],
                ),
            ]),
        ];
        for sarif in cases {
            if let Err(err) = schemas.validate(&sarif, index) {
                panic!("{err:#}\n{sarif:#}");
            }
        }
    }
}
