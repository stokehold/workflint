//! The combinations of a matrix, as GitHub expands them, for the labels that the evaluator works
//! out.
//!
//! GitHub takes every combination of the row values, in row order, removes each combination that
//! an `exclude` item matches, and then applies `include`. An `include` item extends each
//! combination whose row values it does not change, and is a combination of its own when it
//! extends none. The workflow syntax reference describes this under
//! `jobs.<job_id>.strategy.matrix.include`.

use workflint_syntax::{
    Matrix, MatrixCombination, MatrixRow, RawValue, Value, contains_expression,
};

use super::eval::MAX_VALUES;

/// One combination: the value of each matrix key, keyed by the folded key, in the order the keys
/// were added.
pub(super) type Combination<'m, 'a> = Vec<(&'m str, &'m RawValue<'a>)>;

/// Returns the combinations of `matrix`, or `None` if they cannot be worked out: the matrix, a
/// row, `include`, or `exclude` is an expression or invalid, an `include` or `exclude` item
/// compares a value that is not a scalar without an expression, or there are more than
/// [`MAX_VALUES`] combinations.
pub(super) fn combinations<'m, 'a>(matrix: &'m Matrix<'a>) -> Option<Vec<Combination<'m, 'a>>> {
    let mut combinations = rows(matrix)?;
    let exclude = items(&matrix.exclude)?;
    let include = items(&matrix.include)?;

    let mut kept = Vec::with_capacity(combinations.len());
    for combination in combinations.drain(..) {
        let mut excluded = false;
        for assigns in &exclude {
            if matches_exclude(&combination, assigns)? {
                excluded = true;
                break;
            }
        }
        if !excluded {
            kept.push(combination);
        }
    }

    // `include` items extend only the combinations of the rows, not those that earlier items
    // added.
    let base = kept.len();
    for assigns in include {
        let mut extended = false;
        for combination in &mut kept[..base] {
            if keeps_row_values(combination, &assigns, matrix)? {
                for &(key, value) in &assigns {
                    set(combination, key, value);
                }
                extended = true;
            }
        }
        if !extended {
            kept.push(assigns);
        }
    }
    (kept.len() <= MAX_VALUES).then_some(kept)
}

/// Returns every combination of the row values, with the first row changing slowest, or `None`
/// if a row is not a list of values, the matrix has an `${{ insert }}` expression, or there are
/// more than [`MAX_VALUES`] combinations. A matrix without rows has no combinations.
fn rows<'m, 'a>(matrix: &'m Matrix<'a>) -> Option<Vec<Combination<'m, 'a>>> {
    if !matrix.rows.inserts().is_empty() {
        return None;
    }
    if matrix.rows.is_empty() {
        return Some(Vec::new());
    }
    let mut combinations: Vec<Combination<'m, 'a>> = vec![Vec::new()];
    for row in &matrix.rows {
        let MatrixRow::Values(values) = &row.value else {
            return None;
        };
        if combinations.len().checked_mul(values.len())? > MAX_VALUES {
            return None;
        }
        combinations = combinations
            .iter()
            .flat_map(|combination| {
                values.iter().map(|value| {
                    let mut combination = combination.clone();
                    combination.push((row.folded(), value));
                    combination
                })
            })
            .collect();
    }
    Some(combinations)
}

/// Returns the items of `include` or `exclude` as lists of folded keys and values, or `None` if
/// the section or one of its items is an expression or invalid, or an item has an
/// `${{ insert }}` expression.
fn items<'m, 'a>(
    section: &'m Value<'a, std::sync::Arc<[MatrixCombination<'a>]>>,
) -> Option<Vec<Combination<'m, 'a>>> {
    let items = match section {
        Value::Absent => return Some(Vec::new()),
        Value::Literal { value, .. } => value,
        Value::Invalid { .. } | Value::Expression { .. } => return None,
    };
    items
        .iter()
        .map(|item| match item {
            MatrixCombination::Assigns { assigns, .. } if assigns.inserts().is_empty() => Some(
                assigns
                    .iter()
                    .map(|assign| (assign.folded(), &assign.value))
                    .collect(),
            ),
            MatrixCombination::Assigns { .. }
            | MatrixCombination::Expression(_)
            | MatrixCombination::Invalid(_) => None,
        })
        .collect()
}

/// Returns whether the `exclude` item `assigns` matches `combination`: the combination has each
/// of its keys with the same value. `None` means a value could not be compared.
fn matches_exclude(
    combination: &Combination<'_, '_>,
    assigns: &Combination<'_, '_>,
) -> Option<bool> {
    for (key, value) in assigns {
        let Some(have) = get(combination, key) else {
            return Some(false);
        };
        if !same(have, value)? {
            return Some(false);
        }
    }
    Some(true)
}

/// Returns whether the `include` item `assigns` keeps the row values of `combination`: each of
/// its keys that is a row has the combination's value. `None` means a value could not be
/// compared.
fn keeps_row_values(
    combination: &Combination<'_, '_>,
    assigns: &Combination<'_, '_>,
    matrix: &Matrix<'_>,
) -> Option<bool> {
    for (key, value) in assigns {
        if !matrix.rows.contains_key(key) {
            continue;
        }
        let Some(have) = get(combination, key) else {
            return Some(false);
        };
        if !same(have, value)? {
            return Some(false);
        }
    }
    Some(true)
}

/// Returns whether two values are the same scalar text, or `None` if either is not a scalar
/// without an expression.
fn same(a: &RawValue<'_>, b: &RawValue<'_>) -> Option<bool> {
    match (a, b) {
        (RawValue::Scalar(a), RawValue::Scalar(b))
            if !contains_expression(a.value()) && !contains_expression(b.value()) =>
        {
            Some(a.value() == b.value())
        }
        _ => None,
    }
}

/// Returns the value of `key` in `combination`.
pub(super) fn get<'m, 'a>(
    combination: &Combination<'m, 'a>,
    key: &str,
) -> Option<&'m RawValue<'a>> {
    combination
        .iter()
        .find(|(have, _)| *have == key)
        .map(|(_, value)| *value)
}

/// Sets `key` to `value` in `combination`, replacing the value it has.
fn set<'m, 'a>(combination: &mut Combination<'m, 'a>, key: &'m str, value: &'m RawValue<'a>) {
    match combination.iter_mut().find(|(have, _)| *have == key) {
        Some(entry) => entry.1 = value,
        None => combination.push((key, value)),
    }
}
