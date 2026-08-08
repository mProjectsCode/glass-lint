use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    analysis::facts::FactId,
    project::{EvidenceRole, ModuleId},
};

static NEXT_TRACE_ARENA_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// Opaque handle to a node owned by one specific trace arena.
pub struct TraceNodeId {
    arena: u64,
    node: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct QualifiedEvent {
    module: ModuleId,
    fact: FactId,
}

/// One ordered event and evidence role in a reconstructed trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TraceStep {
    event: QualifiedEvent,
    role: EvidenceRole,
}

impl TraceStep {
    fn new(event: QualifiedEvent, role: EvidenceRole) -> Self {
        Self { event, role }
    }

    pub(crate) fn event(&self) -> QualifiedEvent {
        self.event
    }

    pub(crate) fn role(&self) -> EvidenceRole {
        self.role
    }
}

impl QualifiedEvent {
    pub(crate) fn new(module: ModuleId, fact: FactId) -> Self {
        Self { module, fact }
    }

    pub(crate) fn module(self) -> ModuleId {
        self.module
    }

    pub(crate) fn fact(self) -> FactId {
        self.fact
    }
}

#[derive(Debug)]
struct TraceNode {
    parent: Option<TraceNodeId>,
    event: QualifiedEvent,
    role: EvidenceRole,
}

#[derive(Debug)]
pub struct TraceArena {
    arena: u64,
    nodes: Vec<TraceNode>,
    intern: HashMap<(Option<TraceNodeId>, QualifiedEvent, EvidenceRole), TraceNodeId>,
    exhausted: bool,
    limit: usize,
}

impl TraceArena {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            arena: NEXT_TRACE_ARENA_ID.fetch_add(1, Ordering::Relaxed),
            nodes: Vec::new(),
            intern: HashMap::new(),
            exhausted: false,
            limit,
        }
    }

    fn intern(
        &mut self,
        parent: Option<TraceNodeId>,
        event: QualifiedEvent,
        role: EvidenceRole,
    ) -> Option<TraceNodeId> {
        if parent.is_some_and(|parent| parent.arena != self.arena) {
            self.exhausted = true;
            return None;
        }
        let key = (parent, event, role);
        if let Some(&id) = self.intern.get(&key) {
            return Some(id);
        }
        if self.nodes.len() >= self.limit {
            self.exhausted = true;
            return None;
        }
        let Some(id) = TraceNodeId::from_node_count(self.arena, self.nodes.len()) else {
            self.exhausted = true;
            return None;
        };
        self.nodes.push(TraceNode {
            parent,
            event,
            role,
        });
        self.intern.insert(key, id);
        Some(id)
    }

    /// Intern an ordered source-to-sink chain, returning its final node.
    pub(crate) fn intern_chain(
        &mut self,
        steps: impl IntoIterator<Item = (QualifiedEvent, EvidenceRole)>,
    ) -> Option<TraceNodeId> {
        let mut tail = None;
        for (event, role) in steps {
            tail = Some(self.intern(tail, event, role)?);
        }
        tail
    }

    /// Reconstruct a complete trace, returning `None` for a foreign or
    /// otherwise invalid handle rather than silently truncating the chain.
    pub(crate) fn reconstruct_trace(&self, head: TraceNodeId) -> Option<Vec<TraceStep>> {
        let mut steps = Vec::new();
        let mut current = Some(head);
        while let Some(id) = current {
            let node = self.node(id)?;
            steps.push(TraceStep::new(node.event, node.role));
            current = node.parent;
        }
        steps.reverse();
        Some(steps)
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    fn node(&self, id: TraceNodeId) -> Option<&TraceNode> {
        (id.arena == self.arena)
            .then(|| self.nodes.get(id.index()))
            .flatten()
    }
}

/// Intern the common lifecycle evidence shape while leaving prior-sink role
/// policy with the caller that owns the local or cross-module semantics.
pub(super) fn intern_lifecycle_trace(
    arena: &mut TraceArena,
    source: QualifiedEvent,
    requirements: impl IntoIterator<Item = QualifiedEvent>,
    prior_sinks: impl IntoIterator<Item = QualifiedEvent>,
    terminal: QualifiedEvent,
    prior_sink_role: EvidenceRole,
) -> Option<TraceNodeId> {
    let steps = std::iter::once((source, EvidenceRole::Source))
        .chain(
            requirements
                .into_iter()
                .map(|event| (event, EvidenceRole::Requirement)),
        )
        .chain(
            prior_sinks
                .into_iter()
                .map(|event| (event, prior_sink_role)),
        )
        .chain(std::iter::once((terminal, EvidenceRole::Sink)));
    arena.intern_chain(steps)
}

impl TraceNodeId {
    fn from_node_count(arena: u64, count: usize) -> Option<Self> {
        u32::try_from(count).ok().map(|node| Self { arena, node })
    }

    fn index(self) -> usize {
        self.node as usize
    }
}

#[cfg(test)]
fn qe(module: u32, fact: u32) -> QualifiedEvent {
    QualifiedEvent::new(ModuleId::new(module), FactId::new(fact))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_shares_identical_nodes() {
        let mut arena = TraceArena::new(100);
        let id1 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        let id2 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(arena.node_count(), 1);
    }

    #[test]
    fn different_events_get_different_ids() {
        let mut arena = TraceArena::new(100);
        let id1 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        let id2 = arena.intern(None, qe(0, 2), EvidenceRole::Source).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(arena.node_count(), 2);
    }

    #[test]
    fn parent_distinguishes_nodes() {
        let mut arena = TraceArena::new(100);
        let parent = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        let child1 = arena
            .intern(Some(parent), qe(0, 2), EvidenceRole::Sink)
            .unwrap();
        let child2 = arena.intern(None, qe(0, 2), EvidenceRole::Sink).unwrap();
        assert_ne!(child1, child2);
        assert_eq!(arena.node_count(), 3);
    }

    #[test]
    fn reconstruct_trace_walks_parent_chain() {
        let mut arena = TraceArena::new(100);
        let step1 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        let step2 = arena
            .intern(Some(step1), qe(0, 2), EvidenceRole::Requirement)
            .unwrap();
        let step3 = arena
            .intern(Some(step2), qe(0, 3), EvidenceRole::Sink)
            .unwrap();
        let trace = arena.reconstruct_trace(step3).unwrap();
        assert_eq!(trace.len(), 3);
        assert_eq!(trace[0], TraceStep::new(qe(0, 1), EvidenceRole::Source));
        assert_eq!(
            trace[1],
            TraceStep::new(qe(0, 2), EvidenceRole::Requirement)
        );
        assert_eq!(trace[2], TraceStep::new(qe(0, 3), EvidenceRole::Sink));
    }

    #[test]
    fn reconstruct_single_step_trace() {
        let mut arena = TraceArena::new(100);
        let head = arena
            .intern(None, qe(1, 5), EvidenceRole::Occurrence)
            .unwrap();
        let trace = arena.reconstruct_trace(head).unwrap();
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0], TraceStep::new(qe(1, 5), EvidenceRole::Occurrence));
    }

    #[test]
    fn arena_exhaustion_returns_none() {
        let mut arena = TraceArena::new(2);
        assert!(arena.intern(None, qe(0, 1), EvidenceRole::Source).is_some());
        assert!(
            arena
                .intern(None, qe(0, 2), EvidenceRole::Requirement)
                .is_some()
        );
        assert!(arena.intern(None, qe(0, 3), EvidenceRole::Sink).is_none());
        assert!(arena.is_exhausted());
    }

    #[test]
    fn deterministic_ordering_of_nodes() {
        let mut arena = TraceArena::new(100);
        let id1 = arena.intern(None, qe(0, 2), EvidenceRole::Source).unwrap();
        let id2 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        assert_eq!(arena.node(id1).unwrap().event, qe(0, 2));
        assert_eq!(arena.node(id2).unwrap().event, qe(0, 1));
    }

    #[test]
    fn cross_module_qualification() {
        let mut arena = TraceArena::new(100);
        let ev1 = arena.intern(None, qe(1, 10), EvidenceRole::Source).unwrap();
        let ev2 = arena
            .intern(Some(ev1), qe(2, 20), EvidenceRole::Sink)
            .unwrap();
        let trace = arena.reconstruct_trace(ev2).unwrap();
        assert_eq!(trace[0].event().module, ModuleId::new(1));
        assert_eq!(trace[0].event().fact.raw_for_test(), 10);
        assert_eq!(trace[1].event().module, ModuleId::new(2));
        assert_eq!(trace[1].event().fact.raw_for_test(), 20);
    }

    #[test]
    fn exhausted_arena_does_not_affect_earlier_nodes() {
        let mut arena = TraceArena::new(1);
        let id = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        assert!(arena.intern(None, qe(0, 2), EvidenceRole::Sink).is_none());
        let trace = arena.reconstruct_trace(id).unwrap();
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0], TraceStep::new(qe(0, 1), EvidenceRole::Source));
    }

    #[test]
    fn foreign_handles_are_rejected_and_reconstruction_is_explicitly_invalid() {
        let mut arena = TraceArena::new(10);
        let mut foreign = TraceArena::new(10);
        let foreign_head = foreign
            .intern(None, qe(1, 1), EvidenceRole::Source)
            .unwrap();

        assert!(
            arena
                .intern(Some(foreign_head), qe(0, 1), EvidenceRole::Sink)
                .is_none()
        );
        assert!(arena.is_exhausted());
        assert_eq!(arena.reconstruct_trace(foreign_head), None);
    }
}
