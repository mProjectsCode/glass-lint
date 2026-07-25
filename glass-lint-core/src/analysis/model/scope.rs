use glass_lint_datastructures::{NameId, NamePath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub usize);

impl ScopeId {
    pub fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for ScopeId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopedName {
    scope: ScopeId,
    name: NameId,
}

impl ScopedName {
    pub fn new(scope: ScopeId, name: NameId) -> Self {
        Self { scope, name }
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub fn name(&self) -> NameId {
        self.name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingVersion(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(pub u32);

impl From<FunctionId> for u32 {
    fn from(id: FunctionId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingRoot {
    Binding {
        function: FunctionId,
        binding: BindingId,
        version: BindingVersion,
    },
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingKey {
    root: BindingRoot,
    path: NamePath,
}

impl BindingKey {
    pub fn new(root: BindingRoot) -> Self {
        Self {
            root,
            path: NamePath::new(),
        }
    }

    pub fn append_segment(&mut self, segment: NameId) {
        self.path.append(segment);
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::NameTable;

    use super::*;

    #[test]
    fn binding_versions_are_part_of_identity() {
        let mut first = BindingKey::new(BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(0),
        });
        let mut names = NameTable::default();
        let value = names.intern("value").unwrap();
        first.append_segment(value);
        let mut second = BindingKey::new(BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(1),
        });
        second.append_segment(value);
        assert_ne!(first, second);
    }
}
