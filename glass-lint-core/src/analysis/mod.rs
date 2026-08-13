//! Private semantic analysis and project linking.
//!
//! Local construction and matcher projection are deliberately separate. A
//! source is parsed and semantically visited once into a matcher-independent
//! model; rules query a linked project model afterwards.
//!
//! Local scopes and value arenas remain partitioned by module. Linking adds
//! qualified identities and bounded flow overlays, never lexical facts from
//! one module into another.

use crate::project::{LinkedModuleTarget, ModuleId};

mod facts;
pub mod flow;
mod local;
mod matching;
pub mod model;
mod module_request;
pub mod project;
mod resolution;
mod scope;
mod semantic;
mod syntax;
pub mod trace;

pub use local::{
    ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, ProjectModule,
    SemanticArtifact,
};
pub use matching::display_span;
pub(in crate::analysis) use project::model::{ExportResolution, QualifiedFunctionId};
pub use project::model::{ProjectSemanticModel, QualifiedRequestId, ResolvedLinkInput};
pub(in crate::analysis) use semantic::budget::SemanticBudget;
pub use semantic::{
    AnalyzedSource, SemanticAnalyzer,
    status::{AnalysisStatus, IncompleteReason, StatusDiagnostics, StatusScope},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis) enum DerivedPhaseAvailability {
    #[default]
    Enabled,
    DisabledByIncompleteAnalysis,
}

impl DerivedPhaseAvailability {
    pub(in crate::analysis) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct DerivedPhaseCapabilities {
    export_origins: DerivedPhaseAvailability,
    fact_index: DerivedPhaseAvailability,
    effects: DerivedPhaseAvailability,
}

impl DerivedPhaseCapabilities {
    pub(in crate::analysis) const fn enabled() -> Self {
        Self {
            export_origins: DerivedPhaseAvailability::Enabled,
            fact_index: DerivedPhaseAvailability::Enabled,
            effects: DerivedPhaseAvailability::Enabled,
        }
    }

    pub(in crate::analysis) fn disable_derived_phases(&mut self) {
        let disabled = DerivedPhaseAvailability::DisabledByIncompleteAnalysis;
        self.export_origins = disabled;
        self.fact_index = disabled;
        self.effects = disabled;
    }

    pub(in crate::analysis) const fn export_origins(self) -> DerivedPhaseAvailability {
        self.export_origins
    }

    pub(in crate::analysis) const fn fact_index(self) -> DerivedPhaseAvailability {
        self.fact_index
    }

    pub(in crate::analysis) const fn effects(self) -> DerivedPhaseAvailability {
        self.effects
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        AnalysisLimits, Environment, Severity,
        analysis::{local::LocatedSourceContext, semantic::SpanNormalizer},
        api::{
            classification::RuleIndex,
            compiler::{CompiledRuleRecord, CompiledRuleSelection, rule::CompiledMatcherPlan},
            rule::{Confidence, EventQuery},
        },
        project::{SourceFile, SourceText},
    };

    #[test]
    fn local_model_is_unchanged_by_matcher_projection() {
        let text = "fetch('/remote'); document.createElement('div');";
        let parsed =
            crate::parse_test_source(text, "projection-invariant.js").expect("source should parse");
        let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(text));
        let local = SemanticAnalyzer::new(&Environment::default(), &AnalysisLimits::default())
            .analyze_program(&parsed.program, &coordinates);
        let source = SourceFile::new(
            "projection-invariant.js",
            "fetch('/remote'); document.createElement('div');",
        )
        .unwrap();
        let project = ProjectSemanticModel::single(
            "projection-invariant.js",
            LocatedSourceContext::new(&source),
            LocalArtifact::from_analyzed(AnalyzedSource::new(
                LocatedSourceContext::new(&source),
                Arc::new(local),
            )),
        );
        let before = format!(
            "{:?}",
            project
                .modules()
                .next()
                .expect("one module")
                .local()
                .facts()
        );

        let fetch_plan =
            CompiledMatcherPlan::compile(&[EventQuery::call_global("fetch").unwrap().into_query()])
                .unwrap();
        let selected = [RuleIndex::new(0)];
        let fetch_rule = CompiledRuleRecord {
            rule_id: crate::RuleId::parse("test:fetch").unwrap(),
            description: "fetch".into(),
            query_explanations: Vec::new(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            matcher: fetch_plan,
        };
        let fetch_rules = [fetch_rule];
        let (_model, _outcome) =
            project.project(CompiledRuleSelection::new(&fetch_rules, &selected).unwrap());

        let member_plan = CompiledMatcherPlan::compile(&[EventQuery::member_call_heuristic(
            "document.createElement",
        )
        .unwrap()
        .into_query()])
        .unwrap();
        let member_rule = CompiledRuleRecord {
            rule_id: crate::RuleId::parse("test:member").unwrap(),
            description: "member".into(),
            query_explanations: Vec::new(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            matcher: member_plan,
        };
        let member_rules = [member_rule];
        let (_model, _outcome) =
            project.project(CompiledRuleSelection::new(&member_rules, &selected).unwrap());

        let after = format!(
            "{:?}",
            project
                .modules()
                .next()
                .expect("one module")
                .local()
                .facts()
        );
        assert_eq!(before, after);
    }

    #[test]
    fn project_matcher_rejects_a_module_from_another_project() {
        fn project(source_text: &str, path: &str) -> ProjectSemanticModel {
            let source = SourceFile::new(path, source_text).unwrap();
            let parsed = crate::parse_test_source(source_text, path).expect("source should parse");
            let coordinates =
                SpanNormalizer::new(parsed.source_start, &SourceText::from(source_text));
            let mut environment = Environment::default();
            environment.add_global("fetch").unwrap();
            let local = SemanticAnalyzer::new(&environment, &AnalysisLimits::default())
                .analyze_program(&parsed.program, &coordinates);
            ProjectSemanticModel::single(
                path,
                LocatedSourceContext::new(&source),
                LocalArtifact::from_analyzed(AnalyzedSource::new(
                    LocatedSourceContext::new(&source),
                    Arc::new(local),
                )),
            )
        }

        let first = project("fetch('/first');", "first.js");
        let second = project("", "second.js");
        let plan =
            CompiledMatcherPlan::compile(&[EventQuery::call_global("fetch").unwrap().into_query()])
                .unwrap();
        let rule_index = RuleIndex::new(0);
        let records = [CompiledRuleRecord {
            rule_id: crate::RuleId::parse("test:fetch").unwrap(),
            description: "fetch".into(),
            query_explanations: Vec::new(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            matcher: plan,
        }];
        let selected = [rule_index];
        let (matcher, _outcome) =
            first.project(CompiledRuleSelection::new(&records, &selected).unwrap());
        let first_module = matcher.modules().next().expect("first module");
        let (second_matcher, _outcome) =
            second.project(CompiledRuleSelection::new(&records, &selected).unwrap());
        let second_module = second_matcher.modules().next().expect("second module");

        assert!(
            !matcher
                .evidence_for(first_module, rule_index, usize::MAX)
                .unwrap()
                .is_empty()
        );
        assert!(
            matcher
                .evidence_for(second_module, rule_index, usize::MAX)
                .is_err()
        );
    }
}
