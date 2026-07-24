use glass_lint_datastructures::NamePath;

use crate::analysis::matching::occurrence::{
    InstanceMemberKey, ModuleOccurrences, NameOccurrences, OccurrenceIndex, Occurrences,
    ReturnedMemberKey,
};

#[derive(Debug, Default)]
pub(super) struct CallIndexes {
    pub(super) calls: NameOccurrences,
    pub(super) global_calls: Occurrences,
    pub(super) module_calls: ModuleOccurrences,
}

impl CallIndexes {
    pub(super) fn normalize(&mut self) {
        self.calls.normalize();
        self.global_calls.normalize();
        self.module_calls.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.global_calls.is_empty() && self.module_calls.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct MemberIndexes {
    pub(super) calls: OccurrenceIndex<NamePath>,
    pub(super) rooted_calls: OccurrenceIndex<NamePath>,
    pub(super) module_calls: ModuleOccurrences,
    pub(super) reads: OccurrenceIndex<NamePath>,
    pub(super) rooted_reads: OccurrenceIndex<NamePath>,
    pub(super) module_reads: ModuleOccurrences,
    pub(super) returned_calls: OccurrenceIndex<ReturnedMemberKey>,
    pub(super) returned_reads: OccurrenceIndex<ReturnedMemberKey>,
    pub(super) instance_calls: OccurrenceIndex<InstanceMemberKey>,
}

impl MemberIndexes {
    pub(super) fn normalize(&mut self) {
        self.calls.normalize();
        self.rooted_calls.normalize();
        self.module_calls.normalize();
        self.reads.normalize();
        self.rooted_reads.normalize();
        self.module_reads.normalize();
        self.returned_calls.normalize();
        self.returned_reads.normalize();
        self.instance_calls.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty()
            && self.rooted_calls.is_empty()
            && self.module_calls.is_empty()
            && self.reads.is_empty()
            && self.rooted_reads.is_empty()
            && self.module_reads.is_empty()
            && self.returned_calls.is_empty()
            && self.returned_reads.is_empty()
            && self.instance_calls.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConstructionIndexes {
    pub(super) classes: Occurrences,
    pub(super) module_classes: ModuleOccurrences,
    pub(super) constructors: NameOccurrences,
    pub(super) global_constructors: Occurrences,
    pub(super) module_constructors: ModuleOccurrences,
}

impl ConstructionIndexes {
    pub(super) fn normalize(&mut self) {
        self.classes.normalize();
        self.module_classes.normalize();
        self.constructors.normalize();
        self.global_constructors.normalize();
        self.module_constructors.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.classes.is_empty()
            && self.module_classes.is_empty()
            && self.constructors.is_empty()
            && self.global_constructors.is_empty()
            && self.module_constructors.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct LiteralIndexes {
    pub(super) imports: Occurrences,
    pub(super) strings: Occurrences,
}

impl LiteralIndexes {
    pub(super) fn normalize(&mut self) {
        self.imports.normalize();
        self.strings.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.imports.is_empty() && self.strings.is_empty()
    }
}
