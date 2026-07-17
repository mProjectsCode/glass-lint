//! Validation of matcher invariants at the rule construction boundary.
//!
//! Validation rejects empty, malformed, dynamic, or over-sized declarations
//! before normalization and compilation. Error paths identify the matcher
//! field that failed so provider authors can correct the rule
//! deterministically.

use super::matcher::{
    ApiMatcher, ArgumentConstraint, ArgumentMatcher, CallProvenance, FlowCompletion, FlowCondition,
    FlowSinkMatcher, Matcher, MemberCallMatcher, MemberCallProvenance, MemberReadProvenance,
    ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, StaticStringPredicate,
    ValueMatcher, ValueMatcherKind,
};

const MAX_ARGUMENT_INDEX: usize = 1 << 20;
const MAX_EXPRESSION_NODES: usize = 4096;

/// Validate all matcher families in one assembled API matcher.
pub(super) fn validate(matcher: &ApiMatcher) -> Result<(), String> {
    for call in &matcher.calls {
        validate_name_at(&call.name, "call name")?;
        call.provenance.validate_at("provenance")?;
        validate_arguments(&call.arguments)?;
    }
    for call in &matcher.member_calls {
        call.validate()?;
    }
    for read in &matcher.member_reads {
        validate_chain(&read.chain, "member read chain")?;
        read.provenance.validate_at("module name")?;
    }
    for value in matcher.imports.iter().chain(&matcher.string_literals) {
        if value.trim().is_empty() {
            return Err("literal matcher value must not be empty".into());
        }
    }
    for class in &matcher.classes {
        validate_name_at(&class.name, "class name")?;
        class.provenance.validate_at("provenance")?;
        if matches!(class.provenance, CallProvenance::Global) {
            return Err("class provenance cannot be global".into());
        }
    }
    for constructor in &matcher.constructors {
        validate_name_at(&constructor.name, "constructor name")?;
        constructor.provenance.validate_at("provenance")?;
    }
    for returned in &matcher.returned_member_calls {
        validate_chain(&returned.source, "returned-member source")?;
        validate_name_at(&returned.member, "returned-member name")?;
    }
    for returned in &matcher.returned_member_reads {
        validate_chain(&returned.source, "returned-member source")?;
        validate_name_at(&returned.member, "returned-member name")?;
    }
    for instance in &matcher.instance_member_calls {
        validate_name_at(&instance.module, "instance module")?;
        validate_name_at(&instance.export, "instance export")?;
        validate_name_at(&instance.member, "instance member")?;
    }
    for flow in &matcher.flows {
        validate_object_flow(flow, "flow")?;
    }
    Ok(())
}

/// Validate one matcher while preserving its catalog position in errors.
pub fn validate_matcher_at(matcher: &Matcher, index: usize) -> Result<(), String> {
    if let Matcher::ObjectFlow(flow) = matcher {
        let path = format!("matcher[{index}].flow");
        validate_name_at(&flow.symbol, &format!("{path}.symbol"))?;
        if flow.sources.is_empty() {
            return Err(format!("{path}.source: at least one source is required"));
        }
        for source in &flow.sources {
            source.validate_at(&format!("{path}.source"))?;
        }
        if let Some(condition) = &flow.condition {
            condition.validate_at(&format!("{path}.condition"))?;
        }
        if let Some(completion) = &flow.completion {
            completion.validate_at(&format!("{path}.completion"))?;
        }
    }
    Ok(())
}

/// Validate a complete object-flow lifecycle declaration.
pub fn validate_object_flow(flow: &ObjectFlowMatcher, path: &str) -> Result<(), String> {
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
        source.validate_at(&format!("{path}.source[{index}]"))?;
    }
    if let Some(condition) = &flow.condition {
        condition.validate_at(&format!("{path}.condition"))?;
    }
    if let Some(completion) = &flow.completion {
        completion.validate_at(&format!("{path}.completion"))?;
    }
    Ok(())
}

impl ObjectSourceMatcher {
    fn validate_at(&self, path: &str) -> Result<(), String> {
        self.call.validate_at(path)
    }
}

impl MemberCallMatcher {
    fn validate(&self) -> Result<(), String> {
        validate_chain(&self.chain, "member call chain")?;
        self.provenance.validate_at("provenance")?;
        validate_arguments(&self.arguments)
    }

    fn validate_at(&self, path: &str) -> Result<(), String> {
        validate_chain_at(&self.chain, &format!("{path}.call"))?;
        self.provenance
            .validate_at(&format!("{path}.call.provenance"))?;
        validate_arguments_at(&self.arguments, &format!("{path}.call.argument"))
    }

    fn validate_without_arguments_at(&self, path: &str) -> Result<(), String> {
        validate_chain_at(&self.chain, &format!("{path}.call"))?;
        self.provenance
            .validate_at(&format!("{path}.call.provenance"))?;
        if !self.arguments.is_empty() {
            return Err(format!(
                "{path}.call: sink calls must not have argument predicates"
            ));
        }
        Ok(())
    }
}

impl FlowCondition {
    fn validate_at(&self, path: &str) -> Result<(), String> {
        let events = match self {
            Self::AnyOf(events) | Self::AllOf(events) => events,
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
            event.validate_at(&format!("{path}[{index}]"))?;
        }
        Ok(())
    }
}

impl ObjectEventMatcher {
    fn validate_at(&self, path: &str) -> Result<(), String> {
        match self {
            Self::PropertyWrite { property, value } => {
                validate_name_at(property, &format!("{path}.property"))?;
                value.validate_at(&format!("{path}.value"))
            }
            Self::MemberCall { member, arguments } => {
                validate_name_at(member, &format!("{path}.member"))?;
                validate_arguments_at(arguments, path)
            }
        }
    }
}

impl FlowCompletion {
    fn validate_at(&self, path: &str) -> Result<(), String> {
        match self {
            Self::Configuration => Ok(()),
            Self::AnySink(sinks) => {
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
                            call.validate_without_arguments_at(&sink_path)?;
                            validate_index_at(*index, &format!("{sink_path}.argument"))?;
                        }
                        FlowSinkMatcher::AnyArgumentOf { call } => {
                            call.validate_without_arguments_at(&sink_path)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
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
        argument.validate_at(&argument_path)?;
    }
    Ok(())
}

impl ValueMatcher {
    /// Validate the payload-specific invariants of a value predicate.
    fn validate_at(&self, path: &str) -> Result<(), String> {
        if let ValueMatcherKind::StaticString(predicate) = &self.kind {
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
}

impl ArgumentConstraint {
    /// Validate one indexed argument predicate and retain its path context.
    fn validate_at(&self, path: &str) -> Result<(), String> {
        validate_index_at(self.index, path)?;
        match &self.matcher {
            ArgumentMatcher::Value(value) => value.validate_at(&format!("{path}.value")),
            ArgumentMatcher::ObjectKeys(keys) => {
                validate_non_empty_strings_at(keys, &format!("{path}.object_keys"))
            }
            ArgumentMatcher::RootedExpressions(chains) => {
                let chain_path = format!("{path}.rooted_expressions");
                validate_non_empty_strings_at(chains, &chain_path)?;
                for chain in chains {
                    validate_chain_at(chain, &chain_path)?;
                }
                Ok(())
            }
        }
    }
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

impl CallProvenance {
    /// Validate module provenance while preserving the caller's error path.
    fn validate_at(&self, path: &str) -> Result<(), String> {
        if let Self::ModuleExport { module } = self {
            validate_name_at(module, &format!("{path}.module"))?;
        }
        Ok(())
    }
}

impl MemberCallProvenance {
    fn validate_at(&self, path: &str) -> Result<(), String> {
        if let Self::ModuleNamespace { module } = self {
            validate_name_at(module, &format!("{path}.module"))?;
        }
        Ok(())
    }
}

impl MemberReadProvenance {
    fn validate_at(&self, path: &str) -> Result<(), String> {
        if let Self::ModuleNamespace { module } = self {
            validate_name_at(module, path)?;
        }
        Ok(())
    }
}
