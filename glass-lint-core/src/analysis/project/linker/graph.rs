//! Module graph edge construction and SCC-DAG preparation.

use std::collections::BTreeMap;

use glass_lint_datastructures::Budget;

use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId, ProjectModule, QualifiedRequestId,
        project::{
            model::MAX_SCC_SIZE,
            state::{ModuleGraph, NormalizedModuleGraph, SccPartition},
        },
        semantic::status::{
            AnalysisComponent, AnalysisStatus, IncompleteReason, ResolutionKind, StatusScope,
        },
    },
    project::is_internal_module_request,
};

pub(super) struct GraphBuild {
    pub(super) graph: NormalizedModuleGraph,
    pub(super) scc_partition: Result<SccPartition, GraphBuildError>,
    pub(super) status: AnalysisStatus,
    pub(super) exhausted: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum GraphBuildError {
    SccTooLarge { limit: usize },
}

impl GraphBuild {
    /// Convert internal resolution records into bounded graph edges, compute
    /// SCCs, build the SCC DAG, and compute the topological order.
    pub(super) fn build(
        modules: &BTreeMap<ModuleId, ProjectModule>,
        resolutions: &BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
        link_limit: usize,
    ) -> Self {
        let mut graph = ModuleGraph::default();
        let mut status = AnalysisStatus::default();
        let mut edge_budget = Budget::new(link_limit);
        for module in modules.values() {
            graph.ensure_node(module.id());
            for request in module.local().interface().requests() {
                let request_id = QualifiedRequestId::new(module.id(), request.id());
                let Some(resolution) = resolutions.get(&request_id) else {
                    if is_internal_module_request(request.specifier()) {
                        status.record(
                            StatusScope::File(module.path().clone()),
                            IncompleteReason::MissingInternalResolution {
                                request: request.specifier().to_string(),
                            },
                        );
                    }
                    continue;
                };
                if let LinkedModuleTarget::Internal { id } = resolution {
                    if edge_budget.try_push() {
                        graph.insert_edge(module.id(), *id);
                    }
                } else if matches!(resolution, LinkedModuleTarget::Missing)
                    && is_internal_module_request(request.specifier())
                {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::MissingInternalResolution {
                            request: request.specifier().to_string(),
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::OutsideProject { .. }) {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: ResolutionKind::OutsideProject,
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::Unsupported { .. }) {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: ResolutionKind::Unsupported,
                        },
                    );
                }
            }
        }

        let mut exhausted = edge_budget.exhausted();
        let graph = graph.normalize();
        let scc_partition = graph.scc_partition(MAX_SCC_SIZE).ok_or_else(|| {
            exhausted = true;
            status.record(
                StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Linking,
                    limit: link_limit,
                    observed: None,
                },
            );
            GraphBuildError::SccTooLarge {
                limit: MAX_SCC_SIZE,
            }
        });
        Self {
            graph,
            scc_partition,
            status,
            exhausted,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use super::*;
    use crate::{
        AnalysisLimits, Environment,
        analysis::{
            AnalyzedSource, SemanticAnalyzer, local::LocatedSourceContext, semantic::SpanNormalizer,
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
            crate::analysis::LocalArtifact::from_analyzed(AnalyzedSource::new(
                LocatedSourceContext::new(&source),
                Arc::new(semantic),
            )),
        )
    }

    #[test]
    fn oversized_scc_is_rejected_with_linking_status() {
        let template = imported_module();
        let request = template.local().interface().requests().next().unwrap();
        let count = MAX_SCC_SIZE + 1;
        let mut modules = BTreeMap::new();
        let mut resolutions = BTreeMap::new();

        for index in 0..count {
            let id = ModuleId::new(u32::try_from(index).unwrap());
            let next = ModuleId::new(u32::try_from((index + 1) % count).unwrap());
            modules.insert(id, ProjectModule::new(id, template.local().clone()));
            resolutions.insert(
                QualifiedRequestId::new(id, request.id()),
                LinkedModuleTarget::Internal { id: next },
            );
        }

        let result = GraphBuild::build(&modules, &resolutions, count);

        assert!(matches!(
            result.scc_partition,
            Err(GraphBuildError::SccTooLarge {
                limit: MAX_SCC_SIZE
            })
        ));
        assert!(result.exhausted);
        let (_, project) = result.status.diagnostics().into_parts();
        assert_eq!(project.len(), 1);
        assert_eq!(project[0].code().as_str(), "graph_link_budget_exhausted");
    }
}
