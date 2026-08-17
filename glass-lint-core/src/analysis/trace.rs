use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    analysis::facts::FactId,
    project::{EvidenceRole, ModuleId},
};

static NEXT_TRACE_ARENA_ID: AtomicU64 = AtomicU64::new(1);

/// Opaque handle to a node owned by one specific trace arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
        let Some(id) = self.node_id(self.nodes.len()) else {
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

    /// Build the node id for a node count within this arena.
    fn node_id(&self, count: usize) -> Option<TraceNodeId> {
        u32::try_from(count).ok().map(|node| TraceNodeId {
            arena: self.arena,
            node,
        })
    }

    /// Intern the common lifecycle evidence shape while leaving prior-sink role
    /// policy with the caller that owns the local or cross-module semantics.
    pub(crate) fn intern_lifecycle_trace(
        &mut self,
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
        self.intern_chain(steps)
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

impl TraceNodeId {
    fn index(self) -> usize {
        self.node as usize
    }
}

#[cfg(test)]
fn qe(module: u32, fact: u32) -> QualifiedEvent {
    QualifiedEvent::new(ModuleId::new(module), FactId::new(fact))
}

#[cfg(test)]
mod tests;
