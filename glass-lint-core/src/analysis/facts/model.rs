//! Rule-independent semantic fact identities and payloads.

use swc_common::Span;

use super::super::{
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
    value::{FunctionId, PathId, ValueId},
};

// ── Fact stream types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct FactId(pub(in crate::analysis) u32);

impl FactId {
    pub(in crate::analysis) fn from_index(index: usize) -> Option<Self> {
        (index < MAX_FACTS)
            .then(|| u32::try_from(index).ok().map(Self))
            .flatten()
    }

    pub(in crate::analysis) fn index(self) -> Option<usize> {
        usize::try_from(self.0)
            .ok()
            .filter(|index| *index < MAX_FACTS)
    }
}

/// Semantic categories for facts stored in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) enum FactKind {
    Declaration,
    Assignment,
    PropertyWrite,
    Call,
    Construction,
    Reference,
    MemberRead,
    Function,
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum ControlKind {
    BranchStart,
    BranchThen,
    BranchElse,
    BranchEnd,
    LoopStart { guaranteed: bool },
    LoopUpdate,
    LoopEnd,
    SwitchStart,
    SwitchCase { is_default: bool },
    SwitchEnd,
    TryStart,
    CatchStart,
    FinallyStart,
    TryEnd,
    Break,
    Continue,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum FunctionBoundary {
    Enter,
    Exit,
}

/// Pre-computed evaluation of a single argument at a call site.  Stored in
/// the `Call` fact so argument predicates never need to reach back to the AST.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallArgInfo {
    pub(in crate::analysis) value: ValueId,
    pub(in crate::analysis) base_value: ValueId,
    pub(in crate::analysis) base_path: PathId,
    pub(in crate::analysis) static_string: Option<String>,
    pub(in crate::analysis) object_keys: Option<Vec<String>>,
    pub(in crate::analysis) rooted_chain: Option<String>,
    /// Values reachable from this argument through a statically known object
    /// or array shape. The root is included with an empty path.
    pub(in crate::analysis) projections: Vec<ValueProjection>,
    /// A spread argument is intentionally not projected: its arity and
    /// element identities are not known to the summary pass.
    pub(in crate::analysis) spread: bool,
    pub(in crate::analysis) provenance: SymbolCallProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct ValueProjection {
    pub(in crate::analysis) path: PathId,
    pub(in crate::analysis) value: ValueId,
}

/// One binding introduced by a function parameter pattern. `path` identifies
/// the value inside the corresponding top-level argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct ParameterBinding {
    pub(in crate::analysis) parameter_index: usize,
    pub(in crate::analysis) path: PathId,
    pub(in crate::analysis) value: ValueId,
    pub(in crate::analysis) default: Option<ValueId>,
    pub(in crate::analysis) rest: bool,
}

/// Information about a `.call()`/`.apply()` unwrapping at a call site.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallUnwrap {
    /// The chain spelling of the target being called (e.g. `"fetch"` or
    /// `"mod.fn"`).
    pub(in crate::analysis) chain: String,
    /// Effective arguments after removing the receiver and options/array
    /// wrapper.
    pub(in crate::analysis) effective_args: Vec<CallArgInfo>,
}

/// Compact, typed payloads carried by facts.  Must not contain borrowed AST
/// nodes, formatted identity strings used as matcher/rule indexes, or
/// matcher-specific state.  All provenance is resolved once at build time.
#[derive(Debug, Clone)]
pub(in crate::analysis) enum FactPayload {
    /// Identifier or literal reference. A static string is a projection of
    /// this value/reference fact, not a parallel `StringLiteral` fact kind;
    /// string matchers consume the projection through the occurrence index.
    Reference {
        // Kept even before a matcher consumes it: reference identity is the
        // canonical input for future value-use and connected-flow matchers.
        #[allow(dead_code)]
        value: ValueId,
        static_string: Option<String>,
        provenance: SymbolCallProvenance,
    },
    /// Member expression read.
    MemberRead {
        // Kept so a future read/value-flow matcher can connect this event to
        // declarations and assignments without another AST traversal.
        #[allow(dead_code)]
        value: ValueId,
        syntactic_chain: Option<String>,
        rooted_chain: Option<String>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(String, String)>,
    },
    /// Variable declaration.
    Declaration { target: ValueId, source: ValueId },
    /// Assignment expression.
    Assignment {
        target: ValueId,
        source: ValueId,
        receiver: Option<ValueId>,
    },
    /// Property write (obj.prop = value).
    PropertyWrite {
        target: ValueId,
        receiver: ValueId,
        // Static-value matching uses `static_value` today, while `source`
        // preserves the RHS identity needed by future property-flow matchers.
        #[allow(dead_code)]
        source: ValueId,
        property: Option<String>,
        static_value: Option<String>,
    },
    /// Function or method call.
    Call {
        // Provenance matchers do not consume the identity yet, but callable
        // value flow needs the callee linked to its declaration or alias.
        #[allow(dead_code)]
        callee: ValueId,
        receiver: Option<ValueId>,
        result: ValueId,
        callee_span: Span,
        callee_name: Option<String>,
        call_provenance: SymbolCallProvenance,
        syntactic_chain: Option<String>,
        rooted_chain: Option<String>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(String, String)>,
        instance_class: Option<(String, String)>,
        target_function: Option<FunctionId>,
        /// Pre-computed argument evaluation for predicates.
        args: Vec<CallArgInfo>,
        /// Present when this is a `.call()`/`.apply()` wrapper; the effective
        /// target and arguments after unwrapping.
        unwrap: Option<Box<CallUnwrap>>,
    },
    /// A function declaration/expression and its parameter value identities.
    Function {
        id: FunctionId,
        owner: FunctionId,
        parameters: Vec<ParameterBinding>,
        boundary: FunctionBoundary,
    },
    Control {
        kind: ControlKind,
        region: u32,
        value: ValueId,
    },
    /// `new Constructor()`.
    Construction {
        // Constructor identity and the allocated value are retained together
        // so construction can become a connected-flow source without
        // changing matcher-independent fact construction.
        #[allow(dead_code)]
        callee: ValueId,
        #[allow(dead_code)]
        result: ValueId,
        callee_span: Span,
        callee_name: Option<String>,
        provenance: SymbolCallProvenance,
    },
    /// Import declaration.
    Import { module: String },
    /// Class declaration or expression, or `instanceof` operand.
    Class {
        name: String,
        provenance: Option<(String, String)>,
    },
}

/// A single, immutable semantic fact in the canonical stream.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct SemanticFact {
    pub(in crate::analysis) id: FactId,
    pub(in crate::analysis) span: Span,
    pub(in crate::analysis) function: FunctionId,
    #[cfg(test)]
    pub(in crate::analysis) kind: FactKind,
    pub(in crate::analysis) payload: FactPayload,
}

impl SemanticFact {
    pub(in crate::analysis) fn new(
        id: FactId,
        span: Span,
        function: FunctionId,
        kind: FactKind,
        payload: FactPayload,
    ) -> Self {
        #[cfg(not(test))]
        let _ = kind;
        Self {
            id,
            span,
            function,
            #[cfg(test)]
            kind,
            payload,
        }
    }

    pub(in crate::analysis) fn id(&self) -> FactId {
        self.id
    }

    #[cfg(test)]
    pub(in crate::analysis) fn kind(&self) -> FactKind {
        self.kind
    }
}

pub(in crate::analysis) const MAX_FACTS: usize = 1 << 20;
