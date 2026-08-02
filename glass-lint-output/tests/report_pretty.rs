//! Rendering contracts for human-readable single-file and grouped reports.
//!
//! Reports come from the public analysis pipeline so rendering is exercised
//! against real report data rather than construction seams.

use glass_lint_core::{
    Environment, Linter, LinterConfig, Rule, RuleCatalog,
    project::{FileReport, SourceFile},
    rules::{
        Category, Confidence, EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent,
        LifecycleQuery, LifecycleSink, QueryBuildError, QueryDecl, Severity, ValueMatcher,
    },
};
use glass_lint_output::{PrettyFile, PrettyOptions, PrettyReport, PrettyReports};

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
    starts
}

fn fetch_rule(description: &str, severity: Severity) -> Rule {
    Rule::builder("fetch")
        .description(description)
        .category(Category::new("network").unwrap())
        .severity(severity)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap()
}

fn linter(rules: Vec<Rule>, globals: &[&str]) -> Linter {
    let mut environment = Environment::default();
    environment
        .add_globals(globals.iter().map(ToString::to_string))
        .unwrap();
    Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", rules).unwrap()],
        environment,
    ))
    .unwrap()
}

fn lint_file(source: &str, filename: &str, rule: Rule, globals: &[&str]) -> FileReport {
    linter(vec![rule], globals)
        .lint_source(SourceFile::new(filename, source).unwrap())
        .unwrap()
        .files()[0]
        .clone()
}

fn script_insertion_flow() -> Result<LifecycleQuery, QueryBuildError> {
    LifecycleQuery::builder("script-insert")
        .source(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .with_arg(0, ValueMatcher::static_string().equals("script")),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("document.head.appendChild", 0),
        ]))
        .build()
}

fn flow_rule(id: &str, description: &str) -> Rule {
    Rule::builder(id)
        .description(description)
        .category(Category::new("test").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(QueryDecl::lifecycle(script_insertion_flow()))
        .build()
        .unwrap()
}

#[test]
fn groups_by_rule_then_sorts_evidence_by_file_and_location() {
    let report_a = lint_file(
        "fetch('/a1');\nfetch('/a2');",
        "a.js",
        fetch_rule("Uses fetch", Severity::Warning),
        &["fetch"],
    );
    let report_b = lint_file(
        "fetch('/b');",
        "b.js",
        fetch_rule("Uses fetch", Severity::Warning),
        &["fetch"],
    );
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
    let report = lint_file(
        "fetch('x');",
        "main.js",
        fetch_rule("Uses fetch", Severity::Warning),
        &["fetch"],
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
        "warning[test:fetch] (definite) Uses fetch\n  main.js:1:1 - call of \"fetch\"\n"
    );
}

#[test]
fn renders_flow_trace_steps_and_their_source() {
    let source =
        "const s = document.createElement('script');\ns.src = url;\ndocument.head.appendChild(s);";
    let report = lint_file(
        source,
        "helper.js",
        flow_rule("flow", "Script insertion flow"),
        &["document", "url"],
    );
    let files = [PrettyFile::new(&report, "helper.js", source)];

    let rendered = PrettyReports::new(&files, PrettyOptions::default()).to_string();

    assert!(rendered.contains("helper.js:3:1 - flow sink"));
    assert!(rendered.contains("trace 1:"));
    assert!(rendered.contains("helper.js:1:11 - flow source"));
    assert!(rendered.contains("const s = document.createElement('script');"));
    assert!(rendered.contains("helper.js:2:1 - flow requirement"));
    assert!(rendered.contains("s.src = url;"));
}

#[test]
fn explains_possible_path_certainty() {
    let source = "const script = document.createElement('script'); if (flag) script.src = url; document.head.appendChild(script);";
    let report = lint_file(
        source,
        "main.js",
        flow_rule("flow", "Script insertion flow"),
        &["document", "flag", "url"],
    );
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        source,
        PrettyOptions {
            show_evidence_source: false,
            ..PrettyOptions::default()
        },
        &line_starts(source),
    )
    .to_string();

    assert!(rendered.contains("(possible path)"));
    assert!(rendered.contains(
        "Proven on at least one modeled control-flow path; runtime reachability is not established."
    ));
}

#[test]
fn renders_empty_reports_without_extra_output() {
    let report = lint_file(
        "const x = 1;",
        "main.js",
        fetch_rule("Uses fetch", Severity::Warning),
        &["fetch"],
    );
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
    let report = lint_file(
        "fetch('x');",
        "main.js",
        fetch_rule("message\u{1b}[31m", Severity::Warning),
        &["fetch"],
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
    let rule = Rule::builder("long-line")
        .description("long line")
        .category(Category::new("test").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::string_contains("fetch"))
        .build()
        .unwrap();
    let source = format!("const s = '{}fetch';", "x".repeat(200));
    let report = lint_file(&source, "main.js", rule, &["s"]);
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
    let rule = Rule::builder("unicode")
        .description("unicode")
        .category(Category::new("test").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::string_contains("\u{1f600}"))
        .build()
        .unwrap();
    let source = "\t\tconst s = '\u{1f600}';\n";
    let report = lint_file(source, "main.js", rule, &["s"]);
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
    let source = "\tfetch('x');";
    let report = lint_file(
        source,
        "main.js",
        fetch_rule("Uses fetch", Severity::Warning),
        &["fetch"],
    );
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
    let rule = Rule::builder("missing")
        .description("missing")
        .category(Category::new("test").unwrap())
        .severity(Severity::Error)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let report = lint_file("const x = 1;\nfetch('x');", "main.js", rule, &["fetch"]);
    let rendered = PrettyReport::new(
        &report,
        "main.js",
        "",
        PrettyOptions::default(),
        &line_starts(""),
    )
    .to_string();
    assert!(rendered.contains("error[test:missing] (definite) missing"));
    assert!(rendered.contains("main.js:2:1 - call of \"fetch\""));
}

#[test]
fn renders_colored_findings_when_enabled() {
    let rule = Rule::builder("color")
        .description("colored")
        .category(Category::new("test").unwrap())
        .severity(Severity::Error)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let source = "fetch('x');";
    let report = lint_file(source, "main.js", rule, &["fetch"]);
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
