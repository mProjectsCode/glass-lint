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
pub(crate) mod limits;
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
        CompilerInvariantDiagnostic, MatcherBuildError, ModuleSpecifierPattern, QueryDiagnostic,
        query::{IdentitySpec, QueryDecl},
    },
};

/// Canonical compiled matcher plan consumed by analysis.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    physical_plan: PhysicalPlan,
}
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum IdentityConstraint {
    Any {
        name: SmolStr,
    },
    Global {
        name: SmolStr,
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
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct EvidenceDescriptor {
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

// ── Lowering: declaration types → compiler IR ────────────────────────────

pub(crate) fn lower_identity(spec: &IdentitySpec) -> IdentityConstraint {
    match spec {
        IdentitySpec::Global { name } => IdentityConstraint::Global { name: name.clone() },
        IdentitySpec::Heuristic { name } => IdentityConstraint::Any { name: name.clone() },
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

struct QueryPlanAccumulator {
    roots: Vec<physical::PhysicalRoot>,
    budget: physical::RootBudget,
}

impl QueryPlanAccumulator {
    fn finish(self) -> Result<PhysicalPlan, MatcherBuildError> {
        PhysicalPlan::from_roots(physical::optimize_roots(self.roots))
            .map_err(|error| MatcherBuildError::InvalidPhysicalPlan(error.into()))
    }
}

/// Compile query declarations into one deterministic, aggregate physical plan.
fn compile_queries(queries: &[QueryDecl]) -> Result<PhysicalPlan, MatcherBuildError> {
    let mut accumulator = QueryPlanAccumulator {
        roots: Vec::new(),
        budget: physical::RootBudget::new(),
    };

    for query in queries {
        validate_query_decl(query).map_err(map_query_compile_error)?;
        let normalized: NormalizedQuery =
            normalize::normalize_query_decl(query).map_err(map_query_compile_error)?;
        physical::plan_normalized_roots_into(
            &normalized,
            &mut accumulator.budget,
            &mut accumulator.roots,
        )
        .map_err(|error| MatcherBuildError::InvalidPhysicalPlan(error.into()))?;
    }

    accumulator.finish()
}

fn map_query_compile_error(error: validate::QueryCompileError) -> MatcherBuildError {
    match error {
        validate::QueryCompileError::IncompleteSameEvent { missing } => {
            MatcherBuildError::CompilerInvariant(CompilerInvariantDiagnostic::IncompleteSameEvent {
                missing: missing.to_owned(),
            })
        }
        validate::QueryCompileError::InternalInvariant { detail } => {
            MatcherBuildError::CompilerInvariant(CompilerInvariantDiagnostic::Internal { detail })
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

    pub(crate) fn requirements(&self) -> &requirements::PlanRequirements {
        self.physical_plan.requirements()
    }

    pub(crate) fn compile(queries: &[QueryDecl]) -> Result<Self, MatcherBuildError> {
        let physical_plan = compile_queries(queries)?;
        Ok(Self { physical_plan })
    }
}
