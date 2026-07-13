//! Validation of matcher invariants at the rule construction boundary.

use super::matcher::{
    ApiMatcher, ArgumentConstraint, ArgumentMatcher, CallProvenance, FlowCompletion, FlowCondition,
    FlowSinkMatcher, Matcher, MemberCallMatcher, MemberCallProvenance, MemberReadProvenance,
    ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, StaticStringPredicate,
    ValueMatcher, ValueMatcherKind,
};

const MAX_ARGUMENT_INDEX: usize = 1 << 20;
const MAX_EXPRESSION_NODES: usize = 4096;

pub(super) fn validate(matcher: &ApiMatcher) -> Result<(), String> {
    for call in &matcher.calls {
        validate_name(&call.name, "call name")?;
        validate_provenance(&call.provenance)?;
        validate_arguments(&call.arguments)?;
    }
    for call in &matcher.member_calls {
        validate_member_call(call)?;
    }
    for read in &matcher.member_reads {
        validate_chain(&read.chain, "member read chain")?;
        validate_member_read_provenance(&read.provenance)?;
    }
    for value in matcher.imports.iter().chain(&matcher.string_literals) {
        if value.trim().is_empty() {
            return Err("literal matcher value must not be empty".into());
        }
    }
    for class in &matcher.classes {
        validate_name(&class.name, "class name")?;
        validate_provenance(&class.provenance)?;
    }
    for constructor in &matcher.constructors {
        validate_name(&constructor.name, "constructor name")?;
        validate_provenance(&constructor.provenance)?;
    }
    for returned in &matcher.returned_member_calls {
        validate_chain(&returned.source, "returned-member source")?;
        validate_name(&returned.member, "returned-member name")?;
    }
    for returned in &matcher.returned_member_reads {
        validate_chain(&returned.source, "returned-member source")?;
        validate_name(&returned.member, "returned-member name")?;
    }
    for instance in &matcher.instance_member_calls {
        validate_name(&instance.module, "instance module")?;
        validate_name(&instance.export, "instance export")?;
        validate_name(&instance.member, "instance member")?;
    }
    for flow in &matcher.flows {
        validate_object_flow(flow, "flow")?;
    }
    Ok(())
}

pub(crate) fn validate_matcher_at(matcher: &Matcher, index: usize) -> Result<(), String> {
    if let Matcher::ObjectFlow(flow) = matcher {
        let path = format!("matcher[{index}].flow");
        validate_name_at(&flow.symbol, &format!("{path}.symbol"))?;
        if flow.sources.is_empty() {
            return Err(format!("{path}.source: at least one source is required"));
        }
        for source in &flow.sources {
            validate_source(source, &format!("{path}.source"))?;
        }
        if let Some(condition) = &flow.condition {
            validate_condition(condition, &format!("{path}.condition"))?;
        }
        if let Some(completion) = &flow.completion {
            validate_completion(completion, &format!("{path}.completion"))?;
        }
    }
    Ok(())
}

pub(crate) fn validate_object_flow(flow: &ObjectFlowMatcher, path: &str) -> Result<(), String> {
    validate_name_at(&flow.symbol, &format!("{path}.symbol"))?;
    if flow.sources.is_empty() {
        return Err(format!("{path}.source: at least one source is required"));
    }
    if flow.sources.len() > MAX_EXPRESSION_NODES {
        return Err(format!(
            "{path}.source exceeds {MAX_EXPRESSION_NODES} alternatives"
        ));
    }
    if flow.condition.is_none() {
        return Err(format!("{path}.configured_by: a condition is required"));
    }
    if flow.completion.is_none() {
        return Err(format!("{path}.complete_at: a completion is required"));
    }
    for (index, source) in flow.sources.iter().enumerate() {
        validate_source(source, &format!("{path}.source[{index}]"))?;
    }
    if let Some(condition) = &flow.condition {
        validate_condition(condition, &format!("{path}.condition"))?;
    }
    if let Some(completion) = &flow.completion {
        validate_completion(completion, &format!("{path}.completion"))?;
    }
    Ok(())
}

fn validate_source(source: &ObjectSourceMatcher, path: &str) -> Result<(), String> {
    validate_member_call(&source.call).map_err(|error| format!("{path}.call: {error}"))
}

fn validate_condition(condition: &FlowCondition, path: &str) -> Result<(), String> {
    let events = match condition {
        FlowCondition::AnyOf(events) | FlowCondition::AllOf(events) => events,
    };
    if events.is_empty() {
        return Err(format!("{path}: alternatives must not be empty"));
    }
    if events.len() > MAX_EXPRESSION_NODES {
        return Err(format!(
            "{path}: expression exceeds {MAX_EXPRESSION_NODES} events"
        ));
    }
    for (index, event) in events.iter().enumerate() {
        validate_event(event, &format!("{path}[{index}]"))?;
    }
    Ok(())
}

fn validate_event(event: &ObjectEventMatcher, path: &str) -> Result<(), String> {
    match event {
        ObjectEventMatcher::PropertyWrite { property, value } => {
            validate_name_at(property, &format!("{path}.property"))?;
            validate_value(value, &format!("{path}.value"))
        }
        ObjectEventMatcher::MemberCall { member, arguments } => {
            validate_name_at(member, &format!("{path}.member"))?;
            validate_arguments_at(arguments, path)
        }
    }
}

fn validate_completion(completion: &FlowCompletion, path: &str) -> Result<(), String> {
    match completion {
        FlowCompletion::Configuration => Ok(()),
        FlowCompletion::AnySink(sinks) => {
            if sinks.is_empty() {
                return Err(format!("{path}.any_sink: alternatives must not be empty"));
            }
            if sinks.len() > MAX_EXPRESSION_NODES {
                return Err(format!(
                    "{path}.any_sink exceeds {MAX_EXPRESSION_NODES} alternatives"
                ));
            }
            for (index, sink) in sinks.iter().enumerate() {
                let sink_path = format!("{path}.any_sink[{index}]");
                match sink {
                    FlowSinkMatcher::ArgumentOf { call, index } => {
                        validate_member_call_without_arguments(call, &sink_path)?;
                        validate_index_at(*index, &format!("{sink_path}.argument"))?;
                    }
                    FlowSinkMatcher::AnyArgumentOf { call } => {
                        validate_member_call_without_arguments(call, &sink_path)?;
                    }
                }
            }
            Ok(())
        }
    }
}

fn validate_member_call(call: &MemberCallMatcher) -> Result<(), String> {
    validate_chain(&call.chain, "member call chain")?;
    validate_member_provenance(&call.provenance)?;
    validate_arguments(&call.arguments)
}

fn validate_member_call_without_arguments(
    call: &MemberCallMatcher,
    path: &str,
) -> Result<(), String> {
    validate_chain_at(&call.chain, &format!("{path}.call"))?;
    validate_member_provenance_at(&call.provenance, &format!("{path}.call.provenance"))?;
    if !call.arguments.is_empty() {
        return Err(format!(
            "{path}.call: sink calls must not have argument predicates"
        ));
    }
    Ok(())
}

fn validate_arguments(arguments: &[ArgumentConstraint]) -> Result<(), String> {
    validate_arguments_at(arguments, "argument")
}

fn validate_arguments_at(arguments: &[ArgumentConstraint], path: &str) -> Result<(), String> {
    if arguments.len() > MAX_EXPRESSION_NODES {
        return Err(format!(
            "{path}: expression exceeds {MAX_EXPRESSION_NODES} arguments"
        ));
    }
    for argument in arguments {
        let argument_path = format!("{path}[{}]", argument.index);
        validate_index_at(argument.index, &argument_path)?;
        match &argument.matcher {
            ArgumentMatcher::Value(value) => {
                validate_value(value, &format!("{argument_path}.value"))?
            }
            ArgumentMatcher::ObjectKeys(keys) => {
                validate_non_empty_strings_at(keys, &format!("{argument_path}.object_keys"))?
            }
            ArgumentMatcher::RootedExpressions(chains) => {
                validate_non_empty_strings_at(
                    chains,
                    &format!("{argument_path}.rooted_expressions"),
                )?;
                for chain in chains {
                    validate_chain_at(chain, &format!("{argument_path}.rooted_expressions"))?;
                }
            }
        }
    }
    Ok(())
}

fn validate_value(value: &ValueMatcher, path: &str) -> Result<(), String> {
    if let ValueMatcherKind::StaticString(predicate) = &value.kind {
        match predicate {
            StaticStringPredicate::Any => Ok(()),
            StaticStringPredicate::Exact(values)
            | StaticStringPredicate::Prefix(values)
            | StaticStringPredicate::ContainsAny(values)
            | StaticStringPredicate::ContainsAll(values) => {
                validate_non_empty_strings_at(values, path)
            }
        }
    } else {
        Ok(())
    }
}

fn validate_name(value: &str, field: &str) -> Result<(), String> {
    validate_name_at(value, field)
}

fn validate_name_at(value: &str, field: &str) -> Result<(), String> {
    (!value.trim().is_empty())
        .then_some(())
        .ok_or_else(|| format!("{field} must not be empty"))
}

fn validate_non_empty_strings_at(values: &[String], field: &str) -> Result<(), String> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(format!("{field} must contain non-empty values"));
    }
    Ok(())
}

fn validate_chain(value: &str, field: &str) -> Result<(), String> {
    validate_chain_at(value, field)
}

fn validate_chain_at(value: &str, field: &str) -> Result<(), String> {
    validate_name_at(value, field)?;
    if value.trim().split('.').any(|part| part.trim().is_empty()) {
        return Err(format!("{field} contains an empty segment"));
    }
    Ok(())
}

fn validate_index_at(index: usize, field: &str) -> Result<(), String> {
    (index <= MAX_ARGUMENT_INDEX)
        .then_some(())
        .ok_or_else(|| format!("{field} index {index} exceeds {MAX_ARGUMENT_INDEX}"))
}

fn validate_provenance(value: &CallProvenance) -> Result<(), String> {
    validate_provenance_at(value, "provenance")
}

fn validate_provenance_at(value: &CallProvenance, path: &str) -> Result<(), String> {
    if let CallProvenance::ModuleExport { module } = value {
        validate_name_at(module, &format!("{path}.module"))?;
    }
    Ok(())
}

fn validate_member_provenance(value: &MemberCallProvenance) -> Result<(), String> {
    validate_member_provenance_at(value, "provenance")
}

fn validate_member_provenance_at(value: &MemberCallProvenance, path: &str) -> Result<(), String> {
    if let MemberCallProvenance::ModuleNamespace { module } = value {
        validate_name_at(module, &format!("{path}.module"))?;
    }
    Ok(())
}

fn validate_member_read_provenance(value: &MemberReadProvenance) -> Result<(), String> {
    if let MemberReadProvenance::ModuleNamespace { module } = value {
        validate_name(module, "module name")?;
    }
    Ok(())
}
