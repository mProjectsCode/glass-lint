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
    pub(in crate::analysis) facts: SemanticFacts,
    pub(in crate::analysis) export_origins: BTreeMap<String, SymbolCallProvenance>,
    pub(crate) effects: flow::effect::FunctionEffects,
}

impl LocalModuleModel {
    pub(crate) fn analyze(program: &Program, environment: &Environment) -> Self {
        let resolver = resolution::Resolver::collect_with_environment(program, environment);
        let facts = SemanticFacts::build(program, &resolver);
        let export_origins = facts
            .interface
            .exports
            .values()
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
        let effects = flow::effect::collect(&facts.stream);
        Self {
            facts,
            export_origins,
            effects,
        }
    }

    pub(crate) fn interface(&self) -> &ModuleInterface {
        &self.facts.interface
    }
}

/// A successfully analyzed source together with the data needed to report
/// findings in its original file.
pub(crate) struct ProjectModule {
    pub(crate) id: ModuleId,
    pub(crate) path: String,
    pub(crate) source_map: Lrc<SourceMap>,
    pub(crate) local: LocalModuleModel,
}
