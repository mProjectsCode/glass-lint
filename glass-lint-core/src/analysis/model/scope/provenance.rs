use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;
use swc_common::Span;

use super::{BindingKey, BindingVersion, ScopeId};
use crate::analysis::{
    model::StaticProperties,
    syntax::{SymbolCallProvenance, SymbolMemberProvenance, constant::ConstValue},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingProvenance {
    Local,
    ValueAlias {
        target: NamePath,
    },
    BoundCallable {
        target: NamePath,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    BoundModuleCallable {
        module: SmolStr,
        export: SmolStr,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    ReturnedObject {
        source: NamePath,
    },
    ModuleExport {
        module: SmolStr,
        export: SmolStr,
    },
    /// A default ESM import is callable as the module's `default` export and
    /// also acts as a namespace-like object for member access.
    DefaultImport {
        module: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    ConstructedInstance {
        module: SmolStr,
        export: SmolStr,
    },
    StaticString(String),
    StaticNumber(usize),
    StaticStringArray(Vec<String>),
    StaticObjectKeys(StaticProperties<()>),
    StaticObjectValues(StaticProperties<NamePath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundArgument {
    StaticString(String),
    RootedExpression(NamePath),
}

#[derive(Debug, Clone)]
pub struct IdentValueSeed {
    pub(in crate::analysis) call: SymbolCallProvenance,
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) constant: ConstValue,
    pub(in crate::analysis) bound_arguments: Option<Vec<Option<BoundArgument>>>,
}

#[derive(Debug, Clone)]
pub struct MemberValueSeed {
    pub(in crate::analysis) syntactic_chain: Option<SymbolPath>,
    pub(in crate::analysis) rooted_chain: Option<NamePath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) module_member: Option<SymbolMemberProvenance>,
    pub(in crate::analysis) returned_member: Option<(NamePath, NamePath)>,
}

/// The bounded set of provenance alternatives retained for one assignment.
///
/// A precise write carries one provenance; a control-flow join retains the
/// bounded union of the reachable path alternatives and is marked joined.
/// Unknown and exhausted alternatives are never retained as provenances: they
/// are represented by the `unknown` and `exhausted` flags and cannot establish
/// a witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceAlternatives {
    provenances: Vec<BindingProvenance>,
    unknown: bool,
    joined: bool,
    exhausted: bool,
}

impl ProvenanceAlternatives {
    pub fn single(provenance: BindingProvenance) -> Self {
        Self {
            provenances: vec![provenance],
            unknown: false,
            joined: false,
            exhausted: false,
        }
    }

    /// An unknown alternative with no retained witness.
    pub fn unknown() -> Self {
        Self {
            provenances: vec![],
            unknown: true,
            joined: false,
            exhausted: false,
        }
    }

    /// Union `other` into this set, deduplicating and bounding retention to
    /// `limit`. When the bound is exceeded the set becomes both exhausted and
    /// unknown, because the retained alternatives are no longer complete and
    /// cannot establish a witness.
    fn add_bounded(&mut self, other: &Self, limit: usize) {
        self.unknown |= other.unknown;
        self.exhausted |= other.exhausted;
        self.joined |= other.joined;
        for provenance in &other.provenances {
            if !self.insert_bounded(provenance, limit) {
                return;
            }
        }
    }

    fn insert_bounded(&mut self, provenance: &BindingProvenance, limit: usize) -> bool {
        if self.provenances.contains(provenance) {
            return true;
        }
        if self.provenances.len() >= limit {
            self.exhausted = true;
            self.unknown = true;
            return false;
        }
        self.provenances.push(provenance.clone());
        true
    }

    pub fn is_joined(&self) -> bool {
        self.joined
    }

    #[cfg(test)]
    pub fn is_unknown(&self) -> bool {
        self.unknown
    }

    #[cfg(test)]
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn has_complete_witness(&self) -> bool {
        !self.provenances.is_empty()
    }

    fn is_incomplete(&self) -> bool {
        self.unknown || self.exhausted
    }

    /// The preferred strict witness at a use position: the single retained
    /// provenance for a precise write, or the first non-local alternative
    /// retained after a control-flow join. `None` when no complete witness is
    /// retained.
    pub fn preferred_witness(&self) -> Option<&BindingProvenance> {
        if self.joined {
            self.provenances
                .iter()
                .find(|p| !matches!(p, BindingProvenance::Local))
        } else {
            self.provenances.first()
        }
    }

    /// Iterate the complete (non-unknown) witnesses retained by this
    /// assignment. Unknown-only assignments iterate nothing.
    pub fn complete_witnesses(&self) -> impl Iterator<Item = &BindingProvenance> + '_ {
        self.provenances.iter()
    }
}

/// A control-flow join whose retention bound is fixed when the merge starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct ProvenanceJoin {
    alternatives: ProvenanceAlternatives,
    limit: usize,
}

impl ProvenanceJoin {
    pub(in crate::analysis) fn new(limit: usize) -> Self {
        Self {
            alternatives: ProvenanceAlternatives {
                provenances: Vec::new(),
                unknown: false,
                joined: true,
                exhausted: false,
            },
            limit,
        }
    }

    pub(in crate::analysis) fn add(&mut self, other: &ProvenanceAlternatives) {
        self.alternatives.add_bounded(other, self.limit);
    }

    pub(in crate::analysis) fn alternatives(&self) -> &ProvenanceAlternatives {
        &self.alternatives
    }

    fn into_alternatives(self) -> ProvenanceAlternatives {
        self.alternatives
    }
}

#[derive(Debug, Clone)]
pub struct AliasAssignment {
    span: Span,
    scope: ScopeId,
    name: NameId,
    version: BindingVersion,
    alternatives: ProvenanceAlternatives,
}

impl AliasAssignment {
    /// A precise write carrying a single provenance.
    pub fn single(
        span: Span,
        scope: ScopeId,
        name: NameId,
        version: BindingVersion,
        provenance: BindingProvenance,
    ) -> Self {
        Self {
            span,
            scope,
            name,
            version,
            alternatives: ProvenanceAlternatives::single(provenance),
        }
    }

    /// A synthetic assignment installed after a control-flow join. The
    /// `alternatives` set is the bounded union of the reachable paths.
    pub(in crate::analysis) fn joined(
        span: Span,
        scope: ScopeId,
        name: NameId,
        version: BindingVersion,
        join: ProvenanceJoin,
    ) -> Self {
        Self {
            span,
            scope,
            name,
            version,
            alternatives: join.into_alternatives(),
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub fn name(&self) -> NameId {
        self.name
    }

    pub fn version(&self) -> BindingVersion {
        self.version
    }

    pub fn is_joined(&self) -> bool {
        self.alternatives.is_joined()
    }

    /// Whether this assignment retained an unknown or exhausted alternative.
    pub fn is_incomplete(&self) -> bool {
        self.alternatives.is_incomplete()
    }

    pub fn preferred_witness(&self) -> Option<&BindingProvenance> {
        self.alternatives.preferred_witness()
    }

    pub fn complete_witnesses(&self) -> impl Iterator<Item = &BindingProvenance> + '_ {
        self.alternatives.complete_witnesses()
    }

    #[cfg(test)]
    pub fn alternatives(&self) -> &ProvenanceAlternatives {
        &self.alternatives
    }
}
