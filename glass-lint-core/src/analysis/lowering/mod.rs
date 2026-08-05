//! Private parser-to-artifact lowering boundary.
//!
//! Parser and AST details stop here. Downstream project analysis receives an
//! immutable local artifact and its source map, never a parsed program.

pub(super) mod budget;
pub(super) mod status;

use std::{collections::BTreeMap, sync::Arc};

use glass_lint_datastructures::{ByteRange, NameTable};
use swc_common::{Span, Spanned};
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

#[cfg(test)]
use crate::analysis::{facts::MAX_FACTS, resolution::test_environment};
use crate::{
    AnalysisLimits, Environment, ParseDiagnostic,
    analysis::{
        LocatedSourceContext, SemanticArtifact, SemanticBudget,
        facts::{self, Building, BuiltFacts, FactStream, SemanticFacts},
        lowering::status::{AnalysisComponent, AnalysisStatus, IncompleteReason, StatusScope},
        module,
        resolution::Resolver,
        scope::{ScopeCollectionIssue, ScopeGraph, ScopedProgram},
        syntax::name::MAX_NAMES,
    },
    parse::SourceParser,
    project::{SourceFile, SourceText},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct InvalidParserSpan;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct ParserSpanKey {
    lo: u32,
    hi: u32,
}

impl From<swc_common::Span> for ParserSpanKey {
    fn from(span: swc_common::Span) -> Self {
        Self {
            lo: span.lo.0,
            hi: span.hi.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
/// Converts SWC `BytePos` spans to zero-based `ByteRange` values relative to
/// the authored source text. Validation ensures the result is within bounds
/// and on UTF-8 character boundaries.
pub(in crate::analysis) struct SpanNormalizer {
    /// SWC `BytePos` value assigned to authored byte offset zero.
    start: u32,
    /// Authored source text, used for UTF-8 boundary validation.
    source: SourceText,
}

impl SpanNormalizer {
    pub(in crate::analysis) fn new(source_start: swc_common::BytePos, source: &SourceText) -> Self {
        Self {
            start: source_start.0,
            source: source.clone(),
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn for_program(program: &Program, source: &str) -> Self {
        Self::new(program.span().lo, &SourceText::from(source))
    }

    pub(in crate::analysis) fn normalize(
        &self,
        span: swc_common::Span,
    ) -> Result<ByteRange, InvalidParserSpan> {
        let offset = span.lo.0.checked_sub(self.start).ok_or(InvalidParserSpan)?;
        let end = span.hi.0.checked_sub(self.start).ok_or(InvalidParserSpan)?;
        let source_len = u32::try_from(self.source.len()).unwrap_or(u32::MAX);

        if end > source_len {
            return Err(InvalidParserSpan);
        }

        if !self.source.is_char_boundary(offset as usize)
            || !self.source.is_char_boundary(end as usize)
        {
            return Err(InvalidParserSpan);
        }

        ByteRange::new(offset, end).map_err(|_| InvalidParserSpan)
    }
}

#[derive(Clone)]
pub struct LoweredSource {
    source: LocatedSourceContext,
    semantic: Arc<SemanticArtifact>,
}

impl LoweredSource {
    pub(crate) fn new(source: LocatedSourceContext, semantic: Arc<SemanticArtifact>) -> Self {
        Self { source, semantic }
    }

    pub(crate) fn into_parts(self) -> (LocatedSourceContext, Arc<SemanticArtifact>) {
        (self.source, self.semantic)
    }
}

/// Per-file lowering stage. Owns the environment and limits that the
/// lowering pipeline needs, without coupling to the full `Linter`.
pub struct Lowerer<'a> {
    environment: &'a Environment,
    limits: &'a AnalysisLimits,
    name_limit: usize,
}

impl<'a> Lowerer<'a> {
    pub fn new(environment: &'a Environment, limits: &'a AnalysisLimits) -> Self {
        Self {
            environment,
            limits,
            name_limit: MAX_NAMES,
        }
    }

    pub fn environment(&self) -> &Environment {
        self.environment
    }

    pub fn limits(&self) -> &AnalysisLimits {
        self.limits
    }

    /// Lower an already-parsed SWC program into an immutable semantic
    /// artifact. The scope graph, resolver, and fact builder are deliberately
    /// consumed in order so no intermediate analysis state escapes this
    /// boundary.
    pub(in crate::analysis) fn lower_program(
        &self,
        program: &Program,
        coordinates: &SpanNormalizer,
    ) -> SemanticArtifact {
        let budget = SemanticBudget::new(self.limits.semantic_operations());
        let names = NameTable::with_max_entries(self.name_limit);
        let scoped_program =
            ScopeGraph::collect_scoped_program(program, self.environment, names, &budget);

        ResolvedProgram::collect(
            scoped_program,
            program,
            coordinates.clone(),
            &budget,
            self.limits.semantic_operations(),
        )
        .freeze(self.environment, self.limits, program.span())
    }

    #[cfg(test)]
    fn with_name_limit(mut self, name_limit: usize) -> Self {
        self.name_limit = name_limit;
        self
    }

    /// Lower one source file into an immutable semantic artifact. The lowering
    /// runs scope planning, collection against the plan, and fact building
    /// against the frozen resolver. Matcher indexes and function effects are
    /// then derived together from the frozen fact tape. The result is ready for
    /// project linking and matcher projection.
    pub fn lower_source(&self, source: &SourceFile) -> Result<LoweredSource, ParseDiagnostic> {
        let parsed =
            SourceParser::with_syntax_depth(source, self.limits.syntax_depth())?.parse()?;
        let coordinates = SpanNormalizer::new(parsed.source_start, source.source());
        let semantic = self.lower_program(&parsed.program, &coordinates);

        Ok(LoweredSource::new(
            LocatedSourceContext::new(source),
            Arc::new(semantic),
        ))
    }
}

fn check_facts_budget(
    stream: &FactStream<Building>,
    resolver: &Resolver,
    limits: &AnalysisLimits,
    budget: &SemanticBudget,
) -> Option<IncompleteReason> {
    if budget.exhausted() {
        return Some(IncompleteReason::SemanticBudgetExhausted {
            limit: limits.semantic_operations(),
            used: budget.used(),
        });
    }
    if stream.budget_exhausted() {
        return Some(IncompleteReason::FactCapacityExhausted {
            limit: stream.max_facts(),
        });
    }
    if stream.path_exhausted() {
        return Some(IncompleteReason::PathCapacityExhausted);
    }
    if resolver.value_arena_exhausted() && !resolver.name_table_exhausted() {
        return Some(IncompleteReason::ValueArenaExhausted);
    }
    if !stream.is_structurally_valid() && !stream.name_exhausted() {
        return Some(IncompleteReason::BudgetExhausted {
            component: AnalysisComponent::Facts,
            limit: limits.semantic_operations(),
            observed: Some(budget.used()),
        });
    }
    None
}

fn check_invalid_parser_span(stream: &FactStream<Building>) -> Option<IncompleteReason> {
    stream
        .invalid_parser_span()
        .then_some(IncompleteReason::InvalidParserSpan)
}

fn check_name_exhaustion(resolver: &Resolver) -> Option<IncompleteReason> {
    resolver
        .name_exhaustion()
        .map(|exhaustion| IncompleteReason::NameExhausted {
            limit: exhaustion.limit,
            attempted: exhaustion.attempted,
        })
}

#[derive(Clone, Copy, Debug)]
struct LoweringCapabilities {
    export_origins: bool,
    effects: bool,
}

#[derive(Debug)]
struct LoweringCompletion {
    status: AnalysisStatus,
    capabilities: LoweringCapabilities,
}

impl LoweringCompletion {
    fn assess(
        issues: &[ScopeCollectionIssue],
        stream: &FactStream<Building>,
        resolver: &Resolver,
        limits: &AnalysisLimits,
    ) -> Self {
        let budget = resolver.budget;
        let mut status = AnalysisStatus::default();

        if !issues.is_empty() {
            status.record(
                StatusScope::Project,
                IncompleteReason::ScopeShapeMismatch {
                    count: issues.len(),
                },
            );
        }

        let budget_exhausted = budget.exhausted()
            || stream.budget_exhausted()
            || stream.path_exhausted()
            || resolver.value_arena_exhausted()
            || !stream.is_structurally_valid();
        if let Some(reason) = check_facts_budget(stream, resolver, limits, budget) {
            status.record(StatusScope::Project, reason);
        }
        if let Some(reason) = check_invalid_parser_span(stream) {
            status.record(StatusScope::Project, reason);
        }
        if let Some(reason) = check_name_exhaustion(resolver) {
            status.record(StatusScope::Project, reason);
        }

        Self {
            status,
            capabilities: LoweringCapabilities {
                export_origins: !budget_exhausted,
                effects: !budget_exhausted,
            },
        }
    }
}

/// The resolved local-analysis phase. The scope-frozen resolver, the
/// scope-collection issues, and the built (unfrozen) fact stream travel
/// together until the single consuming `freeze` transition seals the name and
/// value tables into the immutable artifact.
pub(in crate::analysis) struct ResolvedProgram<'a> {
    resolver: Resolver<'a>,
    issues: Vec<ScopeCollectionIssue>,
    built: BuiltFacts,
}

impl<'a> ResolvedProgram<'a> {
    /// Collect the scoped program and run the fact-building walk against the
    /// resolver, retaining all resolved-phase state for the freeze transition.
    pub(in crate::analysis) fn collect(
        scoped: ScopedProgram,
        program: &Program,
        coordinates: SpanNormalizer,
        budget: &'a SemanticBudget,
        max_facts: usize,
    ) -> Self {
        let ScopedProgram { graph, issues } = scoped;
        let mut resolver = Resolver::new(graph, coordinates, budget);
        let mut builder = facts::FactBuilder::with_limit(&mut resolver, max_facts);

        VisitWith::visit_with(program, &mut builder);
        let built = builder.into_built_facts();

        Self {
            resolver,
            issues,
            built,
        }
    }

    /// One consuming transition from the resolved phase to the immutable
    /// artifact. The name and value tables are extracted from the resolver
    /// inside this transition and sealed into the frozen stream.
    pub(in crate::analysis) fn freeze(
        self,
        environment: &Environment,
        limits: &AnalysisLimits,
        program_span: Span,
    ) -> SemanticArtifact {
        let Self {
            resolver,
            issues,
            built,
        } = self;
        let mut stream = built.stream;
        let interface = built.interface;
        let completion = LoweringCompletion::assess(&issues, &stream, &resolver, limits);

        let export_origins = if completion.capabilities.export_origins {
            interface
                .exports()
                .filter_map(|(_, export)| match export {
                    module::ModuleExport::Local { name } => Some((
                        name.clone(),
                        resolver.exported_provenance(name, program_span),
                    )),
                    module::ModuleExport::Value
                    | module::ModuleExport::ReExport { .. }
                    | module::ModuleExport::Namespace { .. }
                    | module::ModuleExport::Unknown => None,
                })
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        };

        if resolver.name_table_exhausted() {
            stream.mark_name_exhausted();
        }

        let stream = resolver.freeze_into(stream);
        let facts = SemanticFacts::from_lowering(stream, interface, environment);
        SemanticArtifact::from_lowering(
            facts,
            export_origins,
            limits.effect_operations(),
            completion.capabilities.effects,
            completion.status,
        )
    }
}

#[cfg(test)]
impl ResolvedProgram<'static> {
    pub(in crate::analysis) fn collect_for_test(program: &Program, source: &str) -> Self {
        let environment = test_environment();
        let budget = Box::leak(Box::new(SemanticBudget::default()));
        let names = NameTable::with_max_entries(MAX_NAMES);
        let scoped = ScopeGraph::collect_scoped_program(program, &environment, names, budget);
        Self::collect(
            scoped,
            program,
            SpanNormalizer::for_program(program, source),
            budget,
            MAX_FACTS,
        )
    }
}

#[cfg(test)]
mod tests {
    use swc_common::{BytePos, Span};

    use super::*;

    #[test]
    fn swc_span_is_normalized_to_zero_based_byte_range_once() {
        let normalizer = SpanNormalizer::new(BytePos(40), &SourceText::from("aé\r\n"));
        assert_eq!(
            normalizer.normalize(Span::new(BytePos(40), BytePos(43))),
            Ok(glass_lint_datastructures::ByteRange::new(0, 3).unwrap())
        );
        assert!(
            normalizer
                .normalize(Span::new(BytePos(42), BytePos(43)))
                .is_err()
        );
        assert!(
            normalizer
                .normalize(Span::new(BytePos(40), BytePos(46)))
                .is_err()
        );
    }

    #[test]
    fn name_exhaustion_invalidates_indexes_and_effects_with_an_accurate_status() {
        let source = "function helper(options) { return options.send; } helper({ send: 1 });";
        let parsed =
            crate::parse_test_source(source, "name-exhaustion.js").expect("source should parse");
        let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));
        let artifact = Lowerer::new(
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
        )
        .with_name_limit(2)
        .lower_program(&parsed.program, &coordinates);

        assert!(!artifact.facts().stream().is_valid());
        assert!(artifact.facts().matcher_index().is_empty());
        assert!(artifact.effects().iter_effects().next().is_none());
        let (_, project_diagnostics) = artifact.status().diagnostics();
        assert_eq!(project_diagnostics.len(), 1);
        assert_eq!(
            project_diagnostics[0].code().as_str(),
            "semantic_name_budget_exhausted"
        );
        assert!(project_diagnostics[0].message().contains("limit=2"));
        assert!(project_diagnostics[0].message().contains("attempted=3"));

        let repeated = Lowerer::new(
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
        )
        .with_name_limit(2)
        .lower_program(&parsed.program, &coordinates);
        assert_eq!(
            format!("{:?}", artifact.facts().stream().facts()),
            format!("{:?}", repeated.facts().stream().facts())
        );
        assert_eq!(artifact.status(), repeated.status());
    }

    #[test]
    fn tiny_semantic_budget_stops_traversal_and_skips_derived_phases() {
        let source = "
            function helper(a, b) { return a + b; }
            function process(c, d) { return helper(c, d); }
            function compute(e, f) { return process(e, f); }
            export const result = compute(1, 2);
            export function identity(x) { return x; }
        ";
        let parsed =
            crate::parse_test_source(source, "budget-exhaustion.js").expect("source should parse");
        let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));

        let limits = crate::AnalysisLimits::default()
            .with_semantic_operations(10)
            .expect("valid limit");
        let artifact = Lowerer::new(&crate::Environment::default(), &limits)
            .lower_program(&parsed.program, &coordinates);

        assert!(!artifact.status().is_complete());
        assert!(artifact.effects().iter_effects().next().is_none());
        // With budget of 10, the fact stream has very few facts
        assert!(artifact.facts().stream().facts().len() < 5);
        // Export origin lookups return nothing since the phase was skipped
        assert!(artifact.export_origin("result").is_none());
        assert!(artifact.export_origin("identity").is_none());
    }

    #[test]
    fn large_semantic_budget_produces_complete_artifact_with_export_origins() {
        let source = "
            function helper(a, b) { return a + b; }
            function process(c, d) { return helper(c, d); }
            function compute(e, f) { return process(e, f); }
            export const result = compute(1, 2);
            export function identity(x) { return x; }
        ";
        let parsed =
            crate::parse_test_source(source, "budget-sufficient.js").expect("source should parse");
        let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));

        let artifact = Lowerer::new(
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
        )
        .lower_program(&parsed.program, &coordinates);

        assert!(artifact.status().is_complete());
        assert!(artifact.facts().stream().facts().len() > 10);
        assert!(artifact.effects().iter_effects().next().is_some());
        // Export origins should be present since the phase ran
        assert!(artifact.export_origin("result").is_some());
        assert!(artifact.export_origin("identity").is_some());
    }

    #[test]
    fn invalid_parser_span_records_incomplete_without_fake_location() {
        let source = "fetch('/remote');";
        let parsed = crate::parse_test_source(source, "main.js").unwrap();
        let invalid = SpanNormalizer::new(
            BytePos(parsed.source_start.0 + 100),
            &SourceText::from(source),
        );
        let artifact = Lowerer::new(
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
        )
        .lower_program(&parsed.program, &invalid);
        assert!(!artifact.status().is_complete());
        assert!(artifact.facts().stream().facts().is_empty());
        let (files, project) = artifact.status().diagnostics();
        assert!(files.is_empty());
        assert_eq!(project.len(), 1);
        assert_eq!(project[0].code().as_str(), "invalid_parser_span");
        assert!(project[0].location().is_none());
    }
}
