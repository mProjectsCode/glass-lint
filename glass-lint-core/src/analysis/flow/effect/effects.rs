use std::collections::BTreeMap;

use glass_lint_datastructures::Budget;

use super::{function::FunctionEffect, types::ParameterRef};
use crate::analysis::{
    facts::{ControlKind, FactPayload, FactStream, Frozen, FunctionBoundary},
    model::flow::FunctionTable,
    value::FunctionId,
};

#[derive(Clone, Debug, Default)]
pub struct FunctionEffects {
    by_id: FunctionTable<FunctionEffect>,
    budget_exhausted: bool,
    operation_count: usize,
}

impl FunctionEffects {
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&FunctionEffect> {
        self.by_id.get(id)
    }

    pub(in crate::analysis) fn iter_effects(&self) -> impl Iterator<Item = &FunctionEffect> {
        self.by_id.values()
    }

    pub(in crate::analysis) fn budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    pub(in crate::analysis) fn operation_count(&self) -> usize {
        self.operation_count
    }

    #[allow(clippy::too_many_lines)]
    // Single pass over the fact stream: the match dispatches each payload
    // variant to an existing method on FunctionEffect. Extracting the
    // dispatch into FunctionEffect would require threading `stream`,
    // `budget`, and `value_provenance` through every call.
    pub(in crate::analysis) fn collect(stream: &FactStream<Frozen>, limit: usize) -> Self {
        let mut effects = Self::default();
        if !stream.is_valid() {
            return effects;
        }
        let mut budget = Budget::new(limit);
        let mut value_provenance = BTreeMap::new();

        if budget.try_push() {
            effects.by_id.insert(
                FunctionId(0),
                FunctionEffect {
                    id: FunctionId(0),
                    calls: Vec::new(),
                    uses: Vec::new(),
                    returns: Vec::new(),
                    invalid: false,
                    value_roots: BTreeMap::new(),
                    parameter_index: BTreeMap::new(),
                },
            );
        }

        for fact in stream.facts() {
            if let FactPayload::Function {
                id,
                boundary: FunctionBoundary::Enter,
                ..
            } = &fact.payload
            {
                if !effects.by_id.contains(*id) && !budget.try_push() {
                    continue;
                }
                let params = stream.function_parameters(*id);
                effects.by_id.insert(
                    *id,
                    FunctionEffect {
                        id: *id,
                        calls: Vec::new(),
                        uses: Vec::new(),
                        returns: Vec::new(),
                        invalid: false,
                        value_roots: params.iter().map(|p| (p.value, p.value)).collect(),
                        parameter_index: params
                            .iter()
                            .map(|p| {
                                (
                                    p.value,
                                    ParameterRef {
                                        index: p.parameter_index,
                                        path: p.path,
                                    },
                                )
                            })
                            .collect(),
                    },
                );
                continue;
            }

            let Some(effect) = effects.by_id.get_mut(fact.function) else {
                continue;
            };
            match &fact.payload {
                FactPayload::Reference { value, provenance } => {
                    effect.record_reference(*value, provenance, &mut value_provenance);
                }
                FactPayload::Declaration { target, source }
                | FactPayload::Assignment {
                    target,
                    source,
                    receiver: None,
                } => effect.record_copy(*target, *source),
                FactPayload::Assignment {
                    receiver: Some(_), ..
                } => effect.invalid = true,
                FactPayload::PropertyWrite {
                    receiver,
                    property,
                    value: _,
                } => effect.record_property_write(
                    fact.id,
                    *receiver,
                    property.and_then(|id| stream.resolve_name(id)),
                    stream,
                    &mut budget,
                ),
                FactPayload::Call { .. } => effect.record_call(fact, stream, &mut budget),
                FactPayload::Control {
                    kind: ControlKind::Return,
                    return_value,
                    ..
                } => {
                    effect.record_return(*return_value, &value_provenance, stream, &mut budget);
                }
                _ => {}
            }
            effect.mark_unsupported_control(&fact.payload);
        }
        effects.budget_exhausted = budget.exhausted();
        effects.operation_count = budget.used();
        effects
    }
}
