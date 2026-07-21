//! Rendering contracts for human-readable single-file and grouped reports.
//!
//! These assertions lock down deterministic ordering, source excerpts, display
//! width bounds, missing-source resilience, and optional terminal coloring.

use glass_lint_core::{
    Evidence, FileReport, Finding, Position, PrettyFile, PrettyOptions, PrettyReport,
    PrettyReports, ProjectRelativePath, RuleId, Severity, SourceLocation, SourceRange,
};

fn path(path: &str) -> ProjectRelativePath {
    ProjectRelativePath::new(path).unwrap()
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
    starts
}

fn location(range: SourceRange) -> SourceLocation {
    SourceLocation {
        path: path("main.js"),
        range,
    }
}

fn range(line: u32, start: u32, end: u32) -> SourceRange {
    SourceRange::new(
        Position::new(line, start).unwrap(),
        Position::new(line, end).unwrap(),
    )
    .unwrap()
}

fn file(findings: Vec<Finding>) -> FileReport {
    FileReport {
        path: path("main.js"),
        findings,
        diagnostics: Vec::new(),
    }
}

#[test]
fn groups_by_rule_then_sorts_evidence_by_file_and_location() {
    let range = |line| range(line, 1, 6);
    let finding = |line| Finding {
        rule_id: RuleId::parse("test:fetch").unwrap(),
        message_id: "detected".into(),
        message: "Uses fetch".into(),
        severity: Severity::Warning,
        location: location(range(line)),
        evidence: vec![Evidence {
            message: "call of \"fetch\"".into(),
            count: 1,
            evidence_truncated: false,
            location: Some(location(range(line))),
        }]
        .into_iter()
        .collect(),
    };
    let report_a = file(vec![finding(2), finding(1)]);
    let report_b = file(vec![finding(1)]);
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
                show_evidence_source: true,
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
fn can_hide_source_excerpts_for_evidence_rows() {
    let range = range(1, 1, 6);
    let report = FileReport {
        path: path("main.js"),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:fetch").unwrap(),
            message_id: "detected".into(),
            message: "Uses fetch".into(),
            severity: Severity::Warning,
            location: location(range.clone()),
            evidence: vec![Evidence {
                message: "call of fetch".into(),
                count: 1,
                evidence_truncated: false,
                location: Some(location(range)),
            }]
            .into_iter()
            .collect(),
        }],
        diagnostics: vec![],
    };

    let line_starts = line_starts("fetch('x');");
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "fetch('x');",
        PrettyOptions {
            show_evidence_source: false,
            ..PrettyOptions::default()
        },
        &line_starts,
    )
    .to_string();

    assert_eq!(
        rendered,
        "warning[test:fetch] Uses fetch\n  main.js:1:1 - evidence: call of fetch\n"
    );
}

#[test]
fn renders_empty_reports_without_extra_output() {
    let report = FileReport {
        path: path("main.js"),
        findings: vec![],
        diagnostics: vec![],
    };
    let line_starts = line_starts("");
    assert_eq!(
        PrettyReport::new(
            &report,
            "main.js",
            "",
            PrettyOptions {
                max_width: 20,
                color: false,
                show_evidence_source: true,
            },
            &line_starts,
        )
        .to_string(),
        ""
    );
}

#[test]
fn renders_terminal_controls_visibly() {
    let report = FileReport {
        path: path("main.js"),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:fetch").unwrap(),
            message_id: "detected".into(),
            message: "message\u{1b}[31m".into(),
            severity: Severity::Warning,
            location: location(range(1, 1, 2)),
            evidence: Vec::new().into_iter().collect(),
        }],
        diagnostics: vec![],
    };
    let line_starts = line_starts("x");
    let output =
        PrettyReport::new(&report, "bad\u{1b}[x.js", "x", PrettyOptions::default(), &line_starts).to_string();
    assert!(output.contains("bad\\u{001b}[x.js"));
    assert!(output.contains("message\\u{001b}[31m"));
}

#[test]
fn bounds_long_excerpt() {
    let report = FileReport {
        path: path("main.js"),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:long-line").unwrap(),
            message_id: "detected".into(),
            message: "long line".into(),
            severity: Severity::Warning,
            location: location(range(1, 201, 206)),
            evidence: Vec::new().into_iter().collect(),
        }],
        diagnostics: vec![],
    };
    let source = format!("{}fetch('x')", "x".repeat(200));
    let line_starts = line_starts(&source);
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        &source,
        PrettyOptions {
            max_width: 20,
            color: false,
            show_evidence_source: true,
        },
        &line_starts,
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
    let report = FileReport {
        path: path("main.js"),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:unicode").unwrap(),
            message_id: "detected".into(),
            message: "unicode".into(),
            severity: Severity::Info,
            location: location(range(1, 9, 12)),
            evidence: Vec::new().into_iter().collect(),
        }],
        diagnostics: vec![],
    };
    let line_starts = line_starts("\t\tconst 😀 = true;\n");
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "\t\tconst 😀 = true;\n",
        PrettyOptions {
            max_width: 14,
            color: false,
            show_evidence_source: true,
        },
        &line_starts,
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
    let report = FileReport {
        path: path("main.js"),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:missing").unwrap(),
            message_id: "detected".into(),
            message: "missing".into(),
            severity: Severity::Error,
            location: location(range(99, 1, 2)),
            evidence: Vec::new().into_iter().collect(),
        }],
        diagnostics: vec![],
    };
    let line_starts = line_starts("");
    let rendered = PrettyReport::new(&report, "main.js", "", PrettyOptions::default(), &line_starts).to_string();
    assert!(rendered.contains("error[test:missing] missing"));
    assert!(rendered.contains("main.js:99:1 - match"));
}

#[test]
fn renders_colored_findings_when_enabled() {
    let report = FileReport {
        path: path("main.js"),
        findings: vec![Finding {
            rule_id: RuleId::parse("test:color").unwrap(),
            message_id: "detected".into(),
            message: "colored".into(),
            severity: Severity::Error,
            location: location(range(1, 1, 2)),
            evidence: Vec::new().into_iter().collect(),
        }],
        diagnostics: vec![],
    };
    let line_starts = line_starts("x();");
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "x();",
        PrettyOptions {
            max_width: 20,
            color: true,
            show_evidence_source: true,
        },
        &line_starts,
    )
    .to_string();
    assert!(rendered.contains("\u{1b}[31merror\u{1b}[0m"));
    assert!(rendered.contains("\u{1b}[36mtest:color\u{1b}[0m"));
}
