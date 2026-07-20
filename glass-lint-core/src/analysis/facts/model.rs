//! Rule-independent semantic fact identities and payloads.

use smol_str::SmolStr;

use super::super::{
    name::NameId,
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
    value::{FunctionId, PathId, SymbolPath, ValueId},
};
use crate::ByteRange;

// ── Fact stream types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Bounded position in the canonical fact stream.
///
/// IDs are dense from zero while the stream is valid; values outside
/// `MAX_FACTS` cannot be converted into an index and therefore fail closed.
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

/// Identity of one control-flow construct within a local artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(in crate::analysis) struct ControlRegionId(pub(in crate::analysis) u32);

#[cfg(test)]
mod control_region_tests {
    use super::ControlRegionId;

    #[test]
    fn control_regions_are_typed_and_orderable() {
        assert!(ControlRegionId(1) < ControlRegionId(2));
        assert_eq!(ControlRegionId::default(), ControlRegionId(0));
    }
}

/// Semantic categories for facts stored in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) enum FactKind {
    /// A binding, import, class, or other declaration was introduced.
    Declaration,
    /// A value binding was overwritten or invalidated.
    Assignment,
    /// A property on an object identity was overwritten.
    PropertyWrite,
    /// A function-like value was invoked.
    Call,
    /// A constructor-like value was invoked with `new`.
    Construction,
    /// An identifier or literal was evaluated as a reference.
    Reference,
    /// A member expression was evaluated as a read.
    MemberRead,
    /// A function boundary and its parameter bindings.
    Function,
    /// A branch, loop, switch, exception, or completion boundary.
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum ControlKind {
    /// Start of a conditional branch region.
    BranchStart,
    /// Entry into the then arm.
    BranchThen,
    /// Entry into the else arm.
    BranchElse,
    /// End of a conditional branch region.
    BranchEnd,
    /// Start of a loop; `guaranteed` indicates whether its body runs once.
    LoopStart { guaranteed: bool },
    /// Point at which a loop update is evaluated.
    LoopUpdate,
    /// End of a loop region.
    LoopEnd,
    /// Start of a switch region.
    SwitchStart,
    /// Entry into one switch case; records whether it is the default case.
    SwitchCase { is_default: bool },
    /// End of a switch region.
    SwitchEnd,
    /// Start of a try region.
    TryStart,
    /// Entry into a catch handler.
    CatchStart,
    /// Entry into a finally block.
    FinallyStart,
    /// End of a try/catch/finally region.
    TryEnd,
    /// An abrupt break completion.
    Break,
    /// An abrupt continue completion.
    Continue,
    /// A return completion from the current function.
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Semantic role of a class fact.
pub(in crate::analysis) enum ClassFactRole {
    /// Class declaration or named class expression.
    Declaration,
    /// Right-hand operand of an `instanceof` expression.
    InstanceofOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum FunctionBoundary {
    /// Entry marker emitted before the function body.
    Enter,
    /// Exit marker emitted after the function body.
    Exit,
}

/// Pre-computed evaluation of a single argument at a call site.  Stored in
/// the `Call` fact so argument predicates never need to reach back to the AST.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallArgInfo {
    /// Value identity of the complete argument expression.
    pub(in crate::analysis) value: ValueId,
    /// Root identity used when the argument is a member projection.
    pub(in crate::analysis) base_value: ValueId,
    /// Static path from `base_value` to the argument, if proven.
    pub(in crate::analysis) base_path: PathId,
    /// Statically evaluated string value, when available.
    pub(in crate::analysis) static_string: Option<String>,
    /// Statically known keys of a finite object argument.
    pub(in crate::analysis) object_keys: Option<Vec<super::super::name::NameId>>,
    /// Statically known direct object-property string values.
    pub(in crate::analysis) property_strings: Vec<(super::super::name::NameId, String)>,
    /// Proven rooted member chain for this argument.
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    /// Values reachable from this argument through a statically known object
    /// or array shape. The root is included with an empty path.
    pub(in crate::analysis) projections: Vec<ValueProjection>,
    /// A spread argument is intentionally not projected: its arity and
    /// element identities are not known to the summary pass.
    pub(in crate::analysis) spread: bool,
    /// Provenance of the argument's callable or rooted identity.
    pub(in crate::analysis) provenance: SymbolCallProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One statically addressable value reachable from an argument or parameter.
pub(in crate::analysis) struct ValueProjection {
    /// Interned property/index path from the argument root.
    pub(in crate::analysis) path: PathId,
    /// Value identity at that path.
    pub(in crate::analysis) value: ValueId,
}

/// One binding introduced by a function parameter pattern. `path` identifies
/// the value inside the corresponding top-level argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct ParameterBinding {
    /// Zero-based top-level argument position.
    pub(in crate::analysis) parameter_index: usize,
    /// Path within that argument selected by destructuring.
    pub(in crate::analysis) path: PathId,
    /// Identity assigned to the bound parameter.
    pub(in crate::analysis) value: ValueId,
    /// Default expression identity, if the pattern supplies one.
    pub(in crate::analysis) default: Option<ValueId>,
    /// Whether the binding consumes a rest portion of the argument.
    pub(in crate::analysis) rest: bool,
}

/// Information about a `.call()`/`.apply()` unwrapping at a call site.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallUnwrap {
    /// The chain spelling of the target being called (e.g. `"fetch"` or
    /// `"mod.fn"`).
    pub(in crate::analysis) chain: SymbolPath,
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
        value: ValueId,
        /// Constant string projection, if this reference is statically known.
        static_string: Option<String>,
        /// Resolver-backed provenance of the referenced value.
        provenance: SymbolCallProvenance,
    },
    /// Member expression read.
    MemberRead {
        /// Original member spelling when statically recoverable.
        syntactic_chain: Option<SymbolPath>,
        /// Proven rooted chain used by strict member matchers.
        rooted_chain: Option<SymbolPath>,
        /// Proven module namespace member, if applicable.
        module_member: Option<SymbolMemberProvenance>,
        /// Member returned by a previously resolved call, if applicable.
        returned_member: Option<(SymbolPath, SymbolPath)>,
    },
    /// Variable declaration.
    Declaration {
        /// Identity introduced by the declaration pattern.
        target: ValueId,
        /// Identity or constant value assigned by its initializer.
        source: ValueId,
    },
    /// Assignment expression.
    Assignment {
        /// Identity invalidated or rebound by the assignment.
        target: ValueId,
        /// New identity, or unknown for compound/destructuring writes.
        source: ValueId,
        /// Receiver identity for a member write.
        receiver: Option<ValueId>,
    },
    /// Property write (obj.prop = value).
    PropertyWrite {
        /// Resolved identity for the written member.
        target: ValueId,
        /// Object identity receiving the property write.
        receiver: ValueId,
        /// Statically known property name, if the key is not dynamic.
        property: Option<NameId>,
        /// Statically evaluated string assigned to the property.
        static_value: Option<String>,
    },
    /// Function or method call.
    Call {
        callee: ValueId,
        /// Receiver identity for member calls.
        receiver: Option<ValueId>,
        /// Value identity allocated for this call's result.
        result: ValueId,
        /// Byte range of the callee expression, distinct from the full call
        /// range.
        callee_span: ByteRange,
        /// Direct callee name when syntax supplies one.
        callee_name: Option<NameId>,
        /// Resolver-backed callable provenance.
        call_provenance: SymbolCallProvenance,
        /// Member chain as written at the call site.
        syntactic_chain: Option<SymbolPath>,
        /// Proven rooted member chain, if available.
        rooted_chain: Option<SymbolPath>,
        /// Proven module member target, if available.
        module_member: Option<SymbolMemberProvenance>,
        /// Proven member returned by an earlier call, if available.
        returned_member: Option<(SymbolPath, SymbolPath)>,
        /// Proven superclass identity for an instance method call.
        instance_class: Option<(SmolStr, SmolStr)>,
        /// Lexical function identity when the callee resolves to one.
        target_function: Option<FunctionId>,
        /// Pre-computed argument evaluation for predicates.
        args: Vec<CallArgInfo>,
        /// Present when this is a `.call()`/`.apply()` wrapper; the effective
        /// target and arguments after unwrapping.
        unwrap: Option<Box<CallUnwrap>>,
    },
    /// A function declaration/expression and its parameter value identities.
    Function {
        /// Function identity of the body being entered or exited.
        id: FunctionId,
        /// Path-aware bindings extracted from the parameters.
        parameters: Vec<ParameterBinding>,
        /// Whether this fact marks entry or exit.
        boundary: FunctionBoundary,
    },
    /// Control-flow boundary used by bounded state projection.
    Control {
        /// Kind of boundary and completion event.
        kind: ControlKind,
        /// Region identity shared by all markers for one construct.
        region: ControlRegionId,
        /// Returned value identity for `Return`; unknown for branch and loop
        /// boundary markers.
        return_value: ValueId,
    },
    /// `new Constructor()`.
    Construction {
        /// Byte range of the constructor expression.
        callee_span: ByteRange,
        /// Constructor name when strict provenance permits one.
        callee_name: Option<NameId>,
        /// Resolver-backed constructor provenance.
        provenance: SymbolCallProvenance,
    },
    /// Import declaration.
    Import {
        /// Literal module specifier recorded for project resolution.
        module: String,
    },
    /// Class declaration or expression, or `instanceof` operand.
    Class {
        /// Authored class name for declaration facts; absent for an
        /// `instanceof` operand.
        name: Option<SmolStr>,
        /// Role represented by this class fact.
        role: ClassFactRole,
        /// Proven superclass/module identity, if available.
        provenance: Option<(SmolStr, SmolStr)>,
    },
}

/// A single, immutable semantic fact in the canonical stream.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct SemanticFact {
    /// Dense identity used by indexes and evidence ordering.
    pub(in crate::analysis) id: FactId,
    /// Original source byte range for the semantic event.
    pub(in crate::analysis) span: ByteRange,
    /// Lexical function owning the event.
    pub(in crate::analysis) function: FunctionId,
    #[cfg(test)]
    pub(in crate::analysis) kind: FactKind,
    /// Matcher-independent payload for this semantic role.
    pub(in crate::analysis) payload: FactPayload,
}

impl SemanticFact {
    pub(in crate::analysis) fn new(
        id: FactId,
        span: ByteRange,
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

/// Maximum number of facts retained for one source file.
pub(in crate::analysis) const MAX_FACTS: usize = 1 << 20;
