use std::collections::HashMap;

use crate::Handle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetChange<T> {
    pub handle: Handle<T>,
    pub revision: u64,
    pub kind: AssetChangeKind,
}

/// Generic typed asset storage.
pub struct Assets<T> {
    data: HashMap<u64, T>,
    revisions: HashMap<u64, u64>,
    changes: Vec<AssetChange<T>>,
}

impl<T> Assets<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            revisions: HashMap::new(),
            changes: Vec::new(),
        }
    }

    /// Inserts or replaces an asset and returns the handle's new revision.
    ///
    /// Revisions start at 1 and increment on every replacement. They are useful
    /// for renderer/editor caches that need to know when a stable handle now
    /// points at a newer value after hot reload.
    pub fn insert(&mut self, handle: Handle<T>, asset: T) -> u64 {
        let id = handle.id();
        let revision = self
            .revisions
            .get(&id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        let kind = if self.data.contains_key(&id) {
            AssetChangeKind::Modified
        } else {
            AssetChangeKind::Added
        };
        self.data.insert(handle.id(), asset);
        self.revisions.insert(id, revision);
        self.changes.push(AssetChange {
            handle,
            revision,
            kind,
        });
        revision
    }

    pub fn get(&self, handle: &Handle<T>) -> Option<&T> {
        self.data.get(&handle.id())
    }

    pub fn get_mut(&mut self, handle: &Handle<T>) -> Option<&mut T> {
        self.data.get_mut(&handle.id())
    }

    /// Marks an existing asset as changed and returns a mutable reference plus
    /// the new revision.
    pub fn get_mut_mark_changed(&mut self, handle: &Handle<T>) -> Option<(&mut T, u64)> {
        let id = handle.id();
        let asset = self.data.get_mut(&id)?;
        let revision = self
            .revisions
            .get(&id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.revisions.insert(id, revision);
        self.changes.push(AssetChange {
            handle: *handle,
            revision,
            kind: AssetChangeKind::Modified,
        });
        Some((asset, revision))
    }

    pub fn remove(&mut self, handle: &Handle<T>) -> Option<T> {
        let revision = self.revisions.remove(&handle.id())?;
        let asset = self.data.remove(&handle.id())?;
        self.changes.push(AssetChange {
            handle: *handle,
            revision,
            kind: AssetChangeKind::Removed,
        });
        Some(asset)
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

    /// Returns the current revision for a loaded asset handle.
    pub fn revision(&self, handle: &Handle<T>) -> Option<u64> {
        self.data
            .contains_key(&handle.id())
            .then(|| self.revisions.get(&handle.id()).copied().unwrap_or(0))
    }

    /// Returns true when the loaded asset revision is newer than `revision`.
    pub fn changed_since(&self, handle: &Handle<T>, revision: u64) -> bool {
        self.revision(handle)
            .map(|current| current > revision)
            .unwrap_or(false)
    }

    /// Returns pending asset change records without clearing them.
    pub fn changes(&self) -> &[AssetChange<T>] {
        &self.changes
    }

    /// Drains pending asset change records.
    pub fn drain_changes(&mut self) -> impl Iterator<Item = AssetChange<T>> + '_ {
        self.changes.drain(..)
    }

    /// Clears pending asset change records.
    pub fn clear_changes(&mut self) {
        self.changes.clear();
    }
}

impl<T> Default for Assets<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HandleAllocator;

    #[test]
    fn asset_revisions_increment_on_replace_and_mark_changed() {
        let mut allocator = HandleAllocator::new();
        let handle = allocator.allocate::<String>();
        let mut assets = Assets::new();

        assert_eq!(assets.revision(&handle), None);
        assert_eq!(assets.insert(handle, "first".to_string()), 1);
        assert_eq!(assets.revision(&handle), Some(1));
        assert_eq!(assets.changes()[0].kind, AssetChangeKind::Added);
        assert!(!assets.changed_since(&handle, 1));

        assert_eq!(assets.insert(handle, "second".to_string()), 2);
        assert_eq!(assets.revision(&handle), Some(2));
        assert_eq!(assets.changes()[1].kind, AssetChangeKind::Modified);
        assert!(assets.changed_since(&handle, 1));

        let (asset, revision) = assets.get_mut_mark_changed(&handle).unwrap();
        asset.push_str(" edited");
        assert_eq!(revision, 3);
        assert_eq!(assets.get(&handle).unwrap(), "second edited");
        assert_eq!(assets.revision(&handle), Some(3));
        assert_eq!(assets.changes()[2].kind, AssetChangeKind::Modified);
        assert_eq!(assets.drain_changes().count(), 3);
        assert!(assets.changes().is_empty());
    }

    #[test]
    fn asset_revisions_are_removed_with_assets() {
        let mut allocator = HandleAllocator::new();
        let handle = allocator.allocate::<u32>();
        let mut assets = Assets::new();

        assets.insert(handle, 7);
        assert_eq!(assets.revision(&handle), Some(1));
        assets.clear_changes();
        assert_eq!(assets.remove(&handle), Some(7));
        assert_eq!(assets.revision(&handle), None);
        assert!(!assets.changed_since(&handle, 0));
        assert_eq!(
            assets.changes(),
            &[AssetChange {
                handle,
                revision: 1,
                kind: AssetChangeKind::Removed
            }]
        );
    }
}
