//! Mesh renderer component

use oxide_ecs::Component;

use oxide_renderer::descriptor::MaterialType;
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
