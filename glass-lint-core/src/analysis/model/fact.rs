#![allow(private_interfaces)]

use glass_lint_datastructures::{ByteRange, NameId, NamePath, PathId};
use smol_str::SmolStr;

use crate::analysis::{
    model::{scope::FunctionId, value::ValueId},
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactId(pub u32);

impl FactId {
    #[cfg(test)]
    pub fn from_index(index: usize) -> Option<Self> {
        if index < MAX_FACTS {
            Some(Self(u32::try_from(index).ok()?))
        } else {
            None
        }
    }

    pub fn index(self) -> Option<usize> {
        let idx = self.0 as usize;
        if idx < MAX_FACTS { Some(idx) } else { None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ControlRegionId(pub u32);

#[cfg(test)]
mod control_region_tests {
    use super::*;

    #[test]
    fn control_regions_are_typed_and_orderable() {
        assert!(ControlRegionId(1) < ControlRegionId(2));
        assert_eq!(ControlRegionId::default(), ControlRegionId(0));
    }
}

#[cfg(test)]
mod fact_id_tests {
    use super::*;

    #[test]
    fn fact_id_from_index_rejects_overflow() {
        assert!(FactId::from_index(MAX_FACTS).is_none());
        assert!(FactId::from_index(MAX_FACTS - 1).is_some());
    }

    #[test]
    fn fact_id_index_rejects_overflow() {
        assert!(FactId(u32::MAX).index().is_none());
        assert!(FactId(0).index().is_some());
    }
}

#[cfg(test)]
mod call_arg_info_tests {
    use super::*;

    #[test]
    fn call_arg_info_unknown_creates_default() {
        let info = CallArgInfo::unknown();
        assert_eq!(info.value, ValueId::UNKNOWN);
        assert_eq!(info.base_value, ValueId::UNKNOWN);
        assert_eq!(info.base_path, PathId::EMPTY);
        assert!(!info.spread);
    }
}

#[cfg(test)]
mod parameter_binding_tests {
    use super::*;

    #[test]
    fn parameter_binding_constructs_with_all_fields() {
        let binding = ParameterBinding {
            parameter_index: 2,
            path: PathId::EMPTY,
            value: ValueId::UNKNOWN,
            default: Some(ValueId::UNKNOWN),
            rest: true,
        };
        assert_eq!(binding.parameter_index, 2);
        assert!(binding.rest);
        assert!(binding.default.is_some());
    }

    #[test]
    fn parameter_binding_without_default() {
        let binding = ParameterBinding {
            parameter_index: 0,
            path: PathId::EMPTY,
            value: ValueId::UNKNOWN,
            default: None,
            rest: false,
        };
        assert_eq!(binding.parameter_index, 0);
        assert!(binding.default.is_none());
        assert!(!binding.rest);
    }
}

#[cfg(test)]
mod semantic_fact_tests {
    use super::*;

    #[test]
    fn semantic_fact_new_creates_fact_with_all_fields() {
        let fact = SemanticFact::new(
            FactId(1),
            ByteRange::new(0, 5).unwrap(),
            FunctionId(0),
            FactKind::Reference,
            FactPayload::Reference {
                value: ValueId::UNKNOWN,
                provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
            },
        );
        assert_eq!(fact.id(), FactId(1));
        assert_eq!(fact.kind(), FactKind::Reference);
    }

    #[test]
    fn semantic_fact_round_trips_span() {
        let range = ByteRange::new(10, 20).unwrap();
        let fact = SemanticFact::new(
            FactId(2),
            range,
            FunctionId(1),
            FactKind::Call,
            FactPayload::Call {
                callee: ValueId::UNKNOWN,
                receiver: None,
                result: ValueId::UNKNOWN,
                callee_span: range,
                callee_name: None,
                call_provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
                syntactic_path: None,
                rooted_chain: None,
                module_member: None,
                returned_member: None,
                instance_class: None,
                target_function: None,
                args: Vec::new(),
                unwrap: None,
            },
        );
        assert_eq!(fact.id(), FactId(2));
    }
}

#[cfg(test)]
mod fact_payload_tests {
    use super::*;

    #[test]
    fn fact_payload_import_holds_module_string() {
        let payload = FactPayload::Import {
            module: "fs".into(),
        };
        let FactPayload::Import { module } = &payload else {
            panic!("expected Import");
        };
        assert_eq!(module, "fs");
    }

    #[test]
    fn fact_payload_class_declaration_holds_name_and_role() {
        let payload = FactPayload::Class {
            name: Some(SmolStr::new("MyClass")),
            role: ClassFactRole::Declaration,
            provenance: None,
        };
        let FactPayload::Class { name, role, .. } = &payload else {
            panic!("expected Class");
        };
        assert_eq!(name.as_ref().map(SmolStr::as_str), Some("MyClass"));
        assert_eq!(*role, ClassFactRole::Declaration);
    }

    #[test]
    fn fact_payload_class_instanceof_holds_role() {
        let payload = FactPayload::Class {
            name: None,
            role: ClassFactRole::InstanceofOperand,
            provenance: Some((SmolStr::new("React"), SmolStr::new("Component"))),
        };
        let FactPayload::Class {
            role, provenance, ..
        } = &payload
        else {
            panic!("expected Class");
        };
        assert_eq!(*role, ClassFactRole::InstanceofOperand);
        assert_eq!(
            provenance.as_ref().map(|(m, e)| (m.as_str(), e.as_str())),
            Some(("React", "Component"))
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactKind {
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
pub enum ControlKind {
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
pub enum ClassFactRole {
    Declaration,
    SuperclassOperand,
    InstanceofOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionBoundary {
    Enter,
    Exit,
}

#[derive(Debug, Clone)]
pub struct CallArgInfo {
    pub value: ValueId,
    pub base_value: ValueId,
    pub base_path: PathId,
    pub spread: bool,
    pub provenance: SymbolCallProvenance,
}

impl CallArgInfo {
    pub fn unknown() -> Self {
        Self {
            value: ValueId::UNKNOWN,
            base_value: ValueId::UNKNOWN,
            base_path: PathId::EMPTY,
            spread: false,
            provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
        }
    }
}

pub struct ArgumentView<'a> {
    pub argument: &'a CallArgInfo,
    pub static_string: Option<&'a str>,
    pub object_entries: Option<&'a [(NameId, ValueId)]>,
    pub rooted_chain: Option<&'a NamePath>,
}

impl<'a> ArgumentView<'a> {
    pub fn new(argument: &'a CallArgInfo) -> Self {
        Self {
            argument,
            static_string: None,
            object_entries: None,
            rooted_chain: None,
        }
    }

    pub fn with_static_string(mut self, value: &'a str) -> Self {
        self.static_string = Some(value);
        self
    }

    pub fn with_object_entries(mut self, entries: Option<&'a [(NameId, ValueId)]>) -> Self {
        self.object_entries = entries;
        self
    }

    pub fn with_rooted_chain(mut self, chain: Option<&'a NamePath>) -> Self {
        self.rooted_chain = chain;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterBinding {
    pub parameter_index: usize,
    pub path: PathId,
    pub value: ValueId,
    pub default: Option<ValueId>,
    pub rest: bool,
}

#[derive(Debug, Clone)]
pub struct CallUnwrap {
    pub chain_path: Option<NamePath>,
    pub effective_args: Vec<CallArgInfo>,
}

#[derive(Debug, Clone)]
pub enum FactPayload {
    Reference {
        value: ValueId,
        provenance: SymbolCallProvenance,
    },
    MemberRead {
        syntactic_path: Option<NamePath>,
        rooted_chain: Option<NamePath>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(NamePath, NamePath)>,
    },
    Declaration {
        target: ValueId,
        source: ValueId,
    },
    Assignment {
        target: ValueId,
        source: ValueId,
        receiver: Option<ValueId>,
    },
    PropertyWrite {
        receiver: ValueId,
        property: Option<NameId>,
        rooted_chain: Option<NamePath>,
        value: ValueId,
    },
    Call {
        callee: ValueId,
        receiver: Option<ValueId>,
        result: ValueId,
        callee_span: ByteRange,
        callee_name: Option<NameId>,
        call_provenance: SymbolCallProvenance,
        syntactic_path: Option<NamePath>,
        rooted_chain: Option<NamePath>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(NamePath, NamePath)>,
        instance_class: Option<(SmolStr, SmolStr)>,
        target_function: Option<FunctionId>,
        args: Vec<CallArgInfo>,
        unwrap: Option<Box<CallUnwrap>>,
    },
    Function {
        id: FunctionId,
        boundary: FunctionBoundary,
    },
    Control {
        kind: ControlKind,
        region: ControlRegionId,
        return_value: ValueId,
    },
    Construction {
        callee_span: ByteRange,
        callee_name: Option<NameId>,
        provenance: SymbolCallProvenance,
    },
    Import {
        module: String,
    },
    Class {
        name: Option<SmolStr>,
        role: ClassFactRole,
        provenance: Option<(SmolStr, SmolStr)>,
    },
}

#[derive(Debug, Clone)]
pub struct SemanticFact {
    pub id: FactId,
    pub span: ByteRange,
    pub function: FunctionId,
    #[cfg(test)]
    pub kind: FactKind,
    pub payload: FactPayload,
}

impl SemanticFact {
    pub fn new(
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

    #[cfg(test)]
    pub fn id(&self) -> FactId {
        self.id
    }

    #[cfg(test)]
    pub fn kind(&self) -> FactKind {
        self.kind
    }
}

pub const MAX_FACTS: usize = 1 << 20;

/// Marker type for the mutable building phase of [`FactStream`].
#[derive(Debug)]
pub(in crate::analysis) struct Building;

/// Marker type for the immutable frozen phase of [`FactStream`].
/// Accessors for names and values are only available in this phase.
#[derive(Debug)]
pub(in crate::analysis) struct Frozen;
