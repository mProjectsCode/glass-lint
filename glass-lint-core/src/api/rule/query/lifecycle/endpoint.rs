use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::query::MemberChain;

/// The identity kind of a lifecycle call endpoint. This is parsed when the
/// query is authored and remains typed through normalization and execution;
/// later phases never infer identity from the endpoint's display spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleCallTarget {
    Global(SmolStr),
    RootedMember(SymbolPath),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LifecycleCallEndpoint {
    chain: MemberChain,
    target: LifecycleCallTarget,
}

impl LifecycleCallEndpoint {
    pub(in crate::api::rule::query::lifecycle) fn new(
        chain: MemberChain,
        target: LifecycleCallTarget,
    ) -> Self {
        Self { chain, target }
    }

    pub(crate) fn target(&self) -> &LifecycleCallTarget {
        &self.target
    }

    pub(crate) fn chain(&self) -> &str {
        self.chain.as_str()
    }
}
