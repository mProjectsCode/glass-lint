use super::{ControlKind, FactBuilder, FactKind, FactPayload, Span};

impl FactBuilder<'_> {
    pub(super) fn next_control_region(&mut self) -> u32 {
        self.traversal.next_control_region()
    }

    pub(super) fn emit_control(&mut self, span: Span, kind: ControlKind, region: u32) {
        self.emit(
            FactKind::Control,
            span,
            FactPayload::Control {
                kind,
                region,
                value: super::ValueId::UNKNOWN,
            },
        );
    }
}
