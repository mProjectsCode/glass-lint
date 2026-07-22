//! Function summaries projected from the canonical fact stream.
//!
//! A summary is keyed only by `FunctionId`. Parameter paths and argument
//! projections keep destructuring precise, while the fixed point joins helper
//! calls (including recursive and mutually recursive helpers) without walking
//! AST bodies again.
//!
//! Summaries are monotone and conservative: unsupported reassignment,
//! dynamic arguments, missing paths, or incompatible invocations do not create
//! a projected sink. Recursive propagation stops at a fixed point or its
//! explicit round bound.

use std::collections::HashMap;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, ParameterBinding},
    flow::{
        effect::{EffectCall, FunctionEffects},
        index::{FlowId, FlowIndex},
        table::FunctionTable,
    },
    value::{FunctionId, PathId, PathInterner, PathSegment, Value, ValueId, ValueTable},
};

const MAX_SUMMARY_ROUNDS: usize = 64;
const MAX_SUMMARY_PATH_NODES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct SummaryPathId(u32);

impl SummaryPathId {
    pub(super) const EMPTY: Self = Self(0);

    pub(super) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    fn index(self) -> Option<usize> {
        usize::try_from(self.0)
            .ok()
            .filter(|index| *index < MAX_SUMMARY_PATH_NODES)
    }
}

#[derive(Debug, Clone)]
struct SummaryPathNode {
    parent: SummaryPathId,
    depth: u32,
    segment: Option<PathSegment>,
}

#[derive(Debug, Clone)]
pub(super) struct SummaryPathInterner {
    nodes: Vec<SummaryPathNode>,
    by_edge: HashMap<(SummaryPathId, PathSegment), SummaryPathId>,
    frozen_map: HashMap<PathId, SummaryPathId>,
}

impl SummaryPathInterner {
    fn new() -> Self {
        Self {
            nodes: vec![SummaryPathNode {
                parent: SummaryPathId::EMPTY,
                depth: 0,
                segment: None,
            }],
            by_edge: HashMap::new(),
            frozen_map: HashMap::new(),
        }
    }

    fn append(&mut self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        let parent_index = parent.index()?;
        if parent_index >= self.nodes.len() {
            return None;
        }
        if let Some(path) = self.by_edge.get(&(parent, segment.clone())) {
            return Some(*path);
        }
        if self.nodes.len() >= MAX_SUMMARY_PATH_NODES {
            return None;
        }
        let id = SummaryPathId(u32::try_from(self.nodes.len()).ok()?);
        let depth = self.nodes[parent_index].depth.checked_add(1)?;
        self.nodes.push(SummaryPathNode {
            parent,
            depth,
            segment: Some(segment.clone()),
        });
        self.by_edge.insert((parent, segment), id);
        Some(id)
    }

    pub(super) fn intern_frozen(
        &mut self,
        frozen: &PathInterner,
        path: PathId,
    ) -> Option<SummaryPathId> {
        if let Some(&existing) = self.frozen_map.get(&path) {
            return Some(existing);
        }
        let segments = frozen.owned_segments(path)?;
        let mut result = SummaryPathId::EMPTY;
        for segment in segments {
            result = self.append(result, segment)?;
        }
        self.frozen_map.insert(path, result);
        Some(result)
    }

    pub(super) fn join(
        &mut self,
        prefix: SummaryPathId,
        suffix: SummaryPathId,
    ) -> Option<SummaryPathId> {
        if suffix.is_empty() {
            return Some(prefix);
        }
        let suffix_node = self.nodes.get(suffix.index()?)?;
        let segment = suffix_node.segment.clone()?;
        let parent_joined = self.join(prefix, suffix_node.parent)?;
        if let Some(existing) = self.find_edge(parent_joined, &segment) {
            return Some(existing);
        }
        self.append(parent_joined, segment)
    }

    pub(super) fn resolve_frozen(&self, path: PathId) -> Option<SummaryPathId> {
        self.frozen_map.get(&path).copied()
    }

    fn depth(&self, id: SummaryPathId) -> Option<u32> {
        self.nodes.get(id.index()?).map(|node| node.depth)
    }

    pub(super) fn starts_with(&self, id: SummaryPathId, prefix: SummaryPathId) -> bool {
        let Some(path_depth) = self.depth(id) else {
            return false;
        };
        let Some(prefix_depth) = self.depth(prefix) else {
            return false;
        };
        if prefix_depth > path_depth {
            return false;
        }
        let mut current = id;
        for _ in 0..(path_depth - prefix_depth) {
            let Some(index) = current.index() else {
                return false;
            };
            let Some(node) = self.nodes.get(index) else {
                return false;
            };
            current = node.parent;
        }
        current == prefix
    }

    pub(super) fn matches_frozen(&self, id: SummaryPathId, base: PathId) -> bool {
        self.frozen_map.get(&base).copied() == Some(id)
    }

    pub(super) fn starts_with_frozen(&self, id: SummaryPathId, prefix: PathId) -> bool {
        if let Some(&prefix_id) = self.frozen_map.get(&prefix) {
            return self.starts_with(id, prefix_id);
        }
        false
    }

    fn segment(&self, id: SummaryPathId) -> Option<&PathSegment> {
        if id.is_empty() {
            return None;
        }
        self.nodes.get(id.index()?)?.segment.as_ref()
    }

    fn first_segment_of(&self, id: SummaryPathId) -> Option<&PathSegment> {
        let mut current = id;
        let mut last = None;
        while !current.is_empty() {
            last = Some(self.segment(current)?);
            current = self.nodes.get(current.index()?)?.parent;
        }
        last
    }

    pub(super) fn first_index(&self, id: SummaryPathId) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    pub(super) fn without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    fn find_edge(&self, parent: SummaryPathId, segment: &PathSegment) -> Option<SummaryPathId> {
        self.by_edge.get(&(parent, segment.clone())).copied()
    }

    fn rebuild_without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        let node = self.nodes.get(id.index()?)?;
        let segment = self.segment(id)?;
        if node.parent.is_empty() {
            return Some(SummaryPathId::EMPTY);
        }
        let parent = self.rebuild_without_first(node.parent)?;
        self.find_edge(parent, segment)
    }

    pub(super) fn owned_segments(&self, id: SummaryPathId) -> Option<Vec<PathSegment>> {
        let depth = self.nodes.get(id.index()?)?.depth as usize;
        let mut segments = Vec::with_capacity(depth);
        let mut current = id;
        while !current.is_empty() {
            let node = self.nodes.get(current.index()?)?;
            segments.push(node.segment.clone()?);
            current = node.parent;
        }
        segments.reverse();
        Some(segments)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FunctionSinkSummary {
    flow: FlowId,
    parameter_index: usize,
    path: SummaryPathId,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSummary {
    id: FunctionId,
    parameters: Vec<ParameterBinding>,
    parameter_count: usize,
    has_rest: bool,
    calls: Vec<FactId>,
    sinks: SinkSet,
    sinks_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SinkSet(Vec<FunctionSinkSummary>);
impl SinkSet {
    fn contains(&self, sink: &FunctionSinkSummary) -> bool {
        self.0.iter().any(|existing| existing == sink)
    }

    fn push(&mut self, sink: FunctionSinkSummary) {
        self.0.push(sink);
    }

    fn sort_and_dedup(&mut self) {
        self.0.sort_by(|left, right| {
            (left.flow(), left.parameter_index(), left.path()).cmp(&(
                right.flow(),
                right.parameter_index(),
                right.path(),
            ))
        });
        self.0.dedup();
    }
}
impl<'a> IntoIterator for &'a SinkSet {
    type IntoIter = std::slice::Iter<'a, FunctionSinkSummary>;
    type Item = &'a FunctionSinkSummary;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl IntoIterator for SinkSet {
    type IntoIter = std::vec::IntoIter<FunctionSinkSummary>;
    type Item = FunctionSinkSummary;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FunctionSinkSummary {
    pub(super) fn flow(&self) -> FlowId {
        self.flow
    }

    pub(super) fn parameter_index(&self) -> usize {
        self.parameter_index
    }

    pub(super) fn path(&self) -> SummaryPathId {
        self.path
    }
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSummaries {
    by_id: FunctionTable<FunctionSummary>,
    paths: SummaryPathInterner,
}

impl Default for FunctionSummaries {
    fn default() -> Self {
        Self {
            by_id: FunctionTable::default(),
            paths: SummaryPathInterner::new(),
        }
    }
}

impl FunctionSummaries {
    pub(super) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(id)
    }

    pub(super) fn path_interner(&self) -> &SummaryPathInterner {
        &self.paths
    }

    fn insert(&mut self, summary: FunctionSummary) {
        self.by_id.insert(summary.id, summary);
    }

    pub(super) fn collect(
        stream: &FactStream,
        effects: &FunctionEffects,
        flow_index: &FlowIndex<'_>,
    ) -> Self {
        let mut summaries = Self::default();
        summaries.collect_facts(effects, stream.paths());
        summaries.collect_direct_sinks(stream, flow_index);
        summaries.propagate_sinks(stream);
        for (_, summary) in summaries.by_id.iter_mut() {
            summary.sinks.sort_and_dedup();
        }
        summaries
    }

    fn collect_facts(&mut self, effects: &FunctionEffects, frozen: &PathInterner) {
        for effect in effects.iter_effects() {
            if self.get(effect.id()).is_none() {
                for param in effect.parameters() {
                    self.paths.intern_frozen(frozen, param.path);
                }
                self.insert(FunctionSummary {
                    id: effect.id(),
                    parameters: effect.parameters().to_vec(),
                    parameter_count: effect
                        .parameters()
                        .iter()
                        .map(|parameter| parameter.parameter_index)
                        .max()
                        .map_or(0, |index| index.saturating_add(1)),
                    has_rest: effect.parameters().iter().any(|parameter| parameter.rest),
                    calls: effect.calls().iter().map(EffectCall::event).collect(),
                    sinks: SinkSet::default(),
                    sinks_offset: 0,
                });
            }
        }
    }

    fn collect_direct_sinks(&mut self, stream: &FactStream, flow_index: &FlowIndex<'_>) {
        let ids: Vec<FunctionId> = self.by_id.iter().map(|(id, _)| id).collect();
        for id in ids {
            let calls: Vec<FactId> = self
                .by_id
                .get(id)
                .map(|s| s.calls.clone())
                .unwrap_or_default();
            let Some(summary) = self.by_id.get_mut(id) else {
                continue;
            };
            for call_id in &calls {
                summary.collect_sinks_for_call(stream, flow_index, &mut self.paths, *call_id);
            }
        }
    }

    fn propagate_sinks(&mut self, stream: &FactStream) {
        for _ in 0..MAX_SUMMARY_ROUNDS {
            let mut changed = false;
            let prev_offsets: Vec<(FunctionId, usize)> = self
                .by_id
                .iter()
                .map(|(id, s)| (id, s.sinks_offset))
                .collect();
            for (_, summary) in self.by_id.iter_mut() {
                summary.sinks_offset = summary.sinks.0.len();
            }
            let function_ids: Vec<FunctionId> = self.by_id.iter().map(|(id, _)| id).collect();
            for caller in function_ids {
                let calls: Vec<FactId> = self
                    .by_id
                    .get(caller)
                    .map(|s| s.calls.clone())
                    .unwrap_or_default();
                for call_id in &calls {
                    changed |= self.propagate_call_sinks(*call_id, caller, stream, &prev_offsets);
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn propagate_call_sinks(
        &mut self,
        call_id: FactId,
        caller: FunctionId,
        stream: &FactStream,
        prev_offsets: &[(FunctionId, usize)],
    ) -> bool {
        let Some((target, args)) = resolve_call_target(call_id, stream) else {
            return false;
        };
        let target_sinks_offset = prev_offsets
            .iter()
            .find(|(id, _)| *id == target)
            .map_or(0, |(_, offset)| *offset);
        let projections: Vec<FunctionSinkSummary> = {
            let target_summary = match self.by_id.get(target) {
                Some(s) if s.is_invocation_compatible(stream, args, &self.paths) => s,
                _ => return false,
            };
            let Some(caller_summary) = self.by_id.get(caller) else {
                return false;
            };
            let target_params = &target_summary.parameters;
            let caller_params = &caller_summary.parameters;
            let sink_count = target_summary.sinks.0.len();
            let mut projections = Vec::new();
            for sink_idx in target_sinks_offset..sink_count {
                let sink = &target_summary.sinks.0[sink_idx];
                if let Some(proj) = try_project_sink(
                    target_params,
                    caller_params,
                    sink,
                    stream,
                    args,
                    &self.paths,
                ) {
                    projections.push(proj);
                }
            }
            projections
        };
        if projections.is_empty() {
            return false;
        }
        let Some(caller_summary) = self.by_id.get_mut(caller) else {
            return false;
        };
        let mut changed = false;
        for proj in projections {
            changed |= caller_summary.add_sink(proj);
        }
        changed
    }
}

fn resolve_call_target(
    call_id: FactId,
    stream: &FactStream,
) -> Option<(FunctionId, &[CallArgInfo])> {
    let FactPayload::Call {
        target_function,
        args,
        ..
    } = &stream.fact(call_id)?.payload
    else {
        return None;
    };
    Some(((*target_function)?, args))
}

fn try_project_sink(
    target_parameters: &[ParameterBinding],
    caller_parameters: &[ParameterBinding],
    sink: &FunctionSinkSummary,
    stream: &FactStream,
    args: &[CallArgInfo],
    paths: &SummaryPathInterner,
) -> Option<FunctionSinkSummary> {
    let target_parameter = target_parameters.iter().find(|parameter| {
        parameter.parameter_index == sink.parameter_index()
            && (paths.matches_frozen(sink.path(), parameter.path)
                || (parameter.rest && paths.starts_with_frozen(sink.path(), parameter.path)))
    })?;
    let argument = target_parameter.project_argument_at(stream, args, paths, sink.path())?;
    let caller_parameter = caller_parameters.iter().find(|parameter| {
        !parameter.rest && parameter.value != ValueId::UNKNOWN && parameter.value == argument
    })?;
    let caller_path = paths.resolve_frozen(caller_parameter.path)?;
    Some(FunctionSinkSummary {
        flow: sink.flow(),
        parameter_index: caller_parameter.parameter_index,
        path: caller_path,
    })
}

impl FunctionSummary {
    pub(super) fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }

    pub(super) fn sinks(&self) -> &SinkSet {
        &self.sinks
    }

    fn add_sink(&mut self, sink: FunctionSinkSummary) -> bool {
        if self.sinks.contains(&sink) {
            return false;
        }
        self.sinks.push(sink);
        true
    }
}

impl FunctionSummary {
    pub(super) fn is_invocation_compatible(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
        paths: &SummaryPathInterner,
    ) -> bool {
        if args.iter().any(|argument| argument.spread) {
            return false;
        }
        if !self.has_rest && args.len() > self.parameter_count {
            return false;
        }
        for argument in args.iter().take(self.parameter_count) {
            if argument.value == ValueId::UNKNOWN {
                return false;
            }
        }
        for parameter in &self.parameters {
            if parameter.rest || parameter.parameter_index >= args.len() {
                if parameter.parameter_index >= args.len()
                    && parameter.default.is_none()
                    && !parameter.rest
                {
                    return false;
                }
                continue;
            }
            if parameter.path.is_empty() {
                continue;
            }
            if parameter.project_argument(stream, args, paths).is_none()
                && parameter.default.is_none()
            {
                return false;
            }
        }
        true
    }
}

impl FunctionSummary {
    fn collect_sinks_for_call(
        &mut self,
        stream: &FactStream,
        flow_index: &FlowIndex<'_>,
        paths: &mut SummaryPathInterner,
        call_id: FactId,
    ) {
        let Some(FactPayload::Call {
            syntactic_path,
            rooted_chain,
            args,
            ..
        }) = stream.fact(call_id).map(|fact| &fact.payload)
        else {
            return;
        };
        let Some(chain) = rooted_chain.as_ref().or(syntactic_path.as_ref()) else {
            return;
        };
        let frozen = stream.paths();
        for flow_id in flow_index.sink_ids(chain).into_iter().flatten() {
            let Some(flow) = flow_index.get(*flow_id) else {
                continue;
            };
            for sink in &flow.sinks {
                if !sink.member_calls.iter().any(|member| {
                    stream.names().is_some_and(|names| {
                        crate::analysis::value::NamePath::from_symbol_path(member, names)
                            .is_some_and(|member| member == *chain)
                    })
                }) {
                    continue;
                }
                for argument_index in sink.args.present_indices(args.len()) {
                    let Some(argument) = args.get(argument_index) else {
                        continue;
                    };
                    let Some(parameter) = self.parameters.iter().find(|parameter| {
                        parameter.value != ValueId::UNKNOWN
                            && parameter.value == argument.base_value
                    }) else {
                        continue;
                    };
                    let Some(prefix_id) = paths.intern_frozen(frozen, parameter.path) else {
                        continue;
                    };
                    let Some(suffix_id) = paths.intern_frozen(frozen, argument.base_path) else {
                        continue;
                    };
                    let Some(path) = paths.join(prefix_id, suffix_id) else {
                        continue;
                    };
                    self.add_sink(FunctionSinkSummary {
                        flow: *flow_id,
                        parameter_index: parameter.parameter_index,
                        path,
                    });
                }
            }
        }
    }
}

impl ParameterBinding {
    pub(super) fn project_argument(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
        paths: &SummaryPathInterner,
    ) -> Option<ValueId> {
        let param_path = paths.resolve_frozen(self.path)?;
        self.project_argument_at(stream, args, paths, param_path)
    }

    pub(super) fn project_argument_at(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
        paths: &SummaryPathInterner,
        path: SummaryPathId,
    ) -> Option<ValueId> {
        let Some(argument) = args.get(self.parameter_index) else {
            return self
                .path
                .is_empty()
                .then_some(self.default)
                .flatten()
                .filter(|value| *value != ValueId::UNKNOWN);
        };
        if argument.spread {
            return None;
        }

        if self.rest {
            let index = paths.first_index(path)?;
            let argument = args.get(self.parameter_index.saturating_add(index as usize))?;
            if argument.spread {
                return None;
            }
            let path = paths.without_first(path)?;
            if path.is_empty() {
                return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
            }
            return stream.values().and_then(|values| {
                let segments = paths.owned_segments(path).unwrap_or_default();
                value_at_path(values, argument.value, &segments)
            });
        }

        if path.is_empty() {
            return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
        }

        stream.values().map_or_else(
            || self.default.filter(|value| *value != ValueId::UNKNOWN),
            |values| {
                let segments = paths.owned_segments(path).unwrap_or_default();
                let id = value_at_path(values, argument.value, &segments)
                    .filter(|v| *v != ValueId::UNKNOWN);
                id.or_else(|| self.default.filter(|value| *value != ValueId::UNKNOWN))
            },
        )
    }
}

fn value_at_path(
    values: &ValueTable,
    value_id: ValueId,
    segments: &[PathSegment],
) -> Option<ValueId> {
    let mut current = value_id;
    for segment in segments {
        let value = values.resolve(current)?;
        current = match value {
            Value::StaticObject(entries) => match segment {
                PathSegment::Property(name_id) => entries
                    .iter()
                    .find(|(k, _)| k == name_id)
                    .map(|(_, v)| *v)?,
                PathSegment::Index(_) => return None,
            },
            Value::StaticArray(elements) => match segment {
                PathSegment::Index(index) => elements.get(*index as usize).copied()?,
                PathSegment::Property(_) => return None,
            },
            _ => return None,
        };
    }
    (current != ValueId::UNKNOWN).then_some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{facts, resolution::Resolver};

    #[test]
    fn same_name_siblings_are_keyed_by_function_id() {
        let parsed = crate::parse(
            "function first(x) { document.body.appendChild(x); } function second(x) { console.log(x); }",
            "summary-siblings.js",
        )
        .expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream = facts::build::build_test_stream(&parsed.program, &resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let summaries = FunctionSummaries::collect(
            &stream,
            &effects,
            &FlowIndex::new(&[], stream.names().expect("test stream names")),
        );
        assert!(summaries.by_id.len() >= 2);
        assert_eq!(
            summaries
                .by_id
                .iter()
                .map(|(_, summary)| summary)
                .filter(|summary| summary.parameters.len() == 1)
                .count(),
            2
        );
    }
}
