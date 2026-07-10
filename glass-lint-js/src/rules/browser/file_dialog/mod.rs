use glass_lint_core::rules::{Confidence, FlowMatcher, FlowValueMatcher, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.file-dialog")
        .label("Uses browser file input dialogs")
        .category("browser/file-dialog")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::flow(
            FlowMatcher::new("file input element")
                .source_member_call("document.createElement")
                .source_arg_string(0, ["input"])
                .property_write(
                    "type",
                    FlowValueMatcher::StaticExact(vec!["file".to_string()]),
                )
                .emit_when_requirements_met(),
        ))
        .build()
        .unwrap()
}
