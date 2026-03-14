use std::collections::HashMap;

use crate::Handle;

/// Generic typed asset storage.
pub struct Assets<T> {
    data: HashMap<u64, T>,
}

impl<T> Assets<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn insert(&mut self, handle: Handle<T>, asset: T) {
        self.data.insert(handle.id(), asset);
    }

    pub fn get(&self, handle: &Handle<T>) -> Option<&T> {
        self.data.get(&handle.id())
    }

    pub fn get_mut(&mut self, handle: &Handle<T>) -> Option<&mut T> {
        self.data.get_mut(&handle.id())
    }

    pub fn remove(&mut self, handle: &Handle<T>) -> Option<T> {
        self.data.remove(&handle.id())
    }

    pub fn contains(&self, handle: &Handle<T>) -> bool {
        self.data.contains_key(&handle.id())
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl<T> Default for Assets<T> {
    fn default() -> Self {
        Self::new()
    }
}
