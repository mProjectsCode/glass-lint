use std::collections::BTreeMap;

use swc_common::{SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::Program;

use crate::Environment;
use crate::project::ModuleId;

use super::{facts, flow, module, resolution, syntax};

use super::module::ModuleInterface;
use facts::SemanticFacts;
use syntax::SymbolCallProvenance;

/// The immutable, matcher-independent result of analyzing one source.
#[derive(Debug)]
pub(crate) struct LocalModuleModel {
    facts: SemanticFacts,
    export_origins: BTreeMap<String, SymbolCallProvenance>,
    effects: flow::effect::FunctionEffects,
}

impl LocalModuleModel {
    pub(crate) fn analyze(program: &Program, environment: &Environment) -> Self {
        let resolver = resolution::Resolver::collect_with_environment(program, environment);
        let facts = SemanticFacts::build(program, &resolver);
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
        }
    }

    pub(crate) fn interface(&self) -> &ModuleInterface {
        self.facts.interface()
    }

    pub(in crate::analysis) fn facts(&self) -> &SemanticFacts {
        &self.facts
    }

    pub(in crate::analysis) fn effects(&self) -> &flow::effect::FunctionEffects {
        &self.effects
    }

    pub(in crate::analysis) fn export_origin(&self, name: &str) -> Option<&SymbolCallProvenance> {
        self.export_origins.get(name)
    }
}

/// A successfully analyzed source together with the data needed to report
/// findings in its original file.
pub(crate) struct ProjectModule {
    id: ModuleId,
    path: String,
    source_map: Lrc<SourceMap>,
    local: LocalModuleModel,
}

impl ProjectModule {
    pub(crate) fn new(
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
    pub(crate) fn id(&self) -> ModuleId {
        self.id
    }
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn source_map(&self) -> &Lrc<SourceMap> {
        &self.source_map
    }
    pub(crate) fn local(&self) -> &LocalModuleModel {
        &self.local
    }

    pub(in crate::analysis) fn diagnostics(&self) -> Vec<crate::ProjectDiagnostic> {
        let mut diagnostics = Vec::new();
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
