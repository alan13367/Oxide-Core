use std::collections::HashMap;
use std::marker::PhantomData;

use crate::Handle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssetChange<T> {
    pub handle: Handle<T>,
    pub revision: u64,
    pub kind: AssetChangeKind,
}

impl<T> Copy for AssetChange<T> {}

impl<T> Clone for AssetChange<T> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Per-consumer cursor for reading asset change records without draining them.
///
/// Renderer, editor, importer, and tooling caches can keep their own cursor and
/// call [`read`](Self::read) each frame to receive only the records they have
/// not observed yet. This avoids a global `drain_changes` owner and lets
/// several systems react to the same asset store independently.
pub struct AssetChangeCursor<T> {
    next_change: usize,
    seen_generation: u64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> AssetChangeCursor<T> {
    /// Creates a cursor positioned at the beginning of the current change log.
    pub fn new() -> Self {
        Self {
            next_change: 0,
            seen_generation: 0,
            _marker: PhantomData,
        }
    }

    /// Returns unread changes and advances the cursor to the end of the log.
    ///
    /// If the asset log was cleared since the previous read, the cursor resumes
    /// from the earliest retained record.
    pub fn read(&mut self, assets: &Assets<T>) -> Vec<AssetChange<T>> {
        let start = self.start_index(assets);
        let changes = assets.changes[start..].to_vec();
        self.next_change = assets.changes.len();
        self.seen_generation = assets.change_generation;
        changes
    }

    /// Returns unread changes without advancing the cursor.
    pub fn peek<'a>(&self, assets: &'a Assets<T>) -> &'a [AssetChange<T>] {
        let start = self.start_index(assets);
        &assets.changes[start..]
    }

    /// Returns the number of unread retained change records.
    pub fn pending_len(&self, assets: &Assets<T>) -> usize {
        self.peek(assets).len()
    }

    /// Positions the cursor at the start of the retained change log.
    pub fn rewind(&mut self) {
        self.next_change = 0;
    }

    /// Positions the cursor at the end of the current change log.
    pub fn skip_existing(&mut self, assets: &Assets<T>) {
        self.next_change = assets.changes.len();
        self.seen_generation = assets.change_generation;
    }

    fn start_index(&self, assets: &Assets<T>) -> usize {
        if self.seen_generation != assets.change_generation
            || self.next_change > assets.changes.len()
        {
            0
        } else {
            self.next_change
        }
    }
}

impl<T> Default for AssetChangeCursor<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Generic typed asset storage.
pub struct Assets<T> {
    data: HashMap<u64, T>,
    revisions: HashMap<u64, u64>,
    changes: Vec<AssetChange<T>>,
    change_generation: u64,
}

impl<T> Assets<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            revisions: HashMap::new(),
            changes: Vec::new(),
            change_generation: 0,
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

    /// Iterates loaded assets with their typed handles.
    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.data
            .iter()
            .map(|(id, asset)| (Handle::new(*id), asset))
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
        self.change_generation = self.change_generation.saturating_add(1);
        self.changes.drain(..)
    }

    /// Clears pending asset change records.
    pub fn clear_changes(&mut self) {
        self.change_generation = self.change_generation.saturating_add(1);
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

    #[test]
    fn assets_iter_returns_typed_handles_and_values() {
        let mut allocator = HandleAllocator::new();
        let first = allocator.allocate::<u32>();
        let second = allocator.allocate::<u32>();
        let mut assets = Assets::new();

        assets.insert(first, 7);
        assets.insert(second, 11);

        let mut entries = assets
            .iter()
            .map(|(handle, value)| (handle.id(), *value))
            .collect::<Vec<_>>();
        entries.sort_unstable();
        assert_eq!(entries, vec![(first.id(), 7), (second.id(), 11)]);
    }

    #[test]
    fn asset_change_cursor_reads_each_retained_change_once() {
        let mut allocator = HandleAllocator::new();
        let first = allocator.allocate::<String>();
        let second = allocator.allocate::<String>();
        let mut assets = Assets::new();
        let mut renderer_cursor = AssetChangeCursor::new();
        let mut editor_cursor = AssetChangeCursor::new();

        assets.insert(first, "mesh".to_string());
        assets.insert(second, "texture".to_string());

        assert_eq!(renderer_cursor.pending_len(&assets), 2);
        assert_eq!(
            renderer_cursor.read(&assets),
            vec![
                AssetChange {
                    handle: first,
                    revision: 1,
                    kind: AssetChangeKind::Added,
                },
                AssetChange {
                    handle: second,
                    revision: 1,
                    kind: AssetChangeKind::Added,
                },
            ]
        );
        assert!(renderer_cursor.read(&assets).is_empty());

        let (asset, _) = assets.get_mut_mark_changed(&first).unwrap();
        asset.push_str(" v2");
        assets.remove(&second);

        assert_eq!(
            renderer_cursor.read(&assets),
            vec![
                AssetChange {
                    handle: first,
                    revision: 2,
                    kind: AssetChangeKind::Modified,
                },
                AssetChange {
                    handle: second,
                    revision: 1,
                    kind: AssetChangeKind::Removed,
                },
            ]
        );

        assert_eq!(editor_cursor.read(&assets).len(), 4);
    }

    #[test]
    fn asset_change_cursor_can_skip_existing_changes() {
        let mut allocator = HandleAllocator::new();
        let handle = allocator.allocate::<u32>();
        let mut assets = Assets::new();
        let mut cursor = AssetChangeCursor::new();

        assets.insert(handle, 1);
        cursor.skip_existing(&assets);
        assert!(cursor.read(&assets).is_empty());

        assets.insert(handle, 2);
        assert_eq!(
            cursor.peek(&assets),
            &[AssetChange {
                handle,
                revision: 2,
                kind: AssetChangeKind::Modified,
            }]
        );
        assert_eq!(cursor.read(&assets).len(), 1);

        cursor.rewind();
        assert_eq!(cursor.read(&assets).len(), 2);
    }

    #[test]
    fn asset_change_cursor_recovers_after_global_clear() {
        let mut allocator = HandleAllocator::new();
        let handle = allocator.allocate::<u32>();
        let mut assets = Assets::new();
        let mut cursor = AssetChangeCursor::new();

        assets.insert(handle, 1);
        assert_eq!(cursor.read(&assets).len(), 1);

        assets.clear_changes();
        assets.insert(handle, 2);

        assert_eq!(
            cursor.read(&assets),
            vec![AssetChange {
                handle,
                revision: 2,
                kind: AssetChangeKind::Modified,
            }]
        );
    }
}
