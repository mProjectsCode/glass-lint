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

pub mod model;

mod facts;
pub mod flow;
mod local;
mod lowering;
mod matching;
mod module_request;
pub use matching::display_span;
pub mod module;
pub mod project;
mod resolution;
mod scope;
mod syntax;
pub mod trace;
mod value;

pub use local::{
    ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, ProjectModule,
    SemanticArtifact, SharedSemanticArtifact,
};
pub(in crate::analysis) use lowering::budget::SemanticBudget;
pub use lowering::{LoweredSource, Lowerer};
pub(in crate::analysis) use project::model::ExportResolution;
pub use project::model::{ProjectSemanticModel, QualifiedRequestId, ResolvedLinkInput};
pub use value::matches_global_object_alias;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        AnalysisLimits, Environment, Severity,
        analysis::{local::LocatedSourceContext, lowering::SpanNormalizer},
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
        let local = Lowerer::new(&Environment::default(), &AnalysisLimits::default())
            .lower_program(&parsed.program, &coordinates);
        let source = SourceFile::new(
            "projection-invariant.js",
            "fetch('/remote'); document.createElement('div');",
        )
        .unwrap();
        let project = ProjectSemanticModel::single(
            "projection-invariant.js",
            LocatedSourceContext::new(&source),
            LocalArtifact::new(LocatedSourceContext::new(&source), Arc::new(local)),
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
            description: "fetch".into(),
            query_explanations: Vec::new(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            matcher: fetch_plan,
        };
        let fetch_rules = [fetch_rule];
        let (_model, _outcome) =
            project.project(CompiledRuleSelection::new(&fetch_rules, &selected));

        let member_plan = CompiledMatcherPlan::compile(&[EventQuery::member_call_heuristic(
            "document.createElement",
        )
        .unwrap()
        .into_query()])
        .unwrap();
        let member_rule = CompiledRuleRecord {
            description: "member".into(),
            query_explanations: Vec::new(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            matcher: member_plan,
        };
        let member_rules = [member_rule];
        let (_model, _outcome) =
            project.project(CompiledRuleSelection::new(&member_rules, &selected));

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
}
