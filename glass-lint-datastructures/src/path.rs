use std::{borrow::Borrow, fmt, ops::Deref};

use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::name::NameId;

/// A generic path stored as a sequence of segments in a container `S`.
///
/// Use [`NamePath`] or [`SymbolPath`] for concrete instantiations.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path<S>(S);

/// An interned path stored as a sequence of [`NameId`]s.
///
/// Uses a [`SmallVec`] with a 4-element inline capacity so short paths avoid
/// heap allocation.
pub type NamePath = Path<SmallVec<[NameId; 4]>>;

/// A path stored as human-readable string segments.
///
/// Useful for debug output, rule configuration, and interop with external
/// representations like dot-separated chains (`"a.b.c"`).
pub type SymbolPath = Path<Vec<SmolStr>>;

impl<T: Clone, S> Path<S>
where
    S: Deref<Target = [T]> + Default + FromIterator<T> + Extend<T>,
{
    /// Creates an empty path.
    pub fn new() -> Self {
        Self(S::default())
    }

    /// Appends a segment to the end.
    pub fn append(&mut self, segment: T) {
        self.0.extend(std::iter::once(segment));
    }

    /// Returns the segment slice.
    pub fn segments(&self) -> &[T] {
        &self.0
    }

    /// The first segment, or `None` if empty.
    pub fn first_segment(&self) -> Option<&T> {
        self.0.first()
    }

    /// The last segment, or `None` if empty.
    pub fn last_segment(&self) -> Option<&T> {
        self.0.last()
    }

    /// Returns a new path with the last segment removed.
    ///
    /// Returns `None` if the path is empty.
    pub fn without_last_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(self.0[..self.0.len() - 1].iter().cloned().collect()))
        }
    }

    /// Returns a new path with the first segment removed.
    ///
    /// Returns `None` if the path is empty.
    pub fn without_first_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(self.0[1..].iter().cloned().collect()))
        }
    }

    /// Returns a new path with `suffix` appended.
    #[must_use]
    pub fn append_path(&self, suffix: &Self) -> Self {
        let mut path: S = self.0.iter().cloned().collect();
        path.extend(suffix.0.iter().cloned());
        Self(path)
    }

    /// Returns `true` if the path has 0 or 1 segments.
    pub fn is_root(&self) -> bool {
        self.0.len() <= 1
    }

    /// Returns `true` if this path starts with the segments of `root`.
    pub fn is_equal_or_descendant_of(&self, root: &Self) -> bool
    where
        T: PartialEq,
    {
        self.0.len() >= root.0.len() && self.0[..root.0.len()] == root.0[..]
    }

    /// Creates a path from an iterator of segments.
    pub fn from_ids(ids: impl IntoIterator<Item = T>) -> Self {
        Self(ids.into_iter().collect())
    }

    /// The number of segments.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the path has no segments.
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
    /// Returns a [`PathView`] borrowing all segments.
    pub fn as_view(&self) -> PathView<'_, T> {
        PathView(&self.0)
    }

    /// Returns a [`PathView`] without the last segment.
    ///
    /// Returns `None` if the path is empty. Zero-copy equivalent of
    /// [`without_last_segment`](Self::without_last_segment).
    pub fn view_without_last_segment(&self) -> Option<PathView<'_, T>> {
        if self.0.is_empty() {
            None
        } else {
            Some(PathView(&self.0[..self.0.len() - 1]))
        }
    }

    /// Returns a [`PathView`] without the first segment.
    ///
    /// Returns `None` if the path is empty. Zero-copy equivalent of
    /// [`without_first_segment`](Self::without_first_segment).
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
    /// Parses a dot-separated chain into a path.
    ///
    /// Empty segments (from leading, trailing, or consecutive dots) are
    /// silently skipped.
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

    /// Creates a path from a vector of segments.
    pub fn from_segments(segments: Vec<SmolStr>) -> Self {
        Self(segments)
    }

    /// Returns `true` if this path's segments match the dot-separated `chain`.
    pub fn eq_chain(&self, chain: &str) -> bool {
        self.0.iter().map(SmolStr::as_str).eq(chain.split('.'))
    }

    /// Returns a new path with `suffix` appended as dot-separated segments.
    ///
    /// A leading dot in `suffix` is optional and stripped if present.
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

    /// Removes the leading `"this"` segment if present.
    #[must_use]
    pub fn without_this_prefix(&self) -> Self {
        if self.0.first().is_some_and(|segment| segment == "this") {
            Self(self.0[1..].to_vec())
        } else {
            self.clone()
        }
    }

    /// Removes the trailing `"bind"` segment if present.
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

/// A borrowed view of a path, backed by a slice reference.
///
/// Provides the same read-only API as [`Path`] without owning the segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathView<'a, T>(&'a [T]);

impl<'a, T> PathView<'a, T> {
    /// Creates a view over the given slice.
    pub fn new(slice: &'a [T]) -> Self {
        Self(slice)
    }

    /// Returns the segment slice.
    pub fn segments(&self) -> &[T] {
        self.0
    }

    /// The first segment, or `None` if empty.
    pub fn first_segment(&self) -> Option<&T> {
        self.0.first()
    }

    /// The last segment, or `None` if empty.
    pub fn last_segment(&self) -> Option<&T> {
        self.0.last()
    }

    /// Returns a new view with the last segment removed.
    ///
    /// Returns `None` if the path is empty.
    pub fn without_last_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(&self.0[..self.0.len() - 1]))
        }
    }

    /// Returns a new view with the first segment removed.
    ///
    /// Returns `None` if the path is empty.
    pub fn without_first_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(&self.0[1..]))
        }
    }

    /// Returns `true` if the path has 0 or 1 segments.
    pub fn is_root(&self) -> bool {
        self.0.len() <= 1
    }

    /// Returns `true` if this path starts with the segments of `root`.
    pub fn is_equal_or_descendant_of(&self, root: &Self) -> bool
    where
        T: PartialEq,
    {
        self.0.len() >= root.0.len() && self.0[..root.0.len()] == root.0[..]
    }

    /// The number of segments.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the path has no segments.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- NamePath tests ----

    #[test]
    fn name_path_empty() {
        let path = NamePath::new();
        assert!(path.is_root());
        assert!(path.segments().is_empty());
        assert_eq!(path.first_segment(), None);
        assert_eq!(path.last_segment(), None);
    }

    #[test]
    fn name_path_single_segment() {
        let id = NameId(42);
        let mut path = NamePath::new();
        path.append(id);
        assert_eq!(path.segments(), &[NameId(42)]);
        assert_eq!(path.first_segment(), Some(&NameId(42)));
        assert_eq!(path.last_segment(), Some(&NameId(42)));
        assert!(path.is_root());
    }

    #[test]
    fn name_path_multi_segment() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        path.append(NameId(2));
        path.append(NameId(3));
        assert_eq!(path.segments(), &[NameId(1), NameId(2), NameId(3)]);
        assert!(!path.is_root());
    }

    #[test]
    fn name_path_without_first() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        path.append(NameId(2));
        let rest = path.without_first_segment().unwrap();
        assert_eq!(rest.segments(), &[NameId(2)]);
    }

    #[test]
    fn name_path_without_first_on_single_returns_empty() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        let rest = path.without_first_segment().unwrap();
        assert!(rest.is_root());
        assert!(rest.segments().is_empty());
    }

    #[test]
    fn name_path_without_first_on_empty_returns_none() {
        let path = NamePath::new();
        assert_eq!(path.without_first_segment(), None);
    }

    #[test]
    fn name_path_without_last() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        path.append(NameId(2));
        let rest = path.without_last_segment().unwrap();
        assert_eq!(rest.segments(), &[NameId(1)]);
    }

    #[test]
    fn name_path_without_last_on_empty_returns_none() {
        let path = NamePath::new();
        assert_eq!(path.without_last_segment(), None);
    }

    #[test]
    fn name_path_append_path() {
        let mut a = NamePath::new();
        a.append(NameId(1));
        let mut b = NamePath::new();
        b.append(NameId(2));
        b.append(NameId(3));
        let c = a.append_path(&b);
        assert_eq!(c.segments(), &[NameId(1), NameId(2), NameId(3)]);
    }

    #[test]
    fn name_path_is_root() {
        assert!(NamePath::new().is_root());
        let mut p = NamePath::new();
        p.append(NameId(1));
        assert!(p.is_root());
        p.append(NameId(2));
        assert!(!p.is_root());
    }

    #[test]
    fn name_path_from_ids() {
        let ids = [NameId(10), NameId(20)];
        let path = NamePath::from_ids(ids);
        assert_eq!(path.segments(), &[NameId(10), NameId(20)]);
    }

    #[test]
    fn name_path_is_equal_or_descendant_of() {
        let mut root = NamePath::new();
        root.append(NameId(1));
        let mut child = NamePath::new();
        child.append(NameId(1));
        child.append(NameId(2));
        assert!(child.is_equal_or_descendant_of(&root));
        assert!(root.is_equal_or_descendant_of(&root));
        assert!(!root.is_equal_or_descendant_of(&child));
    }

    // ---- SymbolPath tests ----

    #[test]
    fn symbol_path_from_chain_with_dots() {
        let path = SymbolPath::from_chain("a.b.c");
        assert_eq!(
            path.segments(),
            &[SmolStr::new("a"), SmolStr::new("b"), SmolStr::new("c")]
        );
    }

    #[test]
    fn symbol_path_without_first() {
        let path = SymbolPath::from_chain("a.b.c");
        let rest = path.without_first_segment().unwrap();
        assert_eq!(rest.segments(), &[SmolStr::new("b"), SmolStr::new("c")]);
    }

    #[test]
    fn symbol_path_without_last() {
        let path = SymbolPath::from_chain("a.b.c");
        let rest = path.without_last_segment().unwrap();
        assert_eq!(rest.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
    }

    #[test]
    fn symbol_path_from_impls() {
        let from_str: SymbolPath = "a.b.c".into();
        let from_string: SymbolPath = String::from("a.b.c").into();
        let from_smol: SymbolPath = SmolStr::new("a.b.c").into();
        assert_eq!(from_str, from_string);
        assert_eq!(from_str, from_smol);
    }

    #[test]
    fn symbol_path_is_root() {
        assert!(SymbolPath::from_chain("a").is_root());
        assert!(!SymbolPath::from_chain("a.b").is_root());
    }

    #[test]
    fn symbol_path_is_equal_or_descendant_of() {
        let root = SymbolPath::from_chain("a.b");
        let child = SymbolPath::from_chain("a.b.c");
        assert!(child.is_equal_or_descendant_of(&root));
        assert!(root.is_equal_or_descendant_of(&root));
        assert!(!root.is_equal_or_descendant_of(&child));
    }

    #[test]
    fn symbol_path_edge_cases() {
        assert!(SymbolPath::from_chain("").is_empty());
        assert!(SymbolPath::from_chain(".").is_empty());
        assert!(SymbolPath::from_chain("..").is_empty());
        assert_eq!(SymbolPath::from_chain(".a."), SymbolPath::from_chain("a"));
    }

    #[test]
    fn symbol_path_from_chain_strips_leading_trailing_consecutive_dots() {
        let path = SymbolPath::from_chain(".a..b.");
        assert_eq!(path.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
    }

    #[test]
    fn symbol_path_append_path() {
        let a = SymbolPath::from_chain("a.b");
        let b = SymbolPath::from_chain("c.d");
        let c = a.append_path(&b);
        assert_eq!(
            c.segments(),
            &[
                SmolStr::new("a"),
                SmolStr::new("b"),
                SmolStr::new("c"),
                SmolStr::new("d"),
            ]
        );
    }

    #[test]
    fn symbol_path_append_empty() {
        let a = SymbolPath::from_chain("a");
        let empty = SymbolPath::from_chain("");
        let c = a.append_path(&empty);
        assert_eq!(c.segments(), &[SmolStr::new("a")]);
    }

    #[test]
    fn symbol_path_first_segment_empty() {
        let path = SymbolPath::from_chain("");
        assert_eq!(path.first_segment(), None);
    }

    #[test]
    fn symbol_path_is_equal_or_descendant_of_not_ancestor() {
        let a = SymbolPath::from_chain("a.b");
        let b = SymbolPath::from_chain("a.c");
        assert!(!a.is_equal_or_descendant_of(&b));
        assert!(!b.is_equal_or_descendant_of(&a));
    }

    #[test]
    fn name_path_len() {
        let mut path = NamePath::new();
        assert_eq!(path.len(), 0);
        path.append(NameId(1));
        assert_eq!(path.len(), 1);
        path.append(NameId(2));
        assert_eq!(path.len(), 2);
    }

    #[test]
    fn name_path_is_empty() {
        assert!(NamePath::new().is_empty());
        let mut path = NamePath::new();
        path.append(NameId(1));
        assert!(!path.is_empty());
    }

    #[test]
    fn name_path_append_path_empty() {
        let mut a = NamePath::new();
        a.append(NameId(1));
        let empty = NamePath::new();
        let c = a.append_path(&empty);
        assert_eq!(c.segments(), &[NameId(1)]);
    }

    // ---- PathView tests ----

    #[test]
    fn path_view_empty() {
        let view = PathView::<i32>::new(&[]);
        assert!(view.is_empty());
        assert!(view.is_root());
        assert_eq!(view.first_segment(), None);
        assert_eq!(view.last_segment(), None);
        assert_eq!(view.len(), 0);
    }

    #[test]
    fn path_view_single() {
        let view = PathView::new(&[42]);
        assert!(!view.is_empty());
        assert!(view.is_root());
        assert_eq!(view.first_segment(), Some(&42));
        assert_eq!(view.last_segment(), Some(&42));
    }

    #[test]
    fn path_view_multi() {
        let view = PathView::new(&[1, 2, 3]);
        assert_eq!(view.segments(), &[1, 2, 3]);
        assert!(!view.is_root());
        assert_eq!(view.first_segment(), Some(&1));
        assert_eq!(view.last_segment(), Some(&3));
    }

    #[test]
    fn path_view_without_last() {
        let view = PathView::new(&[1, 2, 3]);
        let rest = view.without_last_segment().unwrap();
        assert_eq!(rest.segments(), &[1, 2]);
    }

    #[test]
    fn path_view_without_last_on_empty_returns_none() {
        let view = PathView::<i32>::new(&[]);
        assert_eq!(view.without_last_segment(), None);
    }

    #[test]
    fn path_view_without_first() {
        let view = PathView::new(&[1, 2, 3]);
        let rest = view.without_first_segment().unwrap();
        assert_eq!(rest.segments(), &[2, 3]);
    }

    #[test]
    fn path_view_without_first_on_empty_returns_none() {
        let view = PathView::<i32>::new(&[]);
        assert_eq!(view.without_first_segment(), None);
    }

    #[test]
    fn path_view_is_equal_or_descendant_of() {
        let root = PathView::new(&[1, 2]);
        let child = PathView::new(&[1, 2, 3]);
        assert!(child.is_equal_or_descendant_of(&root));
        assert!(root.is_equal_or_descendant_of(&root));
        assert!(!root.is_equal_or_descendant_of(&child));
    }

    #[test]
    fn path_as_view_on_name_path() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        path.append(NameId(2));
        let view = path.as_view();
        assert_eq!(view.segments(), &[NameId(1), NameId(2)]);
        assert_eq!(view.first_segment(), Some(&NameId(1)));
        assert_eq!(view.len(), 2);
    }

    #[test]
    fn path_as_view_on_symbol_path() {
        let path = SymbolPath::from_chain("a.b");
        let view = path.as_view();
        assert_eq!(view.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
        assert_eq!(view.first_segment(), Some(&SmolStr::new("a")));
    }

    #[test]
    fn view_without_last_segment_on_name_path() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        path.append(NameId(2));
        path.append(NameId(3));
        let view = path.view_without_last_segment().unwrap();
        assert_eq!(view.segments(), &[NameId(1), NameId(2)]);
    }

    #[test]
    fn view_without_last_segment_on_symbol_path() {
        let path = SymbolPath::from_chain("a.b.c");
        let view = path.view_without_last_segment().unwrap();
        assert_eq!(view.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
    }

    #[test]
    fn view_without_last_segment_on_empty() {
        let path = NamePath::new();
        assert_eq!(path.view_without_last_segment(), None);
    }

    #[test]
    fn view_without_first_segment_on_name_path() {
        let mut path = NamePath::new();
        path.append(NameId(1));
        path.append(NameId(2));
        let view = path.view_without_first_segment().unwrap();
        assert_eq!(view.segments(), &[NameId(2)]);
    }

    #[test]
    fn view_without_first_segment_on_symbol_path() {
        let path = SymbolPath::from_chain("a.b.c");
        let view = path.view_without_first_segment().unwrap();
        assert_eq!(view.segments(), &[SmolStr::new("b"), SmolStr::new("c")]);
    }

    #[test]
    fn view_without_first_segment_on_empty() {
        let path = NamePath::new();
        assert_eq!(path.view_without_first_segment(), None);
    }
}
