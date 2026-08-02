use std::time::Duration;

/// Immutable phase-timing snapshot shared with harness profiling reports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectPhaseTimings {
    discovery: Duration,
    reads: Duration,
    analyze_source: Duration,
    resolution: Duration,
    linking: Duration,
    matching: Duration,
    total: Duration,
}

impl ProjectPhaseTimings {
    #[must_use]
    pub fn discovery(&self) -> Duration {
        self.discovery
    }

    #[must_use]
    pub fn reads(&self) -> Duration {
        self.reads
    }

    #[must_use]
    pub fn resolution(&self) -> Duration {
        self.resolution
    }

    #[must_use]
    pub fn linking(&self) -> Duration {
        self.linking
    }

    #[must_use]
    pub fn matching(&self) -> Duration {
        self.matching
    }

    #[must_use]
    pub fn total(&self) -> Duration {
        self.total
    }

    #[must_use]
    pub fn parse_and_local_analysis(&self) -> Duration {
        self.analyze_source
    }

    #[must_use]
    pub fn linking_and_matching(&self) -> Duration {
        self.linking.saturating_add(self.matching)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ProjectPhaseTimingsAccumulator {
    discovery: Duration,
    reads: Duration,
    analyze_source: Duration,
    resolution: Duration,
    linking: Duration,
    matching: Duration,
    total: Duration,
}

impl ProjectPhaseTimingsAccumulator {
    pub(super) fn snapshot(self) -> ProjectPhaseTimings {
        ProjectPhaseTimings {
            discovery: self.discovery,
            reads: self.reads,
            analyze_source: self.analyze_source,
            resolution: self.resolution,
            linking: self.linking,
            matching: self.matching,
            total: self.total,
        }
    }

    pub(super) fn record_discovery(&mut self, duration: Duration) {
        self.discovery = self.discovery.saturating_add(duration);
    }

    pub(super) fn record_reads(&mut self, duration: Duration) {
        self.reads = self.reads.saturating_add(duration);
    }

    pub(super) fn record_analyze_source(&mut self, duration: Duration) {
        self.analyze_source = self.analyze_source.saturating_add(duration);
    }

    pub(super) fn record_resolution(&mut self, duration: Duration) {
        self.resolution = self.resolution.saturating_add(duration);
    }

    pub(super) fn record_linking(&mut self, duration: Duration) {
        self.linking = self.linking.saturating_add(duration);
    }

    pub(super) fn record_matching(&mut self, duration: Duration) {
        self.matching = self.matching.saturating_add(duration);
    }

    pub(super) fn record_total(&mut self, duration: Duration) {
        self.total = self.total.saturating_add(duration);
    }
}

impl std::ops::AddAssign for ProjectPhaseTimingsAccumulator {
    fn add_assign(&mut self, rhs: Self) {
        self.discovery = self.discovery.saturating_add(rhs.discovery);
        self.reads = self.reads.saturating_add(rhs.reads);
        self.analyze_source = self.analyze_source.saturating_add(rhs.analyze_source);
        self.resolution = self.resolution.saturating_add(rhs.resolution);
        self.linking = self.linking.saturating_add(rhs.linking);
        self.matching = self.matching.saturating_add(rhs.matching);
        self.total = self.total.saturating_add(rhs.total);
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectLoadMetrics {
    timings: ProjectPhaseTimings,
    files: usize,
    requests: usize,
    edges: usize,
    bytes: u64,
}

impl ProjectLoadMetrics {
    #[must_use]
    pub fn phase_timings(&self) -> ProjectPhaseTimings {
        self.timings
    }

    #[must_use]
    pub fn files(&self) -> usize {
        self.files
    }

    #[must_use]
    pub fn requests(&self) -> usize {
        self.requests
    }

    #[must_use]
    pub fn edges(&self) -> usize {
        self.edges
    }

    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ProjectMetricsAccumulator {
    pub(crate) timings: ProjectPhaseTimingsAccumulator,
    pub(crate) files: usize,
    pub(crate) requests: usize,
    pub(crate) edges: usize,
    pub(crate) bytes: u64,
}

impl ProjectMetricsAccumulator {
    pub(super) fn snapshot(&self) -> ProjectLoadMetrics {
        ProjectLoadMetrics {
            timings: self.timings.snapshot(),
            files: self.files,
            requests: self.requests,
            edges: self.edges,
            bytes: self.bytes,
        }
    }
}

impl std::ops::AddAssign for ProjectMetricsAccumulator {
    fn add_assign(&mut self, rhs: Self) {
        self.timings += rhs.timings;
        self.files = self.files.saturating_add(rhs.files);
        self.requests = self.requests.saturating_add(rhs.requests);
        self.edges = self.edges.saturating_add(rhs.edges);
        self.bytes = self.bytes.saturating_add(rhs.bytes);
    }
}
