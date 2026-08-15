use super::*;

fn parse(source: &str, filename: &str) -> Result<ParsedSource, ParseDiagnostic> {
    let source = SourceFile::with_language(filename, source, SourceLanguage::JavaScript)
        .expect("test parser inputs should have valid relative paths");
    SourceParser::new(&source)?.parse()
}

#[test]
fn diagnostic_accessors_preserve_failure_identity_and_display() {
    let diagnostic = ParseDiagnostic::new(
        ParseFailureKind::SyntaxDepth,
        "depth limit reached",
        "nested.js",
        None,
    );

    assert_eq!(diagnostic.code().as_str(), "syntax_depth_exceeded");
    assert_eq!(diagnostic.message(), "depth limit reached");
    assert_eq!(diagnostic.filename(), "nested.js");
    assert!(diagnostic.range().is_none());
    assert_eq!(diagnostic.failure_kind(), ParseFailureKind::SyntaxDepth);
    assert_eq!(
        diagnostic.to_string(),
        "[syntax_depth_exceeded] depth limit reached"
    );
}

#[test]
fn rejects_excessive_nesting_before_ast_construction() {
    let mut source = "(".repeat(MAX_SYNTAX_DEPTH + 1);
    source.push('0');
    source.push_str(&")".repeat(MAX_SYNTAX_DEPTH + 1));
    let Err(error) = parse(&source, "deep.js") else {
        panic!("deep input unexpectedly parsed")
    };
    assert_eq!(error.code().as_str(), "syntax_depth_exceeded");
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
    assert_eq!(error.code().as_str(), "syntax_depth_exceeded");
}

#[test]
fn parser_range_rejects_invalid_spans_without_panicking() {
    let source = SourceFile::with_language("invalid-span.js", "value", SourceLanguage::JavaScript)
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
    assert_eq!(error.code().as_str(), "syntax_depth_exceeded");
}

#[test]
fn valid_deep_expression_far_from_limit_passes() {
    let source = "((((((((((((((((((((0))))))))))))))))))))";
    assert!(parse(source, "deep.js").is_ok());
}
