use glass_lint_datastructures::ByteRange;

use super::{OriginCheckpoint, OriginMap, OriginSnapshot};
use crate::analysis::{
    SemanticBudget,
    facts::instance::InstanceCallable,
    model::{fact::ClassIdentity, value::ValueId},
};

/// The four provenance channels: instance origins, class origins, instance
/// callables, and static-string origins. Each lifecycle operation is
/// expressed once here with the intentionally asymmetric per-map semantics.
pub(in crate::analysis::facts) struct OriginChannels {
    instances: OriginMap<ClassIdentity>,
    classes: OriginMap<ClassIdentity>,
    instance_callables: OriginMap<InstanceCallable>,
    static_string_origins: OriginMap<ByteRange>,
}

pub(in crate::analysis::facts) struct ProvenanceCheckpoint {
    instance: OriginCheckpoint,
    class: OriginCheckpoint,
    callable: OriginCheckpoint,
    static_string: OriginCheckpoint,
}

pub(in crate::analysis::facts) struct BranchProvenance {
    instances: InstanceProvenanceSnapshot,
    classes: OriginSnapshot<ClassIdentity>,
}

pub(in crate::analysis::facts) struct InstanceProvenanceSnapshot {
    origins: OriginSnapshot<ClassIdentity>,
    callables: OriginSnapshot<InstanceCallable>,
    static_strings: OriginSnapshot<ByteRange>,
}

#[derive(Default)]
pub(in crate::analysis::facts) struct TargetProvenance {
    pub(in crate::analysis::facts) callable: Option<InstanceCallable>,
    pub(in crate::analysis::facts) instance_origin: Option<ClassIdentity>,
    pub(in crate::analysis::facts) class_origin: Option<ClassIdentity>,
    pub(in crate::analysis::facts) static_string_origin: Option<ByteRange>,
}

impl OriginChannels {
    pub(in crate::analysis::facts) fn new() -> Self {
        Self {
            instances: OriginMap::new(),
            classes: OriginMap::new(),
            instance_callables: OriginMap::new(),
            static_string_origins: OriginMap::new(),
        }
    }

    pub(in crate::analysis::facts) fn checkpoint(&mut self) -> ProvenanceCheckpoint {
        ProvenanceCheckpoint {
            instance: self.instances.checkpoint(),
            class: self.classes.checkpoint(),
            callable: self.instance_callables.checkpoint(),
            static_string: self.static_string_origins.checkpoint(),
        }
    }

    pub(in crate::analysis::facts) fn restore_branch_entry(
        &mut self,
        checkpoint: &ProvenanceCheckpoint,
    ) {
        self.instances.restore(&checkpoint.instance);
        self.classes.restore(&checkpoint.class);
        self.instance_callables.restore(&checkpoint.callable);
        self.static_string_origins
            .restore(&checkpoint.static_string);
    }

    pub(in crate::analysis::facts) fn restore_instance_alternative(
        &mut self,
        checkpoint: &ProvenanceCheckpoint,
    ) {
        self.instances.restore(&checkpoint.instance);
        self.instance_callables.restore(&checkpoint.callable);
        self.static_string_origins
            .restore(&checkpoint.static_string);
    }

    /// Complete a control region whose instance origins can flow out of one
    /// modeled alternative, but whose class origins cannot.
    pub(in crate::analysis::facts) fn finish_control_region(
        &mut self,
        checkpoint: &mut ProvenanceCheckpoint,
    ) {
        self.restore_instance_alternative(checkpoint);
        self.instances.commit(&mut checkpoint.instance);
        self.classes.rollback(&mut checkpoint.class);
        self.instance_callables.commit(&mut checkpoint.callable);
        self.static_string_origins
            .commit(&mut checkpoint.static_string);
    }

    pub(in crate::analysis::facts) fn snapshot_instances(
        &self,
        budget: &SemanticBudget,
    ) -> InstanceProvenanceSnapshot {
        InstanceProvenanceSnapshot {
            origins: self.instances.snapshot(budget),
            callables: self.instance_callables.snapshot(budget),
            static_strings: self.static_string_origins.snapshot(budget),
        }
    }

    pub(in crate::analysis::facts) fn snapshot_classes(
        &self,
        budget: &SemanticBudget,
    ) -> OriginSnapshot<ClassIdentity> {
        self.classes.snapshot(budget)
    }

    pub(in crate::analysis::facts) fn branch_provenance(
        &self,
        budget: &SemanticBudget,
    ) -> BranchProvenance {
        BranchProvenance {
            instances: self.snapshot_instances(budget),
            classes: self.snapshot_classes(budget),
        }
    }

    pub(in crate::analysis::facts) fn restore_instance_snapshot(
        &mut self,
        snapshot: InstanceProvenanceSnapshot,
        checkpoint: &mut ProvenanceCheckpoint,
    ) {
        self.instances
            .restore_snapshot(snapshot.origins, &mut checkpoint.instance);
        self.instance_callables
            .restore_snapshot(snapshot.callables, &mut checkpoint.callable);
        self.static_string_origins
            .restore_snapshot(snapshot.static_strings, &mut checkpoint.static_string);
    }

    pub(in crate::analysis::facts) fn retain_common_instance(
        &mut self,
        snapshot: &InstanceProvenanceSnapshot,
        budget: &SemanticBudget,
    ) {
        self.instances.retain_common(&snapshot.origins, budget);
        self.instance_callables
            .retain_common(&snapshot.callables, budget);
        self.static_string_origins
            .retain_common(&snapshot.static_strings, budget);
    }

    pub(in crate::analysis::facts) fn finish_branch_with_else(
        &mut self,
        checkpoint: &mut ProvenanceCheckpoint,
        then: &BranchProvenance,
        budget: &SemanticBudget,
    ) {
        self.instances
            .retain_common(&then.instances.origins, budget);
        self.classes.retain_common(&then.classes, budget);
        self.instance_callables
            .retain_common(&then.instances.callables, budget);
        self.static_string_origins
            .retain_common(&then.instances.static_strings, budget);
        self.instances.commit(&mut checkpoint.instance);
        self.classes.commit(&mut checkpoint.class);
        self.instance_callables.commit(&mut checkpoint.callable);
        self.static_string_origins
            .commit(&mut checkpoint.static_string);
    }

    pub(in crate::analysis::facts) fn finish_branch_without_else(
        &mut self,
        checkpoint: &mut ProvenanceCheckpoint,
    ) {
        self.instances.rollback(&mut checkpoint.instance);
        self.classes.rollback(&mut checkpoint.class);
        self.instance_callables.rollback(&mut checkpoint.callable);
        self.static_string_origins
            .rollback(&mut checkpoint.static_string);
    }

    pub(in crate::analysis::facts) fn replace_target(
        &mut self,
        target: ValueId,
        replacement: &TargetProvenance,
        budget: &SemanticBudget,
    ) {
        self.instances.remove(target, budget);
        self.classes.remove(target, budget);
        self.instance_callables.remove(target, budget);
        self.static_string_origins.remove(target, budget);
        if let Some(origin) = &replacement.instance_origin {
            self.instances.insert(target, origin.clone(), budget);
        }
        if let Some(origin) = &replacement.class_origin {
            self.classes.insert(target, origin.clone(), budget);
        }
        if let Some(callable) = &replacement.callable {
            self.instance_callables
                .insert(target, callable.clone(), budget);
        }
        if let Some(origin) = replacement.static_string_origin {
            self.static_string_origins.insert(target, origin, budget);
        }
    }

    pub(in crate::analysis::facts) fn replace_targets(
        &mut self,
        targets: &[ValueId],
        replacement: &TargetProvenance,
        budget: &SemanticBudget,
    ) {
        for &target in targets {
            self.replace_target(target, replacement, budget);
        }
    }

    pub(in crate::analysis::facts) fn instance_origin(
        &self,
        value: ValueId,
    ) -> Option<ClassIdentity> {
        self.instances.get(value).cloned()
    }

    pub(in crate::analysis::facts) fn record_instance_origin(
        &mut self,
        value: ValueId,
        origin: ClassIdentity,
        budget: &SemanticBudget,
    ) {
        self.instances.insert(value, origin, budget);
    }

    pub(in crate::analysis::facts) fn record_class_origin(
        &mut self,
        value: ValueId,
        origin: ClassIdentity,
        budget: &SemanticBudget,
    ) {
        self.classes.insert(value, origin, budget);
    }

    pub(in crate::analysis::facts) fn class_origin(&self, value: ValueId) -> Option<ClassIdentity> {
        self.classes.get(value).cloned()
    }

    pub(in crate::analysis::facts) fn instance_callable(
        &self,
        value: ValueId,
    ) -> Option<InstanceCallable> {
        self.instance_callables.get(value).cloned()
    }

    pub(in crate::analysis::facts) fn static_string_origin(
        &self,
        value: ValueId,
    ) -> Option<ByteRange> {
        self.static_string_origins.get(value).copied()
    }

    pub(in crate::analysis::facts) fn record_static_string_origin(
        &mut self,
        value: ValueId,
        origin: ByteRange,
        budget: &SemanticBudget,
    ) {
        self.static_string_origins.insert(value, origin, budget);
    }
}
