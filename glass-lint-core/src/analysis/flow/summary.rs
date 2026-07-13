//! Function summaries projected from the canonical fact stream.
//!
//! A summary is keyed only by `FunctionId`. Parameter paths and argument
//! projections keep destructuring precise, while the fixed point joins helper
//! calls (including recursive and mutually recursive helpers) without walking
//! AST bodies again.

use std::collections::BTreeMap;

use super::super::facts::FactId;
use super::super::facts::{
    CallArgInfo, FactPayload, FactStream, ParameterBinding, ProjectionSegment,
};
use super::super::value::{FunctionId, ValueId};
use super::index::{FlowId, FlowIndex};
use crate::api::rule::FlowSinkArgs;

const MAX_SUMMARY_ROUNDS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FunctionSinkSummary {
    pub(super) flow: FlowId,
    pub(super) parameter_index: usize,
    pub(super) path: Vec<ProjectionSegment>,
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
    pub(super) calls: Vec<CallProjection>,
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
    pub(super) by_id: BTreeMap<FunctionId, FunctionSummary>,
}

impl FunctionSummaries {
    pub(super) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(&id)
    }
}

pub(super) fn collect(stream: &FactStream, flow_index: &FlowIndex<'_>) -> FunctionSummaries {
    let mut summaries = FunctionSummaries::default();
    let mut calls_by_function: BTreeMap<FunctionId, Vec<CallProjection>> = BTreeMap::new();

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
                summaries
                    .by_id
                    .entry(*id)
                    .or_insert_with(|| FunctionSummary {
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
            FactPayload::Assignment {
                target, receiver, ..
            } => {
                if let Some(summary) = summaries.by_id.get_mut(&fact.function) {
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
                if let Some(summary) = summaries.by_id.get_mut(&fact.function) {
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
                if let Some(summary) = summaries.by_id.get_mut(&fact.function) {
                    summary.returns.push(ReturnProjection { event: fact.id });
                }
            }
            FactPayload::Call {
                syntactic_chain,
                rooted_chain,
                target_function,
                args,
                ..
            } => calls_by_function
                .entry(fact.function)
                .or_default()
                .push(CallProjection {
                    syntactic_chain: syntactic_chain.clone(),
                    rooted_chain: rooted_chain.clone(),
                    target_function: *target_function,
                    args: args.clone(),
                }),
            _ => {}
        }
    }

    // First collect facts whose sink is directly visible in the function.
    for (function, summary) in &mut summaries.by_id {
        let Some(calls) = calls_by_function.get(function) else {
            continue;
        };
        summary.calls = calls.clone();
        for call in calls {
            let chain = call
                .rooted_chain
                .as_deref()
                .or(call.syntactic_chain.as_deref());
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
                    for argument_index in sink_argument_indices(&sink.args, call.args.len()) {
                        let Some(argument) = call.args.get(argument_index) else {
                            continue;
                        };
                        if let Some(parameter) = summary.parameters.iter().find(|parameter| {
                            parameter.value != ValueId::UNKNOWN
                                && parameter.value == argument.base_value
                        }) {
                            let mut path = parameter.path.clone();
                            path.extend(argument.base_path.clone());
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
        let function_ids = summaries.by_id.keys().copied().collect::<Vec<_>>();
        for caller in function_ids {
            let Some(calls) = calls_by_function.get(&caller) else {
                continue;
            };
            let caller_parameters = summaries
                .get(caller)
                .map(|summary| summary.parameters.clone())
                .unwrap_or_default();
            for call in calls {
                let Some(target) = call.target_function else {
                    continue;
                };
                let Some(target_summary) = summaries.get(target).cloned() else {
                    continue;
                };
                if !invocation_is_compatible(&target_summary, &call.args) {
                    continue;
                }
                for sink in target_summary.sinks {
                    let Some(target_parameter) =
                        target_summary.parameters.iter().find(|parameter| {
                            parameter.parameter_index == sink.parameter_index
                                && (parameter.path == sink.path
                                    || (parameter.rest && sink.path.starts_with(&parameter.path)))
                        })
                    else {
                        continue;
                    };
                    let mut target_parameter = target_parameter.clone();
                    target_parameter.path = sink.path.clone();
                    let Some(argument) = project_parameter_argument(&call.args, &target_parameter)
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
                        path: caller_parameter.path.clone(),
                    };
                    let Some(caller_summary) = summaries.by_id.get_mut(&caller) else {
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

    for summary in summaries.by_id.values_mut() {
        summary
            .sinks
            .sort_by_key(|sink| (sink.flow, sink.parameter_index, sink.path.clone()));
        summary.sinks.dedup();
    }
    summaries
}

pub(super) fn invocation_is_compatible(summary: &FunctionSummary, args: &[CallArgInfo]) -> bool {
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
        if project_parameter_argument(args, parameter).is_none() && parameter.default.is_none() {
            return false;
        }
    }
    true
}

pub(super) fn project_parameter_argument(
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
        let Some(ProjectionSegment::Index(index)) = parameter.path.first() else {
            return None;
        };
        let argument = args.get(parameter.parameter_index.saturating_add(*index))?;
        if argument.spread {
            return None;
        }
        let path = &parameter.path[1..];
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

fn sink_argument_indices(args: &FlowSinkArgs, argument_count: usize) -> Vec<usize> {
    match args {
        FlowSinkArgs::Any => (0..argument_count).collect(),
        FlowSinkArgs::Indices(indices) => indices
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

#[derive(Debug, Clone)]
pub(super) struct CallProjection {
    syntactic_chain: Option<String>,
    rooted_chain: Option<String>,
    target_function: Option<FunctionId>,
    args: Vec<CallArgInfo>,
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
                .values()
                .filter(|summary| summary.parameters.len() == 1)
                .count(),
            2
        );
    }
}
