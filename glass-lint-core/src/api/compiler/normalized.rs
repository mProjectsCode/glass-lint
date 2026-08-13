use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher,
        query::{EventSpec, IdentitySpec, VarId},
    },
};

// ── Normalized IR ──────────────────────────────────────────────────────────

/// A canonical normalized logical query with no `All` variant.
///
/// Normalization merges same-event conjunctions into one event node,
/// detects contradictions, and assigns dense deterministic variable slots.
/// Fields are private to `api/compiler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedQuery {
    pub(crate) root: NormalizedRoot,
    pub(crate) emission: NormalizedEmission,
}

impl NormalizedQuery {
    pub(crate) fn root(&self) -> &NormalizedRoot {
        &self.root
    }

    pub(crate) fn emission(&self) -> &NormalizedEmission {
        &self.emission
    }
}

/// Evidence emission for a normalized query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedEmission {
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

impl NormalizedEmission {
    pub(crate) fn kind(&self) -> MatchKind {
        self.kind
    }

    pub(crate) fn symbol(&self) -> &str {
        &self.symbol
    }
}

/// Normalized root expression — no `All` variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NormalizedRoot {
    Event(NormalizedEvent),
    Any(Box<[Self]>),
    Lifecycle(NormalizedLifecycle),
}

/// Argument constraints in canonical form: grouped by argument index.
///
/// Invariants (maintained by construction):
/// - Groups are ordered by argument index.
/// - Within each group, predicates are sorted and deduplicated.
/// - No group is empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub(crate) struct CanonicalArgumentConstraints {
    pub(crate) groups: Box<[ArgumentConstraintGroup]>,
}

/// A group of predicates all applying to the same argument index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ArgumentConstraintGroup {
    pub(crate) index: ArgumentIndex,
    pub(crate) predicates: Box<[ArgumentMatcher]>,
}

impl CanonicalArgumentConstraints {
    pub(crate) fn groups(&self) -> &[ArgumentConstraintGroup] {
        &self.groups
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (ArgumentIndex, &ArgumentMatcher)> + '_ {
        self.groups.iter().flat_map(|group| {
            std::iter::repeat_n(group.index, group.predicates.len()).zip(group.predicates.iter())
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Build canonical form from arbitrary constraints.
    ///
    /// Empty input represents an unconstrained event. Groups are accumulated
    /// with a Vec per index and frozen once at the end, so every group is
    /// non-empty without requiring callers to pre-sort or deduplicate.
    pub(crate) fn from_constraints(raw: &[ArgumentConstraint]) -> Self {
        let mut raw = raw.to_vec();
        raw.sort_by(|a, b| {
            a.index()
                .cmp(&b.index())
                .then_with(|| a.predicate().cmp(b.predicate()))
        });
        raw.dedup();

        // First pass: count predicates per group so we allocate exactly once.
        let mut group_counts: Vec<(ArgumentIndex, usize)> = Vec::new();
        for c in &raw {
            let idx = c.arg_index();
            if let Some(last) = group_counts.last_mut()
                && last.0 == idx
            {
                last.1 += 1;
            } else {
                group_counts.push((idx, 1));
            }
        }

        // Second pass: fill groups.
        let mut groups = Vec::with_capacity(group_counts.len());
        let mut cursor = 0;
        for (idx, count) in group_counts {
            let mut predicates = Vec::with_capacity(count);
            for _ in 0..count {
                predicates.push(raw[cursor].predicate().clone());
                cursor += 1;
            }
            groups.push(ArgumentConstraintGroup {
                index: idx,
                predicates: predicates.into_boxed_slice(),
            });
        }

        Self {
            groups: groups.into_boxed_slice(),
        }
    }

    /// Flatten canonical groups back into a constraint vector.
    pub(crate) fn to_flat_vec(&self) -> Vec<ArgumentConstraint> {
        let mut v = Vec::new();
        for group in &self.groups {
            for m in &group.predicates {
                v.push(ArgumentConstraint::new(group.index, m.clone()));
            }
        }
        v
    }
}

impl ArgumentConstraintGroup {
    pub(crate) fn index(&self) -> ArgumentIndex {
        self.index
    }

    pub(crate) fn predicates(&self) -> &[ArgumentMatcher] {
        &self.predicates
    }
}

/// Dense slot identifying the event variable bound by a normalized event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct EventSlot(u32);

impl EventSlot {
    pub(crate) fn from_var(var: VarId) -> Self {
        Self(var.get())
    }

    pub(crate) const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

/// Dense slot identifying an object produced or constructed by an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ObjectSlot(u32);

impl ObjectSlot {
    pub(crate) fn from_var(var: VarId) -> Self {
        Self(var.get())
    }

    pub(crate) const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

/// A single normalized event node with merged subject and arguments.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NormalizedEvent {
    pub(crate) slot: EventSlot,
    pub(crate) event: EventSpec,
    pub(crate) subject: NormalizedSubject,
    pub(crate) arguments: CanonicalArgumentConstraints,
}

impl NormalizedEvent {
    pub(crate) fn event(&self) -> &EventSpec {
        &self.event
    }

    pub(crate) fn identity(&self) -> &IdentitySpec {
        match &self.subject {
            NormalizedSubject::Direct { identity } => identity,
            NormalizedSubject::Returned { producer, .. } => producer,
            NormalizedSubject::Instance { constructor, .. } => constructor,
        }
    }

    pub(crate) fn subject(&self) -> &NormalizedSubject {
        &self.subject
    }

    pub(crate) fn arguments(&self) -> &CanonicalArgumentConstraints {
        &self.arguments
    }
}

/// Subject relationship in a normalized event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NormalizedSubject {
    Direct {
        identity: IdentitySpec,
    },
    Returned {
        producer: IdentitySpec,
        object_slot: ObjectSlot,
    },
    Instance {
        constructor: IdentitySpec,
        object_slot: ObjectSlot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NormalizedLifecycleEvent {
    PropertyWrite {
        property: SmolStr,
        value: crate::api::rule::ValueMatcher,
    },
    MemberCall {
        member: SmolStr,
        arguments: CanonicalArgumentConstraints,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NormalizedLifecycleCondition {
    AnyOf(Box<[NormalizedLifecycleEvent]>),
    AllOf(Box<[NormalizedLifecycleEvent]>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NormalizedLifecycleSink {
    ArgumentOf {
        target: crate::api::rule::query::lifecycle::LifecycleCallTarget,
        index: usize,
    },
    AnyArgumentOf {
        target: crate::api::rule::query::lifecycle::LifecycleCallTarget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NormalizedLifecycleCompletion {
    Configuration,
    AnySink(Box<[NormalizedLifecycleSink]>),
    AllSinks(Box<[NormalizedLifecycleSink]>),
}

/// Normalized lifecycle — compiler-owned sources, conditions, and completion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NormalizedLifecycle {
    pub(crate) sources: Vec<NormalizedEvent>,
    pub(crate) condition: Option<NormalizedLifecycleCondition>,
    pub(crate) completion: Option<NormalizedLifecycleCompletion>,
}

impl NormalizedLifecycle {
    pub(crate) fn sources(&self) -> &[NormalizedEvent] {
        &self.sources
    }

    pub(crate) fn condition(&self) -> Option<&NormalizedLifecycleCondition> {
        self.condition.as_ref()
    }

    pub(crate) fn completion(&self) -> Option<&NormalizedLifecycleCompletion> {
        self.completion.as_ref()
    }
}
