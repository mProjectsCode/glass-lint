use crate::name::NameId;

pub const DEFAULT_MAX_PATH_NODES: usize = 1 << 20;

/// An opaque node handle owned by one
/// [`ParentPathStore`](super::ParentPathStore).
///
/// The owner and linked state are deliberately private. Callers can only
/// obtain handles from a store, so a handle from one store cannot be used as a
/// local parent in another store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId {
    index: u32,
    owner: u64,
    linked: bool,
}

impl PathId {
    pub const EMPTY: Self = Self {
        index: 0,
        owner: 0,
        linked: false,
    };
    pub(crate) const LINK_TAG: u32 = 1 << 31;

    #[cfg(test)]
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self {
            index: raw & !Self::LINK_TAG,
            owner: 0,
            linked: raw & Self::LINK_TAG != 0,
        }
    }

    pub(crate) const fn for_store(index: u32, owner: u64) -> Self {
        Self {
            index,
            owner,
            linked: false,
        }
    }

    pub(crate) const fn with_linked(self) -> Self {
        Self {
            linked: true,
            ..self
        }
    }

    pub(crate) const fn without_linked(self) -> Self {
        Self {
            linked: false,
            ..self
        }
    }

    pub(crate) const fn index(self) -> u32 {
        self.index
    }

    pub(crate) const fn owner(self) -> u64 {
        self.owner
    }

    #[cfg(test)]
    pub(crate) const fn is_linked(self) -> bool {
        self.linked
    }

    pub fn is_empty(self) -> bool {
        self.index == 0
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
