use std::{collections::BTreeMap, sync::Arc};

use super::*;
use crate::{
    AnalysisLimits, Environment,
    analysis::{
        LocalArtifact, SemanticAnalyzer, local::LocatedSourceContext, semantic::SpanNormalizer,
    },
    project::{SourceFile, SourceText},
};

fn imported_module() -> ProjectModule {
    let text = "import value from './dep.js';";
    let source = SourceFile::new("module.js", text).unwrap();
    let parsed = crate::parse_test_source(text, "module.js").unwrap();
    let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(text));
    let semantic = SemanticAnalyzer::new(&Environment::default(), &AnalysisLimits::default())
        .analyze_program(&parsed.program, &coordinates);
    ProjectModule::new(
        ModuleId::new(0),
        LocalArtifact::new(LocatedSourceContext::new(&source), Arc::new(semantic)),
    )
}

#[test]
fn oversized_scc_is_rejected_with_linking_status() {
    let template = imported_module();
    let (request_index, _request) = template
        .local()
        .interface()
        .request_entries()
        .next()
        .unwrap();
    let count = MAX_SCC_SIZE + 1;
    let mut modules = BTreeMap::new();
    let mut resolutions = BTreeMap::new();

    for index in 0..count {
        let id = ModuleId::new(u32::try_from(index).unwrap());
        let next = ModuleId::new(u32::try_from((index + 1) % count).unwrap());
        modules.insert(id, ProjectModule::new(id, template.local().clone()));
        resolutions.insert(
            QualifiedRequestId::new(id, request_index),
            LinkedModuleTarget::Internal { id: next },
        );
    }

    let result = GraphBuild::build(&modules, &resolutions, count);

    assert!(result.scc_partition.is_none());
    assert!(result.exhausted);
    let (_, project) = result.status.diagnostics();
    assert_eq!(project.len(), 1);
    assert_eq!(project[0].code().as_str(), "graph_link_budget_exhausted");
}
