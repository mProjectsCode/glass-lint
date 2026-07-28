#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalysisOperationCounts {
    files: usize,
    requests: usize,
    edges: usize,
    exports: usize,
    scc_rounds: usize,
    effect_projections: usize,
    evidence: usize,
    max_live_alternatives: usize,
    trace_nodes: usize,
    trace_heads: usize,
    coalescing_comparisons: usize,
    fixed_point_iterations: usize,
    rendered_traces: usize,
}

impl AnalysisOperationCounts {
    pub fn new(
        files: usize,
        requests: usize,
        edges: usize,
        exports: usize,
        scc_rounds: usize,
        effect_projections: usize,
        evidence: usize,
    ) -> Self {
        Self {
            files,
            requests,
            edges,
            exports,
            scc_rounds,
            effect_projections,
            evidence,
            max_live_alternatives: 0,
            trace_nodes: 0,
            trace_heads: 0,
            coalescing_comparisons: 0,
            fixed_point_iterations: 0,
            rendered_traces: 0,
        }
    }

    pub fn files(&self) -> usize {
        self.files
    }

    pub fn requests(&self) -> usize {
        self.requests
    }

    pub fn edges(&self) -> usize {
        self.edges
    }

    pub fn exports(&self) -> usize {
        self.exports
    }

    pub fn scc_rounds(&self) -> usize {
        self.scc_rounds
    }

    pub fn effect_projections(&self) -> usize {
        self.effect_projections
    }

    pub fn evidence(&self) -> usize {
        self.evidence
    }

    /// Maximum number of correlated semantic alternatives retained at once.
    pub fn max_live_alternatives(&self) -> usize {
        self.max_live_alternatives
    }

    /// Number of interned evidence trace nodes.
    pub fn trace_nodes(&self) -> usize {
        self.trace_nodes
    }

    /// Number of complete trace heads produced by semantic projection.
    pub fn trace_heads(&self) -> usize {
        self.trace_heads
    }

    /// Number of semantic-state comparisons performed while coalescing paths.
    pub fn coalescing_comparisons(&self) -> usize {
        self.coalescing_comparisons
    }

    /// Number of loop fixed-point iterations performed by flow projection.
    pub fn fixed_point_iterations(&self) -> usize {
        self.fixed_point_iterations
    }

    /// Number of traces reconstructed into user-facing findings.
    pub fn rendered_traces(&self) -> usize {
        self.rendered_traces
    }

    pub(crate) fn set_path_metrics(
        &mut self,
        max_live_alternatives: usize,
        trace_nodes: usize,
        trace_heads: usize,
        coalescing_comparisons: usize,
        fixed_point_iterations: usize,
        rendered_traces: usize,
    ) {
        self.max_live_alternatives = max_live_alternatives;
        self.trace_nodes = trace_nodes;
        self.trace_heads = trace_heads;
        self.coalescing_comparisons = coalescing_comparisons;
        self.fixed_point_iterations = fixed_point_iterations;
        self.rendered_traces = rendered_traces;
    }

    pub(crate) fn set_effect_projections(&mut self, value: usize) {
        self.effect_projections = value;
    }

    pub fn into_parts(self) -> (usize, usize, usize, usize, usize, usize, usize) {
        (
            self.files,
            self.requests,
            self.edges,
            self.exports,
            self.scc_rounds,
            self.effect_projections,
            self.evidence,
        )
    }
}

impl std::ops::AddAssign for AnalysisOperationCounts {
    fn add_assign(&mut self, rhs: Self) {
        self.files = self.files.saturating_add(rhs.files);
        self.requests = self.requests.saturating_add(rhs.requests);
        self.edges = self.edges.saturating_add(rhs.edges);
        self.exports = self.exports.saturating_add(rhs.exports);
        self.scc_rounds = self.scc_rounds.saturating_add(rhs.scc_rounds);
        self.effect_projections = self
            .effect_projections
            .saturating_add(rhs.effect_projections);
        self.evidence = self.evidence.saturating_add(rhs.evidence);
        self.max_live_alternatives = self.max_live_alternatives.max(rhs.max_live_alternatives);
        self.trace_nodes = self.trace_nodes.saturating_add(rhs.trace_nodes);
        self.trace_heads = self.trace_heads.saturating_add(rhs.trace_heads);
        self.coalescing_comparisons = self
            .coalescing_comparisons
            .saturating_add(rhs.coalescing_comparisons);
        self.fixed_point_iterations = self
            .fixed_point_iterations
            .saturating_add(rhs.fixed_point_iterations);
        self.rendered_traces = self.rendered_traces.saturating_add(rhs.rendered_traces);
    }
}
