//! Authoritative per-file semantic fact construction.
//!
//! The individual fact collectors are implementation details of this build.
//! Matchers receive only the immutable SemanticFacts result, so adding a
//! matcher cannot introduce another semantic path at the model boundary.

use swc_ecma_ast::Program;

use super::super::result::ApiEvidence;
use super::super::rule::ApiMatcher;
use super::events::EventLog;
use super::index::MatcherFacts;
use super::resolver::Resolver;
use super::{calls, object_flow};

#[derive(Debug)]
pub(super) struct SemanticFacts {
    pub(super) events: EventLog,
    pub(super) index: MatcherFacts,
    pub(super) argument_evidence: Vec<Vec<ApiEvidence>>,
}

impl SemanticFacts {
    pub(super) fn build(
        program: &Program,
        resolver: Resolver,
        events: EventLog,
        matchers: &[ApiMatcher],
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
        let instance_matchers = matchers
            .iter()
            .flat_map(|matcher| matcher.instance_member_calls.iter())
            .collect::<Vec<_>>();

        let mut index = MatcherFacts::default();
        let mut argument_evidence = vec![Vec::new(); rule_count];
        calls::collect(
            program,
            calls::CallContext {
                events: &events,
                resolver: &resolver,
            },
            &member_argument_matchers,
            &call_argument_matchers,
            &instance_matchers,
            &mut index,
            &mut argument_evidence,
        );
        for (rule_index, evidence) in
            object_flow::collect(program, &resolver, &events, &flow_matchers, rule_count)
                .into_iter()
                .enumerate()
        {
            argument_evidence[rule_index].extend(evidence);
        }
        index.normalize_occurrences();
        Self {
            events,
            index,
            argument_evidence,
        }
    }

    pub(super) fn empty(rule_count: usize) -> Self {
        Self {
            events: EventLog::default(),
            index: MatcherFacts::default(),
            argument_evidence: vec![Vec::new(); rule_count],
        }
    }
}
