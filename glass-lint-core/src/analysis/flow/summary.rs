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
//!
//! Path storage uses one shared [`ParentPathStore`] for the summary overlay.
//! A [`SummaryPathId`] is either a frozen [`PathId`] reference (no copying)
//! or an overlay node created during a join.  The overlay is bounded by
//! [`MAX_OVERLAY_NODES`]; exhaustion fails closed.

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{ParentPathStore, PathId, PathInterner, PathSegment};
use indexmap::IndexSet;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen, ParameterBinding},
    flow::{
        effect::{EffectCall, FunctionEffects},
        index::FlowId,
        plan::BoundFlowPlan,
        table::FunctionTable,
    },
    value::{FunctionId, Value, ValueId, ValueTable},
};

const MAX_SUMMARY_ROUNDS: usize = 64;
const MAX_OVERLAY_NODES: usize = 4096;
const OVERLAY_TAG: u32 = 1 << 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct SummaryPathId(u32);

impl SummaryPathId {
    pub(super) const EMPTY: Self = Self(0);

    pub(super) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    fn is_frozen(self) -> bool {
        self.0 & OVERLAY_TAG == 0
    }

    fn from_path_id(id: PathId) -> Self {
        Self(id.as_u32())
    }
}

#[derive(Debug)]
pub(super) struct SummaryPathStore<'a> {
    frozen: &'a PathInterner,
    overlay: ParentPathStore,
}

impl<'a> SummaryPathStore<'a> {
    fn new(frozen: &'a PathInterner) -> Self {
        Self {
            frozen,
            overlay: ParentPathStore::new(MAX_OVERLAY_NODES),
        }
    }

    fn is_valid(&self, id: SummaryPathId) -> bool {
        if id.is_frozen() {
            self.frozen.store().is_valid(id.0)
        } else {
            self.overlay.is_valid(id.0 & !OVERLAY_TAG)
        }
    }

    pub(super) fn intern_frozen(&self, path: PathId) -> Option<SummaryPathId> {
        if !self.frozen.store().is_valid(path.as_u32()) {
            return None;
        }
        Some(SummaryPathId::from_path_id(path))
    }

    pub(super) fn resolve_frozen(&self, path: PathId) -> Option<SummaryPathId> {
        if !self.frozen.store().is_valid(path.as_u32()) {
            return None;
        }
        Some(SummaryPathId::from_path_id(path))
    }

    fn depth_impl(&self, id: u32, is_frozen: bool) -> Option<u32> {
        if is_frozen {
            self.frozen.store().depth(id)
        } else {
            self.overlay.depth(id & !OVERLAY_TAG)
        }
    }

    pub(super) fn depth(&self, id: SummaryPathId) -> Option<u32> {
        self.depth_impl(id.0, id.is_frozen())
    }

    fn parent_impl(&self, id: u32, is_frozen: bool) -> Option<u32> {
        if is_frozen {
            self.frozen.store().parent(id)
        } else {
            self.overlay.parent(id & !OVERLAY_TAG)
        }
    }

    fn parent(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        let raw = self.parent_impl(id.0, id.is_frozen())?;
        Some(SummaryPathId(raw))
    }

    pub(super) fn starts_with(&self, id: SummaryPathId, prefix: SummaryPathId) -> bool {
        let Some(path_depth) = self.depth_impl(id.0, id.is_frozen()) else {
            return false;
        };
        let Some(prefix_depth) = self.depth_impl(prefix.0, prefix.is_frozen()) else {
            return false;
        };
        if prefix_depth > path_depth {
            return false;
        }
        let mut current = id;
        for _ in 0..(path_depth - prefix_depth) {
            match self.parent(current) {
                Some(next) => current = next,
                None => return false,
            }
        }
        current == prefix
    }

    pub(super) fn matches_frozen(id: SummaryPathId, base: PathId) -> bool {
        id == SummaryPathId::from_path_id(base)
    }

    pub(super) fn starts_with_frozen(&self, id: SummaryPathId, prefix: PathId) -> bool {
        let prefix_id = SummaryPathId::from_path_id(prefix);
        if !self.is_valid(prefix_id) {
            return false;
        }
        self.starts_with(id, prefix_id)
    }

    fn segment_impl(&self, raw_id: u32) -> Option<&PathSegment> {
        if raw_id == 0 || raw_id & OVERLAY_TAG == 0 {
            self.frozen.store().segment(raw_id)
        } else {
            self.overlay.segment(raw_id & !OVERLAY_TAG)
        }
    }

    fn segment(&self, id: SummaryPathId) -> Option<&PathSegment> {
        self.segment_impl(id.0)
    }

    fn first_segment_of_impl(&self, raw_id: u32) -> Option<&PathSegment> {
        if raw_id == 0 || raw_id & OVERLAY_TAG == 0 {
            self.frozen.store().first_segment_of(raw_id)
        } else {
            self.overlay.first_segment_of(raw_id & !OVERLAY_TAG)
        }
    }

    fn first_segment_of(&self, id: SummaryPathId) -> Option<&PathSegment> {
        self.first_segment_of_impl(id.0)
    }

    pub(super) fn first_index(&self, id: SummaryPathId) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    fn find_edge_impl(&self, parent: u32, segment: PathSegment) -> Option<u32> {
        if let Some(child) = self.overlay.find_linked_edge(parent, &segment) {
            return Some(child);
        }
        if parent & OVERLAY_TAG == 0
            && let Some(child) = self.frozen.store().find_edge(parent, &segment)
        {
            return Some(child);
        }
        None
    }

    fn find_edge(&self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        self.find_edge_impl(parent.0, segment).map(SummaryPathId)
    }

    fn overlay_append(
        &mut self,
        parent: SummaryPathId,
        segment: PathSegment,
    ) -> Option<SummaryPathId> {
        if self.overlay.node_count() >= self.overlay.max_nodes() {
            return None;
        }
        let depth = self.depth(parent)?.checked_add(1)?;
        self.overlay
            .append_linked(parent.0, segment, depth)
            .map(SummaryPathId)
    }

    fn append(&mut self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        if let Some(child) = self.find_edge(parent, segment) {
            return Some(child);
        }
        self.overlay_append(parent, segment)
    }

    pub(super) fn join(
        &mut self,
        prefix: SummaryPathId,
        suffix: SummaryPathId,
    ) -> Option<SummaryPathId> {
        if suffix.is_empty() {
            return Some(prefix);
        }
        let mut segments = Vec::new();
        let mut current = suffix;
        while !current.is_empty() {
            segments.push(*self.segment_impl(current.0)?);
            current = SummaryPathId(self.parent_impl(current.0, current.is_frozen())?);
        }
        let mut result = prefix;
        for seg in segments.into_iter().rev() {
            result = self.append(result, seg)?;
        }
        Some(result)
    }

    pub(super) fn without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    fn rebuild_without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        let mut segments = Vec::new();
        let mut current = id;
        loop {
            let node_parent = self.parent_impl(current.0, current.is_frozen())?;
            if node_parent == 0 {
                break;
            }
            segments.push(*self.segment_impl(current.0)?);
            current = SummaryPathId(node_parent);
        }
        let mut result = SummaryPathId::EMPTY;
        for seg in segments.into_iter().rev() {
            result = self.find_edge(result, seg)?;
        }
        Some(result)
    }

    #[cfg(test)]
    pub(super) fn owned_segments(&self, id: SummaryPathId) -> Option<Vec<PathSegment>> {
        let depth = self.depth(id)?;
        let mut segments = Vec::with_capacity(depth as usize);
        let mut current = id;
        while !current.is_empty() {
            segments.push(*self.segment_impl(current.0)?);
            let next_parent = self.parent_impl(current.0, current.is_frozen())?;
            current = SummaryPathId(next_parent);
        }
        segments.reverse();
        Some(segments)
    }

    fn visit_segments(
        &self,
        id: SummaryPathId,
        visit: &mut impl FnMut(&PathSegment),
    ) -> Option<()> {
        if id.is_empty() {
            return Some(());
        }
        let mut segments = Vec::new();
        let mut current = id;
        while !current.is_empty() {
            segments.push(*self.segment(current)?);
            current = self.parent(current)?;
        }
        for seg in segments.into_iter().rev() {
            visit(&seg);
        }
        Some(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct FunctionSinkSummary {
    flow: FlowId,
    parameter_index: usize,
    path: SummaryPathId,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSummary {
    id: FunctionId,
    parameter_count: usize,
    has_rest: bool,
    calls: Vec<FactId>,
    sinks: SinkSet,
    sinks_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SinkSet {
    set: IndexSet<FunctionSinkSummary>,
}

impl SinkSet {
    fn push_unique(&mut self, sink: FunctionSinkSummary) -> bool {
        self.set.insert(sink)
    }

    fn sort_and_dedup(&mut self) {
        self.set.sort_by(|left, right| {
            (left.flow(), left.parameter_index(), left.path()).cmp(&(
                right.flow(),
                right.parameter_index(),
                right.path(),
            ))
        });
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.set.len()
    }

    #[allow(dead_code)]
    fn iter(&self) -> indexmap::set::Iter<'_, FunctionSinkSummary> {
        self.set.iter()
    }
}

impl<'a> IntoIterator for &'a SinkSet {
    type IntoIter = indexmap::set::Iter<'a, FunctionSinkSummary>;
    type Item = &'a FunctionSinkSummary;

    fn into_iter(self) -> Self::IntoIter {
        self.set.iter()
    }
}

impl IntoIterator for SinkSet {
    type IntoIter = indexmap::set::IntoIter<FunctionSinkSummary>;
    type Item = FunctionSinkSummary;

    fn into_iter(self) -> Self::IntoIter {
        self.set.into_iter()
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

#[derive(Debug)]
pub(super) struct FunctionSummaries<'a> {
    stream: &'a FactStream<Frozen>,
    by_id: FunctionTable<FunctionSummary>,
    paths: SummaryPathStore<'a>,
    /// Scratch buffer reused across propagate_call_sinks calls to avoid
    /// repeated per-call allocation.
    scratch_projections: Vec<FunctionSinkSummary>,
}

impl<'a> FunctionSummaries<'a> {
    pub(super) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(id)
    }

    pub(super) fn path_interner(&self) -> &SummaryPathStore<'a> {
        &self.paths
    }

    fn insert(&mut self, summary: FunctionSummary) {
        self.by_id.insert(summary.id, summary);
    }

    pub(super) fn collect(
        stream: &'a FactStream<Frozen>,
        effects: &FunctionEffects,
        plan: &BoundFlowPlan<'_>,
    ) -> Self {
        let mut summaries = Self {
            stream,
            by_id: FunctionTable::default(),
            paths: SummaryPathStore::new(stream.paths()),
            scratch_projections: Vec::new(),
        };
        summaries.collect_facts(effects);
        summaries.collect_direct_sinks(stream, plan);
        summaries.propagate_sinks(stream);
        for (_, summary) in summaries.by_id.iter_mut() {
            summary.sinks.sort_and_dedup();
        }
        summaries
    }

    fn collect_facts(&mut self, effects: &FunctionEffects) {
        for effect in effects.iter_effects() {
            if self.get(effect.id()).is_none() {
                let params = effect.parameters(self.stream);
                for param in params {
                    self.paths.intern_frozen(param.path);
                }
                self.insert(FunctionSummary {
                    id: effect.id(),
                    parameter_count: params
                        .iter()
                        .map(|parameter| parameter.parameter_index)
                        .max()
                        .map_or(0, |index| index.saturating_add(1)),
                    has_rest: params.iter().any(|parameter| parameter.rest),
                    calls: effect.calls().iter().map(EffectCall::event).collect(),
                    sinks: SinkSet::default(),
                    sinks_offset: 0,
                });
            }
        }
    }

    fn collect_direct_sinks(&mut self, stream: &FactStream<Frozen>, plan: &BoundFlowPlan<'_>) {
        let entries: Vec<(FunctionId, usize)> = self
            .by_id
            .iter()
            .map(|(id, summary)| (id, summary.calls.len()))
            .collect();
        for (id, count) in entries {
            let Some(summary) = self.by_id.get_mut(id) else {
                continue;
            };
            for idx in 0..count {
                if let Some(call_id) = summary.calls.get(idx).copied() {
                    summary.collect_sinks_for_call(stream, plan, &mut self.paths, call_id);
                }
            }
        }
    }

    fn propagate_sinks(&mut self, stream: &FactStream<Frozen>) {
        // Build reverse call graph: callee -> its callers
        let mut reverse_calls: BTreeMap<FunctionId, Vec<FunctionId>> = BTreeMap::new();
        for (caller_id, summary) in self.by_id.iter() {
            for call_id in &summary.calls {
                if let Some((target, _)) = resolve_call_target(*call_id, stream)
                    && target != caller_id
                {
                    reverse_calls.entry(target).or_default().push(caller_id);
                }
            }
        }
        for callers in reverse_calls.values_mut() {
            callers.sort_unstable();
            callers.dedup();
        }

        // Seed worklist with all functions; first round processes every caller.
        let mut worklist: BTreeSet<FunctionId> = self.by_id.iter().map(|(id, _)| id).collect();

        for _ in 0..MAX_SUMMARY_ROUNDS {
            if worklist.is_empty() {
                break;
            }

            let current_round: Vec<FunctionId> = worklist.iter().copied().collect();
            worklist.clear();

            let mut changed: BTreeSet<FunctionId> = BTreeSet::new();

            for &caller in &current_round {
                let call_count = self
                    .by_id
                    .get(caller)
                    .map_or(0, |summary| summary.calls.len());
                for index in 0..call_count {
                    let Some(call_id) = self
                        .by_id
                        .get(caller)
                        .and_then(|summary| summary.calls.get(index))
                        .copied()
                    else {
                        continue;
                    };
                    if self.propagate_call_sinks(call_id, caller, stream) {
                        changed.insert(caller);
                    }
                }
            }

            // Update sinks_offset ONLY for changed functions (not all)
            for &changed_id in &changed {
                if let Some(summary) = self.by_id.get_mut(changed_id) {
                    summary.sinks_offset = summary.sinks.set.len();
                }
            }

            // Schedule callers of changed functions only
            for &changed_id in &changed {
                if let Some(callers) = reverse_calls.get(&changed_id) {
                    for &c in callers {
                        worklist.insert(c);
                    }
                }
            }
        }
    }

    fn propagate_call_sinks(
        &mut self,
        call_id: FactId,
        caller: FunctionId,
        stream: &FactStream<Frozen>,
    ) -> bool {
        let Some((target, args)) = resolve_call_target(call_id, stream) else {
            return false;
        };
        let target_sinks_offset = self
            .by_id
            .get(target)
            .map_or(0, |summary| summary.sinks_offset);
        if target == caller {
            return false;
        }
        let Some((target_summary, caller_summary)) = self.by_id.get_disjoint(target, caller) else {
            return false;
        };
        let target_summary = match target_summary {
            Some(s) if s.is_invocation_compatible(stream, args, &self.paths) => s,
            _ => return false,
        };
        let Some(caller_summary) = caller_summary else {
            return false;
        };
        self.scratch_projections.clear();
        {
            let target_params = stream.function_parameters(target);
            let caller_params = stream.function_parameters(caller);
            let sink_count = target_summary.sinks.set.len();
            for sink_idx in target_sinks_offset..sink_count {
                let sink = &target_summary.sinks.set[sink_idx];
                if let Some(proj) = try_project_sink(
                    target_params,
                    caller_params,
                    sink,
                    stream,
                    args,
                    &self.paths,
                ) {
                    self.scratch_projections.push(proj);
                }
            }
        }
        let mut changed = false;
        for proj in self.scratch_projections.drain(..) {
            changed |= caller_summary.add_sink(proj);
        }
        changed
    }
}

fn resolve_call_target(
    call_id: FactId,
    stream: &FactStream<Frozen>,
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
    stream: &FactStream<Frozen>,
    args: &[CallArgInfo],
    paths: &SummaryPathStore<'_>,
) -> Option<FunctionSinkSummary> {
    let target_parameter = target_parameters.iter().find(|parameter| {
        parameter.parameter_index == sink.parameter_index()
            && (SummaryPathStore::matches_frozen(sink.path(), parameter.path)
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
    pub(super) fn parameter_bindings<'s>(
        &self,
        stream: &'s FactStream<Frozen>,
    ) -> &'s [ParameterBinding] {
        stream.function_parameters(self.id)
    }

    pub(super) fn sinks(&self) -> &SinkSet {
        &self.sinks
    }

    fn add_sink(&mut self, sink: FunctionSinkSummary) -> bool {
        self.sinks.push_unique(sink)
    }
}

impl FunctionSummary {
    pub(super) fn is_invocation_compatible(
        &self,
        stream: &FactStream<Frozen>,
        args: &[CallArgInfo],
        paths: &SummaryPathStore<'_>,
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
        for parameter in self.parameter_bindings(stream) {
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
        stream: &FactStream<Frozen>,
        plan: &BoundFlowPlan<'_>,
        paths: &mut SummaryPathStore<'_>,
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
        for flow_id in plan.sink_ids(chain).into_iter().flatten() {
            let Some(flow) = plan.get(*flow_id) else {
                continue;
            };
            let sink_members = plan.sink_member_calls(*flow_id);
            for (i, sink) in flow.sinks.iter().enumerate() {
                if !sink_members
                    .get(i)
                    .is_some_and(|members| members.iter().any(|member| member == chain))
                {
                    continue;
                }
                for argument_index in sink.args.present_indices(args.len()) {
                    let Some(argument) = args.get(argument_index) else {
                        continue;
                    };
                    let Some(parameter) =
                        self.parameter_bindings(stream).iter().find(|parameter| {
                            parameter.value != ValueId::UNKNOWN
                                && parameter.value == argument.base_value
                        })
                    else {
                        continue;
                    };
                    let Some(prefix_id) = paths.intern_frozen(parameter.path) else {
                        continue;
                    };
                    let Some(suffix_id) = paths.intern_frozen(argument.base_path) else {
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
        stream: &FactStream<Frozen>,
        args: &[CallArgInfo],
        paths: &SummaryPathStore<'_>,
    ) -> Option<ValueId> {
        let param_path = paths.resolve_frozen(self.path)?;
        self.project_argument_at(stream, args, paths, param_path)
    }

    pub(super) fn project_argument_at(
        &self,
        stream: &FactStream<Frozen>,
        args: &[CallArgInfo],
        paths: &SummaryPathStore<'_>,
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
            return value_at_path(stream.values(), argument.value, paths, path);
        }

        if path.is_empty() {
            return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
        }

        {
            let id = value_at_path(stream.values(), argument.value, paths, path)
                .filter(|v| *v != ValueId::UNKNOWN);
            id.or_else(|| self.default.filter(|value| *value != ValueId::UNKNOWN))
        }
    }
}

fn value_at_path(
    values: &ValueTable,
    value_id: ValueId,
    paths: &SummaryPathStore<'_>,
    path: SummaryPathId,
) -> Option<ValueId> {
    let mut current = value_id;
    let mut valid = true;
    paths.visit_segments(path, &mut |segment| {
        if !valid {
            return;
        }
        let Some(value) = values.resolve(current) else {
            valid = false;
            return;
        };
        let next = match value {
            Value::StaticObject(entries) => match segment {
                PathSegment::Property(name_id) => {
                    entries.iter().find(|(k, _)| k == name_id).map(|(_, v)| *v)
                }
                PathSegment::Index(_) => None,
            },
            Value::StaticArray(elements) => match segment {
                PathSegment::Index(index) => elements.get(*index as usize).copied(),
                PathSegment::Property(_) => None,
            },
            _ => None,
        };
        if let Some(next) = next {
            current = next;
        } else {
            valid = false;
        }
    })?;
    if !valid {
        return None;
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
        let mut resolver = Resolver::collect(&parsed.program);
        let stream = facts::build::build_test_stream(&parsed.program, &mut resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let summaries = FunctionSummaries::collect(&stream, &effects, &plan);
        assert!(summaries.by_id.len() >= 2);
        assert_eq!(
            summaries
                .by_id
                .iter()
                .map(|(_, summary)| summary)
                .filter(|summary| summary.parameter_count == 1)
                .count(),
            2
        );
    }

    // ── Summary path store unit tests ────────────────────────────────────

    fn make_frozen_paths() -> (PathInterner, u32) {
        let mut frozen = PathInterner::new();
        let a = frozen.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
        let _b = frozen.append(a, PathSegment::Index(1)).unwrap();
        let _c = frozen.append(a, PathSegment::Index(2)).unwrap();
        (frozen, a.as_u32())
    }

    #[test]
    fn frozen_path_is_referenced_without_copy() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let store = SummaryPathStore::new(&frozen);
        let s_id = store.intern_frozen(a_id).unwrap();
        assert_eq!(s_id, SummaryPathId::from_path_id(a_id));
        assert!(s_id.is_frozen());
        assert_eq!(store.depth(s_id), Some(1));
    }

    #[test]
    fn invalid_frozen_path_returns_none() {
        let (frozen, _) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        assert!(store.intern_frozen(PathId::from_raw(u32::MAX)).is_none());
        assert!(store.resolve_frozen(PathId::from_raw(u32::MAX)).is_none());
    }

    #[test]
    fn join_frozen_prefix_with_frozen_suffix_creates_overlay_node() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let b_id = PathId::from_raw(a_raw + 1);
        let mut store = SummaryPathStore::new(&frozen);
        let prefix = store.intern_frozen(a_id).unwrap();
        let suffix = store.intern_frozen(b_id).unwrap();
        let joined = store.join(prefix, suffix).unwrap();
        assert!(!joined.is_frozen());
        assert!(!joined.is_empty());
        // a = [Idx(0)], b = [Idx(0), Idx(1)], so join(a,b) = [Idx(0), Idx(0), Idx(1)]
        // depth=3
        assert_eq!(store.depth(joined), Some(3));
    }

    #[test]
    fn join_with_empty_is_identity() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let mut store = SummaryPathStore::new(&frozen);
        let prefix = store.intern_frozen(a_id).unwrap();
        assert_eq!(store.join(prefix, SummaryPathId::EMPTY), Some(prefix));
        assert_eq!(store.join(SummaryPathId::EMPTY, prefix), Some(prefix));
    }

    #[test]
    fn frozen_reference_reused_by_multiple_summaries() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let store = SummaryPathStore::new(&frozen);
        let id1 = store.intern_frozen(a_id).unwrap();
        let id2 = store.intern_frozen(a_id).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn starts_with_mixed_frozen_and_overlay() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let b_id = PathId::from_raw(a_raw + 1);
        let mut store = SummaryPathStore::new(&frozen);
        let a = store.intern_frozen(a_id).unwrap();
        let b = store.intern_frozen(b_id).unwrap();
        let ab = store.join(a, b).unwrap();
        // ab = [Idx(0)] joined with [Idx(0), Idx(1)] = [Idx(0), Idx(0), Idx(1)]
        assert!(store.starts_with(ab, a));
        // ab starts with itself
        assert!(store.starts_with(ab, ab));
    }

    #[test]
    fn matches_frozen_checks_identity() {
        let (_, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        assert!(SummaryPathStore::matches_frozen(
            SummaryPathId::from_path_id(a_id),
            a_id
        ));
        assert!(!SummaryPathStore::matches_frozen(
            SummaryPathId::from_path_id(a_id),
            PathId::from_raw(a_raw + 10),
        ));
    }

    #[test]
    fn starts_with_frozen_checks_prefix() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let b_id = PathId::from_raw(a_raw + 1);
        let mut store = SummaryPathStore::new(&frozen);
        let a = store.intern_frozen(a_id).unwrap();
        let b = store.intern_frozen(b_id).unwrap();
        let ab = store.join(a, b).unwrap();
        assert!(store.starts_with_frozen(ab, a_id));
        assert!(!store.starts_with_frozen(a, b_id));
    }

    #[test]
    fn without_first_on_frozen() {
        let (frozen, a_raw) = make_frozen_paths();
        // ab = [Idx(0), Idx(1)]; after removing first, [Idx(1)] doesn't exist
        // standalone
        let ab_id = PathId::from_raw(a_raw + 1);
        let store = SummaryPathStore::new(&frozen);
        let s_ab = SummaryPathId::from_path_id(ab_id);
        assert!(store.without_first(s_ab).is_none());
    }

    #[test]
    fn without_first_on_overlay() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let b_id = PathId::from_raw(a_raw + 1);
        let mut store = SummaryPathStore::new(&frozen);
        let a = store.intern_frozen(a_id).unwrap();
        let b = store.intern_frozen(b_id).unwrap();
        let ab = store.join(a, b).unwrap();
        // ab = join(a=[Idx(0)], b=[Idx(0), Idx(1)]) = [Idx(0), Idx(0), Idx(1)]
        // without_first removes the first segment, leaving [Idx(0), Idx(1)] which is b
        let result = store.without_first(ab).unwrap();
        assert_eq!(result, b);
    }

    #[test]
    fn owned_segments_on_frozen() {
        let (frozen, a_raw) = make_frozen_paths();
        let ab_id = PathId::from_raw(a_raw + 1);
        let store = SummaryPathStore::new(&frozen);
        let s_ab = SummaryPathId::from_path_id(ab_id);
        let segs = store.owned_segments(s_ab).unwrap();
        assert_eq!(segs, vec![PathSegment::Index(0), PathSegment::Index(1)]);
    }

    #[test]
    fn owned_segments_on_joined_overlay() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let b_id = PathId::from_raw(a_raw + 1);
        let mut store = SummaryPathStore::new(&frozen);
        let a = store.intern_frozen(a_id).unwrap();
        let b = store.intern_frozen(b_id).unwrap();
        let ab = store.join(a, b).unwrap();
        // join(a=[Idx(0)], b=[Idx(0), Idx(1)]) = [Idx(0), Idx(0), Idx(1)]
        let segs = store.owned_segments(ab).unwrap();
        assert_eq!(
            segs,
            vec![
                PathSegment::Index(0),
                PathSegment::Index(0),
                PathSegment::Index(1),
            ]
        );
    }

    #[test]
    fn overlay_budget_exhaustion_fails_closed() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw);
        let b_id = PathId::from_raw(a_raw + 1);
        let mut store = SummaryPathStore {
            frozen: &frozen,
            overlay: ParentPathStore::new(2),
        };
        let a = store.intern_frozen(a_id).unwrap();
        let b = store.intern_frozen(b_id).unwrap();
        assert!(store.join(a, b).is_none());
    }

    #[test]
    fn empty_summary_path_has_no_segments() {
        let (frozen, _) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        assert_eq!(store.depth(SummaryPathId::EMPTY), Some(0));
        assert_eq!(store.first_index(SummaryPathId::EMPTY), None);
        assert_eq!(store.without_first(SummaryPathId::EMPTY), None);
    }

    #[test]
    fn first_index_on_frozen_and_overlay() {
        let (frozen, a_raw) = make_frozen_paths();
        let idx_id = PathId::from_raw(a_raw);
        let store = SummaryPathStore::new(&frozen);
        let s_idx = SummaryPathId::from_path_id(idx_id);
        assert_eq!(store.first_index(s_idx), Some(0));
    }

    #[test]
    fn join_order_with_three_segments() {
        let (frozen, a_raw) = make_frozen_paths();
        let a_id = PathId::from_raw(a_raw); // [Idx(0)]
        let b_id = PathId::from_raw(a_raw + 1); // [Idx(0), Idx(1)]
        let c_id = PathId::from_raw(a_raw + 2); // [Idx(0), Idx(2)]
        let mut store = SummaryPathStore::new(&frozen);
        let a = store.intern_frozen(a_id).unwrap();
        let b = store.intern_frozen(b_id).unwrap();
        let c = store.intern_frozen(c_id).unwrap();
        // ab = join([Idx(0)], [Idx(0), Idx(1)]) = [Idx(0), Idx(0), Idx(1)]
        let ab = store.join(a, b).unwrap();
        // abc = join(ab, [Idx(0), Idx(2)]) = [Idx(0), Idx(0), Idx(1), Idx(0), Idx(2)]
        let abc = store.join(ab, c).unwrap();
        assert_eq!(store.depth(abc), Some(5));
        assert!(store.starts_with(abc, a));
        let segs = store.owned_segments(abc).unwrap();
        assert_eq!(
            segs,
            vec![
                PathSegment::Index(0),
                PathSegment::Index(0),
                PathSegment::Index(1),
                PathSegment::Index(0),
                PathSegment::Index(2),
            ]
        );
    }
}
