use std::time::Duration;

use crate::error::ProjectLoadError;

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

    pub(crate) fn record_discovery(&mut self, duration: Duration) {
        self.discovery = self.discovery.saturating_add(duration);
    }

    pub(crate) fn record_reads(&mut self, duration: Duration) {
        self.reads = self.reads.saturating_add(duration);
    }

    pub(crate) fn record_analyze_source(&mut self, duration: Duration) {
        self.analyze_source = self.analyze_source.saturating_add(duration);
    }

    pub(crate) fn record_resolution(&mut self, duration: Duration) {
        self.resolution = self.resolution.saturating_add(duration);
    }

    pub(crate) fn record_linking(&mut self, duration: Duration) {
        self.linking = self.linking.saturating_add(duration);
    }

    pub(crate) fn record_matching(&mut self, duration: Duration) {
        self.matching = self.matching.saturating_add(duration);
    }

    pub(crate) fn record_total(&mut self, duration: Duration) {
        self.total = self.total.saturating_add(duration);
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

    pub(crate) fn record_discovery(&mut self, duration: Duration) {
        self.timings.record_discovery(duration);
    }

    pub(crate) fn record_reads(&mut self, duration: Duration) {
        self.timings.record_reads(duration);
    }

    pub(crate) fn record_analyze_source(&mut self, duration: Duration) {
        self.timings.record_analyze_source(duration);
    }

    pub(crate) fn record_resolution(&mut self, duration: Duration) {
        self.timings.record_resolution(duration);
    }

    pub(crate) fn record_linking(&mut self, duration: Duration) {
        self.timings.record_linking(duration);
    }

    pub(crate) fn record_matching(&mut self, duration: Duration) {
        self.timings.record_matching(duration);
    }

    pub(crate) fn record_total(&mut self, duration: Duration) {
        self.timings.record_total(duration);
    }

    pub(crate) fn record_files(&mut self, files: usize) {
        self.files = files;
    }

    pub(crate) fn source_bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn admit_requests(
        &mut self,
        count: usize,
        limit: usize,
    ) -> Result<(), ProjectLoadError> {
        self.requests = self
            .requests
            .checked_add(count)
            .ok_or(ProjectLoadError::TooManyRequests(limit))?;
        if self.requests > limit {
            return Err(ProjectLoadError::TooManyRequests(limit));
        }
        Ok(())
    }

    pub(crate) fn record_edge(&mut self) {
        self.edges = self.edges.saturating_add(1);
    }

    pub(crate) fn admit_source_bytes(
        &mut self,
        bytes: u64,
        limit: u64,
    ) -> Result<(), ProjectLoadError> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > limit {
            return Err(ProjectLoadError::ProjectSourceTooLarge {
                bytes: self.bytes,
                limit,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
