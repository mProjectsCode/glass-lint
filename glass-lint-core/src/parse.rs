//! Bounded JavaScript/TypeScript parsing and source-position conversion.

use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};
use swc_ecma_transforms_base::resolver;
use swc_ecma_transforms_typescript::strip;

use crate::{
    MAX_SOURCE_BYTES,
    diagnostic::{Position, SourceRange},
    project::DiagnosticCode,
};

/// Maximum syntactic nesting accepted before invoking recursive parser and
/// visitor machinery. This is deliberately checked on source text so a
/// hostile tree cannot first force an unbounded AST allocation.
#[cfg(test)]
const MAX_SYNTAX_DEPTH: usize = 512;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
/// Structured parser failure with an optional source range.
pub struct ParseDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable parser message.
    pub message: String,
    /// Authored filename.
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
}

/// Source languages accepted by the core parser.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
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
    if syntax_depth(source) > max_syntax_depth {
        return Err(ParseDiagnostic {
            code: crate::project::types::DiagnosticKind::SyntaxDepthExceeded.into(),
            message: format!("source exceeds the {max_syntax_depth} nesting-depth analysis limit"),
            filename: filename.into(),
            range: None,
        });
    }
    let source_map = Lrc::new(SourceMap::default());
    let file =
        source_map.new_source_file(FileName::Custom(filename.into()).into(), source.to_owned());
    let syntax = match language {
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
    };
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

/// Count delimiter and member-chain nesting while ignoring comments and
/// quoted strings. It is a conservative lexical guard; parser validity is
/// still decided by SWC.
fn syntax_depth(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut maximum = 0usize;
    let mut member_depth = 0usize;
    let mut index = 0usize;
    let mut quote = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(delimiter) = quote {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == delimiter {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index = bytes[index..]
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |p| index + p + 1);
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index = bytes[index + 2..]
                .windows(2)
                .position(|w| w == b"*/")
                .map_or(bytes.len(), |p| index + p + 4);
            continue;
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
                | b'?'
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
    fn ignores_delimiters_in_strings_and_comments() {
        let source = "const value = '( [ { ) ] }'; // ( [ {\nvalue;";
        assert!(parse(source, "quoted.js").is_ok());
    }
}
