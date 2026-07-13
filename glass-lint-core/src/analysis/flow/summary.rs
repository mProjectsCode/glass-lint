//! Function summaries projected from the canonical fact stream.
//!
//! A summary is keyed only by `FunctionId`. Parameter paths and argument
//! projections keep destructuring precise, while the fixed point joins helper
//! calls (including recursive and mutually recursive helpers) without walking
//! AST bodies again.

use std::collections::BTreeMap;

use super::super::facts::FactId;
use super::super::facts::{CallArgInfo, FactPayload, FactStream, ParameterBinding};
use super::super::value::{FunctionId, PathId, ValueId};
use super::index::{FlowId, FlowIndex};
use crate::api::compiler::CompiledObjectSinkArgs;

const MAX_SUMMARY_ROUNDS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FunctionSinkSummary {
    pub(super) flow: FlowId,
    pub(super) parameter_index: usize,
    pub(super) path: PathId,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSummary {
    #[allow(dead_code)]
    pub(super) id: FunctionId,
    #[allow(dead_code)]
    pub(super) owner: FunctionId,
    pub(super) parameters: Vec<ParameterBinding>,
    pub(super) parameter_count: usize,
    pub(super) has_rest: bool,
    /// Canonical fact identities avoid retaining cloned call payloads in the
    /// summary. Resolve these through the immutable stream when projecting.
    pub(super) calls: Vec<FactId>,
    pub(super) sinks: Vec<FunctionSinkSummary>,
    pub(super) writes: Vec<PropertyWriteProjection>,
    pub(super) returns: Vec<ReturnProjection>,
    pub(super) invalid: SummaryInvalidation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PropertyWriteProjection {
    pub(super) event: FactId,
    pub(super) target: ValueId,
    pub(super) receiver: Option<ValueId>,
    pub(super) property: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReturnProjection {
    pub(super) event: FactId,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SummaryInvalidation {
    pub(super) reassigned: bool,
    pub(super) dynamic: bool,
}

#[derive(Debug, Default, Clone)]
pub(super) struct FunctionSummaries {
    pub(super) by_id: Vec<Option<FunctionSummary>>,
}

impl FunctionSummaries {
    pub(super) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(usize::try_from(id.0).ok()?)?.as_ref()
    }

    fn insert(&mut self, summary: FunctionSummary) {
        let Some(index) = usize::try_from(summary.id.0).ok() else {
            return;
        };
        if self.by_id.len() <= index {
            self.by_id.resize_with(index + 1, || None);
        }
        self.by_id[index] = Some(summary);
    }
}

pub(super) fn collect(stream: &FactStream, flow_index: &FlowIndex<'_>) -> FunctionSummaries {
    let mut summaries = FunctionSummaries::default();
    let mut calls_by_function: BTreeMap<FunctionId, Vec<FactId>> = BTreeMap::new();

    for fact in stream.facts() {
        match &fact.payload {
            FactPayload::Function {
                id,
                owner,
                parameters,
                boundary: crate::analysis::facts::FunctionBoundary::Enter,
                ..
            } => {
                let parameter_count = parameters
                    .iter()
                    .map(|parameter| parameter.parameter_index)
                    .max()
                    .map_or(0, |index| index.saturating_add(1));
                if summaries.get(*id).is_none() {
                    summaries.insert(FunctionSummary {
                        id: *id,
                        owner: *owner,
                        parameters: parameters.clone(),
                        parameter_count,
                        has_rest: parameters.iter().any(|parameter| parameter.rest),
                        calls: Vec::new(),
                        sinks: Vec::new(),
                        writes: Vec::new(),
                        returns: Vec::new(),
                        invalid: SummaryInvalidation::default(),
                    });
                }
            }
            FactPayload::Assignment {
                target, receiver, ..
            } => {
                if let Some(summary) = summaries
                    .by_id
                    .get_mut(usize::try_from(fact.function.0).unwrap_or(usize::MAX))
                    .and_then(Option::as_mut)
                {
                    summary.invalid.reassigned = true;
                    summary.writes.push(PropertyWriteProjection {
                        event: fact.id,
                        target: *target,
                        receiver: *receiver,
                        property: None,
                    });
                }
            }
            FactPayload::PropertyWrite {
                target,
                receiver,
                property,
                ..
            } => {
                if let Some(summary) = summaries
                    .by_id
                    .get_mut(usize::try_from(fact.function.0).unwrap_or(usize::MAX))
                    .and_then(Option::as_mut)
                {
                    summary.writes.push(PropertyWriteProjection {
                        event: fact.id,
                        target: *target,
                        receiver: Some(*receiver),
                        property: property.clone(),
                    });
                }
            }
            FactPayload::Control {
                kind: crate::analysis::facts::ControlKind::Return,
                ..
            } => {
                if let Some(summary) = summaries
                    .by_id
                    .get_mut(usize::try_from(fact.function.0).unwrap_or(usize::MAX))
                    .and_then(Option::as_mut)
                {
                    summary.returns.push(ReturnProjection { event: fact.id });
                }
            }
            FactPayload::Call { .. } => calls_by_function
                .entry(fact.function)
                .or_default()
                .push(fact.id),
            _ => {}
        }
    }

    // First collect facts whose sink is directly visible in the function.
    for summary in summaries.by_id.iter_mut().filter_map(Option::as_mut) {
        let Some(call_ids) = calls_by_function.get(&summary.id) else {
            continue;
        };
        summary.calls = call_ids.clone();
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
            let chain = rooted_chain.as_deref().or(syntactic_chain.as_deref());
            let Some(chain) = chain else { continue };
            let flow_ids = flow_index.sinks.get(chain).into_iter().flatten();
            for flow_id in flow_ids {
                let Some(flow) = flow_index.get(*flow_id) else {
                    continue;
                };
                for sink in &flow.sinks {
                    if !sink.member_calls.iter().any(|member| member == chain) {
                        continue;
                    }
                    for argument_index in sink_argument_indices(&sink.args, args.len()) {
                        let Some(argument) = args.get(argument_index) else {
                            continue;
                        };
                        if let Some(parameter) = summary.parameters.iter().find(|parameter| {
                            parameter.value != ValueId::UNKNOWN
                                && parameter.value == argument.base_value
                        }) {
                            let Some(path) =
                                stream.concat_paths(parameter.path, argument.base_path)
                            else {
                                continue;
                            };
                            add_sink(
                                summary,
                                FunctionSinkSummary {
                                    flow: *flow_id,
                                    parameter_index: parameter.parameter_index,
                                    path,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    // Propagate sink projections through proven FunctionId call edges. Since
    // every propagation only adds a deduplicated projection, this is a finite
    // monotone fixed point even for recursive SCCs.
    for _ in 0..MAX_SUMMARY_ROUNDS {
        let mut changed = false;
        let function_ids = summaries
            .by_id
            .iter()
            .filter_map(|summary| summary.as_ref().map(|summary| summary.id))
            .collect::<Vec<_>>();
        for caller in function_ids {
            let Some(calls) = calls_by_function.get(&caller) else {
                continue;
            };
            let caller_parameters = summaries
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
                let Some(target_summary) = summaries.get(target).cloned() else {
                    continue;
                };
                if !invocation_is_compatible(stream, &target_summary, args) {
                    continue;
                }
                for sink in target_summary.sinks {
                    let Some(target_parameter) =
                        target_summary.parameters.iter().find(|parameter| {
                            parameter.parameter_index == sink.parameter_index
                                && (parameter.path == sink.path
                                    || (parameter.rest
                                        && stream.paths().starts_with(sink.path, parameter.path)))
                        })
                    else {
                        continue;
                    };
                    let mut target_parameter = target_parameter.clone();
                    target_parameter.path = sink.path;
                    let Some(argument) =
                        project_parameter_argument(stream, args, &target_parameter)
                    else {
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
                        flow: sink.flow,
                        parameter_index: caller_parameter.parameter_index,
                        path: caller_parameter.path,
                    };
                    let Some(caller_summary) = summaries
                        .by_id
                        .get_mut(usize::try_from(caller.0).unwrap_or(usize::MAX))
                        .and_then(Option::as_mut)
                    else {
                        continue;
                    };
                    if !caller_summary.sinks.contains(&projection) {
                        caller_summary.sinks.push(projection);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    for summary in summaries.by_id.iter_mut().filter_map(Option::as_mut) {
        summary
            .sinks
            .sort_by_key(|sink| (sink.flow, sink.parameter_index, sink.path));
        summary.sinks.dedup();
    }
    summaries
}

pub(super) fn invocation_is_compatible(
    stream: &FactStream,
    summary: &FunctionSummary,
    args: &[CallArgInfo],
) -> bool {
    if args.iter().any(|argument| argument.spread) {
        return false;
    }
    if !summary.has_rest && args.len() > summary.parameter_count {
        return false;
    }
    for argument in args.iter().take(summary.parameter_count) {
        if argument.value == ValueId::UNKNOWN {
            return false;
        }
    }
    for parameter in &summary.parameters {
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
        // A missing nested property is unknown unless the leaf has a default.
        if project_parameter_argument(stream, args, parameter).is_none()
            && parameter.default.is_none()
        {
            return false;
        }
    }
    true
}

pub(super) fn project_parameter_argument(
    stream: &FactStream,
    args: &[CallArgInfo],
    parameter: &ParameterBinding,
) -> Option<ValueId> {
    let Some(argument) = args.get(parameter.parameter_index) else {
        return parameter
            .path
            .is_empty()
            .then_some(parameter.default)
            .flatten()
            .filter(|value| *value != ValueId::UNKNOWN);
    };
    if argument.spread {
        return None;
    }

    if parameter.rest {
        let index = stream.paths().first_index(parameter.path)?;
        let argument = args.get(parameter.parameter_index.saturating_add(index as usize))?;
        if argument.spread {
            return None;
        }
        let path = stream.paths().without_first(parameter.path)?;
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

    if parameter.path.is_empty() {
        return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
    }

    argument
        .projections
        .iter()
        .find(|projection| projection.path == parameter.path)
        .map(|projection| projection.value)
        .filter(|value| *value != ValueId::UNKNOWN)
        .or_else(|| parameter.default.filter(|value| *value != ValueId::UNKNOWN))
}

fn sink_argument_indices(args: &CompiledObjectSinkArgs, argument_count: usize) -> Vec<usize> {
    match args {
        CompiledObjectSinkArgs::Any => (0..argument_count).collect(),
        CompiledObjectSinkArgs::Indices(indices) => indices
            .iter()
            .copied()
            .filter(|index| *index < argument_count)
            .collect(),
    }
}

fn add_sink(summary: &mut FunctionSummary, sink: FunctionSinkSummary) {
    if !summary.sinks.contains(&sink) {
        summary.sinks.push(sink);
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
        let summaries = collect(&stream, &FlowIndex::new(&[]));
        assert!(summaries.by_id.len() >= 2);
        assert_eq!(
            summaries
                .by_id
                .iter()
                .filter_map(Option::as_ref)
                .filter(|summary| summary.parameters.len() == 1)
                .count(),
            2
        );
    }
}
