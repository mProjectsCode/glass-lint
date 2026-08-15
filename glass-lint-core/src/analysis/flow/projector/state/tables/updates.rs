use crate::analysis::model::flow::RequirementIndex;

/// One plan-selected requirement update for a property-write transition.
#[derive(Debug, Clone, Copy)]
pub(in crate::analysis::flow::projector) struct PropertyWriteUpdate {
    pub(in crate::analysis::flow::projector) index: RequirementIndex,
    pub(in crate::analysis::flow::projector) value_matches: bool,
}
