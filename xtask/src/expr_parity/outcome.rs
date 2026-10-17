//! What a checker reports for one expression, and how `workflint-expr` gets
//! there.

use std::fmt::{self, Write as _};

use serde::Deserialize;
use workflint_expr::{TypeEnv, parse_expression};

/// An error with its 1-based line and its 1-based column in code points,
/// counted from the start of the expression.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Message {
    pub(crate) line: usize,
    pub(crate) column: usize,
    #[serde(rename = "message")]
    pub(crate) text: String,
}

/// The result of checking one expression: the parse error, or the type, the
/// constant flag, and the errors in the order the checker reports them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum Outcome {
    ParseError {
        parse_error: Message,
    },
    Checked {
        #[serde(rename = "type")]
        ty: String,
        constant: bool,
        #[serde(default)]
        errors: Vec<Message>,
    },
}

impl Outcome {
    /// Sorts each run of errors at the same position by message, and keeps
    /// the order of the runs.
    ///
    /// The linter sorts diagnostics at one position by message
    /// (`docs/differences.md#sort-order`), so their order in the checker
    /// cannot show. The reference's order there can change from run to run:
    /// `format` reports missing placeholders in Go map order.
    #[must_use]
    pub(crate) fn normalized(mut self) -> Self {
        if let Self::Checked { errors, .. } = &mut self {
            for run in errors.chunk_by_mut(|a, b| (a.line, a.column) == (b.line, b.column)) {
                run.sort_by(|a, b| a.text.cmp(&b.text));
            }
        }
        self
    }
}

impl fmt::Display for Outcome {
    /// One line for the result, then one line per error. The allow-list
    /// records outcomes in this form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = |f: &mut fmt::Formatter<'_>, m: &Message| {
            write!(f, "{}:{}: {}", m.line, m.column, m.text)
        };
        match self {
            Self::ParseError { parse_error } => {
                f.write_str("parse error at ")?;
                message(f, parse_error)
            }
            Self::Checked {
                ty,
                constant,
                errors,
            } => {
                write!(f, "type: {ty}, constant: {constant}")?;
                for error in errors {
                    f.write_char('\n')?;
                    message(f, error)?;
                }
                Ok(())
            }
        }
    }
}

/// Parses `expression` with `}}` after it, as the reference's harness does,
/// and checks it in `env`.
pub(crate) fn check(env: &TypeEnv, expression: &str) -> Outcome {
    let src = format!("{expression}}}}}");
    let at = |offset: u32, text: String| {
        let (line, column) = position(&src, offset);
        Message { line, column, text }
    };
    match parse_expression(&src) {
        Err(error) => Outcome::ParseError {
            parse_error: at(error.start, error.message),
        },
        Ok((expr, _)) => {
            let (ty, errors) = env.check(&expr);
            Outcome::Checked {
                ty: ty.to_string(),
                constant: expr.is_constant(),
                errors: errors
                    .into_iter()
                    .map(|error| at(error.start, error.message))
                    .collect(),
            }
        }
    }
}

/// The line and column of the byte `offset` in `src`, as Go's `text/scanner`
/// counts them in the reference's lexer: lines end at `\n`, and columns count
/// code points.
fn position(src: &str, offset: u32) -> (usize, usize) {
    let end = usize::try_from(offset).map_or(src.len(), |offset| offset.min(src.len()));
    let before = src.get(..end).unwrap_or(src);
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    (line, before[line_start..].chars().count() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_lines_and_code_points() {
        assert_eq!(position("abc", 0), (1, 1));
        assert_eq!(position("abc", 2), (1, 3));
        assert_eq!(position("é x", 3), (1, 3));
        assert_eq!(position("a\n  b", 4), (2, 3));
        assert_eq!(position("ab", 9), (1, 3));
    }

    #[test]
    fn a_checked_expression_renders_its_type_and_errors() {
        let mut env = TypeEnv::new();
        env.set_workflow_key("jobs.<job_id>.steps.if");
        let outcome = check(&env, "1 == foo.bar");
        assert_eq!(
            outcome.to_string(),
            "type: bool, constant: false\n1:6: undefined variable \"foo\". available variables are \"env\", \"github\", \"inputs\", \"job\", \"matrix\", \"needs\", \"runner\", \"secrets\", \"steps\", \"strategy\", \"vars\""
        );
    }

    #[test]
    fn a_parse_error_renders_its_position() {
        let outcome = check(&TypeEnv::new(), "a.\n  (");
        assert!(
            outcome.to_string().starts_with("parse error at 2:3: "),
            "{outcome}"
        );
    }

    fn message(line: usize, column: usize, text: &str) -> Message {
        Message {
            line,
            column,
            text: text.to_owned(),
        }
    }

    #[test]
    fn normalizing_sorts_errors_at_one_position_and_keeps_the_rest_in_order() {
        let outcome = Outcome::Checked {
            ty: "string".to_owned(),
            constant: false,
            errors: vec![
                message(1, 5, "z"),
                message(1, 1, "c"),
                message(1, 1, "a"),
                message(1, 5, "b"),
                message(1, 1, "b"),
            ],
        };
        assert_eq!(
            outcome.normalized().to_string(),
            "type: string, constant: false\n1:5: z\n1:1: a\n1:1: c\n1:5: b\n1:1: b"
        );
    }

    #[test]
    fn the_go_program_output_parses_into_outcomes() {
        let checked: Outcome = serde_json::from_str(
            r#"{"type":"any","constant":false,"errors":[{"line":1,"column":2,"message":"m"}]}"#,
        )
        .unwrap();
        assert_eq!(checked.to_string(), "type: any, constant: false\n1:2: m");
        let constant: Outcome =
            serde_json::from_str(r#"{"type":"number","constant":true}"#).unwrap();
        assert_eq!(constant.to_string(), "type: number, constant: true");
        let parse_error: Outcome =
            serde_json::from_str(r#"{"parse_error":{"line":1,"column":3,"message":"m"}}"#).unwrap();
        assert_eq!(parse_error.to_string(), "parse error at 1:3: m");
        assert!(serde_json::from_str::<Outcome>(r#"{"type":"any"}"#).is_err());
    }
}
