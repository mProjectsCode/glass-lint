//! Dense tables for identities allocated by the fact builder.

use crate::analysis::value::FunctionId;

#[derive(Debug, Clone)]
/// Sparse dense-indexed storage for function identities.
///
/// Missing slots are valid and represent functions that were not emitted or
/// exceeded the enclosing analysis budget; callers must handle `None`.
pub(in crate::analysis) struct FunctionTable<T> {
    /// Values positioned by the numeric function identity.
    values: Vec<Option<T>>,
}

impl<T> Default for FunctionTable<T> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

impl<T> FunctionTable<T> {
    /// Borrow a value for a function identity, if present.
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&T> {
        self.values.get(usize::try_from(id.0).ok()?)?.as_ref()
    }

    /// Mutably borrow a value for a function identity, if present.
    pub(in crate::analysis) fn get_mut(&mut self, id: FunctionId) -> Option<&mut T> {
        self.values.get_mut(usize::try_from(id.0).ok()?)?.as_mut()
    }

    /// Insert a value and report whether its slot was previously vacant.
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

    /// Borrow one entry immutably and another entry mutably in one call.
    /// Panics when `read` and `write` refer to the same identity.
    pub(in crate::analysis) fn get_disjoint(
        &mut self,
        read: FunctionId,
        write: FunctionId,
    ) -> (Option<&T>, Option<&mut T>) {
        assert_ne!(
            read, write,
            "get_disjoint requires different read and write identities"
        );
        let max = usize::try_from(read.0.max(write.0)).unwrap_or(usize::MAX);
        if self.values.len() <= max {
            return (None, None);
        }
        let ri = usize::try_from(read.0).unwrap_or(usize::MAX);
        let wi = usize::try_from(write.0).unwrap_or(usize::MAX);
        let ptr = self.values.as_mut_ptr();
        // SAFETY: ri != wi, so the returned references point to different
        // entries and do not alias.
        unsafe {
            let read_ref = if ri < self.values.len() {
                (*ptr.add(ri)).as_ref()
            } else {
                None
            };
            let write_ref = if wi < self.values.len() {
                (*ptr.add(wi)).as_mut()
            } else {
                None
            };
            (read_ref, write_ref)
        }
    }

    /// Iterate present entries in ascending function-ID order.
    pub(in crate::analysis) fn iter(&self) -> impl Iterator<Item = (FunctionId, &T)> {
        self.values.iter().enumerate().filter_map(|(index, value)| {
            value
                .as_ref()
                .map(|value| (FunctionId(u32::try_from(index).unwrap_or(u32::MAX)), value))
        })
    }

    /// Iterate present values without exposing sparse slots.
    pub(in crate::analysis) fn values(&self) -> impl Iterator<Item = &T> {
        self.values.iter().filter_map(Option::as_ref)
    }

    /// Iterate present entries mutably in ascending function-ID order.
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

    #[cfg(test)]
    pub(in crate::analysis) fn len(&self) -> usize {
        self.values.iter().filter(|value| value.is_some()).count()
    }

    /// Check whether an identity has a present value.
    pub(in crate::analysis) fn contains(&self, id: FunctionId) -> bool {
        self.get(id).is_some()
    }
}
