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

#[cfg(test)]
mod tests {
    use super::*;

    fn external(module: &str, export: &str) -> ExportResolution {
        ExportResolution::External {
            module: module.into(),
            export: export.into(),
        }
    }

    #[test]
    fn star_merge_marks_disagreeing_identities_ambiguous() {
        let key = ModuleExportKey::new("pkg", "request");
        let mut merged = ModuleIdentityMap::new();
        merged.insert(key.clone(), external("a", "request"));

        let mut other = ModuleIdentityMap::new();
        other.insert(key.clone(), external("b", "request"));
        merged.merge_star_from(other);

        assert_eq!(merged.get(&key), Some(&ExportResolution::Ambiguous));
    }

    #[test]
    fn missing_merge_preserves_authoritative_identity() {
        let key = ModuleExportKey::new("pkg", "request");
        let mut merged = ModuleIdentityMap::new();
        merged.insert(key.clone(), external("direct", "request"));

        let mut other = ModuleIdentityMap::new();
        other.insert(key.clone(), external("star", "request"));
        merged.merge_missing_from(other);

        assert_eq!(merged.get(&key), Some(&external("direct", "request")));
    }
}
