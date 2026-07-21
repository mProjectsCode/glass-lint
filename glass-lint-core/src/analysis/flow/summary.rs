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

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, ParameterBinding},
    flow::{
        effect::{EffectCall, FunctionEffects},
        index::{FlowId, FlowIndex},
        table::FunctionTable,
    },
    value::{FunctionId, PathId, PathInterner, PathSegment, ValueId},
};

const MAX_SUMMARY_ROUNDS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// A base artifact path or a compact path owned by a function summary.
pub(super) enum SummaryPath {
    Base(PathId),
    Owned(Vec<PathSegment>),
}

impl SummaryPath {
    pub(super) fn base(path: PathId) -> Self {
        Self::Base(path)
    }

    pub(super) fn join(paths: &PathInterner, prefix: PathId, suffix: PathId) -> Option<Self> {
        let mut segments = paths.owned_segments(prefix)?;
        segments.extend(paths.owned_segments(suffix)?);
        Some(Self::Owned(segments))
    }

    pub(super) fn is_empty(&self) -> bool {
        match self {
            Self::Base(path) => path.is_empty(),
            Self::Owned(segments) => segments.is_empty(),
        }
    }

    pub(super) fn matches_base(&self, paths: &PathInterner, base: PathId) -> bool {
        match self {
            Self::Base(path) => *path == base,
            Self::Owned(segments) => paths
                .owned_segments(base)
                .is_some_and(|candidate| candidate == *segments),
        }
    }

    pub(super) fn starts_with_base(&self, paths: &PathInterner, prefix: PathId) -> bool {
        match self {
            Self::Base(path) => paths.starts_with(*path, prefix),
            Self::Owned(segments) => paths.owned_segments(prefix).is_some_and(|prefix| {
                segments.len() >= prefix.len()
                    && segments.iter().zip(prefix.iter()).all(|(a, b)| a == b)
            }),
        }
    }

    pub(super) fn first_index(&self, paths: &PathInterner) -> Option<u32> {
        match self {
            Self::Base(path) => paths.first_index(*path),
            Self::Owned(segments) => match segments.first()? {
                PathSegment::Index(index) => Some(*index),
                PathSegment::Property(_) => None,
            },
        }
    }

    pub(super) fn without_first(&self, paths: &PathInterner) -> Option<Self> {
        match self {
            Self::Base(path) => Some(Self::Base(paths.without_first(*path)?)),
            Self::Owned(segments) if !segments.is_empty() => {
                Some(Self::Owned(segments[1..].to_vec()))
            }
            Self::Owned(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Sink reachable through a function parameter path.
pub(super) struct FunctionSinkSummary {
    /// Flow matcher that owns the sink.
    flow: FlowId,
    /// Top-level parameter receiving the propagated object.
    parameter_index: usize,
    /// Nested path at which the sink consumes the object.
    path: SummaryPath,
}

#[derive(Debug, Clone)]
/// Rule-independent helper summary used to project calls without AST access.
pub(super) struct FunctionSummary {
    /// Function identity owning the summary.
    id: FunctionId,
    /// Parameter bindings and destructuring paths.
    parameters: Vec<ParameterBinding>,
    /// Number of top-level parameters in the callable shape.
    parameter_count: usize,
    /// Whether an additional rest argument is accepted.
    has_rest: bool,
    /// Canonical fact identities avoid retaining cloned call payloads in the
    /// summary. Resolve these through the immutable stream when projecting.
    calls: Vec<FactId>,
    /// Sinks directly or transitively reachable through this helper.
    sinks: SinkSet,
}

#[derive(Debug, Clone, Default)]
/// Deduplicated sink projections for one function summary.
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

    pub(super) fn path(&self) -> &SummaryPath {
        &self.path
    }
}

#[derive(Debug, Default, Clone)]
/// Function summaries indexed by stable function identity.
pub(super) struct FunctionSummaries {
    by_id: FunctionTable<FunctionSummary>,
}

impl FunctionSummaries {
    pub(super) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(id)
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
        let calls_by_function = summaries.collect_facts(effects);
        let paths = stream.paths();

        // First collect facts whose sink is directly visible in the function.
        summaries.collect_direct_sinks(stream, flow_index, &calls_by_function, paths);

        // Propagate sink projections through proven FunctionId call edges. Since
        // every propagation only adds a deduplicated projection, this is a finite
        // monotone fixed point even for recursive SCCs.
        summaries.propagate_sinks(stream, &calls_by_function);

        for (_, summary) in summaries.by_id.iter_mut() {
            summary.sinks.sort_and_dedup();
        }
        summaries
    }

    fn collect_facts(&mut self, effects: &FunctionEffects) -> FunctionTable<Vec<FactId>> {
        let mut calls_by_function = FunctionTable::default();
        for effect in effects.iter_effects() {
            if self.get(effect.id()).is_none() {
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
                });
            }
        }
        // Build calls_by_function from the same data.
        for summary in self.by_id.values() {
            calls_by_function.insert(summary.id, summary.calls.clone());
        }
        calls_by_function
    }

    fn collect_direct_sinks(
        &mut self,
        stream: &FactStream,
        flow_index: &FlowIndex<'_>,
        calls_by_function: &FunctionTable<Vec<FactId>>,
        paths: &PathInterner,
    ) {
        for (_, summary) in self.by_id.iter_mut() {
            let Some(call_ids) = calls_by_function.get(summary.id) else {
                continue;
            };
            summary.calls.clone_from(call_ids);
            for call_id in call_ids {
                summary.collect_sinks_for_call(stream, flow_index, paths, *call_id);
            }
        }
    }

    fn propagate_sinks(
        &mut self,
        stream: &FactStream,
        calls_by_function: &FunctionTable<Vec<FactId>>,
    ) {
        for _ in 0..MAX_SUMMARY_ROUNDS {
            let mut changed = false;
            let function_ids = self.by_id.iter().map(|(id, _)| id).collect::<Vec<_>>();
            for caller in function_ids {
                let Some(calls) = calls_by_function.get(caller) else {
                    continue;
                };
                let caller_parameters = self
                    .get(caller)
                    .map(|summary| summary.parameters.clone())
                    .unwrap_or_default();
                for call_id in calls {
                    changed |=
                        self.propagate_call_sinks(*call_id, caller, &caller_parameters, stream);
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
        caller_parameters: &[ParameterBinding],
        stream: &FactStream,
    ) -> bool {
        let Some((target, args)) = resolve_call_target(call_id, stream) else {
            return false;
        };
        let Some(target_summary) = self.get(target).cloned() else {
            return false;
        };
        if !target_summary.is_invocation_compatible(stream, args) {
            return false;
        }
        let mut changed = false;
        for sink in target_summary.sinks {
            if let Some(projection) = try_project_sink(
                &target_summary.parameters,
                caller_parameters,
                &sink,
                stream,
                args,
            ) && let Some(caller_summary) = self.by_id.get_mut(caller)
            {
                changed |= caller_summary.add_sink(projection);
            }
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

/// Try to map a target function's sink onto a caller parameter.
///
/// Returns `Some(projection)` when the sink's parameter index and path match a
/// target parameter, its argument projects to a concrete caller parameter
/// value, and that value is a non-rest, known parameter.
fn try_project_sink(
    target_parameters: &[ParameterBinding],
    caller_parameters: &[ParameterBinding],
    sink: &FunctionSinkSummary,
    stream: &FactStream,
    args: &[CallArgInfo],
) -> Option<FunctionSinkSummary> {
    let target_parameter = target_parameters.iter().find(|parameter| {
        parameter.parameter_index == sink.parameter_index()
            && (sink.path().matches_base(stream.paths(), parameter.path)
                || (parameter.rest && sink.path().starts_with_base(stream.paths(), parameter.path)))
    })?;
    let argument = target_parameter.project_argument_at(stream, args, sink.path())?;
    let caller_parameter = caller_parameters.iter().find(|parameter| {
        !parameter.rest && parameter.value != ValueId::UNKNOWN && parameter.value == argument
    })?;
    Some(FunctionSinkSummary {
        flow: sink.flow(),
        parameter_index: caller_parameter.parameter_index,
        path: SummaryPath::base(caller_parameter.path),
    })
}

impl FunctionSummary {
    pub(super) fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }

    pub(super) fn sinks(&self) -> &SinkSet {
        &self.sinks
    }

    /// Add a sink projection once. Summaries are propagated to a fixed point,
    /// so deduplication belongs with the summary invariant rather than at
    /// every call site that discovers a projection.
    fn add_sink(&mut self, sink: FunctionSinkSummary) -> bool {
        if self.sinks.contains(&sink) {
            return false;
        }
        self.sinks.push(sink);
        true
    }
}

/// Check whether a call provides enough proven values for a function summary.
/// Unknown and spread arguments fail closed because they cannot safely support
/// a parameter-path projection.
impl FunctionSummary {
    /// Check whether a call provides enough proven values for this summary.
    /// Unknown and spread arguments fail closed because they cannot safely
    /// support a parameter-path projection.
    pub(super) fn is_invocation_compatible(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
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
            // A missing nested property is unknown unless the leaf has a
            // default.
            if parameter.project_argument(stream, args).is_none() && parameter.default.is_none() {
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
        paths: &PathInterner,
        call_id: FactId,
    ) {
        let Some(FactPayload::Call {
            syntactic_chain,
            rooted_chain,
            args,
            ..
        }) = stream.fact(call_id).map(|fact| &fact.payload)
        else {
            return;
        };
        let syntactic_name = syntactic_chain.as_ref().and_then(|path| {
            crate::analysis::value::NamePath::from_symbol_path(path, stream.names()?)
        });
        let Some(chain) = rooted_chain.as_ref().or(syntactic_name.as_ref()) else {
            return;
        };
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
                    let Some(path) = SummaryPath::join(paths, parameter.path, argument.base_path)
                    else {
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
    /// Resolve this parameter against a concrete call's argument facts.
    /// Defaults are used only for the exact missing leaf they cover.
    pub(super) fn project_argument(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
    ) -> Option<ValueId> {
        let path = SummaryPath::base(self.path);
        self.project_argument_at(stream, args, &path)
    }

    pub(super) fn project_argument_at(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
        path: &SummaryPath,
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
            let index = path.first_index(stream.paths())?;
            let argument = args.get(self.parameter_index.saturating_add(index as usize))?;
            if argument.spread {
                return None;
            }
            let path = path.without_first(stream.paths())?;
            if path.is_empty() {
                return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
            }
            return argument
                .projections
                .iter()
                .find(|projection| path.matches_base(stream.paths(), projection.path))
                .map(|projection| projection.value)
                .filter(|value| *value != ValueId::UNKNOWN);
        }

        if path.is_empty() {
            return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
        }

        argument
            .projections
            .iter()
            .find(|projection| path.matches_base(stream.paths(), projection.path))
            .map(|projection| projection.value)
            .filter(|value| *value != ValueId::UNKNOWN)
            .or_else(|| self.default.filter(|value| *value != ValueId::UNKNOWN))
    }
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
