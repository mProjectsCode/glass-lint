use std::marker::PhantomData;

/// Outcome of an [`IndexTable::insert`] operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The value was inserted into a previously vacant slot.
    Inserted,
    /// The value replaced an existing entry at the same id.
    Replaced,
    /// The id is at or beyond the table's capacity.
    OutOfRange,
}

/// Trait for types used as dense index identifiers in an [`IndexTable`].
///
/// Requires `Copy + Into<u32>` so the identifier can be used as a storage
/// index without additional allocation or indirection.
pub trait IdIndex: Copy + Into<u32> {
    /// Constructs an identifier from a raw `u32` value.
    fn from_raw(raw: u32) -> Self;
}

/// A sparse, index-based storage table with an owner-supplied capacity.
///
/// Maps dense `I` identifiers to optional `T` values.  Internally backed by a
/// `Vec<Option<T>>` where the index corresponds to the identifier.  This
/// offers O(1) lookup and efficient iteration over present entries, but is
/// not space-efficient for very sparse populations.
///
/// Insertion refuses IDs at or above the configured `capacity` to prevent
/// uncontrolled allocation from forged or sparse identifiers.
#[derive(Debug, Clone)]
pub struct IndexTable<I, T> {
    values: Vec<Option<T>>,
    occupied: usize,
    capacity: usize,
    _marker: PhantomData<I>,
}

impl<I: IdIndex, T> IndexTable<I, T> {
    /// Creates an empty table with the given capacity.
    ///
    /// IDs at or above `capacity` will be rejected by [`insert`](Self::insert).
    pub fn new(capacity: usize) -> Self {
        Self {
            values: Vec::new(),
            occupied: 0,
            capacity,
            _marker: PhantomData,
        }
    }

    /// Returns a shared reference to the value at `id`, or `None`.
    pub fn get(&self, id: I) -> Option<&T> {
        let index = usize::try_from(id.into()).ok()?;
        self.values.get(index)?.as_ref()
    }

    /// Returns a mutable reference to the value at `id`, or `None`.
    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        let index = usize::try_from(id.into()).ok()?;
        self.values.get_mut(index)?.as_mut()
    }

    /// Inserts `value` at `id`.
    ///
    /// Returns [`InsertOutcome::Inserted`] if the slot was vacant,
    /// [`InsertOutcome::Replaced`] if it was occupied, or
    /// [`InsertOutcome::OutOfRange`] if the id is at or beyond the table's
    /// capacity or could not be converted to a `usize`.
    ///
    /// The vector grows automatically to accommodate the id, up to the
    /// configured capacity.
    pub fn insert(&mut self, id: I, value: T) -> InsertOutcome {
        let raw: u32 = id.into();
        let Some(index) = usize::try_from(raw).ok() else {
            return InsertOutcome::OutOfRange;
        };
        if index >= self.capacity {
            return InsertOutcome::OutOfRange;
        }
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || None);
        }
        let vacant = self.values[index].is_none();
        self.values[index] = Some(value);
        if vacant {
            self.occupied += 1;
            InsertOutcome::Inserted
        } else {
            InsertOutcome::Replaced
        }
    }

    /// Simultaneously borrows one slot for reading and another for writing.
    ///
    /// Returns `None` when `read == write` (the borrows would alias).
    /// Returns `Some((None, None))` when both slots are beyond the current
    /// storage length.
    pub fn get_disjoint(&mut self, read: I, write: I) -> Option<(Option<&T>, Option<&mut T>)> {
        let read_raw: u32 = read.into();
        let write_raw: u32 = write.into();
        if read_raw == write_raw {
            return None;
        }
        let ri = usize::try_from(read_raw).ok()?;
        let wi = usize::try_from(write_raw).ok()?;
        if self.values.len() <= ri.max(wi) {
            return Some((None, None));
        }
        if ri < wi {
            let (left, right) = self.values.split_at_mut(wi);
            let read_ref = left[ri].as_ref();
            let write_ref = right[0].as_mut();
            Some((read_ref, write_ref))
        } else {
            let (left, right) = self.values.split_at_mut(ri);
            let write_ref = left[wi].as_mut();
            let read_ref = right[0].as_ref();
            Some((read_ref, write_ref))
        }
    }

    /// An iterator over `(id, &value)` pairs for present entries.
    ///
    /// Iteration order is by increasing id.
    pub fn iter(&self) -> impl Iterator<Item = (I, &T)> {
        self.values.iter().enumerate().filter_map(|(index, value)| {
            value.as_ref().map(|value| {
                let raw = u32::try_from(index).unwrap_or(u32::MAX);
                (I::from_raw(raw), value)
            })
        })
    }

    /// A mutable iterator over `(id, &mut value)` pairs for present entries.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &mut T)> {
        self.values
            .iter_mut()
            .enumerate()
            .filter_map(|(index, value)| {
                value.as_mut().map(|value| {
                    let raw = u32::try_from(index).unwrap_or(u32::MAX);
                    (I::from_raw(raw), value)
                })
            })
    }

    /// An iterator over shared references to present values.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.values.iter().filter_map(Option::as_ref)
    }

    /// Returns `true` if the slot at `id` is occupied.
    pub fn contains(&self, id: I) -> bool {
        self.get(id).is_some()
    }

    /// The number of occupied slots.
    pub fn len(&self) -> usize {
        self.occupied
    }

    /// Returns `true` if no slots are occupied.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all values from the table, keeping the allocated storage.
    pub fn clear(&mut self) {
        self.values.clear();
        self.occupied = 0;
    }

    /// Shrinks the internal vector to the highest occupied index + 1.
    pub fn shrink_to_fit(&mut self) {
        let present_len = self
            .values
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1);
        self.values.truncate(present_len);
        self.values.shrink_to_fit();
    }
}

#[cfg(test)]
mod tests;
