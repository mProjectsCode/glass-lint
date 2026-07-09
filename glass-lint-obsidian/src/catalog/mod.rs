use std::sync::OnceLock;

use glass_lint_core::rules::{Builder as RuleBuilder, Matcher, Rule};

mod content;
mod disclosures;
mod interface;
mod network;
mod system;

trait ObsidianRuleBuilderExt {
    fn with_heuristic_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_global_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_module_calls<I, S>(self, module: impl Into<String>, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_heuristic_member_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_rooted_member_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_module_member_calls<I, S>(self, module: impl Into<String>, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_heuristic_member_reads<I, S>(self, reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_rooted_member_reads<I, S>(self, reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_module_member_reads<I, S>(self, module: impl Into<String>, reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_imports<I, S>(self, imports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_string_literals<I, S>(self, literals: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_heuristic_classes<I, S>(self, classes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;

    fn with_heuristic_constructors<I, S>(self, constructors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;
}

impl ObsidianRuleBuilderExt for RuleBuilder {
    fn with_heuristic_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        calls.into_iter().fold(self, |builder, call| {
            builder.matcher(Matcher::heuristic_call(call))
        })
    }

    fn with_global_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        calls.into_iter().fold(self, |builder, call| {
            builder.matcher(Matcher::global_call(call))
        })
    }

    fn with_module_calls<I, S>(self, module: impl Into<String>, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let module = module.into();
        calls.into_iter().fold(self, |builder, call| {
            builder.matcher(Matcher::module_call(module.clone(), call))
        })
    }

    fn with_heuristic_member_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        calls.into_iter().fold(self, |builder, call| {
            builder.matcher(Matcher::heuristic_member_call(call))
        })
    }

    fn with_rooted_member_calls<I, S>(self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        calls.into_iter().fold(self, |builder, call| {
            builder.matcher(Matcher::rooted_member_call(call))
        })
    }

    fn with_module_member_calls<I, S>(self, module: impl Into<String>, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let module = module.into();
        calls.into_iter().fold(self, |builder, call| {
            builder.matcher(Matcher::module_member_call(module.clone(), call))
        })
    }

    fn with_heuristic_member_reads<I, S>(self, reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        reads.into_iter().fold(self, |builder, read| {
            builder.matcher(Matcher::heuristic_member_read(read))
        })
    }

    fn with_rooted_member_reads<I, S>(self, reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        reads.into_iter().fold(self, |builder, read| {
            builder.matcher(Matcher::rooted_member_read(read))
        })
    }

    fn with_module_member_reads<I, S>(self, module: impl Into<String>, reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let module = module.into();
        reads.into_iter().fold(self, |builder, read| {
            builder.matcher(Matcher::module_member_read(module.clone(), read))
        })
    }

    fn with_imports<I, S>(self, imports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        imports.into_iter().fold(self, |builder, import| {
            builder.matcher(Matcher::import(import))
        })
    }

    fn with_string_literals<I, S>(self, literals: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        literals.into_iter().fold(self, |builder, literal| {
            builder.matcher(Matcher::string_literal(literal))
        })
    }

    fn with_heuristic_classes<I, S>(self, classes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        classes.into_iter().fold(self, |builder, class| {
            let class = class.into();
            if let Some((module, export)) = class.split_once('.') {
                builder.matcher(Matcher::module_class(module, export))
            } else {
                builder.matcher(Matcher::heuristic_class(class))
            }
        })
    }

    fn with_heuristic_constructors<I, S>(self, constructors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        constructors.into_iter().fold(self, |builder, constructor| {
            let constructor = constructor.into();
            if let Some((module, export)) = constructor.split_once('.') {
                builder.matcher(Matcher::module_constructor(module, export))
            } else {
                builder.matcher(Matcher::heuristic_constructor(constructor))
            }
        })
    }
}

pub(crate) fn obsidian_api_rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            network::rules(),
            content::rules(),
            interface::rules(),
            system::rules(),
        ]
        .into_iter()
        .flatten()
        .collect()
    })
}

pub(crate) fn disclosures_for_rule(rule_id: &str) -> &'static [&'static str] {
    disclosures::for_rule(rule_id)
}
