//! Mesh renderer component

use oxide_ecs::Component;

use oxide_renderer::descriptor::MaterialType;
use oxide_renderer::mesh::Mesh3D;
use oxide_renderer::shader::BuiltinShader;

#[derive(Component)]
pub struct MeshRenderer {
    pub mesh: Mesh3D,
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
