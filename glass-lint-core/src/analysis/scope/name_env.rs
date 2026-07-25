use glass_lint_datastructures::{NameId, NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::Environment;

#[derive(Debug)]
pub(super) struct NameEnvironment {
    pub(super) names: NameTable,
    pub(super) environment: Environment,
}

impl NameEnvironment {
    pub(super) fn new(names: NameTable, environment: Environment) -> Self {
        Self { names, environment }
    }

    pub(super) fn resolve_name_id(&self, name: NameId) -> Option<SmolStr> {
        self.names.resolve(name).map(SmolStr::new)
    }

    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.names.lookup(name)
    }

    pub(super) fn intern_name_mut(&mut self, name: &str) -> Option<NameId> {
        self.names.intern(name).ok()
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.lookup_path(path)
    }

    pub(super) fn name_table_exhausted(&self) -> bool {
        self.names.exhausted()
    }

    pub(super) fn into_name_table(self) -> NameTable {
        self.names
    }

    pub(super) fn name_table_mut(&mut self) -> &mut NameTable {
        &mut self.names
    }

    pub(super) fn name_exhaustion(&self) -> Option<glass_lint_datastructures::NameExhausted> {
        self.names.exhaustion()
    }

    #[cfg(test)]
    pub(super) fn name_snapshot(&self) -> NameTable {
        self.names.clone()
    }

    pub(super) fn symbol_path(&self, path: &NamePath) -> Option<SymbolPath> {
        self.names.resolve_path(path)
    }

    pub(super) fn is_global(&self, name: &str) -> bool {
        self.environment.is_global(name)
    }

    pub(super) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.environment.is_global_member(root, member)
    }

    pub(super) fn global_objects(&self) -> impl Iterator<Item = &str> {
        self.environment.global_objects()
    }
}
