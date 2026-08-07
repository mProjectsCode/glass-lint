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
}

/// Crate-private phase accumulator for the finalized operation-count DTO.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnalysisOperationCountsBuilder {
    counts: AnalysisOperationCounts,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReportPathMetrics {
    pub(crate) max_live_alternatives: usize,
    pub(crate) trace_nodes: usize,
    pub(crate) trace_heads: usize,
    pub(crate) coalescing_comparisons: usize,
    pub(crate) fixed_point_iterations: usize,
    pub(crate) rendered_traces: usize,
}

impl AnalysisOperationCountsBuilder {
    pub(crate) fn record_files(&mut self, value: usize) {
        self.counts.files = value;
    }

    pub(crate) fn record_requests(&mut self, value: usize) {
        self.counts.requests = value;
    }

    pub(crate) fn record_edges(&mut self, value: usize) {
        self.counts.edges = value;
    }

    pub(crate) fn record_exports(&mut self, value: usize) {
        self.counts.exports = value;
    }

    pub(crate) fn record_scc_rounds(&mut self, value: usize) {
        self.counts.scc_rounds = value;
    }

    pub(crate) fn record_effect_projections(&mut self, value: usize) {
        self.counts.effect_projections = value;
    }

    pub(crate) fn record_evidence(&mut self, value: usize) {
        self.counts.evidence = value;
    }

    pub(crate) fn record_path_metrics(&mut self, metrics: ReportPathMetrics) {
        self.counts.max_live_alternatives = metrics.max_live_alternatives;
        self.counts.trace_nodes = metrics.trace_nodes;
        self.counts.trace_heads = metrics.trace_heads;
        self.counts.coalescing_comparisons = metrics.coalescing_comparisons;
        self.counts.fixed_point_iterations = metrics.fixed_point_iterations;
        self.counts.rendered_traces = metrics.rendered_traces;
    }

    pub(crate) fn finish(self) -> AnalysisOperationCounts {
        self.counts
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
