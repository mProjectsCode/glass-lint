use swc_common::{FileName, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, lexer::Lexer};

use crate::{
    MAX_SOURCE_BYTES,
    diagnostic::{Position, SourceRange},
};

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ParseDiagnostic {
    pub code: String,
    pub message: String,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
}

pub(crate) struct ParsedSource {
    pub program: Program,
    pub source_map: Lrc<SourceMap>,
}

pub(crate) fn parse(source: &str, filename: &str) -> Result<ParsedSource, ParseDiagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ParseDiagnostic {
            code: "source_too_large".into(),
            message: format!(
                "JavaScript source exceeds the {} byte analysis limit",
                MAX_SOURCE_BYTES
            ),
            filename: filename.into(),
            range: None,
        });
    }
    let source_map = Lrc::new(SourceMap::default());
    let file =
        source_map.new_source_file(FileName::Custom(filename.into()).into(), source.to_owned());
    let lexer = Lexer::new(
        Syntax::Es(EsSyntax {
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
        EsVersion::EsNext,
        StringInput::from(&*file),
        None,
    );
    Parser::new_from(lexer)
        .parse_program()
        .map(|program| ParsedSource {
            program,
            source_map: source_map.clone(),
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
                message: format!("JavaScript parse error: {}", error.kind().msg()),
                filename: filename.into(),
                range,
            }
        })
}
