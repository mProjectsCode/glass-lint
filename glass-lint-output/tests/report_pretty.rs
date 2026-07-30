//! Rendering contracts for human-readable single-file and grouped reports.

use glass_lint_core::project::{
    EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces, FileReport, Finding, MatchCertainty,
    ProjectRelativePath, SourceLocation,
};
use glass_lint_datastructures::{Position, SourceRange};
use glass_lint_output::{PrettyFile, PrettyOptions, PrettyReport, PrettyReports, RuleId, Severity};

fn path(path: &str) -> ProjectRelativePath {
    ProjectRelativePath::new(path).unwrap()
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
    starts
}

fn location(range: SourceRange) -> SourceLocation {
    SourceLocation::new(path("main.js"), range)
}

fn range(line: u32, start: u32, end: u32) -> SourceRange {
    SourceRange::new(
        Position::new(line, start).unwrap(),
        Position::new(line, end).unwrap(),
    )
    .unwrap()
}

fn file(findings: Vec<Finding>) -> FileReport {
    FileReport::new(path("main.js"), findings, Vec::new())
}

fn step(message: &str, range: SourceRange) -> EvidenceStep {
    EvidenceStep::new(EvidenceRole::Occurrence, message.into(), location(range))
}

#[test]
fn groups_by_rule_then_sorts_evidence_by_file_and_location() {
    let range_at = |line| range(line, 1, 6);
    let finding = |line| {
        Finding::new(
            RuleId::parse("test:fetch").unwrap(),
            "Uses fetch".into(),
            Severity::Warning,
            location(range_at(line)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "call of \"fetch\"",
                range_at(line),
            )])]),
            MatchCertainty::Definite,
        )
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
            "warning[test:fetch] (definite) Uses fetch\n",
            "  a.js:1:1 - call of \"fetch\"\n",
            "    fetch('/a1');\n",
            "    ^^^^^\n",
            "  a.js:2:1 - call of \"fetch\"\n",
            "    fetch('/a2');\n",
            "    ^^^^^\n",
            "  b.js:1:1 - call of \"fetch\"\n",
            "    fetch('/b');\n",
            "    ^^^^^\n",
        )
    );
}

#[test]
fn can_hide_source_excerpts_for_evidence_rows() {
    let r = range(1, 1, 6);
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:fetch").unwrap(),
            "Uses fetch".into(),
            Severity::Warning,
            location(r.clone()),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step("call of fetch", r)])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );

    let starts = line_starts("fetch('x');");
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "fetch('x');",
        PrettyOptions {
            show_evidence_source: false,
            ..PrettyOptions::default()
        },
        &starts,
    )
    .to_string();

    assert_eq!(
        rendered,
        "warning[test:fetch] (definite) Uses fetch\n  main.js:1:1 - call of fetch\n"
    );
}

#[test]
fn renders_flow_trace_steps_and_their_source() {
    let sink = range(1, 1, 8);
    let source = range(1, 1, 7);
    let requirement = range(2, 1, 9);
    let report = FileReport::new(
        path("helper.js"),
        vec![Finding::new(
            RuleId::parse("test:flow").unwrap(),
            "Proves a flow".into(),
            Severity::Warning,
            SourceLocation::new(path("helper.js"), sink.clone()),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![
                EvidenceStep::new(EvidenceRole::Source, "flow source".into(), location(source)),
                EvidenceStep::new(
                    EvidenceRole::Requirement,
                    "flow requirement".into(),
                    SourceLocation::new(path("helper.js"), requirement),
                ),
                EvidenceStep::new(
                    EvidenceRole::Sink,
                    "flow sink".into(),
                    SourceLocation::new(path("helper.js"), sink),
                ),
            ])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let source_report = FileReport::new(path("main.js"), vec![], vec![]);
    let files = [
        PrettyFile::new(
            &report,
            "helper.js",
            "function append() {\n  element.src = url;\n}",
        ),
        PrettyFile::new(&source_report, "main.js", "const element = create();"),
    ];

    let rendered = PrettyReports::new(&files, PrettyOptions::default()).to_string();

    assert!(rendered.contains("helper.js:1:1 - flow sink"));
    assert!(rendered.contains("trace 1:"));
    assert!(rendered.contains("main.js:1:1 - flow source"));
    assert!(rendered.contains("const element = create();"));
    assert!(rendered.contains("helper.js:2:1 - flow requirement"));
    assert!(rendered.contains("element.src = url;"));
}

#[test]
fn explains_possible_path_certainty() {
    let r = range(1, 1, 6);
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:fetch").unwrap(),
            "Uses fetch".into(),
            Severity::Warning,
            location(r.clone()),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step("call of fetch", r)])]),
            MatchCertainty::Possible,
        )],
        vec![],
    );
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "fetch('x');",
        PrettyOptions {
            show_evidence_source: false,
            ..PrettyOptions::default()
        },
        &line_starts("fetch('x');"),
    )
    .to_string();

    assert!(rendered.contains("(possible path)"));
    assert!(rendered.contains(
        "Proven on at least one modeled control-flow path; runtime reachability is not established."
    ));
}

#[test]
fn renders_empty_reports_without_extra_output() {
    let report = FileReport::new(path("main.js"), vec![], vec![]);
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
            &line_starts(""),
        )
        .to_string(),
        ""
    );
}

#[test]
fn renders_terminal_controls_visibly() {
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:fetch").unwrap(),
            "message\u{1b}[31m".into(),
            Severity::Warning,
            location(range(1, 1, 2)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "call of fetch",
                range(1, 1, 2),
            )])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let output = PrettyReport::new(
        &report,
        "bad\u{1b}[x.js",
        "x",
        PrettyOptions::default(),
        &line_starts("x"),
    )
    .to_string();
    assert!(output.contains("bad\\u{001b}[x.js"));
    assert!(output.contains("message\\u{001b}[31m"));
}

#[test]
fn bounds_long_excerpt() {
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:long-line").unwrap(),
            "long line".into(),
            Severity::Warning,
            location(range(1, 201, 206)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "call of fetch",
                range(1, 201, 206),
            )])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let source = format!("{}fetch('x')", "x".repeat(200));
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        &source,
        PrettyOptions {
            max_width: 20,
            color: false,
            show_evidence_source: true,
        },
        &line_starts(&source),
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
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:unicode").unwrap(),
            "unicode".into(),
            Severity::Info,
            location(range(1, 9, 12)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "unicode match",
                range(1, 9, 12),
            )])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let source = "\t\tconst 😀 = true;\n";
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        source,
        PrettyOptions {
            max_width: 14,
            color: false,
            show_evidence_source: true,
        },
        &line_starts(source),
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
fn aligns_caret_after_single_tab_and_wide_character() {
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:alignment").unwrap(),
            "alignment".into(),
            Severity::Info,
            location(range(1, 2, 7)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "call of fetch",
                range(1, 2, 7),
            )])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let source = "\tfetch('x');";
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        source,
        PrettyOptions::default(),
        &line_starts(source),
    )
    .to_string();

    let lines: Vec<_> = rendered.lines().collect();
    assert_eq!(lines[2], "        fetch('x');");
    assert_eq!(lines[3], "        ^^^^^");
}

#[test]
fn renders_missing_source_lines_without_panicking() {
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:missing").unwrap(),
            "missing".into(),
            Severity::Error,
            location(range(99, 1, 2)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "missing call",
                range(99, 1, 2),
            )])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "",
        PrettyOptions::default(),
        &line_starts(""),
    )
    .to_string();
    assert!(rendered.contains("error[test:missing] (definite) missing"));
    assert!(rendered.contains("main.js:99:1 - missing call"));
}

#[test]
fn renders_colored_findings_when_enabled() {
    let report = FileReport::new(
        path("main.js"),
        vec![Finding::new(
            RuleId::parse("test:color").unwrap(),
            "colored".into(),
            Severity::Error,
            location(range(1, 1, 2)),
            EvidenceTraces::new(vec![EvidenceTrace::new(vec![step(
                "color match",
                range(1, 1, 2),
            )])]),
            MatchCertainty::Definite,
        )],
        vec![],
    );
    let source = "x();";
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        source,
        PrettyOptions {
            max_width: 20,
            color: true,
            show_evidence_source: true,
        },
        &line_starts(source),
    )
    .to_string();
    assert!(rendered.contains("\u{1b}[31merror\u{1b}[0m"));
    assert!(rendered.contains("\u{1b}[36mtest:color\u{1b}[0m"));
}
