//! Provenance identities emitted by syntax and scope analysis.
//!
//! These enums are deliberately small and provider-neutral. Proven local
//! identity is distinct from resolution that failed, was unsupported, or was
//! ambiguous.

use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Why a semantic identity could not be proven.
pub(in crate::analysis) enum UnknownReason {
    /// The expression did not resolve to a supported identity.
    Unresolved,
    /// The syntax is outside the supported semantic subset.
    Unsupported,
    /// A bounded operation prevented a resolution answer.
    BudgetExhausted {
        component: BudgetComponent,
        limit: usize,
        observed: Option<usize>,
    },
    /// The requested identity or source record was unavailable.
    Missing,
    /// Resolution encountered a recursive identity cycle.
    Cycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Semantic component whose bounded work prevented a resolution answer.
pub(in crate::analysis) enum BudgetComponent {
    /// Interned value identities reached their per-file cap.
    Values,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a callable symbol at a use position.
pub(in crate::analysis) enum SymbolCallProvenance {
    /// A configured, unshadowed global callable.
    Global { name: SmolStr },
    /// A callable proven to be local to the current artifact.
    Local,
    /// A callable exported by a named module.
    ModuleExport { module: SmolStr, export: SmolStr },
    /// A resolution that did not produce a proven identity.
    Unknown(UnknownReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a member access rooted in a module namespace.
pub(in crate::analysis) enum SymbolMemberProvenance {
    /// A statically named member of an imported namespace.
    ModuleNamespace { module: SmolStr, member: SmolStr },
}