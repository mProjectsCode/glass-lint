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
    }
}
