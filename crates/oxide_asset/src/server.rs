use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::{Assets, Handle, HandleAllocator};

#[derive(thiserror::Error, Debug)]
pub enum AssetServerError {
    #[error("{0}")]
    Message(String),
    #[error("asset type mismatch during async load completion")]
    TypeMismatch,
    #[error("asset loading thread disconnected")]
    ChannelDisconnected,
}

struct PendingAsset {
    type_id: TypeId,
    receiver: Receiver<Result<Box<dyn Any + Send>, AssetServerError>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetLoadStatus {
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, Debug)]
struct AssetMetadata {
    type_id: TypeId,
    path: Option<PathBuf>,
    status: AssetLoadStatus,
    dependencies: Vec<PathBuf>,
}

pub struct AssetServer {
    allocator: HandleAllocator,
    pending: HashMap<u64, PendingAsset>,
    metadata: HashMap<u64, AssetMetadata>,
    paths: HashMap<(TypeId, PathBuf), u64>,
}

impl Default for AssetServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetServer {
    pub fn new() -> Self {
        Self {
            allocator: HandleAllocator::new(),
            pending: HashMap::new(),
            metadata: HashMap::new(),
            paths: HashMap::new(),
        }
    }

    pub fn allocate_handle<T>(&mut self) -> Handle<T> {
        self.allocator.allocate::<T>()
    }

    pub fn load_async<T, F>(&mut self, loader: F) -> Handle<T>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, AssetServerError> + Send + 'static,
    {
        let handle = self.allocate_handle::<T>();
        let id = handle.id();
        let (sender, receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let result = loader().map(|asset| Box::new(asset) as Box<dyn Any + Send>);
            let _ = sender.send(result);
        });

        self.pending.insert(
            id,
            PendingAsset {
                type_id: TypeId::of::<T>(),
                receiver,
            },
        );
        self.metadata.insert(
            id,
            AssetMetadata {
                type_id: TypeId::of::<T>(),
                path: None,
                status: AssetLoadStatus::Loading,
                dependencies: Vec::new(),
            },
        );

        handle
    }

    pub fn load_path_async<T, F>(&mut self, path: impl Into<PathBuf>, loader: F) -> Handle<T>
    where
        T: Send + 'static,
        F: FnOnce(PathBuf) -> Result<T, AssetServerError> + Send + 'static,
    {
        let path = normalize_asset_path(path.into());
        let type_id = TypeId::of::<T>();
        if let Some(id) = self.paths.get(&(type_id, path.clone())).copied() {
            let reusable = self
                .metadata
                .get(&id)
                .map(|metadata| {
                    matches!(
                        metadata.status,
                        AssetLoadStatus::Loading | AssetLoadStatus::Loaded
                    )
                })
                .unwrap_or(false);
            if reusable {
                return Handle::new(id);
            }
            self.paths.remove(&(type_id, path.clone()));
        }

        let handle = self.allocate_handle::<T>();
        let id = handle.id();
        let loader_path = path.clone();
        let (sender, receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let result = loader(loader_path).map(|asset| Box::new(asset) as Box<dyn Any + Send>);
            let _ = sender.send(result);
        });

        self.pending.insert(id, PendingAsset { type_id, receiver });
        self.metadata.insert(
            id,
            AssetMetadata {
                type_id,
                path: Some(path.clone()),
                status: AssetLoadStatus::Loading,
                dependencies: Vec::new(),
            },
        );
        self.paths.insert((type_id, path), id);

        handle
    }

    /// Reloads an already-known typed path into its existing handle.
    ///
    /// This is intended for hot-reload systems: stored handles remain stable
    /// while [`poll_ready`](Self::poll_ready) or [`poll_loaded`](Self::poll_loaded)
    /// later publishes the replacement asset value. Returns `None` when the
    /// path is not currently known for `T`.
    pub fn reload_path_async<T, F>(
        &mut self,
        path: impl Into<PathBuf>,
        loader: F,
    ) -> Option<Handle<T>>
    where
        T: Send + 'static,
        F: FnOnce(PathBuf) -> Result<T, AssetServerError> + Send + 'static,
    {
        let path = normalize_asset_path(path.into());
        let type_id = TypeId::of::<T>();
        let id = self.paths.get(&(type_id, path.clone())).copied()?;

        let metadata = self.metadata.get_mut(&id)?;
        if metadata.type_id != type_id {
            return None;
        }

        let loader_path = path.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = loader(loader_path).map(|asset| Box::new(asset) as Box<dyn Any + Send>);
            let _ = sender.send(result);
        });

        self.pending.insert(id, PendingAsset { type_id, receiver });
        metadata.status = AssetLoadStatus::Loading;
        metadata.path = Some(path);

        Some(Handle::new(id))
    }

    pub fn handle_for_path<T: 'static>(&self, path: impl Into<PathBuf>) -> Option<Handle<T>> {
        let path = normalize_asset_path(path.into());
        self.paths
            .get(&(TypeId::of::<T>(), path))
            .copied()
            .map(Handle::new)
    }

    pub fn asset_status<T: 'static>(&self, handle: &Handle<T>) -> Option<AssetLoadStatus> {
        let metadata = self.metadata.get(&handle.id())?;
        (metadata.type_id == TypeId::of::<T>()).then_some(metadata.status)
    }

    pub fn asset_path<T: 'static>(&self, handle: &Handle<T>) -> Option<&Path> {
        let metadata = self.metadata.get(&handle.id())?;
        (metadata.type_id == TypeId::of::<T>())
            .then_some(metadata.path.as_deref())
            .flatten()
    }

    /// Replaces the path dependencies recorded for a typed asset.
    ///
    /// Dependencies are normalized paths used by hot-reload and importer
    /// systems to find assets affected by changes to secondary source files
    /// such as material includes, texture files, or imported subdocuments.
    pub fn set_asset_dependencies<T: 'static, I, P>(
        &mut self,
        handle: &Handle<T>,
        dependencies: I,
    ) -> bool
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let Some(metadata) = self.metadata.get_mut(&handle.id()) else {
            return false;
        };
        if metadata.type_id != TypeId::of::<T>() {
            return false;
        }

        metadata.dependencies = dependencies
            .into_iter()
            .map(|path| normalize_asset_path(path.into()))
            .collect();
        metadata.dependencies.sort();
        metadata.dependencies.dedup();
        true
    }

    /// Adds one path dependency to a typed asset.
    pub fn add_asset_dependency<T: 'static>(
        &mut self,
        handle: &Handle<T>,
        dependency: impl Into<PathBuf>,
    ) -> bool {
        let Some(metadata) = self.metadata.get_mut(&handle.id()) else {
            return false;
        };
        if metadata.type_id != TypeId::of::<T>() {
            return false;
        }

        let dependency = normalize_asset_path(dependency.into());
        if !metadata.dependencies.contains(&dependency) {
            metadata.dependencies.push(dependency);
            metadata.dependencies.sort();
        }
        true
    }

    /// Returns path dependencies recorded for a typed asset.
    pub fn asset_dependencies<T: 'static>(&self, handle: &Handle<T>) -> Option<&[PathBuf]> {
        let metadata = self.metadata.get(&handle.id())?;
        (metadata.type_id == TypeId::of::<T>()).then_some(metadata.dependencies.as_slice())
    }

    /// Returns typed asset handles whose source path or dependency paths match a changed path.
    pub fn handles_for_changed_path<T: 'static>(&self, path: impl Into<PathBuf>) -> Vec<Handle<T>> {
        let path = normalize_asset_path(path.into());
        self.metadata
            .iter()
            .filter_map(|(id, metadata)| {
                if metadata.type_id != TypeId::of::<T>() {
                    return None;
                }
                let path_matches = metadata.path.as_ref() == Some(&path)
                    || metadata
                        .dependencies
                        .iter()
                        .any(|dependency| dependency == &path);
                path_matches.then(|| Handle::new(*id))
            })
            .collect()
    }

    /// Polls for completed async assets and returns ready `(Handle<T>, T)` pairs.
    pub fn poll_ready<T: Send + 'static>(
        &mut self,
    ) -> Vec<Result<(Handle<T>, T), AssetServerError>> {
        let mut completed = Vec::new();
        let pending_ids: Vec<u64> = self
            .pending
            .iter()
            .filter_map(|(id, pending)| (pending.type_id == TypeId::of::<T>()).then_some(*id))
            .collect();

        for id in pending_ids {
            let status = match self.pending.get(&id) {
                Some(pending) => pending.receiver.try_recv(),
                None => continue,
            };

            match status {
                Ok(result) => {
                    let _ = self.pending.remove(&id);
                    match result {
                        Ok(boxed_asset) => match boxed_asset.downcast::<T>() {
                            Ok(asset) => {
                                self.set_status(id, AssetLoadStatus::Loaded);
                                let handle = Handle::new(id);
                                completed.push(Ok((handle, *asset)));
                            }
                            Err(_) => {
                                self.set_status(id, AssetLoadStatus::Failed);
                                completed.push(Err(AssetServerError::TypeMismatch));
                            }
                        },
                        Err(err) => {
                            self.set_status(id, AssetLoadStatus::Failed);
                            completed.push(Err(err));
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    let _ = self.pending.remove(&id);
                    self.set_status(id, AssetLoadStatus::Failed);
                    completed.push(Err(AssetServerError::ChannelDisconnected));
                }
            }
        }

        completed
    }

    pub fn poll_loaded<T: Send + 'static>(
        &mut self,
        assets: &mut Assets<T>,
    ) -> Vec<Result<Handle<T>, AssetServerError>> {
        let mut completed_handles = Vec::new();
        for result in self.poll_ready::<T>() {
            match result {
                Ok((handle, asset)) => {
                    assets.insert(handle, asset);
                    completed_handles.push(Ok(handle));
                }
                Err(err) => completed_handles.push(Err(err)),
            }
        }
        completed_handles
    }

    pub fn is_loading<T: 'static>(&self, handle: &Handle<T>) -> bool {
        self.pending.contains_key(&handle.id())
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    fn set_status(&mut self, id: u64, status: AssetLoadStatus) {
        if let Some(metadata) = self.metadata.get_mut(&id) {
            metadata.status = status;
        }
    }
}

fn normalize_asset_path(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Assets;
    use std::time::{Duration, Instant};

    fn poll_until_ready<T: Send + 'static>(
        server: &mut AssetServer,
    ) -> Vec<Result<(Handle<T>, T), AssetServerError>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let ready = server.poll_ready::<T>();
            if !ready.is_empty() {
                return ready;
            }
            std::thread::yield_now();
        }
        server.poll_ready::<T>()
    }

    fn poll_until_loaded<T: Send + 'static>(
        server: &mut AssetServer,
        assets: &mut Assets<T>,
    ) -> Vec<Result<Handle<T>, AssetServerError>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let loaded = server.poll_loaded(assets);
            if !loaded.is_empty() {
                return loaded;
            }
            std::thread::yield_now();
        }
        server.poll_loaded(assets)
    }

    #[test]
    fn path_load_reuses_handle_for_same_type_and_path() {
        let mut server = AssetServer::new();
        let handle =
            server.load_path_async("assets/foo.oxscene", |_| Ok::<_, AssetServerError>(7u32));
        let duplicate =
            server.load_path_async("assets/foo.oxscene", |_| Ok::<_, AssetServerError>(9u32));

        assert_eq!(handle, duplicate);
        assert_eq!(
            server.handle_for_path::<u32>("assets/foo.oxscene"),
            Some(handle)
        );
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Loading));

        let ready = poll_until_ready::<u32>(&mut server);
        assert_eq!(ready.len(), 1);
        let (ready_handle, value) = ready.into_iter().next().unwrap().unwrap();
        assert_eq!(ready_handle, handle);
        assert_eq!(value, 7);
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Loaded));
    }

    #[test]
    fn path_identity_is_type_separated() {
        let mut server = AssetServer::new();
        let number =
            server.load_path_async("assets/shared.asset", |_| Ok::<_, AssetServerError>(1u32));
        let text = server.load_path_async("assets/shared.asset", |_| {
            Ok::<_, AssetServerError>("one".to_string())
        });

        assert_ne!(number.id(), text.id());
        assert_eq!(
            server.handle_for_path::<u32>("assets/shared.asset"),
            Some(number)
        );
        assert_eq!(
            server.handle_for_path::<String>("assets/shared.asset"),
            Some(text)
        );
    }

    #[test]
    fn failed_path_load_persists_failed_status() {
        let mut server = AssetServer::new();
        let handle = server.load_path_async::<u32, _>("assets/missing.oxscene", |_| {
            Err(AssetServerError::Message("missing".to_string()))
        });

        let ready = poll_until_ready::<u32>(&mut server);
        assert_eq!(ready.len(), 1);
        assert!(ready.into_iter().next().unwrap().is_err());
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Failed));
    }

    #[test]
    fn failed_path_load_can_be_retried_with_new_handle() {
        let mut server = AssetServer::new();
        let failed = server.load_path_async::<u32, _>("assets/retry.oxscene", |_| {
            Err(AssetServerError::Message("missing".to_string()))
        });
        let ready = poll_until_ready::<u32>(&mut server);
        assert!(ready.into_iter().next().unwrap().is_err());

        let retried =
            server.load_path_async("assets/retry.oxscene", |_| Ok::<_, AssetServerError>(12u32));
        assert_ne!(failed, retried);
        assert_eq!(
            server.handle_for_path::<u32>("assets/retry.oxscene"),
            Some(retried)
        );
    }

    #[test]
    fn poll_loaded_inserts_ready_assets_and_marks_loaded() {
        let mut server = AssetServer::new();
        let mut assets = Assets::<u32>::new();
        let handle =
            server.load_path_async("assets/number.asset", |_| Ok::<_, AssetServerError>(42u32));

        let loaded = poll_until_loaded(&mut server, &mut assets);
        assert_eq!(loaded.len(), 1, "asset did not finish loading");
        assert_eq!(loaded.into_iter().next().unwrap().unwrap(), handle);
        assert_eq!(assets.get(&handle), Some(&42));
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Loaded));
    }

    #[test]
    fn reload_path_reuses_handle_and_replaces_loaded_asset() {
        let mut server = AssetServer::new();
        let mut assets = Assets::<u32>::new();
        let handle =
            server.load_path_async("assets/reload.asset", |_| Ok::<_, AssetServerError>(1u32));

        let loaded = poll_until_loaded(&mut server, &mut assets);
        assert_eq!(loaded.len(), 1, "asset did not finish loading");
        assert_eq!(assets.get(&handle), Some(&1));
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Loaded));

        let reloaded = server
            .reload_path_async("assets/reload.asset", |_| Ok::<_, AssetServerError>(2u32))
            .unwrap();
        assert_eq!(reloaded, handle);
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Loading));

        let loaded = poll_until_loaded(&mut server, &mut assets);
        assert_eq!(loaded.len(), 1, "asset did not finish reloading");
        assert_eq!(loaded.into_iter().next().unwrap().unwrap(), handle);
        assert_eq!(assets.get(&handle), Some(&2));
        assert_eq!(server.asset_status(&handle), Some(AssetLoadStatus::Loaded));
    }

    #[test]
    fn reload_path_requires_known_typed_path_and_preserves_dependencies() {
        let mut server = AssetServer::new();
        let handle = server.load_path_async("assets/reload_scene.oxscene", |_| {
            Ok::<_, AssetServerError>(1u32)
        });
        assert!(server.add_asset_dependency(&handle, "assets/material.oxmat"));

        assert_eq!(
            server.reload_path_async::<String, _>("assets/reload_scene.oxscene", |_| {
                Ok::<_, AssetServerError>("wrong type".to_string())
            }),
            None
        );
        assert_eq!(
            server.reload_path_async::<u32, _>("assets/unknown.oxscene", |_| {
                Ok::<_, AssetServerError>(2u32)
            }),
            None
        );

        assert_eq!(
            server
                .reload_path_async(
                    "assets/reload_scene.oxscene",
                    |_| Ok::<_, AssetServerError>(3u32)
                )
                .unwrap(),
            handle
        );
        assert_eq!(
            server.asset_dependencies(&handle).unwrap(),
            &[PathBuf::from("assets/material.oxmat")]
        );
    }

    #[test]
    fn dependency_paths_are_deduplicated_and_queryable() {
        let mut server = AssetServer::new();
        let handle =
            server.load_path_async("assets/scene.oxscene", |_| Ok::<_, AssetServerError>(1u32));

        assert!(server.set_asset_dependencies(
            &handle,
            [
                "assets/materials/stone.oxmat",
                "assets/materials/stone.oxmat",
                "assets/textures/stone.png",
            ],
        ));

        let dependencies = server.asset_dependencies(&handle).unwrap();
        assert_eq!(dependencies.len(), 2);
        assert!(dependencies.contains(&PathBuf::from("assets/materials/stone.oxmat")));
        assert!(dependencies.contains(&PathBuf::from("assets/textures/stone.png")));
    }

    #[test]
    fn changed_path_query_matches_source_and_dependencies_by_type() {
        let mut server = AssetServer::new();
        let scene =
            server.load_path_async("assets/scene.oxscene", |_| Ok::<_, AssetServerError>(1u32));
        let text = server.load_path_async("assets/scene.oxscene", |_| {
            Ok::<_, AssetServerError>("scene".to_string())
        });

        assert!(server.add_asset_dependency(&scene, "assets/materials/stone.oxmat"));
        assert!(server.add_asset_dependency(&text, "assets/materials/stone.oxmat"));

        assert_eq!(
            server.handles_for_changed_path::<u32>("assets/scene.oxscene"),
            vec![scene]
        );
        assert_eq!(
            server.handles_for_changed_path::<u32>("assets/materials/stone.oxmat"),
            vec![scene]
        );
        assert_eq!(
            server.handles_for_changed_path::<String>("assets/materials/stone.oxmat"),
            vec![text]
        );
    }

    #[test]
    fn dependency_updates_reject_wrong_handle_type() {
        let mut server = AssetServer::new();
        let handle =
            server.load_path_async("assets/number.asset", |_| Ok::<_, AssetServerError>(42u32));
        let wrong_type = Handle::<String>::new(handle.id());

        assert!(!server.add_asset_dependency(&wrong_type, "assets/ignored.txt"));
        assert!(server.asset_dependencies(&wrong_type).is_none());
        assert_eq!(
            server.asset_dependencies(&handle).unwrap(),
            &[] as &[PathBuf]
        );
    }
}
