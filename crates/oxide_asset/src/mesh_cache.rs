//! GPU mesh cache for sharing meshes across entities.

use oxide_renderer::mesh::Mesh3D;

use super::{Assets, Handle};

/// Resource that caches GPU meshes by handle.
pub struct MeshCache {
    meshes: Assets<Mesh3D>,
}

impl MeshCache {
    /// Creates an empty mesh cache.
    pub fn new() -> Self {
        Self {
            meshes: Assets::new(),
        }
    }

    /// Inserts a mesh with the given handle.
    pub fn insert(&mut self, handle: Handle<Mesh3D>, mesh: Mesh3D) {
        self.meshes.insert(handle, mesh);
    }

    /// Gets a mesh by handle.
    pub fn get(&self, handle: Handle<Mesh3D>) -> Option<&Mesh3D> {
        self.meshes.get(&handle)
    }

    /// Removes a mesh by handle.
    pub fn remove(&mut self, handle: Handle<Mesh3D>) -> Option<Mesh3D> {
        self.meshes.remove(&handle)
    }

    /// Returns the number of cached meshes.
    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

impl Default for MeshCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Component that references a mesh and material for rendering.
#[derive(Clone, Debug)]
pub struct MeshFilter {
    /// Handle to the cached mesh.
    pub mesh: Handle<Mesh3D>,
}

impl MeshFilter {
    /// Creates a new mesh filter.
    pub fn new(mesh: Handle<Mesh3D>) -> Self {
        Self { mesh }
    }
}
