use super::MatchedCapability;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Top-level classification result containing capabilities in catalog order.
pub struct ClassificationResult {
    /// Classified capabilities selected for this run.
    capabilities: Vec<MatchedCapability>,
}

impl ClassificationResult {
    pub(crate) fn push_capability(&mut self, capability: MatchedCapability) {
        self.capabilities.push(capability);
    }

    /// Borrow the classified capabilities without copying them.
    pub fn capabilities(&self) -> &[MatchedCapability] {
        &self.capabilities
    }
}
