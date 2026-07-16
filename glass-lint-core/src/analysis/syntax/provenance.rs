//! Provenance identities emitted by syntax and scope analysis.
//!
//! These enums are deliberately small and provider-neutral. `Local` means
//! that no supported global or module identity was proven; it is not a
//! heuristic fallback.

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a callable symbol at a use position.
pub(in crate::analysis) enum SymbolCallProvenance {
    /// A configured, unshadowed global callable.
    Global { name: String },
    /// A local, ambiguous, or unsupported callable.
    Local,
    /// A callable exported by a named module.
    ModuleExport { module: String, export: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a member access rooted in a module namespace.
pub(in crate::analysis) enum SymbolMemberProvenance {
    /// A statically named member of an imported namespace.
    ModuleNamespace { module: String, member: String },
}
