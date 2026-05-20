//! Mesh renderer component

use std::collections::HashMap;

use oxide_ecs::{Component, Resource};

use oxide_renderer::descriptor::{MaterialDescriptor, MaterialType};
use oxide_renderer::mesh::Mesh3D;
use oxide_renderer::shader::BuiltinShader;

#[derive(Component)]
pub struct MeshRenderer {
    pub mesh: Mesh3D,
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
    },
    Named(String),
}

impl Default for RenderMaterial {
    fn default() -> Self {
        Self::Builtin {
            shader: BuiltinShader::Unlit,
            material_type: MaterialType::Unlit,
            name: "default_unlit".to_string(),
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
