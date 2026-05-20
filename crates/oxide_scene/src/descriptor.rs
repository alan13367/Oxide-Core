//! Data-driven scene descriptors for small games and examples.

use std::path::Path;

use glam::{Quat, Vec3};
use oxide_camera::{CameraComponent, CameraController};
use oxide_ecs::prelude::{Entity, World};
use oxide_ecs::{Component, Resource};
use oxide_light::{AmbientLight, DirectionalLight, PointLight};
use oxide_math::transform::Transform;
use oxide_transform::{GlobalTransform, TransformComponent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{MeshPrimitive, RenderMaterial, RenderMesh};

pub const OXSCENE_FORMAT: &str = "oxide.oxscene";
pub const OXSCENE_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OxSceneDocument {
    pub format: String,
    pub version: u32,
    pub scene: SceneDescriptor,
}

impl OxSceneDocument {
    pub fn new(scene: SceneDescriptor) -> Self {
        Self {
            format: OXSCENE_FORMAT.to_string(),
            version: OXSCENE_VERSION,
            scene,
        }
    }

    pub fn validate(self, path: String) -> Result<SceneDescriptor, SceneDescriptorError> {
        if self.format != OXSCENE_FORMAT || self.version != OXSCENE_VERSION {
            return Err(SceneDescriptorError::UnsupportedVersion {
                path,
                format: self.format,
                version: self.version,
            });
        }

        Ok(self.scene)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name(pub String);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneDescriptor {
    #[serde(default)]
    pub entities: Vec<SceneEntityDescriptor>,
}

impl SceneDescriptor {
    pub fn starter_scene() -> Self {
        Self {
            entities: vec![
                SceneEntityDescriptor {
                    name: Some("Camera".to_string()),
                    transform: SceneTransform::from_position([0.0, 2.0, 6.0]),
                    kind: SceneEntityKind::Camera {
                        target: [0.0, 0.0, 0.0],
                        controller: true,
                    },
                },
                SceneEntityDescriptor {
                    name: Some("Key Light".to_string()),
                    kind: SceneEntityKind::DirectionalLight {
                        direction: [0.8, -1.0, -0.4],
                        color: [1.0, 0.96, 0.88],
                        intensity: 0.8,
                    },
                    ..Default::default()
                },
                SceneEntityDescriptor {
                    name: Some("Ambient".to_string()),
                    kind: SceneEntityKind::AmbientLight {
                        color: [0.45, 0.48, 0.55],
                        intensity: 0.25,
                    },
                    ..Default::default()
                },
                SceneEntityDescriptor {
                    name: Some("Cube".to_string()),
                    kind: SceneEntityKind::Mesh {
                        primitive: SceneMeshPrimitive::Cube,
                        material: SceneMaterialDescriptor::default(),
                    },
                    ..Default::default()
                },
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneEntityDescriptor {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub transform: SceneTransform,
    #[serde(flatten)]
    pub kind: SceneEntityKind,
}

impl Default for SceneEntityDescriptor {
    fn default() -> Self {
        Self {
            name: None,
            transform: SceneTransform::default(),
            kind: SceneEntityKind::Empty,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneEntityKind {
    Empty,
    Camera {
        #[serde(default = "default_camera_target")]
        target: [f32; 3],
        #[serde(default)]
        controller: bool,
    },
    AmbientLight {
        #[serde(default = "default_one_vec3")]
        color: [f32; 3],
        #[serde(default = "default_ambient_intensity")]
        intensity: f32,
    },
    DirectionalLight {
        #[serde(default = "default_light_direction")]
        direction: [f32; 3],
        #[serde(default = "default_one_vec3")]
        color: [f32; 3],
        #[serde(default = "default_light_intensity")]
        intensity: f32,
    },
    PointLight {
        #[serde(default = "default_one_vec3")]
        color: [f32; 3],
        #[serde(default = "default_light_intensity")]
        intensity: f32,
        #[serde(default = "default_point_radius")]
        radius: f32,
    },
    Mesh {
        #[serde(default)]
        primitive: SceneMeshPrimitive,
        #[serde(default)]
        material: SceneMaterialDescriptor,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneTransform {
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default = "default_rotation")]
    pub rotation: [f32; 4],
    #[serde(default = "default_one_vec3")]
    pub scale: [f32; 3],
}

impl Default for SceneTransform {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: default_rotation(),
            scale: default_one_vec3(),
        }
    }
}

impl SceneTransform {
    pub fn from_position(position: [f32; 3]) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }
}

impl From<SceneTransform> for Transform {
    fn from(value: SceneTransform) -> Self {
        Self {
            position: vec3(value.position),
            rotation: Quat::from_xyzw(
                value.rotation[0],
                value.rotation[1],
                value.rotation[2],
                value.rotation[3],
            ),
            scale: vec3(value.scale),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneMeshPrimitive {
    #[default]
    Cube,
    Sphere,
}

impl From<SceneMeshPrimitive> for MeshPrimitive {
    fn from(value: SceneMeshPrimitive) -> Self {
        match value {
            SceneMeshPrimitive::Cube => MeshPrimitive::Cube,
            SceneMeshPrimitive::Sphere => MeshPrimitive::Sphere {
                segments: 16,
                rings: 16,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneMaterialDescriptor {
    #[serde(default = "default_material_name")]
    pub name: String,
    #[serde(default)]
    pub shader: SceneBuiltinShader,
    #[serde(default = "default_material_color")]
    pub color: [f32; 4],
}

impl Default for SceneMaterialDescriptor {
    fn default() -> Self {
        Self {
            name: default_material_name(),
            shader: SceneBuiltinShader::Unlit,
            color: default_material_color(),
        }
    }
}

impl From<SceneMaterialDescriptor> for RenderMaterial {
    fn from(value: SceneMaterialDescriptor) -> Self {
        RenderMaterial::Builtin {
            shader: value.shader.into(),
            material_type: value.shader.material_type(),
            name: value.name,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneBuiltinShader {
    Lit,
    #[default]
    Unlit,
}

impl SceneBuiltinShader {
    fn material_type(self) -> oxide_renderer::descriptor::MaterialType {
        match self {
            SceneBuiltinShader::Lit => oxide_renderer::descriptor::MaterialType::Lit,
            SceneBuiltinShader::Unlit => oxide_renderer::descriptor::MaterialType::Unlit,
        }
    }
}

impl From<SceneBuiltinShader> for oxide_renderer::shader::BuiltinShader {
    fn from(value: SceneBuiltinShader) -> Self {
        match value {
            SceneBuiltinShader::Lit => Self::Lit,
            SceneBuiltinShader::Unlit => Self::Unlit,
        }
    }
}

#[derive(Resource, Default)]
pub struct SceneSpawnResult {
    pub entities: Vec<Entity>,
}

#[derive(thiserror::Error, Debug)]
pub enum SceneDescriptorError {
    #[error("Failed to read scene descriptor '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("Failed to parse scene descriptor '{path}': {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error(
        "Unsupported scene descriptor version for '{path}': format '{format}' version {version}"
    )]
    UnsupportedVersion {
        path: String,
        format: String,
        version: u32,
    },
}

pub fn load_scene_descriptor(
    path: impl AsRef<Path>,
) -> Result<SceneDescriptor, SceneDescriptorError> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path).map_err(|source| SceneDescriptorError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let value =
        serde_json::from_str::<Value>(&raw).map_err(|source| SceneDescriptorError::Parse {
            path: path.display().to_string(),
            source,
        })?;
    scene_descriptor_from_value(value, path.display().to_string())
}

pub fn save_scene_descriptor(
    path: impl AsRef<Path>,
    scene: &SceneDescriptor,
) -> Result<(), SceneDescriptorError> {
    let path = path.as_ref();
    let document = OxSceneDocument::new(scene.clone());
    let raw =
        serde_json::to_string_pretty(&document).map_err(|source| SceneDescriptorError::Parse {
            path: path.display().to_string(),
            source,
        })?;
    std::fs::write(path, raw).map_err(|source| SceneDescriptorError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn scene_descriptor_from_value(
    value: Value,
    path: String,
) -> Result<SceneDescriptor, SceneDescriptorError> {
    let is_wrapped = value
        .get("format")
        .and_then(Value::as_str)
        .map(|format| format == OXSCENE_FORMAT)
        .unwrap_or(false)
        || value.get("scene").is_some() && value.get("version").is_some();

    if is_wrapped {
        let document = serde_json::from_value::<OxSceneDocument>(value).map_err(|source| {
            SceneDescriptorError::Parse {
                path: path.clone(),
                source,
            }
        })?;
        document.validate(path)
    } else {
        serde_json::from_value::<SceneDescriptor>(value)
            .map_err(|source| SceneDescriptorError::Parse { path, source })
    }
}

pub fn spawn_scene_descriptor(world: &mut World, scene: &SceneDescriptor) -> Vec<Entity> {
    scene
        .entities
        .iter()
        .map(|entity| spawn_scene_entity(world, entity))
        .collect()
}

fn spawn_scene_entity(world: &mut World, descriptor: &SceneEntityDescriptor) -> Entity {
    let transform = Transform::from(descriptor.transform);
    let mut entity_mut = world.spawn((
        TransformComponent::new(transform),
        GlobalTransform::default(),
    ));

    if let Some(name) = &descriptor.name {
        entity_mut.insert(Name(name.clone()));
    }

    let entity = entity_mut.id();

    match &descriptor.kind {
        SceneEntityKind::Empty => {}
        SceneEntityKind::Camera { target, controller } => {
            let mut camera = CameraComponent::new();
            camera.0.position = transform.position;
            camera.0.target = vec3(*target);
            world.entity_mut(entity).insert(camera);
            if *controller {
                world.entity_mut(entity).insert(CameraController::new());
            }
        }
        SceneEntityKind::AmbientLight { color, intensity } => {
            world
                .entity_mut(entity)
                .insert(AmbientLight::new(vec3(*color), *intensity));
        }
        SceneEntityKind::DirectionalLight {
            direction,
            color,
            intensity,
        } => {
            world.entity_mut(entity).insert(DirectionalLight::new(
                vec3(*direction),
                vec3(*color),
                *intensity,
            ));
        }
        SceneEntityKind::PointLight {
            color,
            intensity,
            radius,
        } => {
            world.entity_mut(entity).insert(PointLight::new(
                transform.position,
                vec3(*color),
                *intensity,
                *radius,
            ));
        }
        SceneEntityKind::Mesh {
            primitive,
            material,
        } => {
            world.entity_mut(entity).insert(
                RenderMesh::new((*primitive).into(), material.clone().into())
                    .with_tint(material.color),
            );
        }
    }

    entity
}

fn vec3(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn default_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_one_vec3() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_camera_target() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

fn default_light_direction() -> [f32; 3] {
    [0.0, -1.0, 0.0]
}

fn default_light_intensity() -> f32 {
    1.0
}

fn default_ambient_intensity() -> f32 {
    0.2
}

fn default_point_radius() -> f32 {
    10.0
}

fn default_material_name() -> String {
    "default_unlit".to_string()
}

fn default_material_color() -> [f32; 4] {
    [0.85, 0.72, 0.48, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn starter_scene_spawns_gameplay_entities() {
        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &SceneDescriptor::starter_scene());
        assert_eq!(roots.len(), 4);

        let mut cameras = world.query::<&CameraComponent>();
        assert_eq!(cameras.iter(&world).count(), 1);

        let mut meshes = world.query::<&RenderMesh>();
        assert_eq!(meshes.iter(&world).count(), 1);
    }

    #[test]
    fn load_scene_descriptor_accepts_legacy_raw_scene_json() {
        let path = temp_path("legacy_scene", "json");
        fs::write(
            &path,
            r#"{
                "entities": [
                    {
                        "name": "Legacy Cube",
                        "type": "mesh",
                        "primitive": "cube"
                    }
                ]
            }"#,
        )
        .unwrap();

        let scene = load_scene_descriptor(&path).unwrap();
        assert_eq!(scene.entities.len(), 1);
        assert_eq!(scene.entities[0].name.as_deref(), Some("Legacy Cube"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_scene_descriptor_accepts_wrapped_oxscene_json() {
        let path = temp_path("wrapped_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 1,
                "scene": {
                    "entities": [
                        {
                            "name": "Wrapped Cube",
                            "type": "mesh",
                            "primitive": "cube"
                        }
                    ]
                }
            }"#,
        )
        .unwrap();

        let scene = load_scene_descriptor(&path).unwrap();
        assert_eq!(scene.entities.len(), 1);
        assert_eq!(scene.entities[0].name.as_deref(), Some("Wrapped Cube"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_scene_descriptor_rejects_unsupported_oxscene_version() {
        let path = temp_path("future_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 99,
                "scene": { "entities": [] }
            }"#,
        )
        .unwrap();

        let err = load_scene_descriptor(&path).unwrap_err();
        assert!(matches!(
            err,
            SceneDescriptorError::UnsupportedVersion { .. }
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_scene_descriptor_writes_wrapped_roundtrip_document() {
        let path = temp_path("roundtrip_scene", "oxscene");
        let scene = SceneDescriptor::starter_scene();
        save_scene_descriptor(&path, &scene).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"format\": \"oxide.oxscene\""));
        assert!(raw.contains("\"version\": 1"));

        let loaded = load_scene_descriptor(&path).unwrap();
        assert_eq!(loaded.entities.len(), scene.entities.len());
        let _ = fs::remove_file(path);
    }

    fn temp_path(name: &str, extension: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{stamp}.{extension}"))
    }
}
