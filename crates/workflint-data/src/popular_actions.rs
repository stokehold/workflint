//! The inputs and outputs of popular actions, and the specs of popular actions whose runner
//! GitHub no longer has.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `PopularActions` in `popular_actions.go`,
//! lines 7-5064, and `OutdatedPopularActionSpecs`, lines 5068-5225, which upstream's
//! `scripts/generate-popular-actions` writes from each action's `action.yml`. The table in
//! [`table`] was converted from that file once. Until M7 replaces it with a generated file
//! (ADR 0011), keep it equal to that file. The tests `popular_actions_match_the_reference_table`
//! and `outdated_specs_match_the_reference_table` compare them.

mod table;

pub use table::{OUTDATED_POPULAR_ACTION_SPECS, POPULAR_ACTIONS};

/// A popular action at one ref, with what its `action.yml` declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopularAction {
    /// The `owner/repo[/path]@ref` that a step's `uses:` names, such as `actions/checkout@v4`.
    pub spec: &'static str,
    /// The `name` of the action's metadata.
    pub name: &'static str,
    /// The inputs, sorted by [`ActionInput::id`].
    pub inputs: &'static [ActionInput],
    /// The outputs, sorted by [`ActionOutput::id`].
    pub outputs: &'static [ActionOutput],
    /// Whether the action takes inputs that its metadata does not declare, so that `with:` is
    /// not checked.
    pub skip_inputs: bool,
    /// Whether the action sets outputs that its metadata does not declare, so that its
    /// `steps.<id>.outputs` object accepts any property besides the declared ones.
    pub skip_outputs: bool,
}

impl PopularAction {
    /// Returns the input whose lowercased name is `id`. Input names are case-insensitive, so
    /// the caller lowercases the key of `with:` first.
    #[must_use]
    pub fn input(&self, id: &str) -> Option<&'static ActionInput> {
        let inputs = self.inputs;
        inputs
            .binary_search_by(|input| input.id.cmp(id))
            .ok()
            .and_then(|i| inputs.get(i))
    }

    /// Returns the output whose lowercased name is `id`.
    #[must_use]
    pub fn output(&self, id: &str) -> Option<&'static ActionOutput> {
        let outputs = self.outputs;
        outputs
            .binary_search_by(|output| output.id.cmp(id))
            .ok()
            .and_then(|i| outputs.get(i))
    }
}

/// An input that a popular action declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionInput {
    /// The name lowercased, as the reference keys it.
    pub id: &'static str,
    /// The name as the metadata writes it, such as `configPath`.
    pub name: &'static str,
    /// Whether a step must set the input: the metadata has `required: true` and no `default`.
    pub required: bool,
    /// Whether the metadata has a `deprecationMessage`, even an empty one.
    pub deprecated: bool,
    /// The `deprecationMessage` with surrounding whitespace removed, or `""`.
    pub deprecation_message: &'static str,
}

/// An output that a popular action declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionOutput {
    /// The name lowercased, as the reference keys it.
    pub id: &'static str,
    /// The name as the metadata writes it, such as `resultsPath`.
    pub name: &'static str,
}

/// Returns the popular action that `spec` names. The spec must match exactly, case included,
/// as in the reference: `actions/checkout@v4` is popular, `Actions/checkout@v4` and
/// `actions/checkout@v4.1.0` are not.
#[must_use]
pub fn popular_action(spec: &str) -> Option<&'static PopularAction> {
    POPULAR_ACTIONS
        .binary_search_by(|action| action.spec.cmp(spec))
        .ok()
        .and_then(|i| POPULAR_ACTIONS.get(i))
}

/// Whether `spec` names a popular action whose runner, such as `node12` or `node16`, GitHub no
/// longer has. The spec must match exactly, case included.
#[must_use]
pub fn is_outdated_popular_action(spec: &str) -> bool {
    OUTDATED_POPULAR_ACTION_SPECS.binary_search(&spec).is_ok()
}
