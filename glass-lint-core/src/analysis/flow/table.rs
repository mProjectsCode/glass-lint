//! Dense tables for identities allocated by the fact builder.

use super::super::value::FunctionId;

#[derive(Debug, Clone)]
pub(in crate::analysis) struct FunctionTable<T> {
    values: Vec<Option<T>>,
}

impl<T> Default for FunctionTable<T> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

impl<T> FunctionTable<T> {
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&T> {
        self.values.get(usize::try_from(id.0).ok()?)?.as_ref()
    }

    pub(in crate::analysis) fn get_mut(&mut self, id: FunctionId) -> Option<&mut T> {
        self.values.get_mut(usize::try_from(id.0).ok()?)?.as_mut()
    }

    pub(in crate::analysis) fn insert(&mut self, id: FunctionId, value: T) -> bool {
        let Some(index) = usize::try_from(id.0).ok() else {
            return false;
        };
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || None);
        }
        let vacant = self.values[index].is_none();
        self.values[index] = Some(value);
        vacant
    }

    pub(in crate::analysis) fn get_mut_or_insert_with(
        &mut self,
        id: FunctionId,
        create: impl FnOnce() -> T,
    ) -> Option<&mut T> {
        let index = usize::try_from(id.0).ok()?;
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || None);
        }
        Some(self.values[index].get_or_insert_with(create))
    }

    pub(in crate::analysis) fn iter(&self) -> impl Iterator<Item = (FunctionId, &T)> {
        self.values.iter().enumerate().filter_map(|(index, value)| {
            value
                .as_ref()
                .map(|value| (FunctionId(u32::try_from(index).unwrap_or(u32::MAX)), value))
        })
    }

    pub(in crate::analysis) fn values(&self) -> impl Iterator<Item = &T> {
        self.values.iter().filter_map(Option::as_ref)
    }

    pub(in crate::analysis) fn iter_mut(&mut self) -> impl Iterator<Item = (FunctionId, &mut T)> {
        self.values
            .iter_mut()
            .enumerate()
            .filter_map(|(index, value)| {
                value
                    .as_mut()
                    .map(|value| (FunctionId(u32::try_from(index).unwrap_or(u32::MAX)), value))
            })
    }

    pub(in crate::analysis) fn len(&self) -> usize {
        self.values.iter().filter(|value| value.is_some()).count()
    }
    pub(in crate::analysis) fn contains(&self, id: FunctionId) -> bool {
        self.get(id).is_some()
    }
}
