use crate::name::NameId;

pub const DEFAULT_MAX_PATH_NODES: usize = 1 << 20;

/// An opaque node handle owned by one [`PathStore`](super::PathStore).
///
/// The owner is deliberately private. Callers can only obtain handles from a
/// store, so a handle from one store cannot be used as a local parent in
/// another store. Linked parents are represented by [`super::PathLink`], not
/// by a second state encoded in this identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId {
    index: u32,
    owner: u64,
}

impl PathId {
    pub const EMPTY: Self = Self { index: 0, owner: 0 };

    pub(crate) const fn for_store(index: u32, owner: u64) -> Self {
        Self { index, owner }
    }

    pub(crate) const fn index(self) -> u32 {
        self.index
    }

    pub(crate) const fn owner(self) -> u64 {
        self.owner
    }

    pub fn is_empty(self) -> bool {
        self == Self::EMPTY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    Property(NameId),
    Index(u32),
}

#[derive(Clone, Copy, Debug)]
pub enum PathSegmentInput<'a> {
    Property(&'a str),
    PropertyId(NameId),
    Index(u32),
}
