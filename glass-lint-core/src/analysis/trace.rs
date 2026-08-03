use std::collections::HashMap;

use crate::{
    analysis::facts::FactId,
    api::classification::TraceNodeId,
    project::{EvidenceRole, ModuleId},
};

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

    pub fn event(&self) -> QualifiedEvent {
        self.event
    }

    pub fn role(&self) -> EvidenceRole {
        self.role
    }
}

impl QualifiedEvent {
    pub fn new(module: ModuleId, fact: FactId) -> Self {
        Self { module, fact }
    }

    pub fn module(&self) -> ModuleId {
        self.module
    }

    pub fn fact(&self) -> FactId {
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
    nodes: Vec<TraceNode>,
    intern: HashMap<(Option<TraceNodeId>, QualifiedEvent, EvidenceRole), TraceNodeId>,
    exhausted: bool,
    limit: usize,
}

impl TraceArena {
    pub fn new(limit: usize) -> Self {
        Self {
            nodes: Vec::new(),
            intern: HashMap::new(),
            exhausted: false,
            limit,
        }
    }

    pub fn intern(
        &mut self,
        parent: Option<TraceNodeId>,
        event: QualifiedEvent,
        role: EvidenceRole,
    ) -> Option<TraceNodeId> {
        let key = (parent, event, role);
        if let Some(&id) = self.intern.get(&key) {
            return Some(id);
        }
        if self.nodes.len() >= self.limit {
            self.exhausted = true;
            return None;
        }
        let id = TraceNodeId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(TraceNode {
            parent,
            event,
            role,
        });
        self.intern.insert(key, id);
        Some(id)
    }

    pub fn reconstruct_trace(&self, head: TraceNodeId) -> Vec<TraceStep> {
        let mut steps = Vec::new();
        let mut current = Some(head);
        while let Some(id) = current {
            if let Some(node) = self.nodes.get(id.0 as usize) {
                steps.push(TraceStep::new(node.event, node.role));
                current = node.parent;
            } else {
                break;
            }
        }
        steps.reverse();
        steps
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
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
        let trace = arena.reconstruct_trace(step3);
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
        let trace = arena.reconstruct_trace(head);
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
        assert_eq!(id1.0, 0);
        assert_eq!(id2.0, 1);
    }

    #[test]
    fn cross_module_qualification() {
        let mut arena = TraceArena::new(100);
        let ev1 = arena.intern(None, qe(1, 10), EvidenceRole::Source).unwrap();
        let ev2 = arena
            .intern(Some(ev1), qe(2, 20), EvidenceRole::Sink)
            .unwrap();
        let trace = arena.reconstruct_trace(ev2);
        assert_eq!(trace[0].event().module.get(), 1);
        assert_eq!(trace[0].event().fact.raw_for_test(), 10);
        assert_eq!(trace[1].event().module.get(), 2);
        assert_eq!(trace[1].event().fact.raw_for_test(), 20);
    }

    #[test]
    fn exhausted_arena_does_not_affect_earlier_nodes() {
        let mut arena = TraceArena::new(1);
        let id = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
        assert!(arena.intern(None, qe(0, 2), EvidenceRole::Sink).is_none());
        let trace = arena.reconstruct_trace(id);
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0], TraceStep::new(qe(0, 1), EvidenceRole::Source));
    }
}
