use glass_lint_datastructures::NamePath;

use crate::analysis::matching::occurrence::{
    InstanceMemberKey, ModuleExportKey, ModuleOccurrences, NameOccurrences, Occurrence,
    OccurrenceIndex, Occurrences, ReturnedMemberKey,
};

#[derive(Debug, Default)]
pub(super) struct CallIndexes {
    calls: NameOccurrences,
    global_calls: Occurrences,
    module_calls: ModuleOccurrences,
}

impl CallIndexes {
    pub(super) fn record_call(
        &mut self,
        name: glass_lint_datastructures::NameId,
        occurrence: Occurrence,
    ) {
        self.calls.push_occurrence(name, occurrence);
    }

    pub(super) fn record_global_call(
        &mut self,
        name: impl Into<smol_str::SmolStr>,
        occurrence: Occurrence,
    ) {
        self.global_calls.push_occurrence(name.into(), occurrence);
    }

    pub(super) fn record_module_call(&mut self, key: ModuleExportKey, occurrence: Occurrence) {
        self.module_calls.push_occurrence(key, occurrence);
    }

    pub(super) fn calls(&self) -> &NameOccurrences {
        &self.calls
    }

    pub(super) fn global_calls(&self) -> &Occurrences {
        &self.global_calls
    }

    pub(super) fn module_calls(&self) -> &ModuleOccurrences {
        &self.module_calls
    }

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

#[derive(Debug, Default)]
pub(super) struct MemberIndexes {
    calls: OccurrenceIndex<NamePath>,
    rooted_calls: OccurrenceIndex<NamePath>,
    module_calls: ModuleOccurrences,
    reads: OccurrenceIndex<NamePath>,
    rooted_reads: OccurrenceIndex<NamePath>,
    rooted_writes: OccurrenceIndex<NamePath>,
    module_reads: ModuleOccurrences,
    returned_calls: OccurrenceIndex<ReturnedMemberKey>,
    returned_reads: OccurrenceIndex<ReturnedMemberKey>,
    instance_calls: OccurrenceIndex<InstanceMemberKey>,
}

impl MemberIndexes {
    pub(super) fn record_call(&mut self, path: NamePath, occurrence: Occurrence) {
        self.calls.push_occurrence(path, occurrence);
    }

    pub(super) fn record_rooted_call(&mut self, path: NamePath, occurrence: Occurrence) {
        self.rooted_calls.push_occurrence(path, occurrence);
    }

    pub(super) fn record_module_call(&mut self, key: ModuleExportKey, occurrence: Occurrence) {
        self.module_calls.push_occurrence(key, occurrence);
    }

    pub(super) fn record_read(&mut self, path: NamePath, occurrence: Occurrence) {
        self.reads.push_occurrence(path, occurrence);
    }

    pub(super) fn record_rooted_read(&mut self, path: NamePath, occurrence: Occurrence) {
        self.rooted_reads.push_occurrence(path, occurrence);
    }

    pub(super) fn record_rooted_write(&mut self, path: NamePath, occurrence: Occurrence) {
        self.rooted_writes.push_occurrence(path, occurrence);
    }

    pub(super) fn record_module_read(&mut self, key: ModuleExportKey, occurrence: Occurrence) {
        self.module_reads.push_occurrence(key, occurrence);
    }

    pub(super) fn record_returned_call(&mut self, key: ReturnedMemberKey, occurrence: Occurrence) {
        self.returned_calls.push_occurrence(key, occurrence);
    }

    pub(super) fn record_returned_read(&mut self, key: ReturnedMemberKey, occurrence: Occurrence) {
        self.returned_reads.push_occurrence(key, occurrence);
    }

    pub(super) fn record_instance_call(&mut self, key: InstanceMemberKey, occurrence: Occurrence) {
        self.instance_calls.push_occurrence(key, occurrence);
    }

    pub(super) fn calls(&self) -> &OccurrenceIndex<NamePath> {
        &self.calls
    }

    pub(super) fn rooted_calls(&self) -> &OccurrenceIndex<NamePath> {
        &self.rooted_calls
    }

    pub(super) fn module_calls(&self) -> &ModuleOccurrences {
        &self.module_calls
    }

    pub(super) fn reads(&self) -> &OccurrenceIndex<NamePath> {
        &self.reads
    }

    pub(super) fn rooted_reads(&self) -> &OccurrenceIndex<NamePath> {
        &self.rooted_reads
    }

    pub(super) fn rooted_writes(&self) -> &OccurrenceIndex<NamePath> {
        &self.rooted_writes
    }

    pub(super) fn module_reads(&self) -> &ModuleOccurrences {
        &self.module_reads
    }

    pub(super) fn returned_calls(&self) -> &OccurrenceIndex<ReturnedMemberKey> {
        &self.returned_calls
    }

    pub(super) fn returned_reads(&self) -> &OccurrenceIndex<ReturnedMemberKey> {
        &self.returned_reads
    }

    pub(super) fn instance_calls(&self) -> &OccurrenceIndex<InstanceMemberKey> {
        &self.instance_calls
    }

    pub(super) fn normalize(&mut self) {
        self.calls.normalize();
        self.rooted_calls.normalize();
        self.module_calls.normalize();
        self.reads.normalize();
        self.rooted_reads.normalize();
        self.rooted_writes.normalize();
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
            && self.rooted_writes.is_empty()
            && self.module_reads.is_empty()
            && self.returned_calls.is_empty()
            && self.returned_reads.is_empty()
            && self.instance_calls.is_empty()
    }
}

#[derive(Debug, Default)]
pub(super) struct ConstructionIndexes {
    classes: Occurrences,
    module_classes: ModuleOccurrences,
    constructors: NameOccurrences,
    global_constructors: Occurrences,
    rooted_constructors: OccurrenceIndex<NamePath>,
    module_constructors: ModuleOccurrences,
}

impl ConstructionIndexes {
    pub(super) fn record_class(
        &mut self,
        name: impl Into<smol_str::SmolStr>,
        occurrence: Occurrence,
    ) {
        self.classes.push_occurrence(name.into(), occurrence);
    }

    pub(super) fn record_module_class(&mut self, key: ModuleExportKey, occurrence: Occurrence) {
        self.module_classes.push_occurrence(key, occurrence);
    }

    pub(super) fn record_constructor(
        &mut self,
        name: glass_lint_datastructures::NameId,
        occurrence: Occurrence,
    ) {
        self.constructors.push_occurrence(name, occurrence);
    }

    pub(super) fn record_global_constructor(
        &mut self,
        name: impl Into<smol_str::SmolStr>,
        occurrence: Occurrence,
    ) {
        self.global_constructors
            .push_occurrence(name.into(), occurrence);
    }

    pub(super) fn record_rooted_constructor(&mut self, path: NamePath, occurrence: Occurrence) {
        self.rooted_constructors.push_occurrence(path, occurrence);
    }

    pub(super) fn record_module_constructor(
        &mut self,
        key: ModuleExportKey,
        occurrence: Occurrence,
    ) {
        self.module_constructors.push_occurrence(key, occurrence);
    }

    pub(super) fn classes(&self) -> &Occurrences {
        &self.classes
    }

    pub(super) fn module_classes(&self) -> &ModuleOccurrences {
        &self.module_classes
    }

    pub(super) fn constructors(&self) -> &NameOccurrences {
        &self.constructors
    }

    pub(super) fn global_constructors(&self) -> &Occurrences {
        &self.global_constructors
    }

    pub(super) fn rooted_constructors(&self) -> &OccurrenceIndex<NamePath> {
        &self.rooted_constructors
    }

    pub(super) fn module_constructors(&self) -> &ModuleOccurrences {
        &self.module_constructors
    }

    pub(super) fn normalize(&mut self) {
        self.classes.normalize();
        self.module_classes.normalize();
        self.constructors.normalize();
        self.global_constructors.normalize();
        self.rooted_constructors.normalize();
        self.module_constructors.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.classes.is_empty()
            && self.module_classes.is_empty()
            && self.constructors.is_empty()
            && self.global_constructors.is_empty()
            && self.rooted_constructors.is_empty()
            && self.module_constructors.is_empty()
    }
}

#[derive(Debug, Default)]
pub(super) struct LiteralIndexes {
    imports: Occurrences,
    strings: Occurrences,
}

impl LiteralIndexes {
    pub(super) fn record_import(
        &mut self,
        module: impl Into<smol_str::SmolStr>,
        occurrence: Occurrence,
    ) {
        self.imports.push_occurrence(module.into(), occurrence);
    }

    pub(super) fn record_string(
        &mut self,
        value: impl Into<smol_str::SmolStr>,
        occurrence: Occurrence,
    ) {
        self.strings.push_occurrence(value.into(), occurrence);
    }

    pub(super) fn imports(&self) -> &Occurrences {
        &self.imports
    }

    pub(super) fn strings(&self) -> &Occurrences {
        &self.strings
    }

    pub(super) fn normalize(&mut self) {
        self.imports.normalize();
        self.strings.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.imports.is_empty() && self.strings.is_empty()
    }
}
