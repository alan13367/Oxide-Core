use std::any::{Any, TypeId};
use std::collections::HashMap;
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

pub struct AssetServer {
    allocator: HandleAllocator,
    pending: HashMap<u64, PendingAsset>,
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

        handle
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
                                let handle = Handle::new(id);
                                completed.push(Ok((handle, *asset)));
                            }
                            Err(_) => completed.push(Err(AssetServerError::TypeMismatch)),
                        },
                        Err(err) => completed.push(Err(err)),
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    let _ = self.pending.remove(&id);
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
}
