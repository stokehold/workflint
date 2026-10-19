//! `strategy` of a job, as the reference's `parseStrategy`, `parseMatrix`,
//! `parseMatrixCombinations`, `parseMaxParallel`, and `parseRawYAMLValue`
//! at `parse.go:873-1005`.
//!
//! Strategy keys are case-sensitive. Matrix keys, the keys of `include` and
//! `exclude` items, and the keys of mappings in matrix values are
//! case-insensitive, and every collection keeps its source order.

use std::sync::Arc;

use workflint_diagnostics::Span;
use workflint_diagnostics::quote::quote;
use workflint_yaml::{Node, NodeKind};

use super::shared::mapping_field;
use super::{Item, KeySpec, Label, MapEntry, Parser, Section};
use crate::rules::UNEXPECTED_NODE_KIND;
use crate::{
    Field, Keyed, KeyedEntry, Matrix, MatrixCombination, MatrixRow, RawValue, Str, Strategy, Value,
};

impl Section for Strategy<'static> {
    type Ast<'a> = Strategy<'a>;
}

static STRATEGY_KEYS: [KeySpec<Strategy<'static>>; 3] = [
    KeySpec {
        name: "matrix",
        parse: |p, s, e| s.matrix = p.parse_matrix(e),
    },
    KeySpec {
        name: "fail-fast",
        parse: |p, s, e| s.fail_fast = p.parse_bool(e.key.string, e.value),
    },
    KeySpec {
        name: "max-parallel",
        parse: |p, s, e| s.max_parallel = p.parse_max_parallel(e),
    },
];

/// A step of the walk in [`Parser::parse_raw_values`].
enum RawStep<'a> {
    /// Parses the node, and pushes its value, or `None` if it is reported,
    /// on the stack of values. The walk reaches the node through the
    /// item's alias path.
    Enter(Item<'a>),
    /// Pops the values of the `len` items of a sequence and pushes the
    /// sequence.
    Sequence { span: Span, len: usize },
    /// Pops the values of the entries of a mapping and pushes the mapping.
    /// `keys` holds the key of each entry.
    Mapping {
        span: Span,
        keys: Vec<Option<Str<'a>>>,
    },
}

impl<'a> Parser<'a, '_> {
    /// Parses `strategy` of a job, the reference's `parseStrategy`: a
    /// mapping with `matrix`, `fail-fast`, and `max-parallel` that must have
    /// entries.
    pub(super) fn parse_strategy(&mut self, entry: &MapEntry<'a>) -> Field<'a, Strategy<'a>> {
        let node = entry.value;
        let mut strategy = Strategy::default();
        let entries = self.parse_mapping(Label::Section("strategy"), node, false, true);
        self.parse_keys(entries, "strategy", &STRATEGY_KEYS, &mut strategy);
        mapping_field(entry.key, node, strategy)
    }

    /// Parses `max-parallel`, the reference's `parseMaxParallel`: an
    /// integer, or a single expression. A literal that is zero or less is
    /// reported and kept.
    fn parse_max_parallel(&mut self, entry: &MapEntry<'a>) -> Value<'a, i64> {
        let value = self.parse_int(entry.key.string, entry.value);
        if let Value::Literal { value: max, .. } = value
            && max <= 0
        {
            self.non_positive("max-parallel", entry.value, &max.to_string());
        }
        value
    }

    /// Parses `matrix`, the reference's `parseMatrix`.
    ///
    /// A scalar must be a single expression, whatever its tag. Anything
    /// else is a case-insensitive mapping that must have entries: `include`,
    /// `exclude`, and the rows. A row whose key is not a scalar is parsed
    /// and then left out, as a job is.
    fn parse_matrix(&mut self, entry: &MapEntry<'a>) -> Value<'a, Matrix<'a>> {
        let key = entry.key.string;
        let node = entry.value;
        if let NodeKind::Scalar(_) = node.kind {
            return self.literal_expression(key, node, "matrix");
        }
        let mut matrix = Matrix::default();
        let mut rows = Vec::new();
        for entry in self.parse_mapping(Label::Section("matrix"), node, false, false) {
            match &*entry.id {
                "include" => {
                    matrix.include =
                        self.within(entry.path, |p| p.parse_combinations("include", &entry));
                }
                "exclude" => {
                    matrix.exclude =
                        self.within(entry.path, |p| p.parse_combinations("exclude", &entry));
                }
                _ => {
                    if let Some(row) = self.within(entry.path, |p| p.parse_matrix_row(entry.value))
                        && let Some(name) = entry.key.string
                    {
                        rows.push(KeyedEntry::new(name, entry.value.span, row));
                    }
                }
            }
        }
        matrix.rows = Keyed::new(rows);
        let span = node.span;
        match node.kind {
            NodeKind::Mapping(_) => Value::Literal {
                key,
                span,
                value: matrix,
            },
            NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => {
                Value::Invalid { key, span }
            }
        }
    }

    /// Parses the value of a matrix row: a single expression or a sequence
    /// of raw values with items.
    ///
    /// A scalar that is not an expression gives [`MatrixRow::Invalid`]. Any
    /// other value that is not a sequence with items gives `None`, so the
    /// row is left out. The reference keeps the first and drops the second
    /// in the same way.
    fn parse_matrix_row(&mut self, node: &'a Node) -> Option<MatrixRow<'a>> {
        if let NodeKind::Scalar(_) = node.kind {
            let row = match self.parse_expression(node, "array value for matrix variations") {
                Some(expr) => MatrixRow::Expression(expr),
                None => MatrixRow::Invalid,
            };
            return Some(row);
        }
        let items = self.check_sequence("matrix values", node, false)?;
        let values: Vec<_> = self.parse_raw_values(items).into_iter().flatten().collect();
        Some(MatrixRow::Values(values.into()))
    }

    /// Parses `include` or `exclude` of a matrix, the reference's
    /// `parseMatrixCombinations`: a single expression, or a sequence with
    /// items.
    ///
    /// An item that is a scalar must be a single expression, and is
    /// reported and left out otherwise. Any other item is a case-insensitive
    /// mapping of raw values that must have entries. An item that is not a
    /// mapping is reported and kept as [`MatrixCombination::Invalid`]. The
    /// reference keeps it as a combination with no values.
    fn parse_combinations(
        &mut self,
        section: &str,
        entry: &MapEntry<'a>,
    ) -> Value<'a, Arc<[MatrixCombination<'a>]>> {
        let key = entry.key.string;
        let node = entry.value;
        if let NodeKind::Scalar(_) = node.kind {
            return self.literal_expression(key, node, "array of matrix combination");
        }
        let Some(items) = self.check_sequence(section, node, false) else {
            return Value::Invalid {
                key,
                span: node.span,
            };
        };
        let label = format!("element in {} section", quote(section));
        let mut combinations = Vec::with_capacity(items.len());
        for item in items {
            let combination = self.within(item.path, |p| p.parse_combination(item.node, &label));
            combinations.extend(combination);
        }
        Value::Literal {
            key,
            span: node.span,
            value: combinations.into(),
        }
    }

    /// Parses `item`, an item of `include` or `exclude`, or returns `None`
    /// if it is a scalar that is not a single expression.
    fn parse_combination(&mut self, item: &'a Node, label: &str) -> Option<MatrixCombination<'a>> {
        if let NodeKind::Scalar(_) = item.kind {
            return self
                .parse_expression(item, "mapping of matrix combination")
                .map(MatrixCombination::Expression);
        }
        let entries = self.parse_mapping(Label::At(label), item, false, false);
        Some(match item.kind {
            NodeKind::Mapping(_) => MatrixCombination::Assigns {
                span: item.span,
                assigns: self.parse_raw_entries(&entries),
            },
            NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => {
                MatrixCombination::Invalid(item.span)
            }
        })
    }

    /// Parses the values of `entries` as raw values, keyed by the entries'
    /// keys. An entry whose key is not a scalar, or whose value is
    /// reported, is left out.
    fn parse_raw_entries(&mut self, entries: &[MapEntry<'a>]) -> Keyed<'a, RawValue<'a>> {
        let keys = entries.iter().map(|entry| entry.key.string).collect();
        let values = self.parse_raw_values(
            entries
                .iter()
                .map(|entry| Item {
                    node: entry.value,
                    path: entry.path,
                })
                .collect(),
        );
        keyed_raw_values(keys, values)
    }

    /// Parses each of `nodes` as a raw value, the reference's
    /// `parseRawYAMLValue`, and returns one value for each. The value is
    /// `None` for a node that is reported.
    ///
    /// A scalar keeps its tag and style. A mapping is case-insensitive and
    /// may be empty. An alias, which only a recursive alias leaves in the
    /// tree, is reported as `unexpected alias node on parsing value in
    /// matrix row` and left out of its sequence or mapping.
    ///
    /// Matrix values may nest as deep as the YAML layer allows, so the walk
    /// keeps its own stack instead of recursing.
    fn parse_raw_values(&mut self, items: Vec<Item<'a>>) -> Vec<Option<RawValue<'a>>> {
        let mut values: Vec<Option<RawValue<'a>>> = Vec::with_capacity(items.len());
        let mut steps: Vec<RawStep<'a>> = items.into_iter().rev().map(RawStep::Enter).collect();
        let outer = self.path;
        while let Some(step) = steps.pop() {
            match step {
                RawStep::Enter(Item { node, path }) => {
                    self.path = path;
                    match &node.kind {
                        NodeKind::Scalar(_) => values.push(
                            self.string(node)
                                .map(|scalar| RawValue::Scalar(self.template(scalar))),
                        ),
                        NodeKind::Sequence(sequence) => {
                            let items = self.items(sequence);
                            steps.push(RawStep::Sequence {
                                span: node.span,
                                len: items.len(),
                            });
                            steps.extend(items.into_iter().rev().map(RawStep::Enter));
                        }
                        NodeKind::Mapping(_) => {
                            let entries = self.parse_mapping(
                                Label::At("matrix row value"),
                                node,
                                true,
                                false,
                            );
                            steps.push(RawStep::Mapping {
                                span: node.span,
                                keys: entries.iter().map(|entry| entry.key.string).collect(),
                            });
                            steps.extend(entries.into_iter().rev().map(|entry| {
                                RawStep::Enter(Item {
                                    node: entry.value,
                                    path: entry.path,
                                })
                            }));
                        }
                        NodeKind::Alias(_) => {
                            self.error(
                                &UNEXPECTED_NODE_KIND,
                                node.span,
                                "unexpected alias node on parsing value in matrix row".to_owned(),
                            );
                            values.push(None);
                        }
                    }
                }
                RawStep::Sequence { span, len } => {
                    let items = values.split_off(values.len().saturating_sub(len));
                    let items: Vec<_> = items.into_iter().flatten().collect();
                    values.push(Some(RawValue::Sequence {
                        span,
                        items: items.into(),
                    }));
                }
                RawStep::Mapping { span, keys } => {
                    let entries = values.split_off(values.len().saturating_sub(keys.len()));
                    values.push(Some(RawValue::Mapping {
                        span,
                        entries: keyed_raw_values(keys, entries),
                    }));
                }
            }
        }
        self.path = outer;
        values
    }
}

/// Pairs each of `keys` with the value at the same index, and leaves out the
/// pairs whose key is not a scalar or whose value is `None`.
fn keyed_raw_values<'a>(
    keys: Vec<Option<Str<'a>>>,
    values: Vec<Option<RawValue<'a>>>,
) -> Keyed<'a, RawValue<'a>> {
    let entries = keys
        .into_iter()
        .zip(values)
        .filter_map(|(key, value)| {
            let (key, value) = (key?, value?);
            Some(KeyedEntry::new(key, value.span(), value))
        })
        .collect();
    Keyed::new(entries)
}

#[cfg(test)]
mod tests;
