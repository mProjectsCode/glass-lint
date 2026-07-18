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

use super::{
    super::{
        facts::{CallArgInfo, FactId, FactPayload, FactStream, ParameterBinding},
        value::{FunctionId, PathId, PathInterner, ValueId},
    },
    index::{FlowId, FlowIndex},
    table::FunctionTable,
};

const MAX_SUMMARY_ROUNDS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Sink reachable through a function parameter path.
pub(super) struct FunctionSinkSummary {
    /// Flow matcher that owns the sink.
    flow: FlowId,
    /// Top-level parameter receiving the propagated object.
    parameter_index: usize,
    /// Nested path at which the sink consumes the object.
    path: PathId,
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
    /// Writes that may invalidate a propagated object.
    writes: Vec<PropertyWriteProjection>,
    /// Reasons this summary must not be used for precise propagation.
    invalid: SummaryInvalidation,
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
        self.0
            .sort_by_key(|sink| (sink.flow(), sink.parameter_index(), sink.path()));
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// Property write retained as a summary invalidation/provenance event.
pub(super) struct PropertyWriteProjection {
    event: FactId,
    target: ValueId,
    receiver: Option<ValueId>,
    property: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Summary invalidation flags that force conservative invocation behavior.
pub(super) struct SummaryInvalidation {
    /// A parameter/object identity was reassigned.
    reassigned: bool,
    /// A dynamic or unsupported shape was encountered.
    dynamic: bool,
}

impl FunctionSinkSummary {
    pub(super) fn flow(&self) -> FlowId {
        self.flow
    }

    pub(super) fn parameter_index(&self) -> usize {
        self.parameter_index
    }

    pub(super) fn path(&self) -> PathId {
        self.path
    }
}

impl SummaryInvalidation {
    fn mark_reassigned(&mut self) {
        self.reassigned = true;
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

    pub(super) fn collect(stream: &FactStream, flow_index: &FlowIndex<'_>) -> Self {
        let mut summaries = Self::default();
        let calls_by_function = summaries.collect_facts(stream);
        let mut paths: PathInterner = stream.paths().clone();

        // First collect facts whose sink is directly visible in the function.
        summaries.collect_direct_sinks(stream, flow_index, &calls_by_function, &mut paths);

        // Propagate sink projections through proven FunctionId call edges. Since
        // every propagation only adds a deduplicated projection, this is a finite
        // monotone fixed point even for recursive SCCs.
        summaries.propagate_sinks(stream, &calls_by_function);

        for (_, summary) in summaries.by_id.iter_mut() {
            summary.sinks.sort_and_dedup();
        }
        summaries
    }

    fn collect_facts(&mut self, stream: &FactStream) -> FunctionTable<Vec<FactId>> {
        let mut calls_by_function = FunctionTable::default();
        for fact in stream.facts() {
            match &fact.payload {
                FactPayload::Function {
                    id,
                    parameters,
                    boundary: crate::analysis::facts::FunctionBoundary::Enter,
                    ..
                } => {
                    let parameter_count = parameters
                        .iter()
                        .map(|parameter| parameter.parameter_index)
                        .max()
                        .map_or(0, |index| index.saturating_add(1));
                    if self.get(*id).is_none() {
                        self.insert(FunctionSummary {
                            id: *id,
                            parameters: parameters.clone(),
                            parameter_count,
                            has_rest: parameters.iter().any(|parameter| parameter.rest),
                            calls: Vec::new(),
                            sinks: SinkSet::default(),
                            writes: Vec::new(),
                            invalid: SummaryInvalidation::default(),
                        });
                    }
                }
                FactPayload::Assignment {
                    target, receiver, ..
                } => {
                    if let Some(summary) = self.by_id.get_mut(fact.function) {
                        summary.record_write(fact.id, *target, *receiver, None, true);
                    }
                }
                FactPayload::PropertyWrite {
                    target,
                    receiver,
                    property,
                    ..
                } => {
                    if let Some(summary) = self.by_id.get_mut(fact.function) {
                        summary.record_write(
                            fact.id,
                            *target,
                            Some(*receiver),
                            property.clone(),
                            false,
                        );
                    }
                }
                FactPayload::Call { .. } => calls_by_function
                    .get_mut_or_insert_with(fact.function, Vec::new)
                    .expect("fact function IDs are allocated densely")
                    .push(fact.id),
                _ => {}
            }
        }
        calls_by_function
    }

    fn collect_direct_sinks(
        &mut self,
        stream: &FactStream,
        flow_index: &FlowIndex<'_>,
        calls_by_function: &FunctionTable<Vec<FactId>>,
        paths: &mut PathInterner,
    ) {
        for (_, summary) in self.by_id.iter_mut() {
            let Some(call_ids) = calls_by_function.get(summary.id) else {
                continue;
            };
            summary.calls.clone_from(call_ids);
            for call_id in call_ids {
                let Some(FactPayload::Call {
                    syntactic_chain,
                    rooted_chain,
                    args,
                    ..
                }) = stream.fact(*call_id).map(|fact| &fact.payload)
                else {
                    continue;
                };
                let Some(chain) = rooted_chain.as_ref().or(syntactic_chain.as_ref()) else {
                    continue;
                };
                for flow_id in flow_index.sink_ids(chain).into_iter().flatten() {
                    let Some(flow) = flow_index.get(*flow_id) else {
                        continue;
                    };
                    for sink in &flow.sinks {
                        if !sink.member_calls.iter().any(|member| member == chain) {
                            continue;
                        }
                        for argument_index in sink.args.present_indices(args.len()) {
                            let Some(argument) = args.get(argument_index) else {
                                continue;
                            };
                            let Some(parameter) = summary.parameters.iter().find(|parameter| {
                                parameter.value != ValueId::UNKNOWN
                                    && parameter.value == argument.base_value
                            }) else {
                                continue;
                            };
                            let Some(path) = paths.concat(parameter.path, argument.base_path)
                            else {
                                continue;
                            };
                            summary.add_sink(FunctionSinkSummary {
                                flow: *flow_id,
                                parameter_index: parameter.parameter_index,
                                path,
                            });
                        }
                    }
                }
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
                    let Some(FactPayload::Call {
                        target_function,
                        args,
                        ..
                    }) = stream.fact(*call_id).map(|fact| &fact.payload)
                    else {
                        continue;
                    };
                    let Some(target) = *target_function else {
                        continue;
                    };
                    let Some(target_summary) = self.get(target).cloned() else {
                        continue;
                    };
                    if !target_summary.is_invocation_compatible(stream, args) {
                        continue;
                    }
                    for sink in target_summary.sinks {
                        let Some(target_parameter) =
                            target_summary.parameters.iter().find(|parameter| {
                                parameter.parameter_index == sink.parameter_index()
                                    && (parameter.path == sink.path()
                                        || (parameter.rest
                                            && stream
                                                .paths()
                                                .starts_with(sink.path(), parameter.path)))
                            })
                        else {
                            continue;
                        };
                        let mut target_parameter = target_parameter.clone();
                        target_parameter.path = sink.path();
                        let Some(argument) = target_parameter.project_argument(stream, args) else {
                            continue;
                        };
                        let Some(caller_parameter) = caller_parameters.iter().find(|parameter| {
                            !parameter.rest
                                && parameter.value != ValueId::UNKNOWN
                                && parameter.value == argument
                        }) else {
                            continue;
                        };
                        let projection = FunctionSinkSummary {
                            flow: sink.flow(),
                            parameter_index: caller_parameter.parameter_index,
                            path: caller_parameter.path,
                        };
                        if let Some(caller_summary) = self.by_id.get_mut(caller) {
                            changed |= caller_summary.add_sink(projection);
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
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

impl FunctionSummary {
    fn record_write(
        &mut self,
        event: FactId,
        target: ValueId,
        receiver: Option<ValueId>,
        property: Option<String>,
        reassigned: bool,
    ) {
        if reassigned {
            self.invalid.mark_reassigned();
        }
        self.writes.push(PropertyWriteProjection {
            event,
            target,
            receiver,
            property,
        });
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

impl ParameterBinding {
    /// Resolve this parameter against a concrete call's argument facts.
    /// Defaults are used only for the exact missing leaf they cover.
    pub(super) fn project_argument(
        &self,
        stream: &FactStream,
        args: &[CallArgInfo],
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
            let index = stream.paths().first_index(self.path)?;
            let argument = args.get(self.parameter_index.saturating_add(index as usize))?;
            if argument.spread {
                return None;
            }
            let path = stream.paths().without_first(self.path)?;
            if path.is_empty() {
                return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
            }
            return argument
                .projections
                .iter()
                .find(|projection| projection.path == path)
                .map(|projection| projection.value)
                .filter(|value| *value != ValueId::UNKNOWN);
        }

        if self.path.is_empty() {
            return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
        }

        argument
            .projections
            .iter()
            .find(|projection| projection.path == self.path)
            .map(|projection| projection.value)
            .filter(|value| *value != ValueId::UNKNOWN)
            .or_else(|| self.default.filter(|value| *value != ValueId::UNKNOWN))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::resolution::Resolver;

    #[test]
    fn same_name_siblings_are_keyed_by_function_id() {
        let parsed = crate::parse(
            "function first(x) { document.body.appendChild(x); } function second(x) { console.log(x); }",
            "summary-siblings.js",
        )
        .expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream =
            super::super::super::facts::build::build_test_stream(&parsed.program, &resolver);
        let summaries = FunctionSummaries::collect(&stream, &FlowIndex::new(&[]));
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
