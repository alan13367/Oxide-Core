//! Handle type for shared asset references

use std::marker::PhantomData;

/// A typed handle to an asset.
pub struct Handle<T> {
    /// Unique identifier for this asset.
    pub id: u64,
    _marker: PhantomData<T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").field("id", &self.id).finish()
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> Handle<T> {
    /// Creates a new handle with the given ID.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Returns the raw ID.
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// A counter for generating unique handles.
#[derive(Default)]
pub struct HandleAllocator {
    next_id: u64,
}

impl HandleAllocator {
    /// Creates a new allocator starting from ID 0.
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    /// Allocates a new handle.
    pub fn allocate<T>(&mut self) -> Handle<T> {
        let id = self.next_id;
        self.next_id += 1;
        Handle::new(id)
    }
}
