use std::{borrow::Borrow, fmt, ops::Deref};

use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::{name::NameId, path::view::PathView};

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path<S>(S);

pub type NamePath = Path<SmallVec<[NameId; 4]>>;

pub type SymbolPath = Path<Vec<SmolStr>>;

impl<T: Clone, S> Path<S>
where
    S: Deref<Target = [T]> + Default + FromIterator<T> + Extend<T>,
{
    pub fn new() -> Self {
        Self(S::default())
    }

    pub fn append(&mut self, segment: T) {
        self.0.extend(std::iter::once(segment));
    }

    pub fn segments(&self) -> &[T] {
        &self.0
    }

    pub fn first_segment(&self) -> Option<&T> {
        self.0.first()
    }

    pub fn last_segment(&self) -> Option<&T> {
        self.0.last()
    }

    pub fn without_last_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(self.0[..self.0.len() - 1].iter().cloned().collect()))
        }
    }

    pub fn without_first_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(self.0[1..].iter().cloned().collect()))
        }
    }

    #[must_use]
    pub fn append_path(&self, suffix: &Self) -> Self {
        let mut path: S = self.0.iter().cloned().collect();
        path.extend(suffix.0.iter().cloned());
        Self(path)
    }

    pub fn is_root(&self) -> bool {
        self.0.len() <= 1
    }

    pub fn is_equal_or_descendant_of(&self, root: &Self) -> bool
    where
        T: PartialEq,
    {
        self.0.len() >= root.0.len() && self.0[..root.0.len()] == root.0[..]
    }

    pub fn from_ids(ids: impl IntoIterator<Item = T>) -> Self {
        Self(ids.into_iter().collect())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T, S> Borrow<[T]> for Path<S>
where
    S: Deref<Target = [T]>,
{
    fn borrow(&self) -> &[T] {
        &self.0
    }
}

impl<T, S> Path<S>
where
    S: Deref<Target = [T]>,
{
    pub fn as_view(&self) -> PathView<'_, T> {
        PathView(&self.0)
    }

    pub fn view_without_last_segment(&self) -> Option<PathView<'_, T>> {
        if self.0.is_empty() {
            None
        } else {
            Some(PathView(&self.0[..self.0.len() - 1]))
        }
    }

    pub fn view_without_first_segment(&self) -> Option<PathView<'_, T>> {
        if self.0.is_empty() {
            None
        } else {
            Some(PathView(&self.0[1..]))
        }
    }
}

impl<T: Clone, S> From<S> for Path<S>
where
    S: Deref<Target = [T]> + Default + FromIterator<T> + Extend<T>,
{
    fn from(value: S) -> Self {
        Self(value)
    }
}

impl Path<Vec<SmolStr>> {
    pub fn from_chain(chain: &str) -> Self {
        Self(
            chain
                .split('.')
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(SmolStr::new)
                .collect(),
        )
    }

    pub fn from_segments(segments: Vec<SmolStr>) -> Self {
        Self(segments)
    }

    pub fn eq_chain(&self, chain: &str) -> bool {
        self.0.iter().map(SmolStr::as_str).eq(chain.split('.'))
    }

    #[must_use]
    pub fn append_chain(&self, suffix: &str) -> Self {
        let mut path = self.0.clone();
        path.extend(
            suffix
                .strip_prefix('.')
                .unwrap_or(suffix)
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(SmolStr::new),
        );
        Self(path)
    }

    #[must_use]
    pub fn without_this_prefix(&self) -> Self {
        if self.0.first().is_some_and(|segment| segment == "this") {
            Self(self.0[1..].to_vec())
        } else {
            self.clone()
        }
    }

    pub fn without_bind_suffix(&self) -> Option<Self> {
        self.0
            .last()
            .is_some_and(|segment| segment == "bind")
            .then(|| self.without_last_segment())
            .flatten()
    }
}

impl fmt::Display for Path<Vec<SmolStr>> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join("."))
    }
}

impl From<String> for Path<Vec<SmolStr>> {
    fn from(value: String) -> Self {
        Self::from_chain(&value)
    }
}

impl From<SmolStr> for Path<Vec<SmolStr>> {
    fn from(value: SmolStr) -> Self {
        Self::from_chain(&value)
    }
}

impl From<&str> for Path<Vec<SmolStr>> {
    fn from(value: &str) -> Self {
        Self::from_chain(value)
    }
}
