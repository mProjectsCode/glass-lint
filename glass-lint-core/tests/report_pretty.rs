use glass_lint_core::{
    Evidence, Finding, LintReport, Position, PrettyFile, PrettyOptions, PrettyReport,
    PrettyReports, RuleId, Severity, SourceRange,
};

#[test]
fn groups_by_rule_then_sorts_evidence_by_file_and_location() {
    let range = |line| SourceRange {
        start: Position { line, column: 1 },
        end: Position { line, column: 6 },
    };
    let finding = |line| Finding {
        rule_id: RuleId::parse("test:fetch").unwrap(),
        message_id: "detected".into(),
        message: "Uses fetch".into(),
        severity: Severity::Warning,
        range: range(line),
        evidence: vec![Evidence {
            message: "call of \"fetch\"".into(),
            range: Some(range(line)),
            source: Some("fetch".into()),
        }],
    };
    let report_a = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![finding(2), finding(1)],
        parse_diagnostics: vec![],
    };
    let report_b = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![finding(1)],
        parse_diagnostics: vec![],
    };
    let files = [
        PrettyFile::new(&report_b, "b.js", "fetch('/b');"),
        PrettyFile::new(&report_a, "a.js", "fetch('/a1');\nfetch('/a2');"),
    ];

    assert_eq!(
        PrettyReports::new(
            &files,
            PrettyOptions {
                max_width: 80,
                color: false,
            },
        )
        .to_string(),
        concat!(
            "warning[test:fetch] Uses fetch\n",
            "  a.js:1:1 - evidence: call of \"fetch\"\n",
            "    fetch('/a1');\n",
            "    ^^^^^\n",
            "  a.js:2:1 - evidence: call of \"fetch\"\n",
            "    fetch('/a2');\n",
            "    ^^^^^\n",
            "  b.js:1:1 - evidence: call of \"fetch\"\n",
            "    fetch('/b');\n",
            "    ^^^^^\n",
        )
    );
}

#[test]
fn renders_empty_reports_without_extra_output() {
    let report = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![],
        parse_diagnostics: vec![],
    };
    assert_eq!(
        PrettyReport::new(
            &report,
            "main.js",
            "",
            PrettyOptions {
                max_width: 20,
                color: false,
            },
        )
        .to_string(),
        ""
    );
}

#[test]
fn bounds_long_excerpt() {
    let report = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:long-line").unwrap(),
            message_id: "detected".into(),
            message: "long line".into(),
            severity: Severity::Warning,
            range: SourceRange {
                start: Position {
                    line: 1,
                    column: 201,
                },
                end: Position {
                    line: 1,
                    column: 206,
                },
            },
            evidence: vec![],
        }],
        parse_diagnostics: vec![],
    };
    let source = format!("{}fetch('x')", "x".repeat(200));
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        &source,
        PrettyOptions {
            max_width: 20,
            color: false,
        },
    )
    .to_string();
    assert!(
        rendered
            .lines()
            .any(|line| line.trim_start().starts_with("...") && line.len() <= 22)
    );
}

#[test]
fn renders_tabs_and_wide_unicode_within_the_display_budget() {
    let report = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:unicode").unwrap(),
            message_id: "detected".into(),
            message: "unicode".into(),
            severity: Severity::Info,
            range: SourceRange {
                start: Position { line: 1, column: 9 },
                end: Position {
                    line: 1,
                    column: 12,
                },
            },
            evidence: vec![],
        }],
        parse_diagnostics: vec![],
    };
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "\t\tconst 😀 = true;\n",
        PrettyOptions {
            max_width: 14,
            color: false,
        },
    )
    .to_string();
    let excerpt_lines = rendered
        .lines()
        .filter(|line| line.starts_with("    "))
        .collect::<Vec<_>>();
    assert_eq!(excerpt_lines.len(), 2);
    assert!(excerpt_lines.iter().all(|line| line.chars().count() <= 14));
}

#[test]
fn renders_missing_source_lines_without_panicking() {
    let report = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:missing").unwrap(),
            message_id: "detected".into(),
            message: "missing".into(),
            severity: Severity::Error,
            range: SourceRange {
                start: Position {
                    line: 99,
                    column: 1,
                },
                end: Position {
                    line: 99,
                    column: 2,
                },
            },
            evidence: vec![],
        }],
        parse_diagnostics: vec![],
    };
    let rendered = PrettyReport::new(&report, "main.js", "", PrettyOptions::default()).to_string();
    assert!(rendered.contains("error[test:missing] missing"));
    assert!(rendered.contains("main.js:99:1 - match"));
}

#[test]
fn renders_colored_findings_when_enabled() {
    let report = LintReport {
        schema_version: 2,
        tool_version: "test".into(),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:color").unwrap(),
            message_id: "detected".into(),
            message: "colored".into(),
            severity: Severity::Error,
            range: SourceRange {
                start: Position { line: 1, column: 1 },
                end: Position { line: 1, column: 2 },
            },
            evidence: vec![],
        }],
        parse_diagnostics: vec![],
    };
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "x();",
        PrettyOptions {
            max_width: 20,
            color: true,
        },
    )
    .to_string();
    assert!(rendered.contains("\u{1b}[31merror\u{1b}[0m"));
    assert!(rendered.contains("\u{1b}[36mtest:color\u{1b}[0m"));
}
