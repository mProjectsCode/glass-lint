//! Provenance identities emitted by syntax and scope analysis.
//!
//! These enums are deliberately small and provider-neutral. Proven local
//! identity is distinct from resolution that failed, was unsupported, or was
//! ambiguous.

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
/// Common outcome vocabulary for bounded semantic resolution.
pub(in crate::analysis) enum Knowledge<T> {
    /// A value proven by the artifact.
    Known(T),
    /// No single value could be proven, with the reason retained.
    Unknown(UnknownReason),
    /// More than one incompatible value remained possible.
    Ambiguous,
}

impl<T> Knowledge<T> {
    pub(in crate::analysis) fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a callable symbol at a use position.
pub(in crate::analysis) enum SymbolCallProvenance {
    /// A configured, unshadowed global callable.
    Global { name: String },
    /// A callable proven to be local to the current artifact.
    Local,
    /// A callable exported by a named module.
    ModuleExport { module: String, export: String },
    /// A resolution that did not produce a proven identity.
    Unknown(UnknownReason),
    /// Multiple incompatible identities were possible.
    Ambiguous,
}

impl SymbolCallProvenance {
    /// Lift this outcome into the shared knowledge vocabulary.
    pub(in crate::analysis) fn knowledge(&self) -> Knowledge<&Self> {
        match self {
            Self::Unknown(reason) => Knowledge::Unknown(reason.clone()),
            Self::Ambiguous => Knowledge::Ambiguous,
            _ => Knowledge::Known(self),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance of a member access rooted in a module namespace.
pub(in crate::analysis) enum SymbolMemberProvenance {
    /// A statically named member of an imported namespace.
    ModuleNamespace { module: String, member: String },
}

#[cfg(test)]
mod tests {
    use super::{BudgetComponent, Knowledge, SymbolCallProvenance, UnknownReason};

    #[test]
    fn provenance_outcomes_keep_unknown_and_ambiguous_distinct() {
        let known = SymbolCallProvenance::Global {
            name: "fetch".into(),
        };
        assert!(known.knowledge().is_known());
        assert!(matches!(
            SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: 65_536,
                observed: None,
            })
            .knowledge(),
            Knowledge::Unknown(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: 65_536,
                observed: None,
            })
        ));
        assert!(matches!(
            SymbolCallProvenance::Ambiguous.knowledge(),
            Knowledge::Ambiguous
        ));
        assert!(matches!(
            SymbolCallProvenance::Unknown(UnknownReason::Cycle).knowledge(),
            Knowledge::Unknown(UnknownReason::Cycle)
        ));
    }
}
