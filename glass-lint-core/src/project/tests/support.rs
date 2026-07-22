use super::*;

pub fn source_file(path: impl Into<String>, source: impl Into<String>) -> SourceFile {
    SourceFile::new(path, source).unwrap()
}

pub fn project_path(path: &str) -> ProjectRelativePath {
    ProjectRelativePath::new(path).unwrap()
}

pub fn test_linter() -> crate::Linter {
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    test_linter_with_environment(environment)
}

pub fn test_linter_with_environment(environment: crate::Environment) -> crate::Linter {
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

pub fn test_linter_with_limits(limits: crate::AnalysisLimits) -> crate::Linter {
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    crate::Linter::new(
        crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_limits(limits),
    )
    .unwrap()
}

pub fn test_linter_with_selection(
    selection: crate::RuleSelection,
    limits: crate::AnalysisLimits,
) -> crate::Linter {
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    crate::Linter::new(
        crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_rules(selection)
        .with_limits(limits),
    )
    .unwrap()
}

pub fn flow_linter() -> crate::Linter {
    let rule = Rule::builder("flow.append")
        .description("Appends a configured script")
        .category("flow")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(
            ObjectFlowMatcher::builder("script insertion")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("script")),
                ))
                .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                    "src",
                    ValueMatcher::any_value(),
                )))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    MemberCallMatcher::rooted("document.head.appendChild"),
                    0,
                )]))
                .build(),
        )
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment
        .add_globals(["document", "url"])
        .expect("test environment globals");
    crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

pub fn key(importer: &str) -> ResolutionRequestKey {
    ResolutionRequestKey {
        importer: ProjectRelativePath::new(importer).unwrap(),
        kind: ResolutionRequestKind::StaticImport,
        range: SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 8).unwrap())
            .unwrap(),
    }
}

pub struct ProjectFixture<'a> {
    session: AnalysisSession<'a>,
}

impl<'a> ProjectFixture<'a> {
    pub fn new(linter: &'a crate::Linter) -> Self {
        Self {
            session: linter.begin_analysis("/project").unwrap(),
        }
    }

    pub fn add(&mut self, path: &str, source: &str) {
        self.session.add_source(source_file(path, source)).unwrap();
    }

    pub fn add_resolved(
        &mut self,
        path: &str,
        source: &str,
        resolutions: impl IntoIterator<Item = ResolverOutcome>,
    ) {
        let requests = self.session.add_source(source_file(path, source)).unwrap();
        for (request, resolution) in requests.into_iter().zip(resolutions) {
            self.session
                .record_resolution(request.key, resolution)
                .unwrap();
        }
    }

    pub fn finish(self) -> AnalysisReport {
        self.session.finish().unwrap()
    }
}
