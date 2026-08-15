//! Bounded JavaScript/TypeScript parsing and source-position conversion.

use glass_lint_datastructures::SourceRange;
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_parser::{
    EsSyntax, Parser, StringInput, Syntax, TsSyntax,
    lexer::Lexer,
    unstable::{Capturing, TokenAndSpan},
};
use swc_ecma_transforms_base::resolver;
use swc_ecma_transforms_typescript::strip;

mod depth;
use depth::DepthScanner;

use crate::{
    MAX_SOURCE_BYTES, SourceLineIndex,
    project::{DiagnosticCode, SourceFile, types::DiagnosticKind},
};

/// Maximum syntactic nesting accepted before invoking recursive parser and
/// visitor machinery. This is deliberately checked on source text so a
/// hostile tree cannot first force an unbounded AST allocation.
#[cfg(test)]
const MAX_SYNTAX_DEPTH: usize = crate::limits::DEFAULT_SYNTAX_DEPTH;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
/// Structured parser failure with an optional source range.
pub struct ParseDiagnostic {
    /// Stable diagnostic code.
    code: DiagnosticCode,
    /// Human-readable parser message.
    message: String,
    /// Authored filename.
    filename: String,
    range: Option<SourceRange>,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) failure: ParseFailureKind,
}

impl ParseDiagnostic {
    pub(crate) fn new(
        failure: ParseFailureKind,
        message: impl Into<String>,
        filename: impl Into<String>,
        range: Option<SourceRange>,
    ) -> Self {
        Self {
            code: failure.diagnostic().0.into(),
            message: message.into(),
            filename: filename.into(),
            range,
            failure,
        }
    }

    #[must_use]
    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    #[must_use]
    pub fn range(&self) -> Option<&SourceRange> {
        self.range.as_ref()
    }

    #[must_use]
    pub fn failure_kind(&self) -> ParseFailureKind {
        self.failure
    }
}

impl std::fmt::Display for ParseDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for ParseDiagnostic {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ParseFailureKind {
    Syntax,
    SourceTooLarge,
    SyntaxDepth,
}

impl ParseFailureKind {
    pub(crate) fn diagnostic(self) -> (DiagnosticKind, &'static str) {
        match self {
            Self::Syntax => (DiagnosticKind::SyntaxError, "source could not be parsed"),
            Self::SourceTooLarge => (
                DiagnosticKind::SourceTooLarge,
                "source exceeds the analysis limit",
            ),
            Self::SyntaxDepth => (
                DiagnosticKind::SyntaxDepthExceeded,
                "source exceeds the nesting-depth analysis limit",
            ),
        }
    }
}

/// Source languages accepted by the core parser.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SourceLanguage {
    /// JavaScript/JSX syntax family.
    JavaScript,
    /// Runtime TypeScript syntax after in-memory stripping.
    TypeScript,
}

impl SourceLanguage {
    fn syntax(self) -> Syntax {
        match self {
            Self::JavaScript => Syntax::Es(EsSyntax {
                jsx: true,
                decorators: true,
                fn_bind: true,
                export_default_from: true,
                import_attributes: true,
                allow_super_outside_method: true,
                allow_return_outside_function: true,
                auto_accessors: true,
                explicit_resource_management: true,
                ..Default::default()
            }),
            Self::TypeScript => Syntax::Typescript(TsSyntax {
                tsx: false,
                decorators: true,
                ..Default::default()
            }),
        }
    }
}

/// Parsed program consumed by semantic analysis.
pub struct ParsedSource {
    /// SWC AST consumed by semantic analysis.
    pub(crate) program: Program,
    /// Absolute SWC position assigned to authored byte offset zero.
    pub(crate) source_start: swc_common::BytePos,
    /// Source-coordinate index built while parser diagnostics are available.
    pub(crate) lines: SourceLineIndex,
}

/// Owns the source, parser mode, bounded-depth policy, and diagnostics needed
/// to parse one accepted source.
pub struct SourceParser {
    source: SourceFile,
    file: Lrc<swc_common::SourceFile>,
    syntax: Syntax,
    depth_guard: SyntaxDepthGuard,
}

impl SourceParser {
    /// Construct a parser using the test-only default depth limit.
    #[cfg(test)]
    pub(crate) fn new(source: &SourceFile) -> Result<Self, ParseDiagnostic> {
        Self::with_syntax_depth(source, MAX_SYNTAX_DEPTH)
    }

    /// Construct a parser with an explicit structural depth limit. The source
    /// carries its validated path, text, and language; TypeScript is normalized
    /// after parsing and JavaScript passes through.
    pub(crate) fn with_syntax_depth(
        source: &SourceFile,
        max_syntax_depth: usize,
    ) -> Result<Self, ParseDiagnostic> {
        Self::validate_source(source)?;
        let source_map = Lrc::new(SourceMap::default());
        let file = source_map.new_source_file(
            FileName::Custom(source.path().as_str().into()).into(),
            source.source().to_string(),
        );
        Ok(Self {
            source: source.clone(),
            file,
            syntax: source.language().syntax(),
            depth_guard: SyntaxDepthGuard::new(
                DepthScanner::raw_bound(source.source()),
                max_syntax_depth,
            ),
        })
    }

    fn validate_source(source: &SourceFile) -> Result<(), ParseDiagnostic> {
        if source.source().len() <= MAX_SOURCE_BYTES {
            return Ok(());
        }
        Err(ParseDiagnostic::new(
            ParseFailureKind::SourceTooLarge,
            format!("source exceeds the {MAX_SOURCE_BYTES} byte analysis limit"),
            source.path().to_string(),
            None,
        ))
    }

    pub(crate) fn parse(self) -> Result<ParsedSource, ParseDiagnostic> {
        let program = self.parse_program()?;
        let lines = SourceLineIndex::from_text(self.source.source().clone());
        Ok(ParsedSource {
            program: self.lower_program(program),
            source_start: self.file.start_pos,
            lines,
        })
    }

    /// Parse and lower a program for consumers that do not need source
    /// coordinates. Parser diagnostics still construct an index on demand;
    /// successful syntax-only analysis does not.
    pub(crate) fn parse_program_only(self) -> Result<Program, ParseDiagnostic> {
        let program = self.parse_program()?;
        Ok(self.lower_program(program))
    }

    fn parse_program(&self) -> Result<Program, ParseDiagnostic> {
        if self.depth_guard.check_before_parse(&self.file, self.syntax) {
            return Err(self.syntax_depth_diagnostic());
        }

        let lexer = Lexer::new(
            self.syntax,
            EsVersion::EsNext,
            StringInput::from(&*self.file),
            None,
        );
        let mut parser = Parser::new_from(Capturing::new(lexer));
        let parsed = parser.parse_program();
        if self
            .depth_guard
            .check_after_parse(parser.input().iter.tokens())
        {
            return Err(self.syntax_depth_diagnostic());
        }
        parsed.map_err(|error| self.parser_diagnostic(&error))
    }

    #[cfg(test)]
    fn syntax_depth(&self) -> SyntaxDepthOutcome {
        self.depth_guard.scan_source(&self.file, self.syntax)
    }

    fn lower_program(&self, program: Program) -> Program {
        match self.source.language() {
            SourceLanguage::JavaScript => program,
            SourceLanguage::TypeScript => GLOBALS.set(&Globals::default(), || {
                let unresolved_mark = Mark::new();
                let top_level_mark = Mark::new();
                let mut program = program;
                program = program.apply(resolver(unresolved_mark, top_level_mark, true));
                program = program.apply(strip(unresolved_mark, top_level_mark));
                program
            }),
        }
    }

    fn syntax_depth_diagnostic(&self) -> ParseDiagnostic {
        ParseDiagnostic::new(
            ParseFailureKind::SyntaxDepth,
            format!(
                "source exceeds the {} nesting-depth analysis limit",
                self.depth_guard.max_depth()
            ),
            self.source.path().to_string(),
            None,
        )
    }

    fn parser_diagnostic(&self, error: &swc_ecma_parser::error::Error) -> ParseDiagnostic {
        let range = self.parser_range(error.span());
        ParseDiagnostic::new(
            ParseFailureKind::Syntax,
            format!(
                "{} parse error: {}",
                match self.source.language() {
                    SourceLanguage::JavaScript => "JavaScript",
                    SourceLanguage::TypeScript => "TypeScript",
                },
                error.kind().msg()
            ),
            self.source.path().to_string(),
            range,
        )
    }

    fn parser_range(&self, span: swc_common::Span) -> Option<SourceRange> {
        if span.is_dummy() {
            return None;
        }

        let start = span.lo.0.checked_sub(self.file.start_pos.0)?;
        let end = span.hi.0.checked_sub(self.file.start_pos.0)?;
        SourceLineIndex::from_text(self.source.source().clone())
            .range_from_offsets(start, end)
            .ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntaxDepthOutcome {
    WithinLimit(usize),
    Exceeded,
}

impl SyntaxDepthOutcome {
    fn is_exceeded(self) -> bool {
        matches!(self, Self::Exceeded)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntaxDepthPhase {
    PreParse,
    PostParse,
}

/// Owns the phase in which syntax-depth scanning is safe for one source.
/// Conservative raw bounds force a source scan before SWC sees the input;
/// otherwise the parser's token stream is checked after parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyntaxDepthGuard {
    max_depth: usize,
    phase: SyntaxDepthPhase,
}

impl SyntaxDepthGuard {
    fn new(raw_bound: usize, max_depth: usize) -> Self {
        let phase = if raw_bound > max_depth {
            SyntaxDepthPhase::PreParse
        } else {
            SyntaxDepthPhase::PostParse
        };
        Self { max_depth, phase }
    }

    fn check_before_parse(self, file: &swc_common::SourceFile, syntax: Syntax) -> bool {
        match self.phase {
            SyntaxDepthPhase::PreParse => DepthScanner::new(self.max_depth)
                .scan_source(file, syntax)
                .is_exceeded(),
            SyntaxDepthPhase::PostParse => false,
        }
    }

    fn check_after_parse(self, tokens: &[TokenAndSpan]) -> bool {
        match self.phase {
            SyntaxDepthPhase::PreParse => false,
            SyntaxDepthPhase::PostParse => DepthScanner::new(self.max_depth)
                .scan_tokens(tokens)
                .is_exceeded(),
        }
    }

    #[cfg(test)]
    fn scan_source(self, file: &swc_common::SourceFile, syntax: Syntax) -> SyntaxDepthOutcome {
        DepthScanner::new(self.max_depth).scan_source(file, syntax)
    }

    fn max_depth(self) -> usize {
        self.max_depth
    }
}

#[cfg(test)]
fn syntax_depth_for_test(source: &str) -> usize {
    let source = SourceFile::with_language("test.js", source, SourceLanguage::JavaScript)
        .expect("test parser input should have a valid relative path");
    let depth = SourceParser::new(&source)
        .expect("test source should be accepted")
        .syntax_depth();
    match depth {
        SyntaxDepthOutcome::WithinLimit(depth) => depth,
        SyntaxDepthOutcome::Exceeded => MAX_SYNTAX_DEPTH + 1,
    }
}

#[cfg(test)]
mod tests;
