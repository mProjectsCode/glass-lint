//! Immutable matcher compilation and catalog selection.
//!
//! Compilation translates validated public matcher declarations once. The
//! resulting plans are provider-neutral and can be projected onto many files
//! without rebuilding matcher semantics.

#![allow(clippy::redundant_pub_crate)]

pub(crate) mod catalog;
pub(crate) mod error;
pub(crate) mod normalize;
pub(crate) mod object_flow;
pub(crate) mod physical;
#[cfg(test)]
pub(crate) mod reference;
pub(crate) mod rule;
pub(crate) mod validate;

pub(crate) use catalog::compile_records;
use glass_lint_datastructures::SymbolPath;
pub(crate) use object_flow::{
    CompiledObjectFlow, CompiledObjectRequirement, CompiledObjectSinkArguments,
};
pub(crate) use rule::{CompiledRuleRecord, CompiledRuleSelection};
use smol_str::SmolStr;

use crate::{
    analysis::matches_global_object_alias,
    api::{
        classification::MatchKind,
        compiler::{
            normalize::NormalizedQuery, physical::PhysicalPlan, validate::validate_query_decl,
        },
        rule::{
            MatcherBuildError, ModuleSpecifierPattern,
            query::{EventSpec, IdentitySpec, QueryDecl},
        },
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
        matches!(self, Self::Rooted { path } if matches_global_object_alias(path, source, environment)
            || source.is_equal_or_descendant_of(path))
    }

    #[allow(dead_code)]
    pub(crate) fn exact_root_matches(&self, source: &SymbolPath) -> bool {
        matches!(self, Self::Rooted { path } if path == source)
    }

    #[allow(dead_code)]
    pub(crate) fn identity_module_matches(&self, module: &str, export: &str) -> bool {
        matches!(self, Self::ModuleExport { module: expected_module, export: expected_export } if expected_module == module && expected_export == export)
            || matches!(self, Self::PackageModuleExport { module: expected_module, export: expected_export } if expected_module.matches(module) && expected_export == export)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum EventPredicate {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
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
        EventSpec::ClassReference => EventPredicate::ClassReference,
        EventSpec::Import => EventPredicate::Import,
        EventSpec::StringReference => EventPredicate::StringReference,
    }
}

/// Compile query declarations into a physical plan.
fn compile_queries(queries: &[QueryDecl]) -> Result<PhysicalPlan, MatcherBuildError> {
    let mut all_roots = Vec::new();
    let mut merged_requirements = normalize::PlanRequirements::default();

    for query in queries {
        validate_query_decl(query).map_err(MatcherBuildError::QueryCompileError)?;

        let normalized: NormalizedQuery =
            normalize::normalize_query_decl(query).map_err(MatcherBuildError::QueryCompileError)?;

        let query_plan = physical::plan_normalized(&normalized);
        all_roots.extend(query_plan.roots().iter().cloned());
        merged_requirements.merge_from(query_plan.requirements());
    }

    let mut sorted_roots: Vec<physical::PhysicalRoot> = all_roots;
    sorted_roots.sort();
    sorted_roots.dedup();
    let physical_plan = PhysicalPlan::new(sorted_roots.into_boxed_slice(), merged_requirements);
    physical::validate_physical_plan(&physical_plan)
        .map_err(|e| MatcherBuildError::InvalidLoweredQuery(e.to_string()))?;

    Ok(physical_plan)
}

impl CompiledMatcherPlan {
    pub(crate) fn physical_roots(&self) -> &[physical::PhysicalRoot] {
        self.physical_plan.roots()
    }

    #[allow(dead_code)]
    pub(crate) fn plan_summary(&self) -> String {
        self.physical_plan.summary()
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

    pub(crate) fn flow_requirements(&self) -> &normalize::FlowRequirements {
        self.physical_plan.requirements().flow()
    }

    pub(crate) fn compile(queries: &[QueryDecl]) -> Result<Self, MatcherBuildError> {
        let physical_plan = compile_queries(queries)?;
        Ok(Self { physical_plan })
    }
}
