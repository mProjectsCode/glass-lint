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

    pub(in crate::analysis) fn into_entries(self) -> Vec<(ModuleExportKey, ExportResolution)> {
        let mut entries = Vec::new();
        for (module, exports) in self.modules {
            for (export, value) in exports {
                entries.push((ModuleExportKey::new(module.clone(), export), value));
            }
        }
        entries
    }
}
