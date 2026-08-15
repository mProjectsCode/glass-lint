use std::collections::BTreeMap;

use crate::analysis::{matching::ModuleExportKey, project::model::ExportResolution};

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct ModuleIdentityMap {
    entries: BTreeMap<ModuleExportKey, ExportResolution>,
}

impl ModuleIdentityMap {
    pub(in crate::analysis) fn new() -> Self {
        Self::default()
    }

    pub(in crate::analysis) fn get(&self, key: &ModuleExportKey) -> Option<&ExportResolution> {
        self.entries.get(key)
    }

    pub(in crate::analysis) fn insert(
        &mut self,
        key: ModuleExportKey,
        value: ExportResolution,
    ) -> Option<ExportResolution> {
        self.entries.insert(key, value)
    }

    /// Merge star-derived identities, marking disagreements as ambiguous.
    pub(in crate::analysis) fn merge_star_from(&mut self, other: Self) {
        for (key, value) in other.entries {
            match self.entries.get(&key) {
                None => {
                    self.entries.insert(key, value);
                }
                Some(existing)
                    if existing == &value || *existing == ExportResolution::Ambiguous => {}
                Some(_) => {
                    self.entries.insert(key, ExportResolution::Ambiguous);
                }
            }
        }
    }

    /// Merge star-derived identities without replacing direct exports.
    pub(in crate::analysis) fn merge_missing_from(&mut self, other: Self) {
        for (key, value) in other.entries {
            self.entries.entry(key).or_insert(value);
        }
    }
}

/// Star-export disagreement policy, the single source of truth for both the
/// overlay walker and the single-export resolver: multiple star paths that
/// resolve to differing identities mark the export `Ambiguous`; a direct or
/// named export from the exporting module wins over any star contribution;
/// unresolved star paths keep each traversal's own unknown handling.
/// `ModuleIdentityContributions::add_star` applies this policy to the matcher
/// overlay, and `ExportResolver::walk_star_exports` applies the same rule to
/// a single export lookup.
#[derive(Default)]
pub(in crate::analysis) struct ModuleIdentityContributions {
    direct: ModuleIdentityMap,
    stars: ModuleIdentityMap,
}

impl ModuleIdentityContributions {
    pub(in crate::analysis) fn new() -> Self {
        Self::default()
    }

    pub(in crate::analysis) fn add_direct(&mut self, entries: ModuleIdentityMap) {
        self.direct.entries.extend(entries.entries);
    }

    pub(in crate::analysis) fn add_star(&mut self, entries: ModuleIdentityMap) {
        self.stars.merge_star_from(entries);
    }

    pub(in crate::analysis) fn finish_into(self, identities: &mut ModuleIdentityMap) {
        identities.entries.extend(self.direct.entries);
        identities.merge_missing_from(self.stars);
    }
}

#[cfg(test)]
mod tests;
