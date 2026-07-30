//! Terminal-oriented report presentation for Glass Lint front ends.

pub use glass_lint_core::{RuleId, Severity};

pub mod project {
    pub use glass_lint_core::project::*;
    pub mod types {
        pub use glass_lint_core::project::types::*;
    }
}

mod report;

pub use report::{PrettyFile, PrettyOptions, PrettyReport, PrettyReports, visible_text};

#[cfg(test)]
mod tests {
    use super::visible_text;

    #[test]
    fn visible_text_escapes_terminal_controls() {
        assert_eq!(visible_text("a\n\t\u{0001}"), "a\\n\\t\\u{0001}");
    }
}
