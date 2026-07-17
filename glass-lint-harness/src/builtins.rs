//! Provider/profile selection shared by adapters and profiling.

use anyhow::{Result, bail};
use glass_lint_core::{Linter, RuleBaseline, RuleSelection};

#[derive(Clone, Copy)]
/// Built-in rule provider available to the harness.
pub enum BuiltInProvider {
    Js,
    Node,
    Electron,
    Obsidian,
}

#[derive(Clone, Copy)]
/// Precision profile used to construct a provider linter.
pub enum BuiltInProfile {
    Recommended,
    Heuristic,
}

/// Construct one built-in provider linter with the caller's host environment.
/// All harness entry points use this boundary so profile and adapter behavior
/// cannot drift when provider defaults change.
pub fn linter(provider: BuiltInProvider, profile: BuiltInProfile) -> Linter {
    let baseline = match profile {
        BuiltInProfile::Recommended => {
            RuleBaseline::MinimumConfidence(glass_lint_core::rules::Confidence::High)
        }
        BuiltInProfile::Heuristic => RuleBaseline::All,
    };
    let config = match provider {
        BuiltInProvider::Js => glass_lint_js::js_config(),
        BuiltInProvider::Node => glass_lint_js::node_config(),
        BuiltInProvider::Electron => glass_lint_js::electron_config(),
        BuiltInProvider::Obsidian => glass_lint_obsidian::config(),
    };
    Linter::new(config.with_rules(RuleSelection::new(baseline)))
        .expect("built-in catalogs are valid")
}

pub fn provider(name: &str) -> Result<BuiltInProvider> {
    match name {
        "js" => Ok(BuiltInProvider::Js),
        "node" => Ok(BuiltInProvider::Node),
        "electron" => Ok(BuiltInProvider::Electron),
        "obsidian" => Ok(BuiltInProvider::Obsidian),
        _ => bail!("unsupported built-in provider {name}"),
    }
}
