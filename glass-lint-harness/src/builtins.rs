use anyhow::{Result, bail};
use glass_lint_core::{Environment, Linter};

#[derive(Clone, Copy)]
pub enum BuiltInProvider {
    Js,
    Obsidian,
}

#[derive(Clone, Copy)]
pub enum BuiltInProfile {
    Recommended,
    Heuristic,
}

/// Construct one built-in provider linter with the caller's host environment.
/// All harness entry points use this boundary so profile and adapter behavior
/// cannot drift when provider defaults change.
pub fn linter(
    provider: BuiltInProvider,
    profile: BuiltInProfile,
    environment: Environment,
) -> Linter {
    match (provider, profile) {
        (BuiltInProvider::Js, BuiltInProfile::Recommended) => {
            glass_lint_js::recommended_linter_with_environment(environment)
        }
        (BuiltInProvider::Js, BuiltInProfile::Heuristic) => {
            glass_lint_js::heuristic_linter_with_environment(environment)
        }
        (BuiltInProvider::Obsidian, BuiltInProfile::Recommended) => {
            glass_lint_obsidian::recommended_linter_with_environment(environment)
        }
        (BuiltInProvider::Obsidian, BuiltInProfile::Heuristic) => {
            glass_lint_obsidian::heuristic_linter_with_environment(environment)
        }
    }
}

pub fn provider(name: &str) -> Result<BuiltInProvider> {
    match name {
        "js" => Ok(BuiltInProvider::Js),
        "obsidian" => Ok(BuiltInProvider::Obsidian),
        _ => bail!("unsupported built-in provider {name}"),
    }
}
