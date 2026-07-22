//! Validation of matcher invariants at the rule construction boundary.
//!
//! Validation rejects empty, malformed, dynamic, or over-sized declarations
//! before normalization and compilation. Error paths identify the matcher
//! field that failed so provider authors can correct the rule
//! deterministically.

use crate::{
    api::rule::{
        MatcherBuildError, ModuleSpecifierPattern,
        matcher::{
            ArgumentConstraint, ArgumentMatcherKind, ClassMatcher, ConstructorMatcher,
            FlowCompletion, FlowCompletionKind, FlowCondition, FlowConditionKind,
            FlowSinkMatcherKind, InstanceMemberCallMatcher, MemberCallMatcher,
            MemberCallProvenance, MemberReadMatcher, ObjectEventMatcher, ObjectEventMatcherKind,
            ObjectFlowMatcher, ObjectSourceMatcher, ReturnedMemberCallMatcher,
            ReturnedMemberReadMatcher, StaticStringPredicateKind, SymbolProvenance, ValueMatcher,
            ValueMatcherKind,
        },
    },
    rules::CallMatcher,
};

const MAX_ARGUMENT_INDEX: usize = 1 << 20;
const MAX_EXPRESSION_NODES: usize = 4096;

pub(super) fn validate_calls(calls: &[CallMatcher]) -> Result<(), MatcherBuildError> {
    for call in calls {
        validate_name_at(call.name(), "call name")?;
        call.provenance().validate_at("provenance")?;
        validate_arguments(call.arguments())?;
    }
    Ok(())
}

pub(super) fn validate_member_calls(calls: &[MemberCallMatcher]) -> Result<(), MatcherBuildError> {
    for call in calls {
        call.validate()?;
    }
    Ok(())
}

pub(super) fn validate_member_reads(reads: &[MemberReadMatcher]) -> Result<(), MatcherBuildError> {
    for read in reads {
        validate_chain(read.chain(), "member read chain")?;
        read.provenance().validate_at("module name")?;
    }
    Ok(())
}

pub(super) fn validate_literal_strings(values: &[String]) -> Result<(), MatcherBuildError> {
    for value in values {
        if value.trim().is_empty() {
            return Err(MatcherBuildError::InvalidModuleSpecifier(
                "literal matcher value must not be empty".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_package_imports(
    patterns: &[ModuleSpecifierPattern],
) -> Result<(), MatcherBuildError> {
    for pattern in patterns {
        if pattern.as_str().trim().is_empty() || !pattern.is_package() {
            return Err(MatcherBuildError::InvalidModuleSpecifier(
                "package import matcher must be a package pattern".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_classes(classes: &[ClassMatcher]) -> Result<(), MatcherBuildError> {
    for class in classes {
        validate_name_at(class.name(), "class name")?;
        class.provenance().validate_at("provenance")?;
        if matches!(class.provenance(), SymbolProvenance::Global) {
            return Err(MatcherBuildError::ConflictingProvenance);
        }
    }
    Ok(())
}

pub(super) fn validate_constructors(
    constructors: &[ConstructorMatcher],
) -> Result<(), MatcherBuildError> {
    for constructor in constructors {
        validate_name_at(constructor.name(), "constructor name")?;
        constructor.provenance().validate_at("provenance")?;
    }
    Ok(())
}

pub(super) fn validate_flows(flows: &[ObjectFlowMatcher]) -> Result<(), MatcherBuildError> {
    for flow in flows {
        validate_object_flow(flow, "flow")?;
    }
    Ok(())
}

pub(super) fn validate_returned_member_calls(
    values: &[ReturnedMemberCallMatcher],
) -> Result<(), MatcherBuildError> {
    for returned in values {
        validate_chain(returned.source(), "returned-member source")?;
        validate_name_at(returned.member(), "returned-member name")?;
    }
    Ok(())
}

pub(super) fn validate_returned_member_reads(
    values: &[ReturnedMemberReadMatcher],
) -> Result<(), MatcherBuildError> {
    for returned in values {
        validate_chain(returned.source(), "returned-member source")?;
        validate_name_at(returned.member(), "returned-member name")?;
    }
    Ok(())
}

pub(super) fn validate_instance_member_calls(
    values: &[InstanceMemberCallMatcher],
) -> Result<(), MatcherBuildError> {
    for instance in values {
        if instance.module_pattern().is_none() {
            validate_name_at(instance.module(), "instance module")?;
        }
        validate_name_at(instance.export(), "instance export")?;
        validate_name_at(instance.member(), "instance member")?;
    }
    Ok(())
}

/// Validate a complete object-flow lifecycle declaration.
pub fn validate_object_flow(flow: &ObjectFlowMatcher, path: &str) -> Result<(), MatcherBuildError> {
    validate_name_at(flow.symbol(), &format!("{path}.symbol"))?;
    if flow.sources().is_empty() {
        return Err(MatcherBuildError::EmptyChain);
    }
    if flow.sources().len() > MAX_EXPRESSION_NODES {
        return Err(MatcherBuildError::from(format!(
            "{path}.source exceeds {MAX_EXPRESSION_NODES} alternatives"
        )));
    }
    if flow.condition().is_none() {
        return Err(MatcherBuildError::MissingRequired);
    }
    if flow.completion().is_none() {
        return Err(MatcherBuildError::MissingRequired);
    }
    for (index, source) in flow.sources().iter().enumerate() {
        source.validate_at(&format!("{path}.source[{index}]"))?;
    }
    if let Some(condition) = flow.condition() {
        condition.validate_at(&format!("{path}.condition"))?;
    }
    if let Some(completion) = flow.completion() {
        completion.validate_at(&format!("{path}.completion"))?;
    }
    Ok(())
}

impl ObjectSourceMatcher {
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        self.call().validate_at(path)
    }
}

impl MemberCallMatcher {
    fn validate(&self) -> Result<(), MatcherBuildError> {
        validate_chain(self.chain(), "member call chain")?;
        self.provenance().validate_at("provenance")?;
        validate_arguments(self.arguments())
    }

    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        validate_chain_at(self.chain(), &format!("{path}.call"))?;
        self.provenance()
            .validate_at(&format!("{path}.call.provenance"))?;
        validate_arguments_at(self.arguments(), &format!("{path}.call.argument"))
    }

    fn validate_without_arguments_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        validate_chain_at(self.chain(), &format!("{path}.call"))?;
        self.provenance()
            .validate_at(&format!("{path}.call.provenance"))?;
        if !self.arguments().is_empty() {
            return Err(MatcherBuildError::from(format!(
                "{path}.call: sink calls must not have argument predicates"
            )));
        }
        Ok(())
    }
}

impl FlowCondition {
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        let events = match self.kind() {
            FlowConditionKind::AnyOf(events) | FlowConditionKind::AllOf(events) => events,
        };
        if events.is_empty() {
            return Err(MatcherBuildError::EmptyChain);
        }
        if events.len() > MAX_EXPRESSION_NODES {
            return Err(MatcherBuildError::from(format!(
                "{path}: expression exceeds {MAX_EXPRESSION_NODES} events"
            )));
        }
        for (index, event) in events.iter().enumerate() {
            event.validate_at(&format!("{path}[{index}]"))?;
        }
        Ok(())
    }
}

impl ObjectEventMatcher {
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        match self.kind() {
            ObjectEventMatcherKind::PropertyWrite { property, value } => {
                validate_name_at(property, &format!("{path}.property"))?;
                value.validate_at(&format!("{path}.value"))
            }
            ObjectEventMatcherKind::MemberCall { member, arguments } => {
                validate_name_at(member, &format!("{path}.member"))?;
                validate_arguments_at(arguments, path)
            }
        }
    }
}

impl FlowCompletion {
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        match self.kind() {
            FlowCompletionKind::Configuration => Ok(()),
            FlowCompletionKind::AnySink(sinks) => {
                if sinks.is_empty() {
                    return Err(MatcherBuildError::EmptyChain);
                }
                if sinks.len() > MAX_EXPRESSION_NODES {
                    return Err(MatcherBuildError::from(format!(
                        "{path}.any_sink exceeds {MAX_EXPRESSION_NODES} alternatives"
                    )));
                }
                for (index, sink) in sinks.iter().enumerate() {
                    let sink_path = format!("{path}.any_sink[{index}]");
                    match sink.kind() {
                        FlowSinkMatcherKind::ArgumentOf { call, index } => {
                            call.validate_without_arguments_at(&sink_path)?;
                            validate_index_at(*index, &format!("{sink_path}.argument"))?;
                        }
                        FlowSinkMatcherKind::AnyArgumentOf { call } => {
                            call.validate_without_arguments_at(&sink_path)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

fn validate_arguments(arguments: &[ArgumentConstraint]) -> Result<(), MatcherBuildError> {
    validate_arguments_at(arguments, "argument")
}

fn validate_arguments_at(
    arguments: &[ArgumentConstraint],
    path: &str,
) -> Result<(), MatcherBuildError> {
    if arguments.len() > MAX_EXPRESSION_NODES {
        return Err(MatcherBuildError::from(format!(
            "{path}: expression exceeds {MAX_EXPRESSION_NODES} arguments"
        )));
    }
    for argument in arguments {
        let argument_path = format!("{path}[{}]", argument.index());
        validate_index_at(argument.index(), &argument_path)?;
        argument.validate_at(&argument_path)?;
    }
    Ok(())
}

impl ValueMatcher {
    /// Validate the payload-specific invariants of a value predicate.
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        if let ValueMatcherKind::StaticString(predicate) = &self.kind {
            match &predicate.kind {
                StaticStringPredicateKind::Any => Ok(()),
                StaticStringPredicateKind::Exact(values)
                | StaticStringPredicateKind::Prefix(values)
                | StaticStringPredicateKind::ContainsAny(values)
                | StaticStringPredicateKind::ContainsAll(values) => {
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
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        validate_index_at(self.index(), path)?;
        match self.matcher().kind() {
            ArgumentMatcherKind::Value(value) => value.validate_at(&format!("{path}.value")),
            ArgumentMatcherKind::ObjectKeys(keys) => {
                validate_non_empty_strings_at(keys, &format!("{path}.object_keys"))
            }
            ArgumentMatcherKind::RootedExpressions(chains) => {
                let chain_path = format!("{path}.rooted_expressions");
                validate_non_empty_strings_at(chains, &chain_path)?;
                for chain in chains {
                    validate_chain_at(chain, &chain_path)?;
                }
                Ok(())
            }
            ArgumentMatcherKind::ObjectPropertyValue { property, value } => {
                validate_name_at(property, &format!("{path}.property"))?;
                value.validate_at(&format!("{path}.value"))
            }
        }
    }
}

fn validate_name_at(value: &str, _field: &str) -> Result<(), MatcherBuildError> {
    (!value.trim().is_empty())
        .then_some(())
        .ok_or(MatcherBuildError::MissingRequired)
}

fn validate_non_empty_strings_at(values: &[String], _field: &str) -> Result<(), MatcherBuildError> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(MatcherBuildError::MissingRequired);
    }
    Ok(())
}

fn validate_chain(value: &str, field: &str) -> Result<(), MatcherBuildError> {
    validate_chain_at(value, field)
}

fn validate_chain_at(value: &str, field: &str) -> Result<(), MatcherBuildError> {
    validate_name_at(value, field)?;
    if value.trim().split('.').any(|part| part.trim().is_empty()) {
        return Err(MatcherBuildError::EmptyChain);
    }
    Ok(())
}

fn validate_index_at(index: usize, _field: &str) -> Result<(), MatcherBuildError> {
    (index <= MAX_ARGUMENT_INDEX)
        .then_some(())
        .ok_or(MatcherBuildError::InvalidArgumentIndex(index))
}

impl SymbolProvenance {
    /// Validate module provenance while preserving the caller's error path.
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        if let Self::ModuleExport { module } = self {
            validate_name_at(module, &format!("{path}.module"))?;
        }
        Ok(())
    }
}

impl MemberCallProvenance {
    fn validate_at(&self, path: &str) -> Result<(), MatcherBuildError> {
        if let Self::ModuleNamespace { module } = self {
            validate_name_at(module, &format!("{path}.module"))?;
        }
        Ok(())
    }
}
