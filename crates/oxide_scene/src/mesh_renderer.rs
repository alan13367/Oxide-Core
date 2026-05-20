//! Mesh renderer component

use std::collections::HashMap;

use oxide_asset::Handle;
use oxide_ecs::{Component, Resource};

use oxide_renderer::descriptor::{MaterialDescriptor, MaterialType};
use oxide_renderer::mesh::Mesh3D;
use oxide_renderer::shader::BuiltinShader;
use oxide_renderer::texture::TextureImage;

pub type MeshHandle = Handle<Mesh3D>;
pub type MaterialDescriptorHandle = Handle<MaterialDescriptor>;
pub type TextureImageHandle = Handle<TextureImage>;

/// Resource that caches GPU meshes by typed asset handle.
#[derive(Resource)]
pub struct MeshCache {
    meshes: oxide_asset::Assets<Mesh3D>,
}

impl MeshCache {
    pub fn new() -> Self {
        Self {
            meshes: oxide_asset::Assets::new(),
        }
    }

    pub fn insert(&mut self, handle: MeshHandle, mesh: Mesh3D) {
        self.meshes.insert(handle, mesh);
    }

    /// Returns the typed mesh asset storage backing this cache.
    pub fn assets(&self) -> &oxide_asset::Assets<Mesh3D> {
        &self.meshes
    }

    /// Returns mutable access to the typed mesh asset storage backing this cache.
    pub fn assets_mut(&mut self) -> &mut oxide_asset::Assets<Mesh3D> {
        &mut self.meshes
    }

    pub fn get(&self, handle: MeshHandle) -> Option<&Mesh3D> {
        self.meshes.get(&handle)
    }

    pub fn remove(&mut self, handle: MeshHandle) -> Option<Mesh3D> {
        self.meshes.remove(&handle)
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

impl Default for MeshCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource storing CPU-side texture images by typed asset handle and label.
#[derive(Resource, Default)]
pub struct TextureImageAssets {
    pub assets: oxide_asset::Assets<TextureImage>,
    labels: HashMap<String, TextureImageHandle>,
}

impl TextureImageAssets {
    /// Registers a texture image and associates it with a material-friendly label.
    pub fn insert_labeled(
        &mut self,
        label: impl Into<String>,
        handle: TextureImageHandle,
        image: TextureImage,
    ) {
        self.labels.insert(label.into(), handle);
        self.assets.insert(handle, image);
    }

    /// Returns an image by handle.
    pub fn get(&self, handle: TextureImageHandle) -> Option<&TextureImage> {
        self.assets.get(&handle)
    }

    /// Returns an image by label, accepting labels with or without a leading `#`.
    pub fn get_labeled(&self, label: &str) -> Option<&TextureImage> {
        let label = label.trim_start_matches('#');
        self.labels.get(label).and_then(|handle| self.get(*handle))
    }

    /// Returns the handle associated with a label.
    pub fn handle_for_label(&self, label: &str) -> Option<TextureImageHandle> {
        let label = label.trim_start_matches('#');
        self.labels.get(label).copied()
    }

    /// Iterates labeled texture images for renderer cache synchronization.
    pub fn iter_labeled(&self) -> impl Iterator<Item = (&str, TextureImageHandle, &TextureImage)> {
        self.labels.iter().filter_map(|(label, handle)| {
            self.assets
                .get(handle)
                .map(|image| (label.as_str(), *handle, image))
        })
    }
}

#[derive(Component)]
pub struct MeshRenderer {
    pub mesh: Mesh3D,
}

/// Component that references a cached mesh asset for rendering.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct MeshFilter {
    pub mesh: MeshHandle,
}

impl MeshFilter {
    pub fn new(mesh: MeshHandle) -> Self {
        Self { mesh }
    }
}

/// Component that references a CPU-side material descriptor for rendering.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct MaterialFilter {
    pub material: MaterialDescriptorHandle,
}

impl MaterialFilter {
    pub fn new(material: MaterialDescriptorHandle) -> Self {
        Self { material }
    }
}

/// Camera/renderable layer mask used by the automatic scene renderer.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderLayers {
    mask: u32,
}

impl Default for RenderLayers {
    fn default() -> Self {
        Self::layer(0)
    }
}

impl RenderLayers {
    /// Maximum number of renderer layers addressable by the bit mask.
    pub const MAX_LAYERS: u8 = 32;

    /// Creates a mask containing a single layer.
    pub fn layer(layer: u8) -> Self {
        assert!(layer < Self::MAX_LAYERS, "render layer index out of range");
        Self {
            mask: 1u32 << layer,
        }
    }

    /// Creates a layer mask from raw authored bits.
    pub fn from_mask(mask: u32) -> Self {
        Self { mask }
    }

    /// Creates a mask that intersects every layer.
    pub fn all() -> Self {
        Self { mask: u32::MAX }
    }

    /// Creates an empty mask that does not render through any camera.
    pub fn none() -> Self {
        Self { mask: 0 }
    }

    /// Returns the raw bit mask for serialization or diagnostics.
    pub fn mask(self) -> u32 {
        self.mask
    }

    /// Returns this mask with `layer` included.
    pub fn with_layer(mut self, layer: u8) -> Self {
        assert!(layer < Self::MAX_LAYERS, "render layer index out of range");
        self.mask |= 1u32 << layer;
        self
    }

    /// Returns this mask with `layer` removed.
    pub fn without_layer(mut self, layer: u8) -> Self {
        assert!(layer < Self::MAX_LAYERS, "render layer index out of range");
        self.mask &= !(1u32 << layer);
        self
    }

    /// Returns true when this mask contains `layer`.
    pub fn contains(self, layer: u8) -> bool {
        assert!(layer < Self::MAX_LAYERS, "render layer index out of range");
        self.mask & (1u32 << layer) != 0
    }

    /// Returns true when the two masks share at least one layer.
    pub fn intersects(self, other: Self) -> bool {
        self.mask & other.mask != 0
    }
}

/// CPU-side primitive request used by higher-level scene/prefab APIs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshPrimitive {
    #[default]
    Cube,
    Sphere {
        segments: u32,
        rings: u32,
    },
}

/// Material intent for renderer-facing scene data.
#[derive(Clone, Debug)]
pub enum RenderMaterial {
    Builtin {
        shader: BuiltinShader,
        material_type: MaterialType,
        name: String,
        base_color: [f32; 4],
        albedo_texture: Option<String>,
    },
    Named(String),
}

impl Default for RenderMaterial {
    fn default() -> Self {
        Self::Builtin {
            shader: BuiltinShader::Unlit,
            material_type: MaterialType::Unlit,
            name: "default_unlit".to_string(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            albedo_texture: None,
        }
    }
}

impl RenderMaterial {
    /// Creates a renderer material intent from a loaded material descriptor.
    pub fn from_descriptor(descriptor: &MaterialDescriptor) -> Self {
        let shader = match descriptor.material_type {
            MaterialType::Lit => BuiltinShader::Lit,
            MaterialType::Unlit => BuiltinShader::Unlit,
            MaterialType::Basic => BuiltinShader::Basic,
        };
        Self::Builtin {
            shader,
            material_type: descriptor.material_type,
            name: descriptor.name.clone(),
            base_color: descriptor.base_color,
            albedo_texture: descriptor.albedo_texture.clone(),
        }
    }

    /// Returns this material with a replacement base color.
    pub fn with_base_color(mut self, base_color: [f32; 4]) -> Self {
        if let Self::Builtin {
            base_color: color, ..
        } = &mut self
        {
            *color = base_color;
        }
        self
    }

    /// Returns the optional albedo texture reference resolved through a material library.
    pub fn albedo_texture_with_library<'a>(
        &'a self,
        library: Option<&'a SceneMaterialLibrary>,
    ) -> Option<&'a str> {
        if let Self::Named(name) = self {
            if let Some(resolved) = library.and_then(|library| library.get(name)) {
                return resolved.albedo_texture_with_library(None);
            }
        }

        match self {
            Self::Builtin { albedo_texture, .. } => albedo_texture.as_deref(),
            Self::Named(_) => None,
        }
    }

    /// Returns the material base color resolved through an optional library.
    pub fn base_color_with_library(&self, library: Option<&SceneMaterialLibrary>) -> [f32; 4] {
        if let Self::Named(name) = self {
            if let Some(resolved) = library.and_then(|library| library.get(name)) {
                return resolved.base_color_with_library(None);
            }
        }

        match self {
            Self::Builtin { base_color, .. } => *base_color,
            Self::Named(_) => [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// Reusable material intents addressable by `RenderMaterial::Named`.
#[derive(Resource, Clone, Debug, Default)]
pub struct SceneMaterialLibrary {
    materials: HashMap<String, RenderMaterial>,
}

impl SceneMaterialLibrary {
    /// Creates an empty scene material library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a named material intent.
    pub fn register(&mut self, name: impl Into<String>, material: RenderMaterial) {
        self.materials.insert(name.into(), material);
    }

    /// Registers a material descriptor under its descriptor name.
    pub fn register_descriptor(&mut self, descriptor: &MaterialDescriptor) {
        self.register(
            descriptor.name.clone(),
            RenderMaterial::from_descriptor(descriptor),
        );
    }

    /// Returns a registered material by name.
    pub fn get(&self, name: &str) -> Option<&RenderMaterial> {
        self.materials.get(name)
    }

    /// Returns true if a material with `name` is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.materials.contains_key(name)
    }

    /// Returns the number of registered materials.
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Returns true when the library has no registered materials.
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

/// High-level render component for scenes and templates.
///
/// This describes what should be rendered without forcing every gameplay app to
/// hold GPU meshes directly in components.
#[derive(Component, Clone, Debug)]
pub struct RenderMesh {
    pub primitive: MeshPrimitive,
    pub material: RenderMaterial,
    pub tint: [f32; 4],
}

impl Default for RenderMesh {
    fn default() -> Self {
        Self::new(MeshPrimitive::default(), RenderMaterial::default())
    }
}

impl RenderMesh {
    pub fn new(primitive: MeshPrimitive, material: RenderMaterial) -> Self {
        Self {
            primitive,
            material,
            tint: [0.85, 0.72, 0.48, 1.0],
        }
    }

    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }
}

impl MeshRenderer {
    pub fn new(mesh: Mesh3D) -> Self {
        Self { mesh }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_image_assets_resolve_labels_with_or_without_hash() {
        let mut assets = TextureImageAssets::default();
        let handle = TextureImageHandle::new(7);
        let image = TextureImage::from_rgba(1, 1, vec![1, 2, 3, 4]).unwrap();

        assets.insert_labeled("image_0", handle, image);

        assert_eq!(assets.handle_for_label("image_0"), Some(handle));
        assert_eq!(assets.handle_for_label("#image_0"), Some(handle));
        assert_eq!(
            assets
                .get_labeled("#image_0")
                .map(|image| image.rgba.as_slice()),
            Some([1, 2, 3, 4].as_slice())
        );
    }
}
