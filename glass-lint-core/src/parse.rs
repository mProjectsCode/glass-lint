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
    /// Selects the parser language for a filename. Unknown names retain the
    /// historical JavaScript fallback for virtual sources.
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

/// Parsed program and its source map for location conversion.
pub struct ParsedSource {
    /// SWC AST consumed by semantic analysis.
    pub program: Program,
    /// Source map retaining authored locations.
    pub source_map: Lrc<SourceMap>,
}

#[cfg(test)]
/// Parse JavaScript using the default JavaScript language mode.
pub fn parse(source: &str, filename: &str) -> Result<ParsedSource, ParseDiagnostic> {
    parse_with_language(source, filename, SourceLanguage::JavaScript)
}

/// Parse a supported language while preserving authored source locations.
pub fn parse_with_language(
    source: &str,
    filename: &str,
    language: SourceLanguage,
) -> Result<ParsedSource, ParseDiagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ParseDiagnostic {
            code: "source_too_large".into(),
            message: format!("source exceeds the {MAX_SOURCE_BYTES} byte analysis limit"),
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
                source_map: source_map.clone(),
            }
        })
        .map_err(|error| {
            let range = (!error.span().is_dummy()).then(|| {
                let start = source_map.lookup_char_pos(error.span().lo());
                let end = source_map.lookup_char_pos(error.span().hi());
                SourceRange {
                    start: Position {
                        line: start.line.try_into().unwrap_or(u32::MAX),
                        column: start
                            .col_display
                            .try_into()
                            .unwrap_or(u32::MAX)
                            .saturating_add(1),
                    },
                    end: Position {
                        line: end.line.try_into().unwrap_or(u32::MAX),
                        column: end
                            .col_display
                            .try_into()
                            .unwrap_or(u32::MAX)
                            .saturating_add(1),
                    },
                }
            });
            ParseDiagnostic {
                code: "syntax_error".into(),
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
