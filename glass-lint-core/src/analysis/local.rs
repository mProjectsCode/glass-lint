//! Matcher-independent analysis of one source module.
//!
//! Local analysis resolves scopes, values, facts, module interfaces, and
//! function effects exactly once. Project linking and rule selection consume
//! this model later without revisiting the AST.

use std::collections::BTreeMap;

use facts::SemanticFacts;
use swc_common::{SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::Program;
use syntax::SymbolCallProvenance;

use super::{facts, flow, module, module::ModuleInterface, resolution, syntax};
use crate::{Environment, project::ModuleId};

/// The immutable, matcher-independent result of analyzing one source.
#[derive(Debug)]
pub struct LocalModuleModel {
    /// Canonical facts, occurrence indexes, and module interface.
    facts: SemanticFacts,
    /// Proven origins for locally named exports.
    export_origins: BTreeMap<String, SymbolCallProvenance>,
    /// Matcher-independent function effects for project flow.
    effects: flow::effect::FunctionEffects,
    semantic_budget_exhausted: bool,
}

impl LocalModuleModel {
    /// Analyze one parsed program against the configured host environment.
    #[allow(dead_code)]
    pub fn analyze(program: &Program, environment: &Environment) -> Self {
        Self::analyze_with_limits(program, environment, &crate::ResourceLimits::default())
    }

    pub fn analyze_with_limits(
        program: &Program,
        environment: &Environment,
        limits: &crate::ResourceLimits,
    ) -> Self {
        let resolver = resolution::Resolver::collect_with_environment(program, environment);
        let facts = SemanticFacts::build_with_limit(program, &resolver, limits.semantic_operations);
        let semantic_budget_exhausted = !facts.is_valid();
        let export_origins = facts
            .interface()
            .exports()
            .map(|(_, export)| export)
            .filter_map(|declaration| match declaration {
                module::ModuleExport::Local { name } => Some((
                    name.clone(),
                    resolver.exported_provenance(name, program.span()),
                )),
                module::ModuleExport::Value
                | module::ModuleExport::ReExport { .. }
                | module::ModuleExport::Namespace { .. }
                | module::ModuleExport::Unknown => None,
            })
            .collect();
        let effects = flow::effect::FunctionEffects::collect(facts.stream());
        Self {
            facts,
            export_origins,
            effects,
            semantic_budget_exhausted,
        }
    }

    /// Borrow the matcher-independent module interface.
    pub fn interface(&self) -> &ModuleInterface {
        self.facts.interface()
    }

    pub(in crate::analysis) fn facts(&self) -> &SemanticFacts {
        &self.facts
    }

    pub(in crate::analysis) fn effects(&self) -> &flow::effect::FunctionEffects {
        &self.effects
    }

    pub(in crate::analysis) fn semantic_budget_exhausted(&self) -> bool {
        self.semantic_budget_exhausted
    }

    pub(in crate::analysis) fn export_origin(&self, name: &str) -> Option<&SymbolCallProvenance> {
        self.export_origins.get(name)
    }
}

/// A successfully analyzed source together with the data needed to report
/// findings in its original file.
pub struct ProjectModule {
    /// Stable project-local module identity.
    id: ModuleId,
    /// Canonical path used in report locations and resolution keys.
    path: String,
    /// Source map for translating fact spans into user locations.
    source_map: Lrc<SourceMap>,
    /// Immutable local semantic model.
    local: LocalModuleModel,
}

impl ProjectModule {
    /// Assemble a linked-project module from a stable identity and local model.
    pub fn new(
        id: ModuleId,
        path: String,
        source_map: Lrc<SourceMap>,
        local: LocalModuleModel,
    ) -> Self {
        Self {
            id,
            path,
            source_map,
            local,
        }
    }

    /// Return the stable module identity.
    pub fn id(&self) -> ModuleId {
        self.id
    }

    /// Return the canonical report/resolution path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Borrow the source map used for location conversion.
    pub fn source_map(&self) -> &Lrc<SourceMap> {
        &self.source_map
    }

    /// Borrow this module's local semantic model.
    pub fn local(&self) -> &LocalModuleModel {
        &self.local
    }

    pub(in crate::analysis) fn diagnostics(&self) -> Vec<crate::ProjectDiagnostic> {
        let mut diagnostics = Vec::new();
        if self.local.semantic_budget_exhausted() {
            diagnostics.push(crate::ProjectDiagnostic {
                code: "semantic_budget_exhausted".into(),
                message: format!(
                    "semantic analysis exceeded its per-file budget in `{}`",
                    self.path
                ),
                location: None,
            });
        }
        if self.local.effects().budget_exhausted() {
            diagnostics.push(crate::ProjectDiagnostic {
                code: "effect_size_budget_exhausted".into(),
                message: format!(
                    "function-effect extraction exceeded a bounded budget in `{}`",
                    self.path
                ),
                location: None,
            });
        }
        if self.local.interface().is_unknown() {
            diagnostics.push(crate::ProjectDiagnostic {
                code: "unsupported_commonjs_exports".into(),
                message: format!(
                    "CommonJS export shape in `{}` is dynamic or ambiguous",
                    self.path
                ),
                location: None,
            });
        }
        diagnostics
    }
}
