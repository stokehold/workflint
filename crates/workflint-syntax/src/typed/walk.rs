//! Decoding into `interface{}` and maps, as go-yaml's `unmarshal`,
//! `sequence`, `mapping`, and `merge` do it.
//!
//! go-yaml recurses once for each level of nesting, and Go grows its stack
//! as needed. [`Decoder::walk`] keeps a stack of frames on the heap
//! instead, so that a value nested as deep as the YAML layer allows
//! (ADR 0022) decodes as it does in go-yaml and cannot overflow the stack.
//! The decoder counts steps, follows aliases, and reports errors in the
//! same order as the recursion.

use std::collections::BTreeSet;
use std::mem;
use std::slice;

use workflint_yaml::{NodeId, NodeKind};

use super::go_value::{GoValue, Key, KeyError};
use super::scalar::{Resolved, resolve};
use super::{Decoder, Fatal, add_merged_key, insert, unhashable, want_map};

/// A node to decode, and the Go type to decode it into.
#[derive(Clone, Copy)]
pub(super) enum Request {
    /// Decode `id` into an `interface{}`, as `Decoder::any` does.
    Any { id: NodeId, build: bool },
    /// Decode the entries of the mapping `id`, which passed the duplicate
    /// key check, into a map: a `map[string]interface{}` if `string_keys`,
    /// else a `map[interface{}]interface{}`. Builds the map only if
    /// `build`.
    Entries {
        id: NodeId,
        string_keys: bool,
        build: bool,
    },
    /// Decode the mapping `id`, or the alias of one, that a `<<` key merges
    /// into the map that a frame below decodes.
    Merged {
        id: NodeId,
        string_keys: bool,
        build: bool,
    },
}

/// What a request decodes.
pub(super) enum Output {
    /// The value of [`Request::Any`] or [`Request::Entries`], if it was
    /// built.
    Value(Option<GoValue>),
    /// The entries that [`Request::Merged`] sets in the map, in order.
    Entries(Vec<(GoValue, GoValue)>),
}

/// What the frame on top of the stack does next.
enum Step {
    /// Decode a node and pass the output back to the frame.
    Request(Request),
    /// Pop the frame and pass its output to the frame below.
    Done(Output),
}

/// A node whose decoding has started and not finished.
enum Frame<'s> {
    /// The aliases that a request entered on the way to its node. They are
    /// left when the frame above, or the output of the node, is done.
    Aliases(u32),
    /// A sequence decoded into a `[]interface{}`.
    Sequence {
        items: slice::Iter<'s, NodeId>,
        build: bool,
        values: Vec<GoValue>,
    },
    /// A mapping decoded into a map.
    Mapping(Mapping<'s>),
}

/// A mapping decoded into a map, go-yaml's `mapping`.
struct Mapping<'s> {
    string_keys: bool,
    build: bool,
    /// Whether the output is [`Output::Entries`], for a mapping that a `<<`
    /// key merges.
    merged_in: bool,
    /// The key and value of each entry, as written.
    entries: &'s [[NodeId; 2]],
    /// The index of the next entry to decode.
    next: usize,
    /// go-yaml's `mergedFields`, which the decoder does not hold while the
    /// entries are decoded.
    merged: Option<BTreeSet<Key>>,
    /// The value of the last `<<` key.
    merge: Option<NodeId>,
    /// The entries set in the map, in order. go-yaml sets each one when it
    /// is decoded; [`Mapping::finish`] replays them.
    set: Vec<(GoValue, GoValue)>,
    state: State,
}

/// Where a [`Mapping`] frame is.
enum State {
    /// Decoding the entries.
    Entries,
    /// Waiting for the key of the entry whose value is `value`.
    Key { value: NodeId },
    /// Waiting for the value under `key`.
    Value { key: GoValue },
    /// Collecting the keys of the mapping, which the mappings that its `<<`
    /// key merges do not override. `next` is the index of the next key.
    MergeKeys {
        merge: NodeId,
        next: usize,
        seen: BTreeSet<Key>,
    },
    /// Merging the mappings `items` that the `<<` key names. `next` is the
    /// index of the next one.
    Merging {
        items: Vec<NodeId>,
        next: usize,
        outermost: bool,
    },
}

impl Mapping<'_> {
    /// Returns the output of the frame: the entries it set, or the map.
    fn finish(&mut self) -> Output {
        let set = mem::take(&mut self.set);
        if self.merged_in {
            return Output::Entries(set);
        }
        if !self.build {
            return Output::Value(None);
        }
        let mut entries = Vec::new();
        for (key, value) in set {
            insert(&mut entries, key, value);
        }
        Output::Value(Some(GoValue::Map {
            string_keys: self.string_keys,
            entries,
        }))
    }
}

impl<'s> Decoder<'s> {
    /// Decodes the node of `request`.
    pub(super) fn walk(&mut self, request: Request) -> Result<Output, Fatal> {
        let mut frames = Vec::new();
        let mut input = self.start(request, &mut frames)?;
        loop {
            let Some(frame) = frames.last_mut() else {
                return Ok(input.unwrap_or(Output::Value(None)));
            };
            let step = match frame {
                Frame::Aliases(count) => {
                    let count = *count;
                    self.leave_aliases(count);
                    Step::Done(input.take().unwrap_or(Output::Value(None)))
                }
                Frame::Sequence {
                    items,
                    build,
                    values,
                } => {
                    if let Some(Output::Value(Some(value))) = input.take() {
                        values.push(value);
                    }
                    match items.next() {
                        Some(&id) => Step::Request(Request::Any { id, build: *build }),
                        None => {
                            Step::Done(Output::Value(build.then(|| GoValue::Slice(mem::take(values)))))
                        }
                    }
                }
                Frame::Mapping(mapping) => self.resume(mapping, input.take())?,
            };
            match step {
                Step::Request(request) => input = self.start(request, &mut frames)?,
                Step::Done(output) => {
                    frames.pop();
                    input = Some(output);
                }
            }
        }
    }

    /// Starts decoding the node of `request`. Returns its output if that
    /// needs no frame, or else pushes the frame and returns `None`.
    fn start(
        &mut self,
        request: Request,
        frames: &mut Vec<Frame<'s>>,
    ) -> Result<Option<Output>, Fatal> {
        match request {
            Request::Any { id, build } => {
                let (id, node, aliases) = self.enter_node(id)?;
                if aliases > 0 {
                    frames.push(Frame::Aliases(aliases));
                }
                match &node.kind {
                    NodeKind::Scalar(scalar) => {
                        let resolved = resolve(scalar)?;
                        Ok(Some(Output::Value(
                            build.then_some(GoValue::Scalar(resolved)),
                        )))
                    }
                    NodeKind::Sequence(_) => {
                        let source = self.source;
                        frames.push(Frame::Sequence {
                            items: source.written(id).iter(),
                            build,
                            values: Vec::new(),
                        });
                        Ok(None)
                    }
                    NodeKind::Mapping(_) => {
                        let string_keys = self.is_string_map(id);
                        if !self.unique_keys(id) {
                            // go-yaml leaves the interface nil.
                            return Ok(Some(Output::Value(
                                build.then_some(GoValue::Scalar(Resolved::Null)),
                            )));
                        }
                        frames.push(Frame::Mapping(self.mapping(id, string_keys, build, false)));
                        Ok(None)
                    }
                    NodeKind::Alias(_) => Ok(Some(Output::Value(None))),
                }
            }
            Request::Entries {
                id,
                string_keys,
                build,
            } => {
                frames.push(Frame::Mapping(self.mapping(id, string_keys, build, false)));
                Ok(None)
            }
            Request::Merged {
                id,
                string_keys,
                build,
            } => {
                let (id, _, aliases) = self.enter_node(id)?;
                if aliases > 0 {
                    frames.push(Frame::Aliases(aliases));
                }
                if !self.unique_keys(id) {
                    return Ok(Some(Output::Entries(Vec::new())));
                }
                frames.push(Frame::Mapping(self.mapping(id, string_keys, build, true)));
                Ok(None)
            }
        }
    }

    /// Returns the frame of the mapping `id`, which takes go-yaml's
    /// `mergedFields` from the decoder.
    fn mapping(
        &mut self,
        id: NodeId,
        string_keys: bool,
        build: bool,
        merged_in: bool,
    ) -> Mapping<'s> {
        let source = self.source;
        Mapping {
            string_keys,
            build,
            merged_in,
            entries: source.written(id).as_chunks::<2>().0,
            next: 0,
            merged: self.merged.take(),
            merge: None,
            set: Vec::new(),
            state: State::Entries,
        }
    }

    /// Continues the mapping `frame` with the output of its last request,
    /// if it made one.
    fn resume(
        &mut self,
        frame: &mut Mapping<'s>,
        mut input: Option<Output>,
    ) -> Result<Step, Fatal> {
        loop {
            match mem::replace(&mut frame.state, State::Entries) {
                State::Entries => {
                    if let Some(step) = self.next_entry(frame)? {
                        return Ok(step);
                    }
                }
                State::Key { value } => {
                    let key = match input.take() {
                        Some(Output::Value(Some(key))) => key,
                        _ => GoValue::Scalar(Resolved::Null),
                    };
                    if let Some(step) = Self::set_key(frame, key, value)? {
                        return Ok(step);
                    }
                }
                State::Value { key } => {
                    if let Some(Output::Value(Some(value))) = input.take()
                        && frame.build
                    {
                        frame.set.push((key, value));
                    }
                }
                State::MergeKeys {
                    merge,
                    next,
                    mut seen,
                } => {
                    if let Some(output) = input.take() {
                        let key = match output {
                            Output::Value(key) => key,
                            Output::Entries(_) => None,
                        };
                        add_merged_key(&mut seen, key)?;
                    }
                    if let Some(&[key, _]) = frame.entries.get(next) {
                        frame.state = State::MergeKeys {
                            merge,
                            next: next + 1,
                            seen,
                        };
                        return Ok(Step::Request(Request::Any {
                            id: key,
                            build: true,
                        }));
                    }
                    self.merged = Some(seen);
                    frame.state = State::Merging {
                        items: self.merge_items(merge)?,
                        next: 0,
                        outermost: true,
                    };
                }
                State::Merging {
                    items,
                    next,
                    outermost,
                } => {
                    if let Some(Output::Entries(set)) = input.take() {
                        frame.set.extend(set);
                    }
                    let Some(&item) = items.get(next) else {
                        if outermost {
                            self.merged = None;
                        }
                        return Ok(Step::Done(frame.finish()));
                    };
                    if !self.source.is_mapping(item) {
                        return Err(want_map());
                    }
                    let request = Request::Merged {
                        id: item,
                        string_keys: frame.string_keys,
                        build: frame.build,
                    };
                    frame.state = State::Merging {
                        items,
                        next: next + 1,
                        outermost,
                    };
                    return Ok(Step::Request(request));
                }
            }
        }
    }

    /// Decodes the next entry of the mapping `frame`, or starts its merge
    /// after the last entry. Returns the step to take, or `None` if the
    /// frame continues with its new state.
    fn next_entry(&mut self, frame: &mut Mapping<'s>) -> Result<Option<Step>, Fatal> {
        let Some(&[key, value]) = frame.entries.get(frame.next) else {
            self.merged = frame.merged.take();
            let Some(merge) = frame.merge else {
                return Ok(Some(Step::Done(frame.finish())));
            };
            frame.state = if self.merged.is_none() {
                State::MergeKeys {
                    merge,
                    next: 0,
                    seen: BTreeSet::new(),
                }
            } else {
                State::Merging {
                    items: self.merge_items(merge)?,
                    next: 0,
                    outermost: false,
                }
            };
            return Ok(None);
        };
        frame.next += 1;
        if self.source.is_merge_key(key) {
            frame.merge = Some(value);
            return Ok(None);
        }
        if !frame.string_keys {
            frame.state = State::Key { value };
            return Ok(Some(Step::Request(Request::Any {
                id: key,
                build: true,
            })));
        }
        let mut text = String::new();
        if !self.string(key, &mut text)? {
            return Ok(None);
        }
        Self::set_key(frame, GoValue::Scalar(Resolved::Str(text)), value)
    }

    /// Continues the entry whose key decoded to `key` and whose value is
    /// `value`. Returns the request for the value, or `None` if a merge
    /// skips the entry.
    fn set_key(frame: &mut Mapping<'s>, key: GoValue, value: NodeId) -> Result<Option<Step>, Fatal> {
        if let Some(seen) = &mut frame.merged {
            match key.key() {
                Ok(key) => {
                    if !seen.insert(key) {
                        return Ok(None);
                    }
                }
                Err(KeyError::NaN) => {}
                Err(KeyError::Unhashable(go_type)) => return Err(unhashable(go_type)),
            }
        }
        if key.is_collection() {
            return Err(Fatal::new(format!(
                "yaml: cannot use '{}' as a map key; try decoding into yaml.Node",
                key.go_syntax()
            )));
        }
        let build = frame.build;
        frame.state = State::Value { key };
        Ok(Some(Step::Request(Request::Any { id: value, build })))
    }
}
