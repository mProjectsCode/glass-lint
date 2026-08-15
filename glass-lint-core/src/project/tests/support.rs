use super::*;
use crate::{
    AnalysisLimits, Environment, Linter, LinterConfig, ProjectAdmissionLimits, RuleCatalog,
    RuleSelection,
    api::rule::{EventQuery, QueryDecl},
    project::ProjectSession,
};

pub fn source_file(path: impl Into<String>, source: impl Into<SourceText>) -> SourceFile {
    let path = path.into();
    let language = if path.to_ascii_lowercase().ends_with(".ts")
        || path.to_ascii_lowercase().ends_with(".cts")
        || path.to_ascii_lowercase().ends_with(".mts")
    {
        crate::SourceLanguage::TypeScript
    } else {
        crate::SourceLanguage::JavaScript
    };
    SourceFile::with_language(path, source, language).unwrap()
}

pub fn project_path(path: &str) -> ProjectRelativePath {
    ProjectRelativePath::new(path).unwrap()
}

pub fn finish_session(collection: ProjectSession<'_>) -> AnalysisReport {
    collection.finish([]).unwrap().into_report()
}

pub fn finish_collection_with(
    collection: ProjectSession<'_>,
    outcomes: impl IntoIterator<Item = (ResolutionRequestKey, ResolverOutcome)>,
) -> AnalysisReport {
    collection.finish(outcomes).unwrap().into_report()
}

pub fn test_linter() -> Linter {
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    test_linter_with_environment(environment)
}

pub fn test_linter_with_environment(environment: Environment) -> Linter {
    let rule = Rule::catalog_builder("network.fetch")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

pub fn test_linter_with_limits(limits: AnalysisLimits) -> Linter {
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::catalog_builder("network.fetch")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    Linter::new(
        LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_limits(limits),
    )
    .unwrap()
}

pub fn test_linter_with_project_limits(limits: ProjectAdmissionLimits) -> Linter {
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::catalog_builder("network.fetch")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    Linter::new(
        LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_project_limits(limits),
    )
    .unwrap()
}

pub fn test_linter_with_selection(selection: RuleSelection, limits: AnalysisLimits) -> Linter {
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::catalog_builder("network.fetch")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    Linter::new(
        LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_rules(selection)
        .with_limits(limits),
    )
    .unwrap()
}

pub fn flow_linter() -> Linter {
    let rule = Rule::catalog_builder("flow.append")
        .description("Appends a configured script")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(QueryDecl::lifecycle(
            LifecycleQuery::catalog_builder("script insertion")
                .source(
                    EventQuery::member_call_rooted("document.createElement")
                        .unwrap()
                        .with_arg(
                            0,
                            ValueMatcher::static_string().try_equals("script").unwrap(),
                        ),
                )
                .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                    "src",
                    ValueMatcher::any_value(),
                )))
                .completion(LifecycleCompletion::any_sink([
                    LifecycleSink::argument_of_member("document.head.appendChild", 0),
                ]))
                .build(),
        ))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment
        .add_globals(["document", "url"])
        .expect("test environment globals");
    Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

pub fn key(importer: &str) -> ResolutionRequestKey {
    ResolutionRequestKey::new(
        ProjectRelativePath::new(importer).unwrap(),
        ResolutionRequestKind::StaticImport,
        SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 8).unwrap()).unwrap(),
    )
}

/// Rebuild a request key as a `Require` at the same location, exercising
/// unknown-request rejection for keys the authored request table does not hold.
pub fn as_require_key(key: &ResolutionRequestKey) -> ResolutionRequestKey {
    ResolutionRequestKey::new(
        key.importer().clone(),
        ResolutionRequestKind::Require,
        key.range().clone(),
    )
}

pub struct ProjectFixture<'a> {
    session: ProjectSession<'a>,
    outcomes: Vec<(ResolutionRequestKey, ResolverOutcome)>,
}

impl<'a> ProjectFixture<'a> {
    pub fn new(linter: &'a Linter) -> Self {
        Self {
            session: linter.begin_project(),
            outcomes: Vec::new(),
        }
    }

    pub fn add(&mut self, path: &str, source: &str) {
        self.session
            .analyze_source(source_file(path, source))
            .unwrap();
    }

    pub fn add_resolved(
        &mut self,
        path: &str,
        source: &str,
        resolutions: impl IntoIterator<Item = ResolverOutcome>,
    ) {
        let requests = self
            .session
            .analyze_source(source_file(path, source))
            .unwrap()
            .into_iter();
        for (request, resolution) in requests.into_iter().zip(resolutions) {
            self.outcomes.push((request.key().clone(), resolution));
        }
    }

    pub fn finish(self) -> AnalysisReport {
        self.session.finish(self.outcomes).unwrap().into_report()
    }
}
