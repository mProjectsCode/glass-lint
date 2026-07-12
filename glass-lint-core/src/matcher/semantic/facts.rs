//! Authoritative per-file semantic fact construction.
//!
//! The individual fact collectors are implementation details of this build.
//! Matchers receive only the immutable SemanticFacts result, so adding a
//! matcher cannot introduce another semantic path at the model boundary.

use std::collections::BTreeMap;

use swc_common::{BytePos, Span};
use swc_ecma_ast::Program;

use super::super::result::ApiEvidence;
use super::super::rule::ApiMatcher;
use super::ast::{SymbolCallProvenance, SymbolMemberProvenance};
use super::fact_builder::FactBuilder;
use super::index::MatcherFacts;
use super::object_flow;
use super::resolver::Resolver;
use super::value::{FunctionId, ValueId};

// ── Fact stream types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub(super) struct FactId(pub(super) u32);

/// Semantic categories for facts stored in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub(super) enum FactKind {
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
pub(super) enum ControlKind {
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
pub(super) enum FunctionBoundary {
    Enter,
    Exit,
}

/// Pre-computed evaluation of a single argument at a call site.  Stored in
/// the `Call` fact so argument predicates never need to reach back to the AST.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct CallArgInfo {
    pub(super) value: ValueId,
    pub(super) base_value: ValueId,
    pub(super) base_path: Vec<ProjectionSegment>,
    pub(super) static_string: Option<String>,
    pub(super) object_keys: Option<Vec<String>>,
    pub(super) rooted_chain: Option<String>,
    /// Values reachable from this argument through a statically known object
    /// or array shape. The root is included with an empty path.
    pub(super) projections: Vec<ValueProjection>,
    /// A spread argument is intentionally not projected: its arity and
    /// element identities are not known to the summary pass.
    pub(super) spread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ProjectionSegment {
    Property(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ValueProjection {
    pub(super) path: Vec<ProjectionSegment>,
    pub(super) value: ValueId,
}

/// One binding introduced by a function parameter pattern. `path` identifies
/// the value inside the corresponding top-level argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParameterBinding {
    pub(super) parameter_index: usize,
    pub(super) path: Vec<ProjectionSegment>,
    pub(super) value: ValueId,
    pub(super) default: Option<ValueId>,
    pub(super) rest: bool,
}

/// Information about a `.call()`/`.apply()` unwrapping at a call site.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct CallUnwrap {
    /// The chain spelling of the target being called (e.g. `"fetch"` or `"mod.fn"`).
    pub(super) chain: String,
    /// Receiver expression for `.call(receiver, ...)` / `.apply(receiver, ...)`.
    pub(super) receiver: Option<String>,
    /// Effective arguments after removing the receiver and options/array wrapper.
    pub(super) effective_args: Vec<CallArgInfo>,
}

/// Compact, typed payloads carried by facts.  Must not contain borrowed AST
/// nodes, formatted identity strings used as matcher/rule indexes, or
/// matcher-specific state.  All provenance is resolved once at build time.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) enum FactPayload {
    /// Identifier reference.
    Reference {
        value: ValueId,
    },
    /// Member expression read.
    MemberRead {
        value: ValueId,
        syntactic_chain: Option<String>,
        rooted_chain: Option<String>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(String, String)>,
    },
    /// Variable declaration.
    Declaration {
        target: ValueId,
        source: ValueId,
    },
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
        source: ValueId,
        property: Option<String>,
        static_value: Option<String>,
    },
    /// Function or method call.
    Call {
        callee: ValueId,
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
        name: Option<String>,
        parameters: Vec<ParameterBinding>,
        boundary: FunctionBoundary,
    },
    Control {
        kind: ControlKind,
        region: u32,
    },
    /// `new Constructor()`.
    Construction {
        callee: ValueId,
        result: ValueId,
        callee_span: Span,
        callee_name: Option<String>,
        provenance: SymbolCallProvenance,
    },
    /// Import declaration.
    Import {
        module: String,
    },
    /// String literal or template quasi.
    StringLiteral {
        value: String,
    },
    /// Class declaration or expression, or `instanceof` operand.
    Class {
        name: String,
        provenance: Option<(String, String)>,
    },
}

/// A single, immutable semantic fact in the canonical stream.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct SemanticFact {
    pub(super) id: FactId,
    pub(super) span: Span,
    pub(super) scope: usize,
    pub(super) function: FunctionId,
    pub(super) kind: FactKind,
    pub(super) payload: FactPayload,
}

/// Key for exact event lookup: `(lo, hi, kind, ordinal)` identifies
/// individual facts at a given source position and semantic role.
/// The ordinal distinguishes canonical same-span facts in deterministic
/// insertion order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub(super) struct ExactEventKey {
    pub(super) lo: BytePos,
    pub(super) hi: BytePos,
    pub(super) kind: FactKind,
    pub(super) ordinal: u32,
}

pub(super) const MAX_FACTS: usize = 1 << 20;

/// The canonical, rule-independent fact stream.  Construction happens once
/// per file; all downstream consumers query this stream rather than the AST.
#[derive(Debug)]
#[allow(dead_code)]
pub(super) struct FactStream {
    facts: Vec<SemanticFact>,
    exact: BTreeMap<ExactEventKey, Vec<FactId>>,
    exact_by_span_kind: BTreeMap<(BytePos, BytePos, FactKind), Vec<FactId>>,
    span_order: BTreeMap<(BytePos, BytePos), FactId>,
    /// Per `(lo, hi, kind)` ordinal counter for deterministic same-span
    /// ordering.  Fails closed on overflow rather than wrapping.
    ordinal_counters: BTreeMap<(BytePos, BytePos, FactKind), u32>,
    valid: bool,
}

impl FactStream {
    pub(super) fn new() -> Self {
        Self {
            facts: Vec::new(),
            exact: BTreeMap::new(),
            exact_by_span_kind: BTreeMap::new(),
            span_order: BTreeMap::new(),
            ordinal_counters: BTreeMap::new(),
            valid: true,
        }
    }

    pub(super) fn push(&mut self, fact: SemanticFact) {
        if !self.valid || self.facts.len() >= MAX_FACTS {
            self.valid = false;
            return;
        }
        let counter_key = (fact.span.lo(), fact.span.hi(), fact.kind);
        let ordinal = self
            .ordinal_counters
            .entry(counter_key)
            .and_modify(|o| {
                if let Some(next) = o.checked_add(1) {
                    *o = next;
                } else {
                    self.valid = false;
                }
            })
            .or_insert(0);
        if !self.valid {
            return;
        }
        let key = ExactEventKey {
            lo: fact.span.lo(),
            hi: fact.span.hi(),
            kind: fact.kind,
            ordinal: *ordinal,
        };
        self.exact.entry(key).or_default().push(fact.id);
        self.exact_by_span_kind
            .entry(counter_key)
            .or_default()
            .push(fact.id);
        self.span_order
            .entry((fact.span.lo(), fact.span.hi()))
            .and_modify(|id| *id = (*id).min(fact.id))
            .or_insert(fact.id);
        self.facts.push(fact);
    }

    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.facts.len()
    }

    pub(super) fn is_valid(&self) -> bool {
        self.valid
    }

    /// Look up all facts at an exact `(lo, hi, kind, ordinal)` position.
    #[allow(dead_code)]
    pub(super) fn exact_lookup(&self, key: &ExactEventKey) -> Vec<FactId> {
        self.exact.get(key).cloned().unwrap_or_default()
    }

    /// Look up all facts at a given `(lo, hi, kind)` in ordinal order.
    #[allow(dead_code)]
    pub(super) fn facts_at(&self, lo: BytePos, hi: BytePos, kind: FactKind) -> Vec<&SemanticFact> {
        self.exact_by_span_kind
            .get(&(lo, hi, kind))
            .into_iter()
            .flatten()
            .filter_map(|id| self.facts.get(id.0 as usize))
            .collect()
    }

    #[allow(dead_code)]
    pub(super) fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }

    #[cfg(test)]
    pub(super) fn fingerprint(&self) -> String {
        format!("{:?}", self.facts)
    }

    /// Return the canonical fact order for an evidence span.  Evidence is
    /// produced from canonical facts, so this indexed fallback is only used
    /// for spans that do not carry an occurrence object (for example, a
    /// synthetic compatibility span).  Equal spans resolve to the earliest
    /// fact deterministically.
    pub(super) fn order_for_span(&self, span: Span) -> Option<FactId> {
        self.span_order.get(&(span.lo(), span.hi())).copied()
    }
}

// ── SemanticFacts ───────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) struct SemanticFacts {
    #[allow(dead_code)]
    pub(super) stream: FactStream,
    pub(super) index: MatcherFacts,
    pub(super) argument_evidence: Vec<Vec<ApiEvidence>>,
}

impl SemanticFacts {
    pub(super) fn build(
        program: &Program,
        resolver: Resolver,
        matchers: &[&ApiMatcher],
        rule_count: usize,
    ) -> Self {
        let member_argument_matchers = matchers
            .iter()
            .enumerate()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .member_calls
                    .iter()
                    .filter(|matcher| {
                        !matcher.arg_strings.is_empty()
                            || !matcher.arg_object_keys.is_empty()
                            || !matcher.arg_rooted_exprs.is_empty()
                    })
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let call_argument_matchers = matchers
            .iter()
            .enumerate()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .calls
                    .iter()
                    .filter(|matcher| !matcher.arg_strings.is_empty())
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let flow_matchers = matchers
            .iter()
            .enumerate()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .flows
                    .iter()
                    .enumerate()
                    .map(move |(flow_index, matcher)| (rule_index, flow_index, matcher))
            })
            .collect::<Vec<_>>();

        // Build the canonical fact stream from the authoritative FactBuilder.
        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(program, &mut builder);
        let stream = builder.into_stream();

        if !stream.is_valid() {
            return Self {
                stream,
                index: MatcherFacts::default(),
                argument_evidence: vec![Vec::new(); rule_count],
            };
        }

        // Project the fact stream into rule-independent occurrence indexes.
        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);

        // Compute argument evidence from pre-computed fact data.
        let mut argument_evidence = vec![Vec::new(); rule_count];
        index.compute_argument_evidence_from_stream(
            &stream,
            &member_argument_matchers,
            &call_argument_matchers,
            &mut argument_evidence,
        );

        for (rule_index, evidence) in object_flow::collect(&stream, &flow_matchers, rule_count)
            .into_iter()
            .enumerate()
        {
            argument_evidence[rule_index].extend(evidence);
        }
        index.normalize_occurrences();
        Self {
            stream,
            index,
            argument_evidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::BytePos;

    fn test_fact(id: u32, kind: FactKind, span: Span) -> SemanticFact {
        SemanticFact {
            id: FactId(id),
            span,
            scope: 0,
            function: FunctionId(0),
            kind,
            payload: match kind {
                FactKind::Call => FactPayload::Call {
                    callee: ValueId::UNKNOWN,
                    result: ValueId::UNKNOWN,
                    callee_span: span,
                    callee_name: None,
                    call_provenance: SymbolCallProvenance::Local,
                    syntactic_chain: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                    instance_class: None,
                    target_function: None,
                    args: Vec::new(),
                    unwrap: None,
                },
                FactKind::MemberRead => FactPayload::MemberRead {
                    value: ValueId::UNKNOWN,
                    syntactic_chain: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                },
                FactKind::Reference => FactPayload::Reference {
                    value: ValueId::UNKNOWN,
                },
                FactKind::Function => FactPayload::Function {
                    id: FunctionId(0),
                    owner: FunctionId(0),
                    name: None,
                    parameters: Vec::new(),
                    boundary: FunctionBoundary::Enter,
                },
                FactKind::Control => FactPayload::Control {
                    kind: ControlKind::BranchStart,
                    region: 0,
                },
                _ => FactPayload::Declaration {
                    target: ValueId::UNKNOWN,
                    source: ValueId::UNKNOWN,
                },
            },
        }
    }

    #[test]
    fn exact_lookup_distinguishes_equal_span_kinds_and_ordinals() {
        let span = Span::new(BytePos(10), BytePos(20));
        let mut stream = FactStream::new();
        stream.push(test_fact(0, FactKind::Call, span));
        stream.push(test_fact(1, FactKind::MemberRead, span));
        stream.push(test_fact(2, FactKind::Call, span));

        assert_eq!(
            stream
                .facts_at(span.lo(), span.hi(), FactKind::Call)
                .iter()
                .map(|fact| fact.id)
                .collect::<Vec<_>>(),
            vec![FactId(0), FactId(2)]
        );
        assert_eq!(
            stream.exact_lookup(&ExactEventKey {
                lo: span.lo(),
                hi: span.hi(),
                kind: FactKind::Call,
                ordinal: 1,
            }),
            vec![FactId(2)]
        );
        assert_eq!(stream.order_for_span(span), Some(FactId(0)));
    }

    /// Verify that the fact-driven index populates expected occurrence maps
    /// for a diverse program.
    #[test]
    fn fact_driven_index_populates_expected_maps() {
        let src = r#"
            import { foo } from 'mod';
            import { Bar } from 'other-mod';
            class MyApp extends Bar {}
            const x = foo;
            function greet(name) { return name; }
            greet("hello");
            x.hello();
            new Bar();
            const s = "world";
            require('path');
            const a = [1, 2];
            a.push(3);
        "#;
        let parsed = crate::parse(src, "char-index.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        assert!(
            index.imports.get("mod").is_some(),
            "should have 'mod' import"
        );
        assert!(
            index.imports.get("other-mod").is_some(),
            "should have 'other-mod' import"
        );
        assert!(
            index.imports.get("path").is_some(),
            "should have 'path' require import"
        );
        assert!(index.calls.get("greet").is_some(), "should have greet call");
        assert!(
            index.string_literals.get("world").is_some(),
            "should have 'world' string literal"
        );
        assert!(!index.classes.is_empty(), "should have class entries");
        assert!(
            index
                .module_classes
                .get(&("other-mod".to_string(), "Bar".to_string()))
                .is_some(),
            "should have module class for Bar from other-mod"
        );
        assert!(
            !index.constructors.is_empty(),
            "should have constructor entries"
        );
        assert!(
            index.member_calls.get("x.hello").is_some()
                || index.rooted_member_calls.iter().next().is_some()
                || index.member_calls.iter().next().is_some(),
            "should have member calls"
        );
    }

    /// Verify that .call()/.apply() unwrapping produces the expected
    /// member call entries for the target.
    #[test]
    fn call_apply_unwrapping_populates_indexes() {
        let src = r#"
            function fetch(url) { return url; }
            fetch.call(null, '/api');
            fetch.apply(null, ['/api']);
        "#;
        let parsed = crate::parse(src, "unwrap.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        // The unwrap should record 'fetch' as a member call.
        assert!(
            index.member_calls.get("fetch").is_some(),
            "should have 'fetch' as member call from unwrapping"
        );
    }
}
