//! Compares diagnostics with the expected lines of a `.out` file, as
//! `checkErrors` in the reference's `linter_test.go:107` does.

use std::fmt::Write as _;

use regex::Regex;

use super::output::Diagnostic;

/// Reads the expected lines of a `.out` file. An empty file expects no
/// errors.
pub fn expected_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

/// Checks `actual` against `expected`.
///
/// The diagnostics are sorted by path, line, column and message first, as
/// the reference sorts its errors. The counts must be equal. Then the n-th
/// expected line must equal the n-th diagnostic, or, if the line starts and
/// ends with `/`, the pattern between the slashes must match it anywhere.
///
/// # Errors
///
/// Returns a report of the count mismatch, or of every line that does not
/// match.
pub fn check_errors(expected: &[String], actual: &[Diagnostic]) -> Result<(), String> {
    let mut sorted = actual.to_vec();
    sorted.sort_by(Diagnostic::reference_order);
    let have: Vec<String> = sorted.iter().map(ToString::to_string).collect();

    if have.len() != expected.len() {
        let mut report = format!(
            "{} errors are expected but actually got {} errors",
            expected.len(),
            have.len()
        );
        for line in &have {
            write!(report, "\n  {line}").expect("writing to a String succeeds");
        }
        return Err(report);
    }

    let mut report = String::new();
    for (index, (want, have)) in expected.iter().zip(&have).enumerate() {
        let number = index + 1;
        match (pattern(want), line_matches(want, have)) {
            (_, Ok(true)) => {}
            (Some(source), Ok(false)) => write!(
                report,
                "\nerror {number} does not match the regular expression\n  want: /{source}/\n  have: {have:?}"
            )
            .expect("writing to a String succeeds"),
            (None, Ok(false)) => write!(
                report,
                "\nerror {number} does not match exactly\n  want: {want:?}\n  have: {have:?}"
            )
            .expect("writing to a String succeeds"),
            (_, Err(error)) => write!(report, "\nexpected line {number} {error}")
                .expect("writing to a String succeeds"),
        }
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report.trim_start().to_owned())
    }
}

/// Whether the error line `have` matches the expected line `want`: equal to
/// it, or, if `want` starts and ends with `/`, matched anywhere by the
/// pattern between the slashes.
///
/// # Errors
///
/// Returns a message that quotes the pattern if it is not a valid regular
/// expression.
pub fn line_matches(want: &str, have: &str) -> Result<bool, String> {
    match pattern(want) {
        None => Ok(want == have),
        Some(source) => Regex::new(&from_go(source))
            .map(|regex| regex.is_match(have))
            .map_err(|error| format!("is not a valid regular expression: /{source}/: {error}")),
    }
}

/// Translates a Go RE2 pattern for the `regex` crate. Go reads a `{` that
/// does not start a repetition such as `{2}`, `{2,}` or `{2,5}` as a literal,
/// and `regex` rejects it, so this escapes such braces. Five `.out` files use
/// them, as in `{.+}` for an object type.
#[must_use]
pub fn from_go(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        match c {
            '\\' => {
                out.extend(chars.get(index..index + 2).unwrap_or(&chars[index..]));
                index += 2;
                continue;
            }
            '[' => {
                let end = class_end(&chars, index);
                out.extend(&chars[index..end]);
                index = end;
                continue;
            }
            '{' => match repetition_len(&chars[index..]) {
                Some(len) => {
                    out.extend(&chars[index..index + len]);
                    index += len;
                    continue;
                }
                None => out.push_str("\\{"),
            },
            '}' => out.push_str("\\}"),
            _ => out.push(c),
        }
        index += 1;
    }
    out
}

/// The index after the `]` that closes the class starting at `start`.
fn class_end(chars: &[char], start: usize) -> usize {
    let mut index = start + 1;
    if chars.get(index) == Some(&'^') {
        index += 1;
    }
    if chars.get(index) == Some(&']') {
        index += 1;
    }
    while let Some(&c) = chars.get(index) {
        match c {
            '\\' => index += 2,
            ']' => return index + 1,
            _ => index += 1,
        }
    }
    chars.len()
}

/// The length of `{n}`, `{n,}` or `{n,m}` at the start of `chars`.
fn repetition_len(chars: &[char]) -> Option<usize> {
    let close = chars.iter().position(|&c| c == '}')?;
    let inner: String = chars[1..close].iter().collect();
    let (min, max) = inner.split_once(',').unwrap_or((&inner, "0"));
    let digits = |text: &str| text.chars().all(|c| c.is_ascii_digit());
    (!min.is_empty() && digits(min) && digits(max)).then_some(close + 1)
}

/// The pattern of an expected line that starts and ends with `/`.
#[must_use]
pub fn pattern(line: &str) -> Option<&str> {
    line.strip_prefix('/')?.strip_suffix('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::output::parse_line;

    fn diagnostics(lines: &[&str]) -> Vec<Diagnostic> {
        lines.iter().map(|line| parse_line(line).unwrap()).collect()
    }

    fn lines(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn exact_lines_match() {
        let actual = diagnostics(&["test.yaml:1:2: a [x]", "test.yaml:3:4: b [y]"]);
        let expected = lines(&["test.yaml:1:2: a [x]", "test.yaml:3:4: b [y]"]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
    }

    #[test]
    fn an_exact_line_with_another_message_does_not_match() {
        let actual = diagnostics(&["test.yaml:1:2: a [x]"]);
        let error = check_errors(&lines(&["test.yaml:1:2: b [x]"]), &actual).unwrap_err();
        assert!(error.contains("error 1 does not match exactly"), "{error}");
    }

    #[test]
    fn a_regex_line_matches_anywhere_in_the_error() {
        let actual = diagnostics(&[
            "test.yaml:5:7: label \"x\" is unknown. labels are \"a\", \"b\" [runner-label]",
        ]);
        let expected = lines(&[r"/test\.yaml:5:7: label .x. is unknown\. .+ \[runner-label\]/"]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
        assert_eq!(check_errors(&lines(&["/is unknown/"]), &actual), Ok(()));
    }

    #[test]
    fn a_regex_line_that_does_not_match_is_reported() {
        let actual = diagnostics(&["test.yaml:5:7: something else [x]"]);
        let error = check_errors(&lines(&["/is unknown/"]), &actual).unwrap_err();
        assert!(error.contains("regular expression"), "{error}");
    }

    #[test]
    fn an_invalid_regex_line_is_reported() {
        let actual = diagnostics(&["test.yaml:5:7: a [x]"]);
        let error = check_errors(&lines(&["/(unclosed/"]), &actual).unwrap_err();
        assert!(error.contains("not a valid regular expression"), "{error}");
    }

    #[test]
    fn a_count_mismatch_lists_the_actual_errors() {
        let actual = diagnostics(&["test.yaml:1:1: a [x]", "test.yaml:2:1: b [x]"]);
        let error = check_errors(&lines(&["test.yaml:1:1: a [x]"]), &actual).unwrap_err();
        assert!(
            error.starts_with("1 errors are expected but actually got 2 errors"),
            "{error}"
        );
        assert!(error.contains("test.yaml:2:1: b [x]"), "{error}");
    }

    #[test]
    fn no_errors_match_an_empty_out_file() {
        assert_eq!(check_errors(&expected_lines(""), &[]), Ok(()));
    }

    #[test]
    fn errors_are_sorted_by_line_and_column_before_comparing() {
        let actual = diagnostics(&[
            "test.yaml:10:1: c [x]",
            "test.yaml:2:9: b [x]",
            "test.yaml:2:3: a [x]",
        ]);
        let expected = lines(&[
            "test.yaml:2:3: a [x]",
            "test.yaml:2:9: b [x]",
            "test.yaml:10:1: c [x]",
        ]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
    }

    #[test]
    fn errors_at_one_position_are_sorted_by_message_not_by_code() {
        // workflint sorts by rule ID before message (docs/differences.md#sort-order).
        // After mapping to kinds, the runner sorts by message as the reference does.
        let actual = diagnostics(&[
            "test.yaml:1:1: b message [action]",
            "test.yaml:1:1: a message [expression]",
        ]);
        let expected = lines(&[
            "test.yaml:1:1: a message [expression]",
            "test.yaml:1:1: b message [action]",
        ]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
    }

    #[test]
    fn errors_in_several_files_are_sorted_by_path_first() {
        let actual = diagnostics(&["workflows/b.yaml:1:1: m [x]", "workflows/a.yaml:9:9: m [x]"]);
        let expected = lines(&["workflows/a.yaml:9:9: m [x]", "workflows/b.yaml:1:1: m [x]"]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
    }

    #[test]
    fn windows_paths_match_slash_paths() {
        let actual = diagnostics(&[r"workflows\nested\a.yaml:1:1: m [x]"]);
        let expected = lines(&["workflows/nested/a.yaml:1:1: m [x]"]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
    }

    #[test]
    fn braces_that_go_reads_as_literals_are_escaped() {
        assert_eq!(from_go("type {.+}"), r"type \{.+\}");
        assert_eq!(from_go("{string => string}"), r"\{string => string\}");
        assert_eq!(
            from_go(r#""{owner}/{repo}@{ref}""#),
            r#""\{owner\}/\{repo\}@\{ref\}""#
        );
    }

    #[test]
    fn repetitions_classes_and_escapes_are_kept() {
        assert_eq!(from_go("a{2}b{1,}c{3,4}"), "a{2}b{1,}c{3,4}");
        assert_eq!(from_go(r"[{}]\{x\}"), r"[{}]\{x\}");
        assert_eq!(from_go(r"[]{]{"), r"[]{]\{");
    }

    #[test]
    fn a_regex_line_with_literal_braces_matches() {
        let actual = diagnostics(&[
            "test.yaml:9:24: property \"events\" is not defined in object type {a: string} [expression]",
        ]);
        let expected = lines(&[
            r#"/test\.yaml:9:24: property "events" is not defined in object type {.+} \[expression\]/"#,
        ]);
        assert_eq!(check_errors(&expected, &actual), Ok(()));
    }

    #[test]
    fn a_lone_slash_is_an_exact_line() {
        assert_eq!(pattern("/"), None);
        assert_eq!(pattern("//"), Some(""));
        assert_eq!(pattern("/a/"), Some("a"));
    }
}
