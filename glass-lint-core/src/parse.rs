//! Bounded JavaScript/TypeScript parsing and source-position conversion.

use glass_lint_datastructures::{Position, SourceRange};
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_parser::{
    EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer, unstable::Token,
};
use swc_ecma_transforms_base::resolver;
use swc_ecma_transforms_typescript::strip;

use crate::{MAX_SOURCE_BYTES, project::DiagnosticCode};

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
    /// Selects the parser language for a filename. Unknown names use
    /// JavaScript for virtual sources and paths without a recognized
    /// extension; callers that know an extensionless source is TypeScript must
    /// provide the language directly.
    #[must_use]
    pub fn from_filename(filename: &str) -> Self {
        Self::from_extension(Self::extension(filename)).unwrap_or(Self::JavaScript)
    }

    /// Returns the language associated with a supported source extension.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "js" | "cjs" | "mjs" => Some(Self::JavaScript),
            "ts" | "cts" | "mts" => Some(Self::TypeScript),
            _ => None,
        }
    }

    /// Returns whether a filename is a discoverable runtime source file.
    /// TypeScript declaration files are excluded because they contain no
    /// runtime behavior for the semantic engine to analyze.
    #[must_use]
    pub fn is_supported_filename(filename: &str) -> bool {
        !Self::is_declaration_filename(filename)
            && Self::from_extension(Self::extension(filename)).is_some()
    }

    fn extension(filename: &str) -> &str {
        filename
            .rsplit(['/', '\\'])
            .next()
            .and_then(|basename| basename.rsplit_once('.'))
            .map_or("", |(_, extension)| extension)
    }

    fn is_declaration_filename(filename: &str) -> bool {
        filename.rsplit(['/', '\\']).next().is_some_and(|basename| {
            let basename = basename.to_ascii_lowercase();
            [".d.ts", ".d.cts", ".d.mts"]
                .iter()
                .any(|suffix| basename.ends_with(suffix))
        })
    }
}

/// Parsed program consumed by lowering.
pub struct ParsedSource {
    /// SWC AST consumed by semantic analysis.
    pub(crate) program: Program,
    /// Absolute SWC position assigned to authored byte offset zero.
    pub(crate) source_start: swc_common::BytePos,
}

#[cfg(test)]
/// Parse JavaScript using the default JavaScript language mode.
pub fn parse(source: &str, filename: &str) -> Result<ParsedSource, ParseDiagnostic> {
    parse_with_language_and_depth(
        source,
        filename,
        SourceLanguage::JavaScript,
        MAX_SYNTAX_DEPTH,
    )
}

/// Parse a source string with an explicit structural nesting limit.
///
/// TypeScript sources are parsed by SWC then lowered: the resolver pass runs,
/// TypeScript syntax is stripped, and the result is treated as JavaScript for
/// semantic purposes. JavaScript sources pass through without transformation.
pub fn parse_with_language_and_depth(
    source: &str,
    filename: &str,
    language: SourceLanguage,
    max_syntax_depth: usize,
) -> Result<ParsedSource, ParseDiagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ParseDiagnostic {
            code: crate::project::types::DiagnosticKind::SourceTooLarge.into(),
            message: format!("source exceeds the {MAX_SOURCE_BYTES} byte analysis limit"),
            filename: filename.into(),
            range: None,
        });
    }
    let source_map = Lrc::new(SourceMap::default());
    let file =
        source_map.new_source_file(FileName::Custom(filename.into()).into(), source.to_owned());
    let syntax = syntax_for(language);
    match syntax_depth(&file, syntax, max_syntax_depth) {
        Ok(_) => {}
        Err(SyntaxDepthError::Exceeded) => {
            return Err(ParseDiagnostic {
                code: crate::project::types::DiagnosticKind::SyntaxDepthExceeded.into(),
                message: format!(
                    "source exceeds the {max_syntax_depth} nesting-depth analysis limit"
                ),
                filename: filename.into(),
                range: None,
            });
        }
        Err(SyntaxDepthError::Malformed) => {
            return Err(ParseDiagnostic {
                code: crate::project::types::DiagnosticKind::SyntaxError.into(),
                message: format!(
                    "{} parse error: lexical error",
                    match language {
                        SourceLanguage::JavaScript => "JavaScript",
                        SourceLanguage::TypeScript => "TypeScript",
                    }
                ),
                filename: filename.into(),
                range: None,
            });
        }
    }
    let lexer = Lexer::new(syntax, EsVersion::EsNext, StringInput::from(&*file), None);
    Parser::new_from(lexer)
        .parse_program()
        .map(|program| {
            let program = match language {
                SourceLanguage::JavaScript => program,
                SourceLanguage::TypeScript => GLOBALS.set(&Globals::default(), || {
                    let unresolved_mark = Mark::new();
                    let top_level_mark = Mark::new();
                    let mut program = program;
                    program = program.apply(resolver(unresolved_mark, top_level_mark, true));
                    program = program.apply(strip(unresolved_mark, top_level_mark));
                    program
                }),
            };
            ParsedSource {
                program,
                source_start: file.start_pos,
            }
        })
        .map_err(|error| {
            let range = (!error.span().is_dummy()).then(|| {
                let start = source_map.lookup_char_pos(error.span().lo());
                let end = source_map.lookup_char_pos(error.span().hi());
                let start = Position::new(
                    start.line.try_into().unwrap_or(u32::MAX),
                    start
                        .col_display
                        .try_into()
                        .unwrap_or(u32::MAX)
                        .saturating_add(1),
                )
                .expect("parser locations are one-based");
                let end = Position::new(
                    end.line.try_into().unwrap_or(u32::MAX),
                    end.col_display
                        .try_into()
                        .unwrap_or(u32::MAX)
                        .saturating_add(1),
                )
                .expect("parser locations are one-based");
                SourceRange::new(start, end).expect("parser spans are ordered")
            });
            ParseDiagnostic {
                code: crate::project::types::DiagnosticKind::SyntaxError.into(),
                message: format!(
                    "{} parse error: {}",
                    match language {
                        SourceLanguage::JavaScript => "JavaScript",
                        SourceLanguage::TypeScript => "TypeScript",
                    },
                    error.kind().msg()
                ),
                filename: filename.into(),
                range,
            }
        })
}

fn syntax_for(language: SourceLanguage) -> Syntax {
    match language {
        SourceLanguage::JavaScript => Syntax::Es(EsSyntax {
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
        SourceLanguage::TypeScript => Syntax::Typescript(TsSyntax {
            tsx: false,
            decorators: true,
            ..Default::default()
        }),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntaxDepthError {
    Exceeded,
    Malformed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delimiter {
    Parenthesis,
    Bracket,
    Brace,
}

/// Count delimiter and member-chain nesting from SWC's token stream.
///
/// The lexer owns regex-vs-division token boundaries; this pass only skips
/// the raw bytes of a regex after using SWC's token-context signal to identify
/// its start. Every token event is charged against the source-byte bound, and
/// malformed tokenization or delimiter order fails closed before AST parsing.
fn syntax_depth(
    file: &swc_common::SourceFile,
    syntax: Syntax,
    max_depth: usize,
) -> Result<usize, SyntaxDepthError> {
    if source_contains_template(file) {
        let depth = template_syntax_depth(file.src.as_ref());
        return (depth <= max_depth)
            .then_some(depth)
            .ok_or(SyntaxDepthError::Exceeded);
    }
    let mut lexer = Lexer::new(syntax, EsVersion::EsNext, StringInput::from(file), None);
    let source: &str = &file.src;
    let source_len = source.len();
    let mut delimiters = Vec::new();
    let mut depth = 0usize;
    let mut maximum = 0usize;
    let mut member_depth = 0usize;
    let mut previous: Option<Token> = None;
    let mut previous_postfix = false;
    let mut expression_can_end = false;
    let mut skip_to = 0usize;
    let mut token_events = 0usize;

    for token_and_span in &mut lexer {
        token_events = token_events
            .checked_add(1)
            .ok_or(SyntaxDepthError::Malformed)?;
        if token_events > source_len.saturating_add(1) {
            return Err(SyntaxDepthError::Malformed);
        }

        let offset = token_and_span
            .span
            .lo
            .0
            .checked_sub(file.start_pos.0)
            .map(|value| value as usize)
            .ok_or(SyntaxDepthError::Malformed)?;
        if offset < skip_to {
            continue;
        }

        let token = token_and_span.token;
        if token == Token::Error {
            return Err(SyntaxDepthError::Malformed);
        }

        if token == Token::Slash
            && !previous_postfix
            && previous.is_none_or(|token| token.before_expr() || token == Token::LBrace)
            && let Some(end) = regex_end(source, offset + 1)
        {
            skip_to = end;
            previous = Some(Token::Regex);
            previous_postfix = false;
            expression_can_end = true;
            continue;
        }

        match token {
            Token::LParen => push_delimiter(
                &mut delimiters,
                Delimiter::Parenthesis,
                &mut depth,
                &mut maximum,
                max_depth,
            )?,
            Token::LBracket => push_delimiter(
                &mut delimiters,
                Delimiter::Bracket,
                &mut depth,
                &mut maximum,
                max_depth,
            )?,
            Token::LBrace | Token::DollarLBrace => push_delimiter(
                &mut delimiters,
                Delimiter::Brace,
                &mut depth,
                &mut maximum,
                max_depth,
            )?,
            Token::RParen => pop_delimiter(&mut delimiters, Delimiter::Parenthesis, &mut depth),
            Token::RBracket => pop_delimiter(&mut delimiters, Delimiter::Bracket, &mut depth),
            Token::RBrace => pop_delimiter(&mut delimiters, Delimiter::Brace, &mut depth),
            Token::Dot | Token::OptionalChain => {
                member_depth = member_depth.saturating_add(1);
                maximum = maximum.max(member_depth);
                if maximum > max_depth {
                    return Err(SyntaxDepthError::Exceeded);
                }
            }
            token if resets_member_depth(token) => member_depth = 0,
            _ => {}
        }
        previous_postfix =
            matches!(token, Token::PlusPlus | Token::MinusMinus) && expression_can_end;
        expression_can_end = token_can_end_expression(token, previous_postfix);
        previous = Some(token);
    }

    Ok(maximum)
}

fn source_contains_template(file: &swc_common::SourceFile) -> bool {
    let source: &str = &file.src;
    source.as_bytes().contains(&b'`')
}

/// SWC's parser drives template rescans, so a standalone lexer cannot expose
/// the expression delimiters in a template consistently. Keep this small
/// template-state pass limited to that lexical construct; ordinary source
/// uses the SWC token pass above.
#[allow(clippy::too_many_lines)]
fn template_syntax_depth(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut maximum = 0usize;
    let mut member_depth = 0usize;
    let mut index = 0usize;
    let mut quote = None;
    let mut template_state = 0usize;
    let mut template_stack = Vec::new();
    let mut in_regex = false;
    let mut in_regex_class = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if template_state == 1 {
            if byte == b'\\' {
                index = index.saturating_add(2);
            } else if byte == b'$' && bytes.get(index + 1) == Some(&b'{') {
                template_state = 2;
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
                index += 2;
            } else if byte == b'`' {
                template_state = template_stack.pop().unwrap_or(0);
                index += 1;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(delimiter) = quote {
            if byte == b'\\' {
                index = index.saturating_add(2);
            } else {
                quote = (byte != delimiter).then_some(delimiter);
                index += 1;
            }
            continue;
        }
        if in_regex {
            if byte == b'\\' {
                index = index.saturating_add(2);
            } else if byte == b'[' && !in_regex_class {
                in_regex_class = true;
                index += 1;
            } else if byte == b']' && in_regex_class {
                in_regex_class = false;
                index += 1;
            } else if byte == b'/' && !in_regex_class {
                in_regex = false;
                index += 1;
            } else {
                index += 1;
            }
            continue;
        }
        if template_state >= 2 {
            if byte == b'$' && bytes.get(index + 1) == Some(&b'{') {
                template_state += 1;
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
                index += 2;
                continue;
            }
            if byte == b'}' {
                template_state -= 1;
                depth = depth.saturating_sub(1);
                index += 1;
                continue;
            }
            if byte == b'`' {
                template_stack.push(template_state);
                template_state = 1;
                index += 1;
                continue;
            }
        }
        if template_state == 0 && byte == b'`' {
            template_state = 1;
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'/' {
            match bytes.get(index + 1) {
                Some(b'/') => {
                    index = bytes[index..]
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map_or(bytes.len(), |offset| index + offset + 1);
                    continue;
                }
                Some(b'*') => {
                    index = bytes[index + 2..]
                        .windows(2)
                        .position(|window| window == b"*/")
                        .map_or(bytes.len(), |offset| index + offset + 4);
                    continue;
                }
                Some(b'=') => member_depth = 0,
                _ if is_template_regex_start(bytes, index) => {
                    in_regex = true;
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }
        if byte == b'.' {
            member_depth = member_depth.saturating_add(1);
            maximum = maximum.max(member_depth);
        } else if matches!(
            byte,
            b';' | b','
                | b'='
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b':'
                | b'!'
                | b'&'
                | b'|'
                | b'<'
                | b'>'
        ) {
            member_depth = 0;
        }
        if matches!(byte, b'(' | b'[' | b'{') {
            depth = depth.saturating_add(1);
            maximum = maximum.max(depth);
        } else if matches!(byte, b')' | b']' | b'}') {
            depth = depth.saturating_sub(1);
        }
        index += 1;
    }
    maximum
}

fn is_template_regex_start(bytes: &[u8], index: usize) -> bool {
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        let byte = bytes[cursor];
        if byte.is_ascii_whitespace() {
            continue;
        }
        if matches!(byte, b')' | b']' | b'}' | b'"' | b'\'' | b'`') {
            return false;
        }
        if byte == b'+' && cursor > 0 && bytes[cursor - 1] == b'+' {
            return false;
        }
        if byte == b'-' && cursor > 0 && bytes[cursor - 1] == b'-' {
            return false;
        }
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') {
            let end = cursor + 1;
            while cursor > 0
                && (bytes[cursor - 1].is_ascii_alphanumeric()
                    || matches!(bytes[cursor - 1], b'_' | b'$'))
            {
                cursor -= 1;
            }
            let word = std::str::from_utf8(&bytes[cursor..end]).unwrap_or("");
            return matches!(
                word,
                "return"
                    | "typeof"
                    | "instanceof"
                    | "void"
                    | "delete"
                    | "throw"
                    | "case"
                    | "in"
                    | "of"
            );
        }
        return true;
    }
    true
}

fn push_delimiter(
    delimiters: &mut Vec<Delimiter>,
    delimiter: Delimiter,
    depth: &mut usize,
    maximum: &mut usize,
    max_depth: usize,
) -> Result<(), SyntaxDepthError> {
    *depth = depth.saturating_add(1);
    *maximum = (*maximum).max(*depth);
    if *maximum > max_depth {
        return Err(SyntaxDepthError::Exceeded);
    }
    delimiters.push(delimiter);
    Ok(())
}

fn pop_delimiter(delimiters: &mut Vec<Delimiter>, expected: Delimiter, depth: &mut usize) {
    if delimiters.last() != Some(&expected) {
        return;
    }
    delimiters.pop();
    *depth = depth.saturating_sub(1);
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

#[cfg(test)]
fn syntax_depth_for_test(source: &str) -> usize {
    let source_map = Lrc::new(SourceMap::default());
    let file =
        source_map.new_source_file(FileName::Custom("test.js".into()).into(), source.to_owned());
    syntax_depth(
        &file,
        syntax_for(SourceLanguage::JavaScript),
        MAX_SYNTAX_DEPTH,
    )
    .unwrap_or_else(|error| match error {
        SyntaxDepthError::Exceeded => MAX_SYNTAX_DEPTH + 1,
        SyntaxDepthError::Malformed => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
