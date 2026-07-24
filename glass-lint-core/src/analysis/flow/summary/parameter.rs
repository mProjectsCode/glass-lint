use glass_lint_datastructures::PathSegment;

use crate::analysis::{
    facts::{CallArgInfo, FactStream, Frozen, ParameterBinding},
    value::{Value, ValueId, ValueTable},
};

use super::store::{SummaryPathId, SummaryPathStore};

impl ParameterBinding {
    pub(super) fn project_argument(
        &self,
        stream: &FactStream<Frozen>,
        args: &[CallArgInfo],
        paths: &SummaryPathStore<'_>,
    ) -> Option<ValueId> {
        let param_path = paths.resolve_frozen(self.path)?;
        self.project_argument_at(stream, args, paths, param_path)
    }

    pub(in crate::analysis::flow) fn project_argument_at(
        &self,
        stream: &FactStream<Frozen>,
        args: &[CallArgInfo],
        paths: &SummaryPathStore<'_>,
        path: SummaryPathId,
    ) -> Option<ValueId> {
        let Some(argument) = args.get(self.parameter_index) else {
            return self
                .path
                .is_empty()
                .then_some(self.default)
                .flatten()
                .filter(|value| *value != ValueId::UNKNOWN);
        };
        if argument.spread {
            return None;
        }

        if self.rest {
            let index = paths.first_index(path)?;
            let argument = args.get(self.parameter_index.saturating_add(index as usize))?;
            if argument.spread {
                return None;
            }
            let path = paths.without_first(path)?;
            if path.is_empty() {
                return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
            }
            return value_at_path(stream.values(), argument.value, paths, path);
        }

        if path.is_empty() {
            return (argument.value != ValueId::UNKNOWN).then_some(argument.value);
        }

        {
            let id = value_at_path(stream.values(), argument.value, paths, path)
                .filter(|v| *v != ValueId::UNKNOWN);
            id.or_else(|| self.default.filter(|value| *value != ValueId::UNKNOWN))
        }
    }
}

fn value_at_path(
    values: &ValueTable,
    value_id: ValueId,
    paths: &SummaryPathStore<'_>,
    path: SummaryPathId,
) -> Option<ValueId> {
    let mut current = value_id;
    let mut valid = true;
    paths.visit_segments(path, &mut |segment| {
        if !valid {
            return;
        }
        let Some(value) = values.resolve(current) else {
            valid = false;
            return;
        };
        let next = match value {
            Value::StaticObject(entries) => match segment {
                PathSegment::Property(name_id) => {
                    entries.iter().find(|(k, _)| k == name_id).map(|(_, v)| *v)
                }
                PathSegment::Index(_) => None,
            },
            Value::StaticArray(elements) => match segment {
                PathSegment::Index(index) => elements.get(*index as usize).copied(),
                PathSegment::Property(_) => None,
            },
            _ => None,
        };
        if let Some(next) = next {
            current = next;
        } else {
            valid = false;
        }
    })?;
    if !valid {
        return None;
    }
    (current != ValueId::UNKNOWN).then_some(current)
}
