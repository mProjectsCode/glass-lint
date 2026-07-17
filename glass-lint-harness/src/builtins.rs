//! Provider/profile selection shared by adapters and profiling.

use anyhow::{Result, bail};
use glass_lint_core::{Linter, RuleBaseline, RuleSelection};

#[derive(Clone, Copy)]
/// Built-in rule provider available to the harness.
pub enum BuiltinProvider {
    Js,
    Node,
    Electron,
    Obsidian,
}

#[derive(Clone, Copy)]
/// Precision profile used to construct a provider linter.
pub enum BuiltinProfile {
    Recommended,
    Heuristic,
}

/// Construct one built-in provider linter with the caller's host environment.
/// All harness entry points use this boundary so profile and adapter behavior
/// cannot drift when provider defaults change.
pub fn linter(provider: BuiltinProvider, profile: BuiltinProfile) -> Linter {
    let baseline = match profile {
        BuiltinProfile::Recommended => {
            RuleBaseline::MinimumConfidence(glass_lint_core::rules::Confidence::High)
        }
        BuiltinProfile::Heuristic => RuleBaseline::All,
    };
    let config = match provider {
        BuiltinProvider::Js => glass_lint_js::js_config(),
        BuiltinProvider::Node => glass_lint_js::node_config(),
        BuiltinProvider::Electron => glass_lint_js::electron_config(),
        BuiltinProvider::Obsidian => glass_lint_obsidian::obsidian_config(),
    };
    Linter::new(config.with_rules(RuleSelection::new(baseline)))
        .expect("built-in catalogs are valid")
}

pub fn provider(name: &str) -> Result<BuiltinProvider> {
    match name {
        "js" => Ok(BuiltinProvider::Js),
        "node" => Ok(BuiltinProvider::Node),
        "electron" => Ok(BuiltinProvider::Electron),
        "obsidian" => Ok(BuiltinProvider::Obsidian),
        _ => bail!("unsupported built-in provider {name}"),
    }
}
