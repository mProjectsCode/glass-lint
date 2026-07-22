use smol_str::ToSmolStr;

use crate::{
    analysis::SymbolPath,
    api::{
        classification::MatchKind,
        compiler::rule::{
            EventPredicate, EvidenceDescriptor, IdentityConstraint, IdentityStrength, QueryClause,
            QueryConstraint, SubjectConstraint,
        },
        rule::{
            ClassMatcher, ConstructorMatcher, InstanceMemberCallMatcher, MemberCallMatcher,
            MemberCallProvenance, MemberReadMatcher, ModuleSpecifierPattern,
            ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, SymbolProvenance,
        },
    },
    rules::CallMatcher,
};

fn call_identity(name: &str, provenance: &SymbolProvenance) -> IdentityConstraint {
    match provenance {
        SymbolProvenance::Any => IdentityConstraint::Any {
            name: name.into(),
            strength: IdentityStrength::Heuristic,
        },
        SymbolProvenance::Global => IdentityConstraint::Global {
            name: name.into(),
            strength: IdentityStrength::Strict,
        },
        SymbolProvenance::ModuleExport { module } => IdentityConstraint::ModuleExport {
            module: module.to_smolstr(),
            export: name.into(),
        },
        SymbolProvenance::PackageModuleExport { module } => {
            IdentityConstraint::PackageModuleExport {
                module: module.clone(),
                export: name.into(),
            }
        }
    }
}

fn member_identity(chain: &str, provenance: &MemberCallProvenance) -> IdentityConstraint {
    match provenance {
        MemberCallProvenance::Any => IdentityConstraint::Any {
            name: chain.to_smolstr(),
            strength: IdentityStrength::Heuristic,
        },
        MemberCallProvenance::Rooted => IdentityConstraint::Rooted {
            path: SymbolPath::from(chain),
        },
        MemberCallProvenance::ModuleNamespace { module } => IdentityConstraint::ModuleNamespace {
            module: module.to_smolstr(),
        },
        MemberCallProvenance::PackageModuleNamespace { module } => {
            IdentityConstraint::PackageModuleNamespace {
                module: module.clone(),
            }
        }
    }
}

fn lower_member_clause(
    identity: IdentityConstraint,
    event: EventPredicate,
    constraints: Box<[QueryConstraint]>,
    kind: MatchKind,
    symbol: String,
) -> QueryClause {
    QueryClause {
        identity,
        event,
        subject: SubjectConstraint::Direct,
        constraints,
        evidence: EvidenceDescriptor { kind, symbol },
    }
}

pub(super) fn lower_calls(values: &[CallMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|call| QueryClause {
            identity: call_identity(call.name(), call.provenance()),
            event: EventPredicate::Call,
            subject: SubjectConstraint::Direct,
            constraints: call
                .arguments()
                .iter()
                .cloned()
                .map(QueryConstraint::Argument)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            evidence: EvidenceDescriptor {
                kind: if call.arguments().is_empty() {
                    MatchKind::Call
                } else {
                    MatchKind::CallArgument
                },
                symbol: call.evidence_symbol(),
            },
        })
        .collect()
}

pub(super) fn lower_member_calls(values: &[MemberCallMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|member| {
            let constraints = member
                .arguments()
                .iter()
                .cloned()
                .map(QueryConstraint::Argument)
                .collect::<Vec<_>>()
                .into_boxed_slice();
            lower_member_clause(
                member_identity(member.chain(), member.provenance()),
                EventPredicate::MemberCall {
                    member: SymbolPath::from(member.chain()),
                },
                constraints,
                if member.arguments().is_empty() {
                    MatchKind::MemberCall
                } else {
                    MatchKind::CallArgument
                },
                member.evidence_symbol(),
            )
        })
        .collect()
}

pub(super) fn lower_member_reads(values: &[MemberReadMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|read| {
            lower_member_clause(
                member_identity(read.chain(), read.provenance()),
                EventPredicate::MemberRead {
                    member: SymbolPath::from(read.chain()),
                },
                Box::new([]),
                MatchKind::MemberRead,
                read.evidence_symbol(),
            )
        })
        .collect()
}

fn literal_clause(value: &str, event: EventPredicate, kind: MatchKind) -> QueryClause {
    QueryClause {
        identity: IdentityConstraint::LiteralString {
            predicate: value.to_owned(),
        },
        event,
        subject: SubjectConstraint::Direct,
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind,
            symbol: value.to_owned(),
        },
    }
}

pub(super) fn lower_imports(values: &[String]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|value| literal_clause(value, EventPredicate::Import, MatchKind::Import))
        .collect()
}

pub(super) fn lower_package_imports(values: &[ModuleSpecifierPattern]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|pattern| QueryClause {
            identity: IdentityConstraint::PackageSpecifier {
                pattern: pattern.clone(),
            },
            event: EventPredicate::Import,
            subject: SubjectConstraint::Direct,
            constraints: Box::new([]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::Import,
                symbol: pattern.to_string(),
            },
        })
        .collect()
}

pub(super) fn lower_string_contains(values: &[String]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|value| {
            literal_clause(
                value,
                EventPredicate::StringReference,
                MatchKind::StringContains,
            )
        })
        .collect()
}

pub(super) fn lower_classes(values: &[ClassMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|class| QueryClause {
            identity: call_identity(class.name(), class.provenance()),
            event: EventPredicate::ClassReference,
            subject: SubjectConstraint::Direct,
            constraints: Box::new([]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::Class,
                symbol: class.evidence_symbol(),
            },
        })
        .collect()
}

pub(super) fn lower_constructors(values: &[ConstructorMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|constructor| QueryClause {
            identity: call_identity(constructor.name(), constructor.provenance()),
            event: EventPredicate::Construct,
            subject: SubjectConstraint::Direct,
            constraints: Box::new([]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::Constructor,
                symbol: constructor.evidence_symbol(),
            },
        })
        .collect()
}

#[allow(unused_variables)]
pub(super) fn lower_flows(values: &[crate::api::rule::ObjectFlowMatcher]) -> Vec<QueryClause> {
    Vec::new()
}

fn returned_member_clause(
    source: &str,
    member: &str,
    event: EventPredicate,
    kind: MatchKind,
) -> QueryClause {
    QueryClause {
        identity: IdentityConstraint::Rooted {
            path: SymbolPath::from(source),
        },
        event,
        subject: SubjectConstraint::ReturnedFrom {
            producer: Box::new(IdentityConstraint::Rooted {
                path: SymbolPath::from(source),
            }),
        },
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind,
            symbol: format!("{source}.{member}"),
        },
    }
}

pub(super) fn lower_returned_member_calls(
    values: &[ReturnedMemberCallMatcher],
) -> Vec<QueryClause> {
    values
        .iter()
        .map(|returned| {
            returned_member_clause(
                returned.source(),
                returned.member(),
                EventPredicate::MemberCall {
                    member: returned.member().into(),
                },
                MatchKind::MemberCall,
            )
        })
        .collect()
}

pub(super) fn lower_returned_member_reads(
    values: &[ReturnedMemberReadMatcher],
) -> Vec<QueryClause> {
    values
        .iter()
        .map(|returned| {
            returned_member_clause(
                returned.source(),
                returned.member(),
                EventPredicate::MemberRead {
                    member: returned.member().into(),
                },
                MatchKind::MemberRead,
            )
        })
        .collect()
}

pub(super) fn lower_instance_members(values: &[InstanceMemberCallMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|instance| {
            let constructor = instance.module_pattern().cloned().map_or_else(
                || IdentityConstraint::ModuleExport {
                    module: instance.module().to_smolstr(),
                    export: instance.export().to_smolstr(),
                },
                |module| IdentityConstraint::PackageModuleExport {
                    module,
                    export: instance.export().to_smolstr(),
                },
            );
            QueryClause {
                identity: constructor.clone(),
                event: EventPredicate::MemberCall {
                    member: SymbolPath::from(instance.member()),
                },
                subject: SubjectConstraint::InstanceOf {
                    constructor: Box::new(constructor),
                },
                constraints: Box::new([]),
                evidence: EvidenceDescriptor {
                    kind: MatchKind::MemberCall,
                    symbol: format!(
                        "{}:{}.{}",
                        instance.module(),
                        instance.export(),
                        instance.member()
                    ),
                },
            }
        })
        .collect()
}
