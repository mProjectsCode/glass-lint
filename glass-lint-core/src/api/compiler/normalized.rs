use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::api::{
    compiler::validate::{SubjectRelationError, classify_subject_relation},
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher, MatchKind,
        query::{EventSpec, IdentitySpec, VarId},
    },
};

// ── Normalized IR ──────────────────────────────────────────────────────────

/// A canonical normalized logical query with no `All` variant.
///
/// Normalization merges same-event conjunctions into one event node,
/// detects contradictions, and assigns dense deterministic variable slots.
/// Fields are private to `api/compiler`; read and construction go through the
/// accessors and [`Self::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedQuery {
    root: NormalizedRoot,
    emission: NormalizedEmission,
}

impl NormalizedQuery {
    pub(crate) fn new(root: NormalizedRoot, emission: NormalizedEmission) -> Self {
        Self { root, emission }
    }

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
    kind: MatchKind,
    symbol: String,
}

impl NormalizedEmission {
    pub(crate) fn new(kind: MatchKind, symbol: String) -> Self {
        Self { kind, symbol }
    }

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
    groups: Box<[ArgumentConstraintGroup]>,
}

/// A group of predicates all applying to the same argument index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ArgumentConstraintGroup {
    index: ArgumentIndex,
    predicates: Box<[ArgumentMatcher]>,
}

impl CanonicalArgumentConstraints {
    pub(crate) fn groups(&self) -> &[ArgumentConstraintGroup] {
        &self.groups
    }

    /// Build raw grouped constraints bypassing canonicalization, for tests
    /// that exercise the physical-validation boundary with non-canonical
    /// shapes (excessive or unsorted groups).
    #[cfg(test)]
    pub(crate) fn from_groups_for_test(groups: Box<[ArgumentConstraintGroup]>) -> Self {
        Self { groups }
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
            a.arg_index()
                .cmp(&b.arg_index())
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

    /// Flatten canonical groups for consumers that expose declaration-shaped
    /// constraints in the reference representation and test assertions.
    #[cfg(test)]
    pub(crate) fn to_flat_vec(&self) -> Vec<ArgumentConstraint> {
        let mut v = Vec::new();
        for group in &self.groups {
            for matcher in &group.predicates {
                v.push(ArgumentConstraint::new(group.index, matcher.clone()));
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

    #[cfg(test)]
    pub(crate) fn new_for_test(index: ArgumentIndex, predicates: Box<[ArgumentMatcher]>) -> Self {
        Self { index, predicates }
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
    slot: EventSlot,
    event: EventSpec,
    subject: NormalizedSubject,
    arguments: CanonicalArgumentConstraints,
}

impl NormalizedEvent {
    pub(crate) fn new(
        slot: EventSlot,
        event: EventSpec,
        subject: NormalizedSubject,
        arguments: CanonicalArgumentConstraints,
    ) -> Result<Self, SubjectRelationError> {
        classify_subject_relation(&event, &subject)?;
        Ok(Self {
            slot,
            event,
            subject,
            arguments,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_unchecked(
        slot: EventSlot,
        event: EventSpec,
        subject: NormalizedSubject,
        arguments: CanonicalArgumentConstraints,
    ) -> Self {
        Self {
            slot,
            event,
            subject,
            arguments,
        }
    }

    pub(crate) fn slot(&self) -> EventSlot {
        self.slot
    }

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
    sources: Vec<NormalizedEvent>,
    condition: Option<NormalizedLifecycleCondition>,
    completion: NormalizedLifecycleCompletion,
}

impl NormalizedLifecycle {
    pub(crate) fn new(
        sources: Vec<NormalizedEvent>,
        condition: Option<NormalizedLifecycleCondition>,
        completion: NormalizedLifecycleCompletion,
    ) -> Self {
        Self {
            sources,
            condition,
            completion,
        }
    }

    pub(crate) fn sources(&self) -> &[NormalizedEvent] {
        &self.sources
    }

    pub(crate) fn condition(&self) -> Option<&NormalizedLifecycleCondition> {
        self.condition.as_ref()
    }

    pub(crate) fn completion(&self) -> &NormalizedLifecycleCompletion {
        &self.completion
    }
}

// ── Slot traversal ─────────────────────────────────────────────────────────

impl NormalizedRoot {
    /// Collect every unique slot value present in the tree, in ascending
    /// order.
    pub(crate) fn collect_slots(&self) -> Vec<u32> {
        let mut slots = Vec::new();
        self.collect_slots_into(&mut slots);
        slots.sort_unstable();
        slots.dedup();
        slots
    }

    fn collect_slots_into(&self, slots: &mut Vec<u32>) {
        match self {
            Self::Event(ev) => {
                slots.push(ev.slot.get());
                match &ev.subject {
                    NormalizedSubject::Returned { object_slot, .. }
                    | NormalizedSubject::Instance { object_slot, .. } => {
                        slots.push(object_slot.get());
                    }
                    NormalizedSubject::Direct { .. } => {}
                }
            }
            Self::Any(branches) => {
                for branch in &**branches {
                    branch.collect_slots_into(slots);
                }
            }
            Self::Lifecycle(lifecycle) => {
                for source in &lifecycle.sources {
                    slots.push(source.slot.get());
                }
            }
        }
    }

    /// Remap every slot in the tree using the given old→new mapping.
    #[allow(clippy::cast_possible_truncation)]
    fn remap_slots(&mut self, map: &BTreeMap<u32, u32>) {
        match self {
            Self::Event(ev) => {
                if let Some(&new_slot) = map.get(&ev.slot.get()) {
                    ev.slot = EventSlot::from_raw(new_slot);
                }
                match &mut ev.subject {
                    NormalizedSubject::Returned { object_slot, .. }
                    | NormalizedSubject::Instance { object_slot, .. } => {
                        if let Some(&new_slot) = map.get(&object_slot.get()) {
                            *object_slot = ObjectSlot::from_raw(new_slot);
                        }
                    }
                    NormalizedSubject::Direct { .. } => {}
                }
            }
            Self::Any(branches) => {
                for branch in &mut **branches {
                    branch.remap_slots(map);
                }
            }
            Self::Lifecycle(lifecycle) => {
                for source in &mut lifecycle.sources {
                    if let Some(&new_slot) = map.get(&source.slot.get()) {
                        source.slot = EventSlot::from_raw(new_slot);
                    }
                }
            }
        }
    }

    /// Alpha-renumber: replace author-assigned slot values with dense 0..n
    /// slots ordered by the original slot values (deterministic).
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn alpha_renumber_slots(&mut self) {
        let slots = self.collect_slots();
        if slots.is_empty() {
            return;
        }
        let mut map = BTreeMap::new();
        for (new_index, &old) in slots.iter().enumerate() {
            map.insert(old, new_index as u32);
        }
        self.remap_slots(&map);
    }
}
