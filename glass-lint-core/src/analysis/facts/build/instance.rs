use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstanceCallable {
    module: SmolStr,
    export: SmolStr,
    member: SymbolPath,
}

impl InstanceCallable {
    pub(super) fn new(
        module: impl Into<SmolStr>,
        export: impl Into<SmolStr>,
        member: SymbolPath,
    ) -> Self {
        Self {
            module: module.into(),
            export: export.into(),
            member,
        }
    }

    pub(super) fn class_identity(&self) -> (SmolStr, SmolStr) {
        (self.module.clone(), self.export.clone())
    }

    pub(super) fn member(&self) -> &SymbolPath {
        &self.member
    }
}
