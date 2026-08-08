//! Immutable matcher compilation and catalog selection.
//!
//! Compilation translates validated public matcher declarations once. The
//! resulting plans are provider-neutral and can be projected onto many files
//! without rebuilding matcher semantics.
//!
//! Query compiler vocabulary:
//! - logical operators are typed events, `any`, same-event `all`, and lifecycle
//!   stages;
//! - relations are identity, event kind, arguments, returned/instance subject,
//!   and path-local lifecycle correlation;
//! - certainty is possible or definite and is reduced by incomplete paths;
//! - correlation keys are validated variables or one tracked lifecycle object,
//!   never source spelling;
//! - every root has a finite limit and explicit preparation requirements;
//! - evidence is emitted by the primary event selected by the declaration; and
//! - physical planning chooses an indexed, constrained, subject, or lifecycle
//!   operator and then canonicalizes the root set.
//!
//! To add a provider-neutral relation, extend the declaration and validation
//! layers first, add normalization and requirements, then reuse an existing
//! executor owner where possible. Add a specialized physical operator only
//! when its access path cannot be expressed by an existing root. Required
//! tests cover positive behavior, adversarial identity/path cases, bounds,
//! certainty, evidence ordering, and operation counts.

#![allow(clippy::redundant_pub_crate)]

pub(crate) mod catalog;
pub(crate) mod contradiction;
pub(crate) mod error;
pub(crate) mod normalize;
pub(crate) mod normalize_all;
pub(crate) mod normalized;
pub(crate) mod object_flow;
pub(crate) mod physical;
#[cfg(test)]
pub(crate) mod reference;
pub(crate) mod requirements;
pub(crate) mod rule;
pub(crate) mod validate;

#[cfg(test)]
mod tests;

pub(crate) use catalog::compile_records;
use glass_lint_datastructures::SymbolPath;
pub(crate) use object_flow::CompiledObjectFlow;
pub(crate) use rule::{CompiledRuleRecord, CompiledRuleSelection};
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    compiler::{
        normalized::NormalizedQuery, physical::PhysicalPlan, validate::validate_query_decl,
    },
    rule::{
        MatcherBuildError, ModuleSpecifierPattern, QueryDiagnostic,
        query::{EventSpec, IdentitySpec, QueryDecl},
    },
};

/// Canonical compiled matcher plan consumed by analysis.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    physical_plan: PhysicalPlan,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum IdentityStrength {
    Strict,
    Heuristic,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum IdentityConstraint {
    Any {
        name: SmolStr,
        strength: IdentityStrength,
    },
    Global {
        name: SmolStr,
        strength: IdentityStrength,
    },
    ModuleExport {
        module: SmolStr,
        export: SmolStr,
    },
    PackageModuleExport {
        module: ModuleSpecifierPattern,
        export: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    PackageModuleNamespace {
        module: ModuleSpecifierPattern,
    },
    Rooted {
        path: SymbolPath,
    },
    LiteralString {
        predicate: String,
    },
    PackageSpecifier {
        pattern: ModuleSpecifierPattern,
    },
}

impl IdentityConstraint {
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Any { name, .. } | Self::Global { name, .. } => name.is_empty(),
            Self::ModuleExport { module, export } => module.is_empty() || export.is_empty(),
            Self::PackageModuleExport { module, export } => {
                module.as_str().is_empty() || export.is_empty()
            }
            Self::ModuleNamespace { module } => module.is_empty(),
            Self::PackageModuleNamespace { module } => module.as_str().is_empty(),
            Self::Rooted { path } => path.is_empty(),
            Self::LiteralString { predicate } => predicate.is_empty(),
            Self::PackageSpecifier { pattern } => pattern.as_str().is_empty(),
        }
    }

    pub(crate) fn root_or_descendant_matches(
        &self,
        source: &SymbolPath,
        environment: &crate::Environment,
    ) -> bool {
        matches!(self, Self::Rooted { path } if environment.global_object_paths_match(path, source)
            || source.is_equal_or_descendant_of(path))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum EventPredicate {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    PropertyWrite { property: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct EvidenceDescriptor {
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

// ── Lowering: declaration types → compiler IR ────────────────────────────

pub(crate) fn lower_identity(spec: &IdentitySpec) -> IdentityConstraint {
    match spec {
        IdentitySpec::Global { name } => IdentityConstraint::Global {
            name: name.clone(),
            strength: IdentityStrength::Strict,
        },
        IdentitySpec::Heuristic { name } => IdentityConstraint::Any {
            name: name.clone(),
            strength: IdentityStrength::Heuristic,
        },
        IdentitySpec::ModuleExport { module, export } => IdentityConstraint::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        },
        IdentitySpec::PackageModuleExport { module, export } => {
            IdentityConstraint::PackageModuleExport {
                module: module.clone(),
                export: export.clone(),
            }
        }
        IdentitySpec::ModuleNamespace { module } => IdentityConstraint::ModuleNamespace {
            module: module.clone(),
        },
        IdentitySpec::PackageModuleNamespace { module } => {
            IdentityConstraint::PackageModuleNamespace {
                module: module.clone(),
            }
        }
        IdentitySpec::Rooted { path } => IdentityConstraint::Rooted { path: path.clone() },
        IdentitySpec::LiteralString { predicate } => IdentityConstraint::LiteralString {
            predicate: predicate.clone(),
        },
        IdentitySpec::PackageSpecifier { pattern } => IdentityConstraint::PackageSpecifier {
            pattern: pattern.clone(),
        },
    }
}

pub(crate) fn lower_event(spec: &EventSpec) -> EventPredicate {
    match spec {
        EventSpec::Call => EventPredicate::Call,
        EventSpec::Construct => EventPredicate::Construct,
        EventSpec::MemberCall { member } => EventPredicate::MemberCall {
            member: member.clone(),
        },
        EventSpec::MemberRead { member } => EventPredicate::MemberRead {
            member: member.clone(),
        },
        EventSpec::PropertyWrite { property } => EventPredicate::PropertyWrite {
            property: property.clone(),
        },
        EventSpec::ClassReference => EventPredicate::ClassReference,
        EventSpec::Import => EventPredicate::Import,
        EventSpec::StringReference => EventPredicate::StringReference,
    }
}

struct QueryPlanAccumulator {
    roots: Vec<physical::PhysicalRoot>,
    requirements: requirements::PlanRequirements,
}

impl QueryPlanAccumulator {
    fn add(&mut self, query_plan: &PhysicalPlan) {
        self.roots.extend(query_plan.roots().iter().cloned());
        self.requirements.merge_from(query_plan.requirements());
    }

    fn finish(self) -> Result<PhysicalPlan, MatcherBuildError> {
        PhysicalPlan::try_new(physical::optimize_roots(self.roots), &self.requirements)
            .map_err(|error| MatcherBuildError::InvalidPhysicalPlan(error.to_string()))
    }
}

/// Compile one query declaration through validation, normalization, and
/// physical planning without mutating the aggregate rule plan.
fn compile_query(query: &QueryDecl) -> Result<PhysicalPlan, MatcherBuildError> {
    validate_query_decl(query).map_err(map_query_compile_error)?;

    let normalized: NormalizedQuery =
        normalize::normalize_query_decl(query).map_err(map_query_compile_error)?;

    physical::plan_normalized(&normalized)
        .map_err(|error| MatcherBuildError::InvalidPhysicalPlan(error.to_string()))
}

/// Compile query declarations into one deterministic, aggregate physical plan.
fn compile_queries(queries: &[QueryDecl]) -> Result<PhysicalPlan, MatcherBuildError> {
    let mut accumulator = QueryPlanAccumulator {
        roots: Vec::new(),
        requirements: requirements::PlanRequirements::default(),
    };

    for query in queries {
        accumulator.add(&compile_query(query)?);
    }

    accumulator.finish()
}

fn map_query_compile_error(error: validate::QueryCompileError) -> MatcherBuildError {
    match error {
        validate::QueryCompileError::IncompleteSameEvent { missing } => {
            MatcherBuildError::CompilerInvariant(format!("same-event merge missing {missing}"))
        }
        validate::QueryCompileError::InternalInvariant { detail } => {
            MatcherBuildError::CompilerInvariant(detail)
        }
        error => MatcherBuildError::QueryCompileError(QueryDiagnostic::new(
            error.diagnostic_name(),
            error.to_string(),
        )),
    }
}

impl CompiledMatcherPlan {
    pub(crate) fn physical_roots(&self) -> &[physical::PhysicalRoot] {
        self.physical_plan.roots()
    }

    /// Explain the canonical executable plan for tests and profiling.
    #[cfg(test)]
    pub(crate) fn plan_explanation(&self) -> String {
        self.physical_plan.explain()
    }

    pub(crate) fn needs_project_overlay(&self) -> bool {
        self.physical_plan.requirements().needs_project_overlay()
    }

    pub(crate) fn needs_module_identities(&self) -> bool {
        self.physical_plan.requirements().needs_module_identities()
    }

    pub(crate) fn needs_call_result_identities(&self) -> bool {
        self.physical_plan
            .requirements()
            .needs_call_result_identities()
    }

    pub(crate) fn flow_requirements(&self) -> &requirements::FlowRequirements {
        self.physical_plan.requirements().flow()
    }

    pub(crate) fn compile(queries: &[QueryDecl]) -> Result<Self, MatcherBuildError> {
        let physical_plan = compile_queries(queries)?;
        Ok(Self { physical_plan })
    }
}
