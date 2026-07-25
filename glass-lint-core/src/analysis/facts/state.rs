//! Role-specific mutable state for the canonical fact traversal.
//!
//! This state is deliberately not part of the fact stream. It tracks only
//! visitor nesting and monotonic control-region allocation, and is restored by
//! balanced enter/leave calls as the AST walk returns from a construct.

use smol_str::SmolStr;

use crate::analysis::{facts::ControlRegionId, scope::ScopeId, value::FunctionId};

#[derive(Debug)]
/// Ephemeral nesting state that affects how the current syntax is interpreted.
pub(super) struct TraversalState {
    /// Monotonic identity source for branch and loop regions.
    next_control_region: ControlRegionId,
    /// Class-superclass provenance for the current nesting stack.
    class_stack: Vec<Option<(SmolStr, SmolStr)>>,
    /// Number of function bodies currently being visited.
    function_depth: usize,
    /// Number of static class methods currently being visited.
    static_method_depth: usize,
    /// Current scope cached during traversal (avoids span-based binary search).
    #[allow(dead_code)]
    current_scope: ScopeId,
    /// Current function cached during traversal (avoids while-loop scope
    /// climb).
    current_function: FunctionId,
}

impl Default for TraversalState {
    fn default() -> Self {
        Self {
            next_control_region: ControlRegionId::default(),
            class_stack: Vec::new(),
            function_depth: 0,
            static_method_depth: 0,
            current_scope: ScopeId::from(0),
            current_function: FunctionId(0),
        }
    }
}

impl TraversalState {
    /// Allocate a monotonic region ID; saturation keeps malformedly large
    /// inputs deterministic instead of wrapping into an earlier region.
    pub(super) fn next_control_region(&mut self) -> ControlRegionId {
        let region = self.next_control_region;
        self.next_control_region = ControlRegionId(region.0.saturating_add(1));
        region
    }

    pub(super) fn enter_class(&mut self, provenance: Option<(SmolStr, SmolStr)>) {
        self.class_stack.push(provenance);
    }

    pub(super) fn leave_class(&mut self) {
        self.class_stack.pop();
    }

    pub(super) fn current_class(&self) -> Option<(SmolStr, SmolStr)> {
        self.class_stack.last().cloned().flatten()
    }

    pub(super) fn enter_function(&mut self) {
        self.function_depth = self.function_depth.saturating_add(1);
    }

    pub(super) fn leave_function(&mut self) {
        self.function_depth = self.function_depth.saturating_sub(1);
    }

    pub(super) fn enter_static_method(&mut self) {
        self.static_method_depth = self.static_method_depth.saturating_add(1);
    }

    pub(super) fn leave_static_method(&mut self) {
        self.static_method_depth = self.static_method_depth.saturating_sub(1);
    }

    pub(super) fn in_function(&self) -> bool {
        self.function_depth > 0
    }

    pub(super) fn in_static_method(&self) -> bool {
        self.static_method_depth > 0
    }

    pub(super) fn current_function(&self) -> FunctionId {
        self.current_function
    }

    pub(super) fn set_function(&mut self, function: FunctionId) {
        self.current_function = function;
    }
}
