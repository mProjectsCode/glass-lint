//! Bounded JavaScript/TypeScript parsing and source-position conversion.

use glass_lint_datastructures::SourceRange;
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_parser::{
    EsSyntax, Parser, StringInput, Syntax, TsSyntax,
    lexer::Lexer,
    unstable::{Capturing, Token, TokenAndSpan},
};
use swc_ecma_transforms_base::resolver;
use swc_ecma_transforms_typescript::strip;

use crate::{
    MAX_SOURCE_BYTES, SourceLineIndex,
    project::{DiagnosticCode, SourceFile, types::DiagnosticKind},
};

/// Maximum syntactic nesting accepted before invoking recursive parser and
/// visitor machinery. This is deliberately checked on source text so a
/// hostile tree cannot first force an unbounded AST allocation.
#[cfg(test)]
const MAX_SYNTAX_DEPTH: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
/// Structured parser failure with an optional source range.
pub struct ParseDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable parser message.
    pub message: String,
    /// Authored filename.
    pub filename: String,
    pub range: Option<SourceRange>,
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
}

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
}

/// Owns the source, parser mode, bounded-depth policy, and diagnostics needed
/// to parse one accepted source.
pub struct SourceParser {
    source: SourceFile,
    file: Lrc<swc_common::SourceFile>,
    lines: SourceLineIndex,
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
            lines: SourceLineIndex::from_text(source.source().clone()),
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
        Ok(ParsedSource {
            program: self.lower_program(program),
            source_start: self.file.start_pos,
        })
    }

    fn parse_program(&self) -> Result<Program, ParseDiagnostic> {
        if self
            .depth_guard
            .check_before_parse(&self.file, self.syntax)
            .is_err()
        {
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
            .is_err()
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
        self.lines.range_from_offsets(start, end).ok()
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

    fn check_before_parse(
        self,
        file: &swc_common::SourceFile,
        syntax: Syntax,
    ) -> Result<(), SyntaxDepthError> {
        match self.phase {
            SyntaxDepthPhase::PreParse => DepthScanner::new(self.max_depth)
                .scan_source(file, syntax)
                .is_exceeded()
                .then_some(SyntaxDepthError::Exceeded)
                .map_or(Ok(()), Err),
            SyntaxDepthPhase::PostParse => Ok(()),
        }
    }

    fn check_after_parse(self, tokens: &[TokenAndSpan]) -> Result<(), SyntaxDepthError> {
        match self.phase {
            SyntaxDepthPhase::PreParse => Ok(()),
            SyntaxDepthPhase::PostParse => DepthScanner::new(self.max_depth)
                .scan_tokens(tokens)
                .is_exceeded()
                .then_some(SyntaxDepthError::Exceeded)
                .map_or(Ok(()), Err),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntaxDepthError {
    Exceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delimiter {
    Parenthesis,
    Bracket,
    Brace,
}

/// Owns the mutable state used while measuring syntactic nesting.
///
/// The source and post-parse scans share delimiter, member-chain, and
/// expression-ending state, while only the source scan needs regex recovery.
struct DepthScanner {
    delimiters: Vec<Delimiter>,
    depth: usize,
    maximum: usize,
    member_depth: usize,
    expression_can_end: bool,
    previous: Option<Token>,
    previous_postfix: bool,
    max_depth: usize,
}

impl DepthScanner {
    fn new(max_depth: usize) -> Self {
        Self {
            delimiters: Vec::new(),
            depth: 0,
            maximum: 0,
            member_depth: 0,
            expression_can_end: false,
            previous: None,
            previous_postfix: false,
            max_depth,
        }
    }

    fn scan_source(&mut self, file: &swc_common::SourceFile, syntax: Syntax) -> SyntaxDepthOutcome {
        let mut lexer = Lexer::new(syntax, EsVersion::EsNext, StringInput::from(file), None);
        let mut skip_to = 0usize;
        self.scan(&mut lexer, |scanner, token_and_span| {
            scanner.source_token(file, &token_and_span, &mut skip_to)
        })
    }

    fn scan_tokens(&mut self, tokens: &[TokenAndSpan]) -> SyntaxDepthOutcome {
        self.scan(tokens.iter(), |_, token_and_span| {
            Some(token_and_span.token)
        })
    }

    fn scan<I, F>(&mut self, tokens: I, mut token_for: F) -> SyntaxDepthOutcome
    where
        I: IntoIterator,
        F: FnMut(&mut Self, I::Item) -> Option<Token>,
    {
        for token_item in tokens {
            let Some(token) = token_for(self, token_item) else {
                continue;
            };
            if token == Token::Error {
                break;
            }
            if self.observe(token).is_err() {
                return SyntaxDepthOutcome::Exceeded;
            }
        }
        SyntaxDepthOutcome::WithinLimit(self.maximum)
    }

    fn source_token(
        &mut self,
        file: &swc_common::SourceFile,
        token_and_span: &TokenAndSpan,
        skip_to: &mut usize,
    ) -> Option<Token> {
        let offset = token_and_span
            .span
            .lo
            .0
            .checked_sub(file.start_pos.0)
            .map_or(0, |value| value as usize);
        if offset < *skip_to {
            return None;
        }

        let token = token_and_span.token;
        if token == Token::Slash
            && !self.previous_postfix
            && self
                .previous
                .is_none_or(|previous| previous.before_expr() || previous == Token::LBrace)
            && let Some(end) = Self::regex_end(&file.src, offset + 1)
        {
            *skip_to = end;
            self.previous = Some(Token::Regex);
            self.previous_postfix = false;
            self.expression_can_end = true;
            return None;
        }
        Some(token)
    }

    fn observe(&mut self, token: Token) -> Result<(), SyntaxDepthError> {
        match token {
            Token::LParen => self.push_delimiter(Delimiter::Parenthesis)?,
            Token::LBracket => self.push_delimiter(Delimiter::Bracket)?,
            Token::LBrace | Token::DollarLBrace | Token::TemplateHead => {
                self.push_delimiter(Delimiter::Brace)?;
            }
            Token::RParen => self.pop_delimiter(Delimiter::Parenthesis),
            Token::RBracket => self.pop_delimiter(Delimiter::Bracket),
            Token::RBrace => self.pop_delimiter(Delimiter::Brace),
            Token::Dot | Token::OptionalChain => {
                self.member_depth = self.member_depth.saturating_add(1);
                self.maximum = self.maximum.max(self.member_depth);
            }
            token if Self::resets_member_depth(token) => self.member_depth = 0,
            _ => {}
        }
        self.previous_postfix =
            matches!(token, Token::PlusPlus | Token::MinusMinus) && self.expression_can_end;
        self.expression_can_end = Self::token_can_end_expression(token, self.previous_postfix);
        self.previous = Some(token);
        Ok(())
    }

    fn push_delimiter(&mut self, delimiter: Delimiter) -> Result<(), SyntaxDepthError> {
        self.depth = self.depth.saturating_add(1);
        self.maximum = self.maximum.max(self.depth);
        if self.maximum > self.max_depth {
            return Err(SyntaxDepthError::Exceeded);
        }
        self.delimiters.push(delimiter);
        Ok(())
    }

    fn pop_delimiter(&mut self, expected: Delimiter) {
        if self.delimiters.last() != Some(&expected) {
            return;
        }
        self.delimiters.pop();
        self.depth = self.depth.saturating_sub(1);
    }

    /// A conservative source-only upper bound used to decide whether a parser
    /// token stream is safe to inspect after parsing. Every delimiter and
    /// member separator that can increase tracked depth is counted, including
    /// ones inside literals and comments.
    fn raw_bound(source: &str) -> usize {
        source
            .bytes()
            .filter(|byte| matches!(byte, b'(' | b'[' | b'{' | b'.'))
            .count()
    }

    fn regex_end(source: &str, start: usize) -> Option<usize> {
        let bytes = source.as_bytes();
        let mut index = start;
        let mut in_character_class = false;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = index.checked_add(2)?,
                b'[' if !in_character_class => {
                    in_character_class = true;
                    index += 1;
                }
                b']' if in_character_class => {
                    in_character_class = false;
                    index += 1;
                }
                b'/' if !in_character_class => return Some(index + 1),
                b'\n' | b'\r' => return None,
                _ => index += 1,
            }
        }
        None
    }

    fn resets_member_depth(token: Token) -> bool {
        matches!(
            token,
            Token::Semi
                | Token::Comma
                | Token::Colon
                | Token::Bang
                | Token::Plus
                | Token::Minus
                | Token::Asterisk
                | Token::Slash
                | Token::Percent
                | Token::Lt
                | Token::Gt
                | Token::Pipe
                | Token::Caret
                | Token::Ampersand
                | Token::Eq
                | Token::PlusPlus
                | Token::MinusMinus
                | Token::Tilde
                | Token::DotDotDot
        ) || token.is_bin_op()
            || token.is_assign_op()
    }

    fn token_can_end_expression(token: Token, postfix: bool) -> bool {
        postfix
            || matches!(
                token,
                Token::Ident
                    | Token::Str
                    | Token::Num
                    | Token::BigInt
                    | Token::Regex
                    | Token::NoSubstitutionTemplateLiteral
                    | Token::TemplateTail
                    | Token::RParen
                    | Token::RBracket
                    | Token::RBrace
                    | Token::Null
                    | Token::True
                    | Token::False
                    | Token::This
                    | Token::Super
            )
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
mod tests {
    use super::*;

    fn parse(source: &str, filename: &str) -> Result<ParsedSource, ParseDiagnostic> {
        let source = SourceFile::with_language(filename, source, SourceLanguage::JavaScript)
            .expect("test parser inputs should have valid relative paths");
        SourceParser::new(&source)?.parse()
    }

    #[test]
    fn rejects_excessive_nesting_before_ast_construction() {
        let mut source = "(".repeat(MAX_SYNTAX_DEPTH + 1);
        source.push('0');
        source.push_str(&")".repeat(MAX_SYNTAX_DEPTH + 1));
        let Err(error) = parse(&source, "deep.js") else {
            panic!("deep input unexpectedly parsed")
        };
        assert_eq!(error.code.as_str(), "syntax_depth_exceeded");
    }

    #[test]
    fn postfix_increment_does_not_make_following_division_a_regex() {
        let mut source = String::from("let value = 1; value++ / ");
        source.push_str(&"(".repeat(MAX_SYNTAX_DEPTH + 1));
        source.push('1');
        source.push_str(&")".repeat(MAX_SYNTAX_DEPTH + 1));

        let Err(error) = parse(&source, "postfix-division.js") else {
            panic!("deep input unexpectedly parsed")
        };
        assert_eq!(error.code.as_str(), "syntax_depth_exceeded");
    }

    #[test]
    fn parser_range_rejects_invalid_spans_without_panicking() {
        let source =
            SourceFile::with_language("invalid-span.js", "value", SourceLanguage::JavaScript)
                .expect("test parser input should have a valid relative path");
        let parser = SourceParser::new(&source).expect("test source should be accepted");
        let start = parser.file.start_pos.0;

        assert!(parser.parser_range(swc_common::DUMMY_SP).is_none());
        assert!(
            parser
                .parser_range(swc_common::Span {
                    lo: swc_common::BytePos(start + 2),
                    hi: swc_common::BytePos(start + 1),
                })
                .is_none()
        );
        assert!(
            parser
                .parser_range(swc_common::Span::new(
                    swc_common::BytePos(start),
                    swc_common::BytePos(start + 7),
                ))
                .is_none()
        );
    }

    #[test]
    fn ignores_delimiters_in_strings_and_comments() {
        let source = "const value = '( [ { ) ] }'; // ( [ {\nvalue;";
        assert!(parse(source, "quoted.js").is_ok());
    }

    #[test]
    fn template_expressions_contribute_to_depth() {
        let source = "`${a[${b}]}`";
        let depth = syntax_depth_for_test(source);
        assert!(
            depth >= 2,
            "template expression nesting should count, got {depth}"
        );
    }

    #[test]
    fn nested_template_expressions_count_depth() {
        let source = "`${a[${b[${c}]}]}`";
        let depth = syntax_depth_for_test(source);
        assert!(
            depth >= 3,
            "nested template expression should count, got {depth}"
        );
    }

    #[test]
    fn nested_template_inside_expression_tracks_depth() {
        let source = "`outer${`inner`}end`";
        let depth = syntax_depth_for_test(source);
        assert!(
            depth >= 1,
            "nested template inside expression should be counted, got {depth}"
        );
    }

    #[test]
    fn optional_chain_tracking() {
        let source = "a?.b?.c?.d";
        let depth = syntax_depth_for_test(source);
        assert!(
            depth >= 3,
            "optional chain member depth should be tracked, got {depth}"
        );
    }

    #[test]
    fn regex_not_mistaken_for_comment() {
        let source = "const re = /abc/; use(re);";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_with_dots_does_not_inflate_member_depth() {
        let source = "const re = /a.b.c.d.e.f.g.h.i.j/;";
        assert!(parse(source, "regex.js").is_ok());
        let depth = syntax_depth_for_test(source);
        assert!(
            depth < 5,
            "regex with many dots should not create large member depth, got {depth}"
        );
    }

    #[test]
    fn regex_with_escapes_and_parens() {
        let source = "const re = /\\(\\)\\[\\]\\.\\//;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_character_class_does_not_leak_dots() {
        let source = "const re = /[a.b/c.d]/;";
        assert!(parse(source, "regex.js").is_ok());
        let depth = syntax_depth_for_test(source);
        assert!(
            depth < 5,
            "regex char class with dots should not inflate depth, got {depth}"
        );
    }

    #[test]
    fn division_operator_not_mistaken_for_regex() {
        let source = "const result = a / b / c;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn division_assignment_not_mistaken_for_regex() {
        let source = "x /= 2; y /= 3;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn escaped_backtick_in_template() {
        let source = r"const str = `\`${inner}\``;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_equals_and_parens() {
        let source = "str.match(/\\d+\\.\\d+/); const re = /a\\.b/;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_return_keyword() {
        let source = "function f() { return /\\d+\\.\\w+/; }";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_typeof() {
        let source = "typeof /abc/";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_throw() {
        let source = "throw /error pattern/;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_void() {
        let source = "void /pattern/";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_at_statement_start() {
        let source = "/pattern/.test(value);";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_bang() {
        let source = "!/[a-z]+/.test(str)";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_after_comma_and_semicolon() {
        let source = "a, /pattern/; /another/;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn regex_mixed_with_comments() {
        let source = "const re = /* before */ /a.b/; // after\nvalue;";
        assert!(parse(source, "regex.js").is_ok());
    }

    #[test]
    fn hostile_depth_still_rejected() {
        let mut source = "(".repeat(MAX_SYNTAX_DEPTH + 1);
        source.push('0');
        source.push_str(&")".repeat(MAX_SYNTAX_DEPTH + 1));
        let Err(error) = parse(&source, "deep.js") else {
            panic!("deep input unexpectedly parsed")
        };
        assert_eq!(error.code.as_str(), "syntax_depth_exceeded");
    }

    #[test]
    fn valid_deep_expression_far_from_limit_passes() {
        let source = "((((((((((((((((((((0))))))))))))))))))))";
        assert!(parse(source, "deep.js").is_ok());
    }
}
