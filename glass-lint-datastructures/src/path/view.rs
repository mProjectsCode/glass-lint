#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathView<'a, T>(pub(super) &'a [T]);

impl<T> Copy for PathView<'_, T> {}

impl<T> Clone for PathView<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> PathView<'a, T> {
    pub fn new(slice: &'a [T]) -> Self {
        Self(slice)
    }

    pub fn segments(&self) -> &'a [T] {
        self.0
    }

    pub fn first_segment(&self) -> Option<&'a T> {
        self.0.first()
    }

    pub fn last_segment(&self) -> Option<&'a T> {
        self.0.last()
    }

    pub fn without_last_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(&self.0[..self.0.len() - 1]))
        }
    }

    pub fn without_first_segment(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(&self.0[1..]))
        }
    }

    pub fn prefix_at(&self, len: usize) -> Option<Self> {
        if len > self.0.len() {
            None
        } else {
            Some(Self(&self.0[..len]))
        }
    }

    pub fn tail_after(&self, count: usize) -> Option<Self> {
        if count > self.0.len() {
            None
        } else {
            Some(Self(&self.0[count..]))
        }
    }

    pub fn prefixes(&self) -> impl DoubleEndedIterator<Item = Self> + 'a {
        (1..=self.0.len()).map(|end| Self(&self.0[..end]))
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

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
