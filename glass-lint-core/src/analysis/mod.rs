//! Private semantic analysis and project linking.
//!
//! Local construction and matcher projection are deliberately separate. A
//! source is parsed and semantically visited once into a matcher-independent
//! model; rules query a linked project model afterwards.
//!
//! Local scopes and value arenas remain partitioned by module. Linking adds
//! qualified identities and bounded flow overlays, never lexical facts from
//! one module into another.

// Re-exports for child modules that access these via `crate::analysis::*`.
use std::collections::{BTreeMap, BTreeSet};

use crate::project::{LinkedModuleTarget, ModuleId};

mod evidence;
mod facts;
pub mod flow;
mod local;
mod lowering;
mod matching;
pub mod module;
mod name;
pub mod project;
mod resolution;
mod scope;
mod status;
mod syntax;
mod value;

pub use local::{
    ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, ProjectModule,
    SemanticArtifact, SharedSemanticArtifact,
};
pub use lowering::{LoweredSource, lower_source};
pub use project::model::ProjectSemanticModel;
pub(in crate::analysis) use project::model::{ExportResolution, QualifiedRequestId};
pub use value::SymbolPath;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Environment,
        api::{
            compiler::{CompiledRuleRecord, CompiledRuleSelection, rule::CompiledMatcherPlan},
            rule::MatcherDecl,
        },
    };

    #[test]
    fn local_model_is_unchanged_by_matcher_projection() {
        let text = "fetch('/remote'); document.createElement('div');";
        let parsed = crate::parse(text, "projection-invariant.js").expect("source should parse");
        let coordinates = lowering::SpanNormalizer::new(parsed.source_start, text);
        let local = lowering::lower_program(
            &parsed.program,
            &Environment::default(),
            &crate::AnalysisLimits::default(),
            &coordinates,
        );
        let source = crate::SourceFile::new(
            "projection-invariant.js",
            "fetch('/remote'); document.createElement('div');",
        )
        .unwrap();
        let project = ProjectSemanticModel::single(
            "projection-invariant.js",
            local::LocatedSourceContext::new(&source),
            LocalArtifact::new(
                local::LocatedSourceContext::new(&source),
                std::sync::Arc::new(local),
            ),
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

        let fetch_plan = CompiledMatcherPlan::compile_decls(&[MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration")])
        .unwrap();
        let selected = [crate::api::classification::RuleIndex::new(0)];
        let fetch_rule = CompiledRuleRecord {
            id: "fetch".into(),
            description: "fetch".into(),
            category: "test".into(),
            severity: crate::Severity::Warning,
            confidence: crate::api::rule::Confidence::High,
            matcher: fetch_plan,
        };
        let fetch_rules = [fetch_rule];
        let (_model, _outcome) =
            project.project(CompiledRuleSelection::new(&fetch_rules, &selected));

        let member_plan = CompiledMatcherPlan::compile_decls(&[MatcherDecl::builder()
            .member_call_heuristic("document.createElement")
            .build()
            .unwrap()])
        .unwrap();
        let member_rule = CompiledRuleRecord {
            id: "member".into(),
            description: "member".into(),
            category: "test".into(),
            severity: crate::Severity::Warning,
            confidence: crate::api::rule::Confidence::High,
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
