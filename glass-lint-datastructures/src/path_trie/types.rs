use crate::name::NameId;

pub const DEFAULT_MAX_PATH_NODES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId(pub(crate) u32);

impl PathId {
    #[inline]
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl PathId {
    pub const EMPTY: Self = Self(0);
    pub const LINK_TAG: u32 = 1 << 31;

    pub fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    pub fn is_linked(self) -> bool {
        self.0 & Self::LINK_TAG != 0
    }

    #[must_use]
    pub fn untag(self) -> Self {
        Self(self.0 & !Self::LINK_TAG)
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
