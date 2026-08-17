//! Private parser-to-artifact semantic-analysis boundary.
//!
//! Parser and AST details stop here. Downstream project analysis receives an
//! immutable local artifact and its source map, never a parsed program.

pub(super) mod budget;
pub(super) mod status;

use std::{collections::BTreeMap, sync::Arc};

use glass_lint_datastructures::{ByteRange, NameTable};
use smol_str::SmolStr;
use swc_common::{Span, Spanned};
use swc_ecma_ast::Program;

#[cfg(test)]
use crate::analysis::resolution::test_environment;
use crate::{
    AnalysisLimits, Environment, ParseDiagnostic,
    analysis::{
        DerivedPhaseCapabilities, LocatedSourceContext, SemanticArtifact, SemanticBudget,
        facts::{self, Building, BuiltFacts, FactStream, MAX_FACTS, SemanticFacts},
        model::module,
        resolution::Resolver,
        scope::{ScopeCollectionIssue, ScopeGraph, ScopedProgram},
        semantic::status::{IncompleteReason, LocalAnalysisStatus},
        syntax::{SymbolCallProvenance, name::MAX_NAMES},
    },
    diagnostic::SourceLineIndex,
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

#[derive(Clone, Debug)]
/// Converts SWC `BytePos` spans to zero-based `ByteRange` values relative to
/// the authored source text. Validation ensures the result is within bounds
/// and on UTF-8 character boundaries.
pub(in crate::analysis) struct SpanNormalizer {
    /// SWC `BytePos` value assigned to authored byte offset zero.
    start: u32,
    /// Shared source-coordinate validator for authored byte ranges.
    lines: Arc<SourceLineIndex>,
}

impl SpanNormalizer {
    pub(in crate::analysis) fn new(source_start: swc_common::BytePos, source: &SourceText) -> Self {
        Self {
            start: source_start.0,
            lines: Arc::new(SourceLineIndex::from_text(source.clone())),
        }
    }

    pub(in crate::analysis) fn with_index(
        source_start: swc_common::BytePos,
        lines: Arc<SourceLineIndex>,
    ) -> Self {
        Self {
            start: source_start.0,
            lines,
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
        self.lines
            .byte_range_from_offsets(offset, end)
            .map_err(|_| InvalidParserSpan)
    }

    pub(in crate::analysis) fn into_lines(self) -> Arc<SourceLineIndex> {
        self.lines
    }
}

impl Default for SpanNormalizer {
    fn default() -> Self {
        Self::new(swc_common::BytePos(0), &SourceText::default())
    }
}

#[derive(Clone)]
pub struct AnalyzedSource {
    source: LocatedSourceContext,
    semantic: Arc<SemanticArtifact>,
}

impl AnalyzedSource {
    pub(crate) fn new(source: LocatedSourceContext, semantic: Arc<SemanticArtifact>) -> Self {
        Self { source, semantic }
    }

    pub(crate) fn into_parts(self) -> (LocatedSourceContext, Arc<SemanticArtifact>) {
        (self.source, self.semantic)
    }

    pub(crate) fn semantic_handle(&self) -> Arc<SemanticArtifact> {
        Arc::clone(&self.semantic)
    }

    pub(crate) fn source_index(&self) -> Arc<SourceLineIndex> {
        self.source.clone_lines()
    }
}

/// Per-file semantic-analysis stage. Owns the environment and limits that the
/// analysis pipeline needs, without coupling to the full `Linter`.
pub struct SemanticAnalyzer<'a> {
    environment: &'a Environment,
    limits: &'a AnalysisLimits,
    name_limit: usize,
}

impl<'a> SemanticAnalyzer<'a> {
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

    /// Analyze an already-parsed SWC program into an immutable semantic
    /// artifact. The scope graph, resolver, and fact builder are deliberately
    /// consumed in order so no intermediate analysis state escapes this
    /// boundary.
    pub(in crate::analysis) fn analyze_program(
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
            MAX_FACTS,
        )
        .freeze(self.environment, self.limits, program.span())
    }

    #[cfg(test)]
    fn with_name_limit(mut self, name_limit: usize) -> Self {
        self.name_limit = name_limit;
        self
    }

    /// Analyze one source file into an immutable semantic artifact. The
    /// analysis runs scope planning, collection against the plan, and fact
    /// building against the frozen resolver. Matcher indexes and function
    /// effects are then derived together from the frozen fact tape. The
    /// result is ready for project linking and matcher projection.
    pub fn analyze_source(&self, source: &SourceFile) -> Result<AnalyzedSource, ParseDiagnostic> {
        let parsed =
            SourceParser::with_syntax_depth(source, self.limits.syntax_depth())?.parse()?;
        let coordinates = SpanNormalizer::with_index(parsed.source_start, Arc::new(parsed.lines));
        let semantic = self.analyze_program(&parsed.program, &coordinates);

        Ok(AnalyzedSource::new(
            LocatedSourceContext::from_normalizer(source.path().clone(), coordinates),
            Arc::new(semantic),
        ))
    }
}

fn check_fact_construction_incompleteness(
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

#[derive(Debug)]
struct AnalysisCompletion {
    status: LocalAnalysisStatus,
    capabilities: DerivedPhaseCapabilities,
}

impl AnalysisCompletion {
    fn new() -> Self {
        Self {
            status: LocalAnalysisStatus::default(),
            capabilities: DerivedPhaseCapabilities::enabled(),
        }
    }

    fn record_scope_issue(&mut self, issue_count: usize) {
        self.record_incomplete(IncompleteReason::ScopeShapeMismatch { count: issue_count });
    }

    fn record_failure(&mut self, reason: Option<IncompleteReason>) {
        if let Some(reason) = reason {
            self.record_incomplete(reason);
        }
    }

    fn record_incomplete(&mut self, reason: IncompleteReason) {
        self.status.record(reason);
        self.capabilities.disable_derived_phases();
    }

    fn assess(
        issues: &[ScopeCollectionIssue],
        stream: &FactStream<Building>,
        resolver: &Resolver,
        limits: &AnalysisLimits,
    ) -> Self {
        let mut policy = Self::new();
        if !issues.is_empty() {
            policy.record_scope_issue(issues.len());
        }

        policy.record_failure(check_fact_construction_incompleteness(
            stream,
            resolver,
            limits,
            resolver.budget(),
        ));
        policy.record_failure(check_invalid_parser_span(stream));
        policy.record_failure(check_name_exhaustion(resolver));
        policy
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
        let built = facts::build(program, &mut resolver, max_facts);

        Self {
            resolver,
            issues,
            built,
        }
    }

    fn assess_completion(&self, limits: &AnalysisLimits) -> AnalysisCompletion {
        AnalysisCompletion::assess(&self.issues, &self.built.stream, &self.resolver, limits)
    }

    fn derive_export_origins(
        &self,
        interface: &module::ModuleInterface,
        completion: &AnalysisCompletion,
        program_span: Span,
    ) -> BTreeMap<SmolStr, SymbolCallProvenance> {
        if !completion.capabilities.availability().is_enabled() {
            return BTreeMap::new();
        }
        interface
            .exports()
            .filter_map(|(_, export)| match export {
                module::ModuleExport::Local { name } => Some((
                    name.clone(),
                    self.resolver.exported_provenance(name, program_span),
                )),
                module::ModuleExport::Value
                | module::ModuleExport::ReExport { .. }
                | module::ModuleExport::Namespace { .. }
                | module::ModuleExport::Unknown => None,
            })
            .collect()
    }

    fn annotate_name_exhaustion(
        mut stream: FactStream<Building>,
        name_table_exhausted: bool,
    ) -> FactStream<Building> {
        if name_table_exhausted {
            stream.mark_name_exhausted();
        }
        stream
    }

    fn seal(
        self,
        environment: &Environment,
        completion: AnalysisCompletion,
        program_span: Span,
        effect_limit: usize,
    ) -> SemanticArtifact {
        let export_origins =
            self.derive_export_origins(&self.built.interface, &completion, program_span);
        let name_table_exhausted = self.resolver.name_table_exhausted();
        let Self {
            resolver, built, ..
        } = self;
        let stream = Self::annotate_name_exhaustion(built.stream, name_table_exhausted);
        let interface = built.interface;
        let stream = resolver.freeze_into(stream);
        let capabilities = completion.capabilities;
        let facts = SemanticFacts::from_analysis(stream, interface, environment, capabilities);
        SemanticArtifact::from_analysis(
            facts,
            export_origins,
            effect_limit,
            capabilities,
            completion.status,
        )
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
        let completion = self.assess_completion(limits);
        self.seal(
            environment,
            completion,
            program_span,
            limits.effect_operations(),
        )
    }
}

#[cfg(test)]
pub(in crate::analysis) fn with_test_collection<R>(
    program: &Program,
    source: &str,
    callback: impl for<'a> FnOnce(ResolvedProgram<'a>) -> R,
) -> R {
    let environment = test_environment();
    let limits = AnalysisLimits::default();
    let budget = SemanticBudget::new(limits.semantic_operations());
    let names = NameTable::with_max_entries(MAX_NAMES);
    let scoped = ScopeGraph::collect_scoped_program(program, &environment, names, &budget);
    let resolved = ResolvedProgram::collect(
        scoped,
        program,
        SpanNormalizer::for_program(program, source),
        &budget,
        MAX_FACTS,
    );
    callback(resolved)
}

#[cfg(test)]
mod tests;
