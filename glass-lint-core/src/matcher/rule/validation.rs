//! Validation of normalized matcher invariants.

use super::matcher::{
    ApiMatcher, ArgStringMatcher, CallProvenance, FlowMatcher, FlowRequirement, FlowSinkArgs,
    FlowValueMatcher, MemberCallProvenance, MemberReadProvenance,
};

const MAX_ARGUMENT_INDEX: usize = 1 << 20;

pub(super) fn validate(matcher: &ApiMatcher) -> Result<(), String> {
    for call in &matcher.calls {
        validate_name(&call.name, "call name")?;
        validate_provenance(&call.provenance)?;
        validate_arg_strings(&call.arg_strings)?;
    }
    for call in &matcher.member_calls {
        validate_chain(&call.chain, "member call chain")?;
        validate_member_provenance(&call.provenance)?;
        validate_arg_strings(&call.arg_strings)?;
        for argument in &call.arg_object_keys {
            validate_index(argument.index)?;
            validate_non_empty_strings(&argument.keys, "object-key predicate")?;
        }
        for argument in &call.arg_rooted_exprs {
            validate_index(argument.index)?;
            if argument.chains.is_empty() {
                return Err("rooted-expression predicate has no chains".into());
            }
            for chain in &argument.chains {
                validate_chain(chain, "rooted-expression chain")?;
            }
        }
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
        validate_flow(flow)?;
    }
    Ok(())
}

fn validate_name(value: &str, field: &str) -> Result<(), String> {
    (!value.trim().is_empty())
        .then_some(())
        .ok_or_else(|| format!("{field} must not be empty"))
}

fn validate_non_empty_strings(values: &[String], field: &str) -> Result<(), String> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(format!("{field} must contain non-empty values"));
    }
    Ok(())
}

fn validate_chain(value: &str, field: &str) -> Result<(), String> {
    validate_name(value, field)?;
    if value.trim().split('.').any(|part| part.trim().is_empty()) {
        return Err(format!("{field} contains an empty segment"));
    }
    Ok(())
}

fn validate_index(index: usize) -> Result<(), String> {
    (index <= MAX_ARGUMENT_INDEX)
        .then_some(())
        .ok_or_else(|| format!("argument index {index} exceeds {MAX_ARGUMENT_INDEX}"))
}

fn validate_arg_strings(values: &[ArgStringMatcher]) -> Result<(), String> {
    for matcher in values {
        validate_index(matcher.index)?;
        if let Some(predicate) = &matcher.predicate {
            validate_flow_value(predicate)?;
        } else if matcher.values.iter().any(|value| value.trim().is_empty()) {
            return Err("argument string matcher contains an empty value".into());
        }
    }
    Ok(())
}

fn validate_flow_value(value: &FlowValueMatcher) -> Result<(), String> {
    match value {
        FlowValueMatcher::Any => Ok(()),
        FlowValueMatcher::StaticExact(values)
        | FlowValueMatcher::StaticPrefix(values)
        | FlowValueMatcher::StaticContainsAny(values)
        | FlowValueMatcher::StaticContainsAll(values) => {
            validate_non_empty_strings(values, "flow predicate")
        }
    }
}

fn validate_provenance(value: &CallProvenance) -> Result<(), String> {
    if let CallProvenance::ModuleExport { module } = value {
        validate_name(module, "module name")?;
    }
    Ok(())
}

fn validate_member_provenance(value: &MemberCallProvenance) -> Result<(), String> {
    if let MemberCallProvenance::ModuleNamespace { module } = value {
        validate_name(module, "module name")?;
    }
    Ok(())
}

fn validate_member_read_provenance(value: &MemberReadProvenance) -> Result<(), String> {
    if let MemberReadProvenance::ModuleNamespace { module } = value {
        validate_name(module, "module name")?;
    }
    Ok(())
}

fn validate_flow(flow: &FlowMatcher) -> Result<(), String> {
    validate_name(&flow.symbol, "flow symbol")?;
    if flow.sources.is_empty() {
        return Err("flow must define a source".into());
    }
    if flow.requirements.is_empty() {
        return Err("flow must define a requirement".into());
    }
    if !flow.emit_on_requirements && flow.sinks.is_empty() {
        return Err("flow must define a sink unless it emits on requirements".into());
    }
    for source in &flow.sources {
        validate_chain(&source.member_call, "flow source")?;
        validate_arg_strings(&source.arg_strings)?;
    }
    for requirement in &flow.requirements {
        match requirement {
            FlowRequirement::PropertyWrite { property, value } => {
                validate_name(property, "flow property")?;
                validate_flow_value(value)?;
            }
            FlowRequirement::MemberCall { member, args } => {
                validate_name(member, "flow member")?;
                for arg in args {
                    validate_index(arg.index)?;
                    validate_flow_value(&arg.value)?;
                }
            }
        }
    }
    for sink in &flow.sinks {
        if sink.member_calls.is_empty() {
            return Err("flow sink must define a member call".into());
        }
        for member in &sink.member_calls {
            validate_chain(member, "flow sink")?;
        }
        if let FlowSinkArgs::Indices(indices) = &sink.args {
            if indices.is_empty() {
                return Err("flow sink index set must not be empty".into());
            }
            for index in indices {
                validate_index(*index)?;
            }
        }
    }
    Ok(())
}
