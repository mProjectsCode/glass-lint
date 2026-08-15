use glass_lint_datastructures::SymbolPath;

use crate::analysis::model::fact::ClassIdentity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstanceCallable {
    identity: ClassIdentity,
    member: SymbolPath,
}

impl InstanceCallable {
    pub(super) fn new(identity: ClassIdentity, member: SymbolPath) -> Self {
        Self { identity, member }
    }

    pub(super) fn class_identity(&self) -> ClassIdentity {
        self.identity.clone()
    }

    pub(super) fn member(&self) -> &SymbolPath {
        &self.member
    }
}
