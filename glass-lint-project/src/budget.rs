use std::time::Instant;

use crate::error::ProjectLoadError;

/// Per-load resource budget that governs discovery, reading, and loading.
///
/// Counters are shared across all phases so budget limits are enforced
/// consistently regardless of how many roots, tsconfig references, or
/// waves a project has.
#[derive(Clone, Debug)]
pub struct ProjectResourceBudget {
    /// Maximum number of directory entries to visit across all walks.
    max_visited: usize,
    /// Visited entry counter shared across all `collect_files` calls.
    visited: usize,
    /// Maximum cumulative bytes for tsconfig files.
    max_config_bytes: u64,
    /// Cumulative tsconfig byte counter.
    config_bytes: u64,
    /// Deadline after which the load must stop.
    deadline: Instant,
}

impl ProjectResourceBudget {
    pub fn new(max_visited: usize, max_config_bytes: u64, deadline: Instant) -> Self {
        Self {
            max_visited,
            visited: 0,
            max_config_bytes,
            config_bytes: 0,
            deadline,
        }
    }

    /// Record one visited entry. Returns error if the limit is exceeded.
    pub fn record_visited(&mut self) -> Result<(), ProjectLoadError> {
        self.visited = self.visited.saturating_add(1);
        if self.visited > self.max_visited {
            return Err(ProjectLoadError::TooManyEntries(self.max_visited));
        }
        Ok(())
    }

    /// Record tsconfig bytes consumed. Returns error if the aggregate limit is
    /// exceeded.
    pub fn record_config_bytes(&mut self, bytes: u64) -> Result<(), ProjectLoadError> {
        self.config_bytes = self.config_bytes.saturating_add(bytes);
        if self.config_bytes > self.max_config_bytes {
            return Err(ProjectLoadError::ProjectSourceTooLarge {
                bytes: self.config_bytes,
                limit: self.max_config_bytes,
            });
        }
        Ok(())
    }

    /// Check deadline; returns Timeout if expired.
    pub fn check_deadline(&self) -> Result<(), ProjectLoadError> {
        (Instant::now() <= self.deadline)
            .then_some(())
            .ok_or(ProjectLoadError::Timeout)
    }

    pub fn max_visited(&self) -> usize {
        self.max_visited
    }

    pub fn max_config_bytes(&self) -> u64 {
        self.max_config_bytes
    }
}
