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
///
/// Each variant is produced by the resolver during value interning and
/// consumed by matchers during fact projection. The provenance determines
/// which matching rules apply to the call site.
pub(in crate::analysis) enum SymbolCallProvenance {
    /// A configured, unshadowed global callable. Produced when the
    /// identifier matches a named global in the environment and no
    /// local declaration shadows it. Consumed by matchers to select
    /// global-specific rules.
    Global { name: SmolStr },
    /// A callable proven to be local to the current artifact.
    /// Produced when the identifier resolves to a local declaration
    /// or parameter. Consumed by matchers to select local call rules.
    Local,
    /// A callable exported by a named module. Produced when the
    /// identifier resolves to an import binding whose module and
    /// export name are both known. Consumed by matchers to select
    /// module-specific rules.
    ModuleExport { module: SmolStr, export: SmolStr },
    /// A resolution that did not produce a proven identity. Produced
    /// when the identifier cannot be resolved, is outside the supported
    /// semantic subset, or exhausted a bounded budget. Consumed by
    /// matchers as a fail-closed sentinel that excludes the call site
    /// from all rules.
    Unknown(UnknownReason),
}

impl SymbolCallProvenance {
    /// Return borrowed module/export parts for an overlay lookup.
    pub(in crate::analysis) fn module_export_parts(&self) -> Option<(&str, &str)> {
        match self {
            Self::ModuleExport { module, export } => Some((module, export)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a member access rooted in a module namespace.
pub(in crate::analysis) enum SymbolMemberProvenance {
    /// A statically named member of an imported namespace.
    ModuleNamespace { module: SmolStr, member: SmolStr },
}
