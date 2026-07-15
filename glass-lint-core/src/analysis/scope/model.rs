//! Structural scope graph types and collected alias facts.

use super::super::syntax::constant::ConstValue;
use super::super::syntax::{SymbolCallProvenance, SymbolMemberProvenance};
use super::super::value::{BindingId, BindingKey, BindingVersion, FunctionId, SymbolPath};
use std::collections::BTreeMap;
use swc_common::{BytePos, Span};

#[derive(Debug, Default, Clone)]
pub(in crate::analysis) struct ScopeGraph {
    environment: crate::Environment,
    scopes: Vec<AliasScope>,
    scopes_by_start: Vec<usize>,
    assignments: BTreeMap<usize, BTreeMap<String, Vec<AliasAssignment>>>,
    binding_ids: BTreeMap<(usize, String), BindingId>,
    function_ids: BTreeMap<usize, FunctionId>,
    function_bindings: BTreeMap<(usize, String), FunctionId>,
    function_aliases: BTreeMap<(usize, String), FunctionId>,
    property_assignments: BTreeMap<(BindingKey, Vec<String>), Vec<PropertyAliasFact>>,
    rooted_property_mutations: BTreeMap<String, Vec<RootedPropertyMutationFact>>,
    parameter_aliases: BTreeMap<(FunctionId, String), BindingProvenance>,
    dynamic_evals: Vec<(usize, Span)>,
    mutable_static_objects: std::collections::BTreeSet<(usize, String)>,
}

impl ScopeGraph {
    pub(super) fn finish_collected_properties(
        &mut self,
        property_assignments: Vec<super::collect::PropertyAliasAssignment>,
        rooted_mutations: Vec<super::collect::RootedPropertyMutation>,
        dynamic_evals: Vec<(usize, Span)>,
    ) {
        for assignment in property_assignments {
            let Some(receiver_key) = self
                .binding_key_for_name(assignment.receiver.sym.as_ref(), assignment.receiver.span)
            else {
                continue;
            };
            self.property_assignments
                .entry((
                    receiver_key,
                    assignment
                        .property
                        .strip_prefix(assignment.receiver.sym.as_ref())
                        .and_then(|path| path.strip_prefix('.'))
                        .map(|path| path.split('.').map(str::to_string).collect::<Vec<_>>())
                        .unwrap_or_default(),
                ))
                .or_default()
                .push(PropertyAliasFact {
                    span: assignment.span,
                    scope: assignment.scope,
                    target: assignment.target.map(std::convert::Into::into),
                });
        }
        for assignments in self.property_assignments.values_mut() {
            assignments.sort_by_key(|assignment| assignment.span.lo);
        }
        for mutation in rooted_mutations {
            self.rooted_property_mutations
                .entry(mutation.receiver)
                .or_default()
                .push(RootedPropertyMutationFact {
                    span: mutation.span,
                    scope: mutation.scope,
                    property: mutation.property,
                });
        }
        for mutations in self.rooted_property_mutations.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span.lo);
        }
        self.dynamic_evals = dynamic_evals
            .into_iter()
            .filter(|(_, span)| self.binding_at("eval", *span).is_none())
            .collect();
    }

    pub(super) fn is_global(&self, name: &str) -> bool {
        self.environment.is_global(name)
    }
    pub(super) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.environment.is_global_member(root, member)
    }
    pub(super) fn assignment_at(
        &self,
        scope: usize,
        name: &str,
        span: Span,
    ) -> Option<&AliasAssignment> {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .and_then(|index| assignments.get(index))
            })
    }
    pub(super) fn binding_id_at(&self, scope: usize, name: &str) -> Option<BindingId> {
        self.binding_ids.get(&(scope, name.to_string())).copied()
    }
    pub(super) fn parameter_alias_for(
        &self,
        scope: usize,
        name: &str,
    ) -> Option<&BindingProvenance> {
        self.function_ids
            .get(&scope)
            .and_then(|function| self.parameter_aliases.get(&(*function, name.to_string())))
    }
    pub(super) fn scope_parent(&self, scope: usize) -> Option<usize> {
        self.scopes.get(scope)?.parent
    }
    pub(super) fn scope_kind(&self, scope: usize) -> Option<ScopeKind> {
        self.scopes.get(scope).map(|scope| scope.kind)
    }
    pub(super) fn scope_span(&self, scope: usize) -> Option<Span> {
        self.scopes.get(scope).map(|scope| scope.span)
    }
    pub(super) fn scope_binding(&self, scope: usize, name: &str) -> Option<&BindingProvenance> {
        self.scopes.get(scope)?.bindings.get(name)
    }

    pub(super) fn from_parts(parts: ScopeGraphParts) -> Self {
        Self {
            environment: parts.environment,
            scopes: parts.scopes,
            scopes_by_start: parts.scopes_by_start,
            assignments: parts.assignments,
            binding_ids: parts.binding_ids,
            function_ids: parts.function_ids,
            function_bindings: parts.function_bindings,
            function_aliases: parts.function_aliases,
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            parameter_aliases: parts.parameter_aliases,
            dynamic_evals: Vec::new(),
            mutable_static_objects: parts.mutable_static_objects,
        }
    }

    pub(super) fn function_for_scope(&self, scope: usize) -> Option<FunctionId> {
        self.function_ids.get(&scope).copied()
    }
    pub(super) fn function_spans(&self) -> impl Iterator<Item = (FunctionId, Span)> + '_ {
        self.function_ids.iter().filter_map(|(scope, function)| {
            self.scopes.get(*scope).map(|scope| (*function, scope.span))
        })
    }
    pub(super) fn function_binding(&self, scope: usize, name: &str) -> Option<FunctionId> {
        self.function_bindings
            .get(&(scope, name.to_string()))
            .copied()
    }
    pub(super) fn function_alias(&self, scope: usize, name: &str) -> Option<FunctionId> {
        self.function_aliases
            .get(&(scope, name.to_string()))
            .copied()
    }
    pub(super) fn reassigned_between(
        &self,
        scope: usize,
        name: &str,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .is_some_and(|assignments| {
                assignments
                    .iter()
                    .any(|assignment| assignment.span.lo > start && assignment.span.lo <= end)
            })
    }
    pub(super) fn binding_version(&self, scope: usize, name: &str, span: Span) -> BindingVersion {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .and_then(|index| assignments.get(index))
            })
            .map_or(BindingVersion(0), |assignment| assignment.version)
    }
    pub(super) fn property_aliases(
        &self,
        key: &(BindingKey, Vec<String>),
    ) -> Option<&[PropertyAliasFact]> {
        self.property_assignments.get(key).map(Vec::as_slice)
    }
    pub(super) fn rooted_mutations(&self, root: &str) -> Option<&[RootedPropertyMutationFact]> {
        self.rooted_property_mutations.get(root).map(Vec::as_slice)
    }
    pub(super) fn is_mutable_static_object(&self, scope: usize, name: &str) -> bool {
        self.mutable_static_objects
            .contains(&(scope, name.to_string()))
    }
    pub(super) fn has_eval_after(&self, scope: usize, span: Span) -> bool {
        self.dynamic_evals.iter().any(|(eval_scope, eval_span)| {
            span.lo > eval_span.hi && self.scope_is_within(scope, *eval_scope)
        })
    }
    pub(super) fn scope_at(&self, span: Span) -> usize {
        let position = self
            .scopes_by_start
            .partition_point(|index| self.scopes[*index].span.lo <= span.lo);
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return 0;
        };
        while !span_contains(self.scopes[scope].span, span) {
            let Some(parent) = self.scopes[scope].parent else {
                return 0;
            };
            scope = parent;
        }
        scope
    }
}

pub(super) struct ScopeGraphParts {
    pub(super) environment: crate::Environment,
    pub(super) scopes: Vec<AliasScope>,
    pub(super) scopes_by_start: Vec<usize>,
    pub(super) assignments: BTreeMap<usize, BTreeMap<String, Vec<AliasAssignment>>>,
    pub(super) binding_ids: BTreeMap<(usize, String), BindingId>,
    pub(super) function_ids: BTreeMap<usize, FunctionId>,
    pub(super) function_bindings: BTreeMap<(usize, String), FunctionId>,
    pub(super) function_aliases: BTreeMap<(usize, String), FunctionId>,
    pub(super) parameter_aliases: BTreeMap<(FunctionId, String), BindingProvenance>,
    pub(super) mutable_static_objects: std::collections::BTreeSet<(usize, String)>,
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.lo <= inner.lo && outer.hi >= inner.hi
}

#[derive(Debug, Clone)]
pub(in crate::analysis::scope) struct RootedPropertyMutationFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: usize,
    pub(in crate::analysis::scope) property: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct AliasScope {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) depth: usize,
    pub(in crate::analysis::scope) kind: ScopeKind,
    pub(in crate::analysis::scope) parent: Option<usize>,
    pub(in crate::analysis::scope) bindings: BTreeMap<String, BindingProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum ScopeKind {
    Program,
    Function,
    Block,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum BindingProvenance {
    Local,
    ValueAlias {
        target: SymbolPath,
    },
    BoundCallable {
        target: SymbolPath,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    BoundModuleCallable {
        module: String,
        export: String,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    ReturnedObject {
        source: SymbolPath,
    },
    ModuleExport {
        module: String,
        export: String,
    },
    ModuleNamespace {
        module: String,
    },
    StaticString(String),
    StaticNumber(usize),
    StaticStringArray(Vec<String>),
    StaticObjectKeys(Vec<String>),
    StaticObjectValues(BTreeMap<String, SymbolPath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum BoundArgument {
    StaticString(String),
    RootedExpression(SymbolPath),
}

/// The collection boundary between lexical analysis and value interning.
///
/// Scope collection may use its compact declaration/assignment representation
/// internally, but the resolver receives one typed snapshot for each node. It
/// therefore does not need to reinterpret `BindingProvenance` while building
/// the authoritative value arena.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct IdentValueSeed {
    pub(in crate::analysis) call: SymbolCallProvenance,
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) constant: ConstValue,
    pub(in crate::analysis) bound_arguments: Option<Vec<Option<BoundArgument>>>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct MemberValueSeed {
    pub(in crate::analysis) syntactic_chain: Option<SymbolPath>,
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) module_member: Option<SymbolMemberProvenance>,
    pub(in crate::analysis) returned_member: Option<(SymbolPath, String)>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct AliasAssignment {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: usize,
    pub(in crate::analysis::scope) name: String,
    pub(in crate::analysis::scope) version: BindingVersion,
    pub(in crate::analysis::scope) provenance: BindingProvenance,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct PropertyAliasFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: usize,
    pub(in crate::analysis::scope) target: Option<SymbolPath>,
}
