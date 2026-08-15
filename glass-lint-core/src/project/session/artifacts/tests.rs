use super::*;
use crate::{
    AnalysisLimits, Environment,
    analysis::SemanticAnalyzer,
    project::{ResolutionRequestKind, SourceFile},
};

fn lower(path: &str, source: &str) -> (ProjectRelativePath, AnalyzedSource) {
    let source = SourceFile::new(path, source).unwrap();
    let analyzed = SemanticAnalyzer::new(&Environment::default(), &AnalysisLimits::default())
        .analyze_source(&source)
        .unwrap();
    (source.path().clone(), analyzed)
}

fn parse_failure(path: &str) -> ParseDiagnostic {
    ParseDiagnostic::new(
        crate::parse::ParseFailureKind::Syntax,
        "invalid syntax",
        path,
        None,
    )
}

#[test]
fn needs_analysis_tracks_completed_and_failed_sources() {
    let mut artifacts = AnalysisArtifacts::default();
    let (analyzed_path, analyzed) = lower("a.js", "fetch('/x');");
    assert!(artifacts.needs_analysis(&analyzed_path));
    artifacts.record_analyzed(&analyzed_path, analyzed);
    assert!(!artifacts.needs_analysis(&analyzed_path));

    let failed_path = ProjectRelativePath::new("b.js").unwrap();
    assert!(artifacts.needs_analysis(&failed_path));
    artifacts.record_parse_failure(failed_path.clone(), parse_failure("b.js"));
    assert!(!artifacts.needs_analysis(&failed_path));
}

#[test]
fn successful_retry_replaces_a_parse_failure() {
    let source = SourceFile::new("retry.js", "fetch('/x');").unwrap();
    let mut sources = SourceTable::default();
    sources.insert(source.clone()).unwrap();
    let mut artifacts = AnalysisArtifacts::default();
    artifacts.record_parse_failure(source.path().clone(), parse_failure("retry.js"));
    artifacts.record_analyzed(
        source.path(),
        lower(source.path().as_str(), "fetch('/x');").1,
    );

    let (_, diagnostics) = artifacts.into_link_input(&sources, []).unwrap();
    assert!(diagnostics.is_empty());
}

#[test]
fn parse_failure_replaces_a_previous_success() {
    let source = SourceFile::new("retry.js", "fetch('/x');").unwrap();
    let mut sources = SourceTable::default();
    sources.insert(source.clone()).unwrap();
    let mut artifacts = AnalysisArtifacts::default();
    artifacts.record_analyzed(
        source.path(),
        lower(source.path().as_str(), "fetch('/x');").1,
    );
    artifacts.record_parse_failure(source.path().clone(), parse_failure("retry.js"));

    let (_, diagnostics) = artifacts.into_link_input(&sources, []).unwrap();
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn qualified_ids_reject_missing_importer_modules() {
    let source = SourceFile::new("missing.js", "import value from './dep.js';").unwrap();
    let mut artifacts = AnalysisArtifacts::default();
    artifacts.record_analyzed(
        source.path(),
        SemanticAnalyzer::new(&Environment::default(), &AnalysisLimits::default())
            .analyze_source(&source)
            .unwrap(),
    );

    assert_eq!(
        artifacts.authored_requests.qualified_ids(&BTreeMap::new()),
        Err(ProjectPhaseError::UnknownImporter("missing.js".into()))
    );
}

#[test]
fn into_link_input_accepts_authored_and_rejects_unknown_outcomes() {
    let source = SourceFile::new("main.js", "import value from './dep.js';").unwrap();
    let mut sources = SourceTable::default();
    sources.insert(source.clone()).unwrap();

    let (link_input, parse_diagnostics) = {
        let mut artifacts = AnalysisArtifacts::default();
        let requests = artifacts.record_analyzed(
            source.path(),
            SemanticAnalyzer::new(&Environment::default(), &AnalysisLimits::default())
                .analyze_source(&source)
                .unwrap(),
        );
        let key = requests[0].key().clone();
        artifacts
            .into_link_input(&sources, [(key, ResolverOutcome::Missing)])
            .unwrap()
    };
    assert!(parse_diagnostics.is_empty());
    assert_eq!(link_input.resolution_count(), 1);

    let mut artifacts = AnalysisArtifacts::default();
    let requests = artifacts.record_analyzed(
        source.path(),
        SemanticAnalyzer::new(&Environment::default(), &AnalysisLimits::default())
            .analyze_source(&source)
            .unwrap(),
    );
    let mut unknown = requests[0].key().clone();
    unknown = ResolutionRequestKey::new(
        unknown.importer().clone(),
        ResolutionRequestKind::Require,
        unknown.range_owned(),
    );
    let error = artifacts.into_link_input(&sources, [(unknown, ResolverOutcome::Missing)]);
    assert!(matches!(error, Err(ProjectPhaseError::UnknownRequest(_))));
}
