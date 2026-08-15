use super::QueryBuildError;

/// Non-empty, bounded, canonical collection: items collected from a fallible
/// iterator, sorted and deduplicated into one deterministic sequence.
///
/// Construction enforces the collection bound while converting and rejects
/// empty input with the caller-supplied error. Items of the same value are
/// deduplicated, so the returned sequence never exceeds `limit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CanonicalCollection<T>(Vec<T>);

impl<T: Ord> CanonicalCollection<T> {
    pub(crate) fn collect<I, F>(
        items: I,
        limit: usize,
        empty: QueryBuildError,
        label: &'static str,
        mut convert: F,
    ) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        F: FnMut(I::Item) -> Result<T, QueryBuildError>,
    {
        let mut collected = Vec::new();
        for item in items {
            if collected.len() >= limit {
                return Err(QueryBuildError::CollectionTooLarge(
                    label,
                    collected.len() + 1,
                ));
            }
            collected.push(convert(item)?);
        }
        collected.sort();
        collected.dedup();
        if collected.is_empty() {
            return Err(empty);
        }
        Ok(Self(collected))
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn into_vec(self) -> Vec<T> {
        self.0
    }
}
