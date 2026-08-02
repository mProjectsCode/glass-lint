use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::analysis::{matching::occurrence::ModuleExportKey, project::model::ExportResolution};

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct ModuleIdentityMap {
    modules: BTreeMap<SmolStr, BTreeMap<SmolStr, ExportResolution>>,
}

impl ModuleIdentityMap {
    pub(in crate::analysis) fn new() -> Self {
        Self::default()
    }

    pub(in crate::analysis) fn get(&self, key: &ModuleExportKey) -> Option<&ExportResolution> {
        self.get_parts(key.module(), key.export())
    }

    pub(in crate::analysis) fn get_parts(
        &self,
        module: &str,
        export: &str,
    ) -> Option<&ExportResolution> {
        self.modules.get(module)?.get(export)
    }

    pub(in crate::analysis) fn insert(
        &mut self,
        key: ModuleExportKey,
        value: ExportResolution,
    ) -> Option<ExportResolution> {
        let (module, export) = key.into_parts();
        self.modules
            .entry(module)
            .or_default()
            .insert(export, value)
    }

    /// Merge star-derived identities, marking disagreements as ambiguous.
    pub(in crate::analysis) fn merge_star_from(&mut self, other: Self) {
        for (module, exports) in other.modules {
            for (export, value) in exports {
                let key = ModuleExportKey::new(module.clone(), export);
                match self.get(&key) {
                    None => {
                        self.insert(key, value);
                    }
                    Some(existing)
                        if existing == &value || *existing == ExportResolution::Ambiguous => {}
                    Some(_) => {
                        self.insert(key, ExportResolution::Ambiguous);
                    }
                }
            }
        }
    }

    /// Merge star-derived identities without replacing direct exports.
    pub(in crate::analysis) fn merge_missing_from(&mut self, other: Self) {
        for (module, exports) in other.modules {
            for (export, value) in exports {
                let key = ModuleExportKey::new(module.clone(), export);
                if self.get(&key).is_none() {
                    self.insert(key, value);
                }
            }
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
