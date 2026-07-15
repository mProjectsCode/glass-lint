//! Role-specific mutable state for the canonical fact traversal.

#[derive(Debug, Default)]
pub(super) struct TraversalState {
    next_control_region: u32,
    class_stack: Vec<Option<(String, String)>>,
    function_depth: usize,
    static_method_depth: usize,
}

impl TraversalState {
    pub(super) fn next_control_region(&mut self) -> u32 {
        let region = self.next_control_region;
        self.next_control_region = self.next_control_region.saturating_add(1);
        region
    }

    pub(super) fn enter_class(&mut self, provenance: Option<(String, String)>) {
        self.class_stack.push(provenance);
    }

    pub(super) fn leave_class(&mut self) {
        self.class_stack.pop();
    }

    pub(super) fn current_class(&self) -> Option<(String, String)> {
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
}
