//! Data-driven scene descriptors for small games and examples.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use glam::{Quat, Vec2, Vec3};
use oxide_camera::{CameraComponent, CameraController, CameraRenderView, CameraViewport};
use oxide_ecs::prelude::{Entity, World};
use oxide_ecs::{Component, Resource};
use oxide_light::{AmbientLight, DirectionalLight, PointLight};
use oxide_math::transform::Transform;
use oxide_transform::{
    attach_child, detach_child, Children, GlobalTransform, Parent, TransformComponent, Visibility,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    MeshPrimitive, RenderLayers, RenderMaterial, RenderMesh, SceneMaterialLibrary, SpriteBillboard,
    SpriteDepthMode, SpriteFacing, SpriteId,
};

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

        self.scene
            .validate()
            .map_err(|source| SceneDescriptorError::Validation { path, source })?;

        Ok(self.scene)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name(pub String);

/// Stable slash-separated authored path assigned when a scene or prefab spawns.
///
/// Named entities use their trimmed `Name` as the path segment. Unnamed sibling
/// entities use `#index`, matching prefab override path syntax.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEntityPath(pub String);

impl SceneEntityPath {
    /// Returns the authored path string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Stable identifier assigned to all entities created by one scene or prefab spawn.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneInstanceId(u64);

impl SceneInstanceId {
    /// Returns the raw monotonically assigned instance ID.
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Resource, Default)]
struct SceneInstanceCounter {
    next: u64,
}

/// Stable authored labels for gameplay queries, editor filters, and tooling.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tags(Vec<String>);

impl Tags {
    /// Creates a tag set, trimming whitespace and removing duplicate labels.
    pub fn new(tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut values = Vec::new();
        for tag in tags {
            let tag = tag.into().trim().to_string();
            if !tag.is_empty() && !values.contains(&tag) {
                values.push(tag);
            }
        }
        Self(values)
    }

    /// Returns true if this entity has `tag`.
    pub fn contains(&self, tag: &str) -> bool {
        let tag = tag.trim();
        self.0.iter().any(|candidate| candidate == tag)
    }

    /// Returns all authored tags in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    /// Returns the number of tags.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no tags are present.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Returns true when `entity` has a [`Tags`] component containing `tag`.
pub fn entity_has_tag(world: &World, entity: Entity, tag: &str) -> bool {
    world
        .get::<Tags>(entity)
        .is_some_and(|tags| tags.contains(tag))
}

/// Returns every entity with a [`Tags`] component containing `tag`.
///
/// Results follow the ECS world's query iteration order. Use stable authored
/// names or hierarchy relationships when gameplay requires deterministic
/// ordering among multiple matching entities. Use
/// [`entities_with_tag_in_instance`] for repeated scene instances.
pub fn entities_with_tag(world: &mut World, tag: &str) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &Tags)>();
    query
        .iter(world)
        .filter_map(|(entity, tags)| tags.contains(tag).then_some(entity))
        .collect()
}

/// Returns every entity with `tag` inside `instance_id`.
pub fn entities_with_tag_in_instance(
    world: &mut World,
    instance_id: SceneInstanceId,
    tag: &str,
) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &Tags)>();
    query
        .iter(world)
        .filter_map(|(entity, tags)| {
            (tags.contains(tag) && scene_instance_id(world, entity) == Some(instance_id))
                .then_some(entity)
        })
        .collect()
}

/// Returns the first entity with a [`Tags`] component containing `tag`.
///
/// This is a convenience for singleton markers such as `player_spawn`. Prefer
/// [`entities_with_tag`] when multiple authored entities may share the tag. Use
/// [`first_entity_with_tag_in_instance`] for repeated scene instances.
pub fn first_entity_with_tag(world: &mut World, tag: &str) -> Option<Entity> {
    let mut query = world.query::<(Entity, &Tags)>();
    query
        .iter(world)
        .find_map(|(entity, tags)| tags.contains(tag).then_some(entity))
}

/// Returns the first entity with `tag` inside `instance_id`.
pub fn first_entity_with_tag_in_instance(
    world: &mut World,
    instance_id: SceneInstanceId,
    tag: &str,
) -> Option<Entity> {
    let mut query = world.query::<(Entity, &Tags)>();
    query.iter(world).find_map(|(entity, tags)| {
        (tags.contains(tag) && scene_instance_id(world, entity) == Some(instance_id))
            .then_some(entity)
    })
}

/// Returns the authored scene path for `entity`, if it was spawned from a scene descriptor.
pub fn scene_entity_path(world: &World, entity: Entity) -> Option<&str> {
    world
        .get::<SceneEntityPath>(entity)
        .map(SceneEntityPath::as_str)
}

/// Returns the scene instance ID for `entity`, if it was spawned from a scene descriptor.
pub fn scene_instance_id(world: &World, entity: Entity) -> Option<SceneInstanceId> {
    world.get::<SceneInstanceId>(entity).copied()
}

/// Returns the entity with the exact authored scene `path`.
///
/// If multiple scene instances contain the same path, the first match in ECS
/// query order is returned. Use [`entity_by_scene_path_in_instance`] when code
/// has a specific loaded scene or prefab instance.
pub fn entity_by_scene_path(world: &mut World, path: &str) -> Option<Entity> {
    let path = normalize_scene_entity_path(path);
    let mut query = world.query::<(Entity, &SceneEntityPath)>();
    query
        .iter(world)
        .find_map(|(entity, entity_path)| (entity_path.as_str() == path).then_some(entity))
}

/// Returns the entity with the exact authored scene `path` inside `instance_id`.
pub fn entity_by_scene_path_in_instance(
    world: &mut World,
    instance_id: SceneInstanceId,
    path: &str,
) -> Option<Entity> {
    let path = normalize_scene_entity_path(path);
    let mut query = world.query::<(Entity, &SceneEntityPath)>();
    query.iter(world).find_map(|(entity, entity_path)| {
        (entity_path.as_str() == path && scene_instance_id(world, entity) == Some(instance_id))
            .then_some(entity)
    })
}

/// Returns every entity at or below the authored scene `path`.
///
/// This is useful for attaching gameplay state to a loaded prefab or scene
/// subtree without manually traversing `Children`.
///
/// If multiple scene instances contain the same path, entities from each
/// matching instance are returned. Use [`entities_under_scene_path_in_instance`]
/// when code has a specific loaded scene or prefab instance.
pub fn entities_under_scene_path(world: &mut World, path: &str) -> Vec<Entity> {
    let path = normalize_scene_entity_path(path);
    if path.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{path}/");
    let mut query = world.query::<(Entity, &SceneEntityPath)>();
    query
        .iter(world)
        .filter_map(|(entity, entity_path)| {
            let entity_path = entity_path.as_str();
            (entity_path == path || entity_path.starts_with(prefix.as_str())).then_some(entity)
        })
        .collect()
}

/// Returns every entity at or below `path` inside `instance_id`.
pub fn entities_under_scene_path_in_instance(
    world: &mut World,
    instance_id: SceneInstanceId,
    path: &str,
) -> Vec<Entity> {
    let path = normalize_scene_entity_path(path);
    if path.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{path}/");
    let mut query = world.query::<(Entity, &SceneEntityPath)>();
    query
        .iter(world)
        .filter_map(|(entity, entity_path)| {
            let entity_path = entity_path.as_str();
            let path_matches = entity_path == path || entity_path.starts_with(prefix.as_str());
            (path_matches && scene_instance_id(world, entity) == Some(instance_id))
                .then_some(entity)
        })
        .collect()
}

/// Returns every currently alive entity in `instance_id`.
pub fn entities_in_scene_instance(world: &mut World, instance_id: SceneInstanceId) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &SceneInstanceId)>();
    query
        .iter(world)
        .filter_map(|(entity, id)| (*id == instance_id).then_some(entity))
        .collect()
}

/// Despawns all entities in `instance_id` and detaches external hierarchy links.
///
/// Entities outside the scene instance are preserved. If an external entity was
/// parented to a despawned scene entity, it is detached before the scene entity
/// is removed.
pub fn despawn_scene_instance(world: &mut World, instance_id: SceneInstanceId) -> Vec<Entity> {
    let entities = entities_in_scene_instance(world, instance_id);
    if entities.is_empty() {
        return Vec::new();
    }

    for entity in &entities {
        if let Some(parent) = world.get::<Parent>(*entity).copied() {
            if !entities.contains(&parent.0) && world.contains(parent.0) {
                detach_child(world, parent.0, *entity);
            }
        }

        if let Some(children) = world.get::<Children>(*entity).cloned() {
            for child in children.iter() {
                if !entities.contains(&child) && world.contains(child) {
                    detach_child(world, *entity, child);
                }
            }
        }
    }

    let mut despawned = Vec::new();
    for entity in entities {
        if world.despawn(entity) {
            despawned.push(entity);
        }
    }
    despawned
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneDescriptor {
    /// Extra source paths that should invalidate this scene during hot reload.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Reusable material intents registered before scene entities are spawned.
    #[serde(default)]
    pub materials: Vec<SceneMaterialDescriptor>,
    /// Reusable entity templates that can be instantiated by prefab entities.
    #[serde(default)]
    pub prefabs: Vec<ScenePrefabDescriptor>,
    /// Root entities spawned into the world.
    #[serde(default)]
    pub entities: Vec<SceneEntityDescriptor>,
}

impl SceneDescriptor {
    pub fn starter_scene() -> Self {
        Self {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![
                SceneEntityDescriptor {
                    name: Some("Camera".to_string()),
                    transform: SceneTransform::from_position([0.0, 2.0, 6.0]),
                    kind: SceneEntityKind::Camera {
                        target: [0.0, 0.0, 0.0],
                        controller: true,
                        order: 0,
                        active: true,
                        viewport: None,
                        clear_color: None,
                    },
                    children: Vec::new(),
                    ..Default::default()
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

    /// Returns the prefab definition with the provided stable ID.
    pub fn prefab(&self, id: &str) -> Option<&ScenePrefabDescriptor> {
        self.prefabs.iter().find(|prefab| prefab.id == id)
    }

    /// Validates prefab references, prefab IDs, and authored entity payloads.
    ///
    /// Use this before saving or spawning editor-authored descriptors when the
    /// caller wants structured diagnostics instead of best-effort spawning.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        let diagnostics = self.validation_diagnostics();
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(SceneValidationError { diagnostics })
        }
    }

    /// Returns all validation diagnostics without short-circuiting on the first
    /// problem. Diagnostics include stable descriptor paths that can be surfaced
    /// in authoring tools.
    pub fn validation_diagnostics(&self) -> Vec<SceneValidationDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen_dependencies = HashSet::new();
        let mut seen_materials = HashSet::new();
        let mut seen_prefabs = HashSet::new();
        let mut prefabs = HashMap::new();

        for (index, dependency) in self.dependencies.iter().enumerate() {
            let path = format!("dependencies[{index}]");
            if dependency.trim().is_empty() {
                diagnostics.push(SceneValidationDiagnostic::new(
                    path,
                    "scene dependency paths must not be empty",
                ));
                continue;
            }
            if !seen_dependencies.insert(dependency.trim()) {
                diagnostics.push(SceneValidationDiagnostic::new(
                    path,
                    format!("duplicate scene dependency path '{}'", dependency.trim()),
                ));
            }
        }

        for (index, material) in self.materials.iter().enumerate() {
            let path = format!("materials[{index}]");
            if material.name.trim().is_empty() {
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.name"),
                    "material names must not be empty",
                ));
                continue;
            }
            if !seen_materials.insert(material.name.as_str()) {
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.name"),
                    format!("duplicate material name '{}'", material.name),
                ));
            }
        }

        for (index, prefab) in self.prefabs.iter().enumerate() {
            let path = format!("prefabs[{index}]");
            if prefab.id.trim().is_empty() {
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.id"),
                    "prefab IDs must not be empty",
                ));
                continue;
            }
            if !seen_prefabs.insert(prefab.id.as_str()) {
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.id"),
                    format!("duplicate prefab ID '{}'", prefab.id),
                ));
                continue;
            }
            prefabs.insert(prefab.id.as_str(), prefab);
        }

        let mut prefab_stack = Vec::new();
        for (index, entity) in self.entities.iter().enumerate() {
            validate_scene_entity(
                entity,
                format!("entities[{index}]"),
                &prefabs,
                &mut prefab_stack,
                &mut diagnostics,
            );
        }
        for prefab in self
            .prefabs
            .iter()
            .filter(|prefab| !prefab.id.trim().is_empty())
        {
            prefab_stack.push(prefab.id.clone());
            for (index, entity) in prefab.entities.iter().enumerate() {
                validate_scene_entity(
                    entity,
                    format!("prefabs['{}'].entities[{index}]", prefab.id),
                    &prefabs,
                    &mut prefab_stack,
                    &mut diagnostics,
                );
            }
            let _ = prefab_stack.pop();
        }

        diagnostics
    }

    fn prefab_lookup(&self) -> HashMap<&str, &ScenePrefabDescriptor> {
        self.prefabs
            .iter()
            .map(|prefab| (prefab.id.as_str(), prefab))
            .collect()
    }
}

/// A reusable set of scene entities that can be instantiated by ID.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenePrefabDescriptor {
    /// Stable prefab identifier used by `SceneEntityKind::Prefab`.
    pub id: String,
    /// Root entities that make up this prefab.
    #[serde(default)]
    pub entities: Vec<SceneEntityDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneEntityDescriptor {
    #[serde(default)]
    pub name: Option<String>,
    /// Stable authored labels inserted as a `Tags` component.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub transform: SceneTransform,
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Optional raw `RenderLayers` mask for camera/renderable filtering.
    #[serde(default)]
    pub render_layers: Option<u32>,
    #[serde(flatten)]
    pub kind: SceneEntityKind,
    /// Child entities attached under this entity with local transforms.
    #[serde(default)]
    pub children: Vec<SceneEntityDescriptor>,
}

impl Default for SceneEntityDescriptor {
    fn default() -> Self {
        Self {
            name: None,
            tags: Vec::new(),
            transform: SceneTransform::default(),
            visible: true,
            render_layers: None,
            kind: SceneEntityKind::Empty,
            children: Vec::new(),
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
        /// Lower values are preferred by the automatic scene renderer.
        #[serde(default)]
        order: i32,
        /// Disabled cameras stay in the world but are ignored for rendering.
        #[serde(default = "default_visible")]
        active: bool,
        /// Optional normalized viewport `[x, y, width, height]`.
        #[serde(default)]
        viewport: Option<[f32; 4]>,
        /// Optional per-camera clear color used by the automatic renderer.
        #[serde(default)]
        clear_color: Option<[f64; 4]>,
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
    Sprite {
        #[serde(flatten)]
        sprite: SceneSpriteDescriptor,
    },
    Prefab {
        /// Prefab ID from `SceneDescriptor::prefabs`.
        id: String,
        /// Per-instance changes applied to named prefab entities before spawn.
        #[serde(default)]
        overrides: Vec<ScenePrefabOverride>,
    },
}

/// Per-instance changes for an entity inside a prefab instance.
///
/// `path` is a slash-separated path of prefab entity names, such as
/// `"Crate Base/Crate Top"`. Unnamed entities can be targeted by their sibling
/// index segment, such as `"#0/#1"`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScenePrefabOverride {
    /// Slash-separated path to a prefab entity.
    pub path: String,
    /// Optional replacement local transform.
    #[serde(default)]
    pub transform: Option<SceneTransform>,
    /// Optional replacement visibility flag.
    #[serde(default)]
    pub visible: Option<bool>,
    /// Optional replacement render layer mask.
    #[serde(default)]
    pub render_layers: Option<u32>,
    /// Optional replacement mesh material.
    #[serde(default)]
    pub material: Option<SceneMaterialDescriptor>,
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

/// Serializable sprite billboard descriptor for `.oxscene` entities.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneSpriteDescriptor {
    pub sprite: String,
    #[serde(default = "default_sprite_size")]
    pub size: [f32; 2],
    #[serde(default = "default_sprite_tint")]
    pub tint: [f32; 4],
    #[serde(default)]
    pub facing: SceneSpriteFacing,
    #[serde(default)]
    pub depth: SceneSpriteDepthMode,
}

impl From<SceneSpriteDescriptor> for SpriteBillboard {
    fn from(value: SceneSpriteDescriptor) -> Self {
        SpriteBillboard::new(value.sprite, Vec2::new(value.size[0], value.size[1]))
            .with_tint(value.tint)
            .with_facing(value.facing.into())
            .with_depth(value.depth.into())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneSpriteFacing {
    #[default]
    YBillboard,
    Camera,
    Fixed,
}

impl From<SceneSpriteFacing> for SpriteFacing {
    fn from(value: SceneSpriteFacing) -> Self {
        match value {
            SceneSpriteFacing::YBillboard => SpriteFacing::YBillboard,
            SceneSpriteFacing::Camera => SpriteFacing::Camera,
            SceneSpriteFacing::Fixed => SpriteFacing::Fixed,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneSpriteDepthMode {
    #[default]
    World,
    Overlay,
}

impl From<SceneSpriteDepthMode> for SpriteDepthMode {
    fn from(value: SceneSpriteDepthMode) -> Self {
        match value {
            SceneSpriteDepthMode::World => SpriteDepthMode::World,
            SceneSpriteDepthMode::Overlay => SpriteDepthMode::Overlay,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneMaterialDescriptor {
    /// Optional `SceneMaterialLibrary` key to use instead of inline shader data.
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
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
            reference: None,
            name: default_material_name(),
            shader: SceneBuiltinShader::Unlit,
            color: default_material_color(),
        }
    }
}

impl From<SceneMaterialDescriptor> for RenderMaterial {
    fn from(value: SceneMaterialDescriptor) -> Self {
        if let Some(reference) = value.reference {
            return RenderMaterial::Named(reference);
        }
        RenderMaterial::Builtin {
            shader: value.shader.into(),
            material_type: value.shader.material_type(),
            name: value.name,
            base_color: [1.0, 1.0, 1.0, 1.0],
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

/// Entities created by one scene or prefab spawn operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnedSceneInstance {
    /// Stable ID shared by every entity spawned for this instance.
    pub id: SceneInstanceId,
    /// Root entities created by the spawn operation.
    pub roots: Vec<Entity>,
}

impl SpawnedSceneInstance {
    /// Returns the first spawned root, if the spawn operation created one.
    pub fn root(&self) -> Option<Entity> {
        self.roots.first().copied()
    }
}

/// One authored-scene validation issue with a descriptor path and message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneValidationDiagnostic {
    pub path: String,
    pub message: String,
}

impl SceneValidationDiagnostic {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Collection of validation diagnostics for a scene descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneValidationError {
    diagnostics: Vec<SceneValidationDiagnostic>,
}

impl SceneValidationError {
    /// Returns the diagnostics that explain why validation failed.
    pub fn diagnostics(&self) -> &[SceneValidationDiagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for SceneValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.diagnostics.is_empty() {
            return write!(f, "scene descriptor validation failed");
        }

        write!(
            f,
            "scene descriptor validation failed with {} issue(s)",
            self.diagnostics.len()
        )?;
        for diagnostic in &self.diagnostics {
            write!(f, "; {}: {}", diagnostic.path, diagnostic.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for SceneValidationError {}

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
    #[error("Invalid scene descriptor '{path}': {source}")]
    Validation {
        path: String,
        source: SceneValidationError,
    },
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum SceneExportError {
    #[error("Cannot export prefab: prefab ID must not be empty")]
    EmptyPrefabId,
    #[error("Cannot export entity {entity:?}: missing TransformComponent")]
    MissingTransform { entity: Entity },
    #[error(
        "Cannot export entity {entity:?}: entity appears more than once in the exported roots"
    )]
    DuplicateEntity { entity: Entity },
    #[error("Cannot export entity {entity:?}: hierarchy cycle detected")]
    HierarchyCycle { entity: Entity },
    #[error("Cannot export entity {entity:?}: built-in shader {shader} is not representable in SceneBuiltinShader")]
    UnsupportedMaterialShader { entity: Entity, shader: String },
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
        let scene = serde_json::from_value::<SceneDescriptor>(value).map_err(|source| {
            SceneDescriptorError::Parse {
                path: path.clone(),
                source,
            }
        })?;
        scene
            .validate()
            .map_err(|source| SceneDescriptorError::Validation { path, source })?;
        Ok(scene)
    }
}

/// Builds a scene descriptor from every root entity with a [`TransformComponent`].
///
/// Root entities are transform-bearing entities without a [`Parent`]. Children
/// are exported recursively using the stored hierarchy order. This is intended
/// for editor save flows and code-first tools that need to persist live Oxide
/// scene data back into `.oxscene` documents.
pub fn scene_descriptor_from_world(world: &mut World) -> Result<SceneDescriptor, SceneExportError> {
    let mut query = world.query::<(Entity, &TransformComponent)>();
    let mut roots: Vec<_> = query
        .iter(world)
        .filter_map(|(entity, _)| world.get::<Parent>(entity).is_none().then_some(entity))
        .collect();
    roots.sort_by_key(|entity| (entity.index(), entity.generation()));
    scene_descriptor_from_roots(world, roots)
}

/// Builds a scene descriptor from the provided root entities.
///
/// Each root must have a [`TransformComponent`]. Descendants are exported
/// recursively when they appear in [`Children`].
pub fn scene_descriptor_from_roots(
    world: &World,
    roots: impl IntoIterator<Item = Entity>,
) -> Result<SceneDescriptor, SceneExportError> {
    Ok(SceneDescriptor {
        entities: scene_entities_from_roots(world, roots)?,
        ..Default::default()
    })
}

/// Builds a reusable prefab descriptor from the provided root entities.
///
/// The roots are exported with the same component coverage as
/// [`scene_descriptor_from_roots`]. Use this for editor actions such as "create
/// prefab from selection" without going through an intermediate scene document.
pub fn scene_prefab_from_roots(
    world: &World,
    id: impl Into<String>,
    roots: impl IntoIterator<Item = Entity>,
) -> Result<ScenePrefabDescriptor, SceneExportError> {
    let id = id.into().trim().to_string();
    if id.is_empty() {
        return Err(SceneExportError::EmptyPrefabId);
    }

    Ok(ScenePrefabDescriptor {
        id,
        entities: scene_entities_from_roots(world, roots)?,
    })
}

fn scene_entities_from_roots(
    world: &World,
    roots: impl IntoIterator<Item = Entity>,
) -> Result<Vec<SceneEntityDescriptor>, SceneExportError> {
    let mut visited = HashSet::new();
    let mut stack = HashSet::new();
    let mut entities = Vec::new();

    for root in roots {
        entities.push(scene_entity_descriptor_from_world(
            world,
            root,
            &mut visited,
            &mut stack,
        )?);
    }

    Ok(entities)
}

fn scene_entity_descriptor_from_world(
    world: &World,
    entity: Entity,
    visited: &mut HashSet<Entity>,
    stack: &mut HashSet<Entity>,
) -> Result<SceneEntityDescriptor, SceneExportError> {
    if !stack.insert(entity) {
        return Err(SceneExportError::HierarchyCycle { entity });
    }
    if !visited.insert(entity) {
        stack.remove(&entity);
        return Err(SceneExportError::DuplicateEntity { entity });
    }

    let transform = world
        .get::<TransformComponent>(entity)
        .ok_or(SceneExportError::MissingTransform { entity })?;
    let children = world
        .get::<Children>(entity)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let descriptor = SceneEntityDescriptor {
        name: world.get::<Name>(entity).map(|name| name.0.clone()),
        tags: world
            .get::<Tags>(entity)
            .map(|tags| tags.iter().map(str::to_string).collect())
            .unwrap_or_default(),
        transform: scene_transform_from_transform(&transform.transform),
        visible: world
            .get::<Visibility>(entity)
            .map(|visibility| visibility.is_visible())
            .unwrap_or(true),
        render_layers: world
            .get::<RenderLayers>(entity)
            .map(|layers| layers.mask()),
        kind: scene_entity_kind_from_world(world, entity)?,
        children: children
            .into_iter()
            .map(|child| scene_entity_descriptor_from_world(world, child, visited, stack))
            .collect::<Result<Vec<_>, _>>()?,
    };

    stack.remove(&entity);
    Ok(descriptor)
}

fn scene_entity_kind_from_world(
    world: &World,
    entity: Entity,
) -> Result<SceneEntityKind, SceneExportError> {
    if let Some(camera) = world.get::<CameraComponent>(entity) {
        let view = world
            .get::<CameraRenderView>(entity)
            .copied()
            .unwrap_or_default();
        return Ok(SceneEntityKind::Camera {
            target: camera.0.target.to_array(),
            controller: world.get::<CameraController>(entity).is_some(),
            order: view.order,
            active: view.is_active,
            viewport: view
                .viewport
                .map(|viewport| [viewport.x, viewport.y, viewport.width, viewport.height]),
            clear_color: view.clear_color,
        });
    }

    if let Some(light) = world.get::<AmbientLight>(entity) {
        return Ok(SceneEntityKind::AmbientLight {
            color: light.color.to_array(),
            intensity: light.intensity,
        });
    }

    if let Some(light) = world.get::<DirectionalLight>(entity) {
        return Ok(SceneEntityKind::DirectionalLight {
            direction: light.direction.to_array(),
            color: light.color.to_array(),
            intensity: light.intensity,
        });
    }

    if let Some(light) = world.get::<PointLight>(entity) {
        return Ok(SceneEntityKind::PointLight {
            color: light.color.to_array(),
            intensity: light.intensity,
            radius: light.radius,
        });
    }

    if let Some(mesh) = world.get::<RenderMesh>(entity) {
        return Ok(SceneEntityKind::Mesh {
            primitive: scene_mesh_primitive_from_mesh(mesh.primitive),
            material: scene_material_descriptor_from_render_material(
                entity,
                &mesh.material,
                mesh.tint,
            )?,
        });
    }

    if let Some(sprite) = world.get::<SpriteBillboard>(entity) {
        return Ok(SceneEntityKind::Sprite {
            sprite: SceneSpriteDescriptor {
                sprite: sprite_id_string(&sprite.sprite),
                size: sprite.size.to_array(),
                tint: sprite.tint,
                facing: scene_sprite_facing_from_sprite(sprite.facing),
                depth: scene_sprite_depth_from_sprite(sprite.depth),
            },
        });
    }

    Ok(SceneEntityKind::Empty)
}

fn scene_transform_from_transform(transform: &Transform) -> SceneTransform {
    SceneTransform {
        position: transform.position.to_array(),
        rotation: [
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ],
        scale: transform.scale.to_array(),
    }
}

fn scene_mesh_primitive_from_mesh(primitive: MeshPrimitive) -> SceneMeshPrimitive {
    match primitive {
        MeshPrimitive::Cube => SceneMeshPrimitive::Cube,
        MeshPrimitive::Sphere { .. } => SceneMeshPrimitive::Sphere,
    }
}

fn scene_material_descriptor_from_render_material(
    entity: Entity,
    material: &RenderMaterial,
    tint: [f32; 4],
) -> Result<SceneMaterialDescriptor, SceneExportError> {
    match material {
        RenderMaterial::Named(reference) => Ok(SceneMaterialDescriptor {
            reference: Some(reference.clone()),
            color: tint,
            ..Default::default()
        }),
        RenderMaterial::Builtin {
            shader,
            name,
            base_color,
            ..
        } => {
            let shader = match shader {
                oxide_renderer::shader::BuiltinShader::Lit => SceneBuiltinShader::Lit,
                oxide_renderer::shader::BuiltinShader::Unlit => SceneBuiltinShader::Unlit,
                shader => {
                    return Err(SceneExportError::UnsupportedMaterialShader {
                        entity,
                        shader: format!("{shader:?}"),
                    });
                }
            };
            Ok(SceneMaterialDescriptor {
                reference: None,
                name: name.clone(),
                shader,
                color: multiply_colors(*base_color, tint),
            })
        }
    }
}

fn multiply_colors(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

fn sprite_id_string(sprite: &SpriteId) -> String {
    sprite.as_str().to_string()
}

fn scene_sprite_facing_from_sprite(facing: SpriteFacing) -> SceneSpriteFacing {
    match facing {
        SpriteFacing::YBillboard => SceneSpriteFacing::YBillboard,
        SpriteFacing::Camera => SceneSpriteFacing::Camera,
        SpriteFacing::Fixed => SceneSpriteFacing::Fixed,
    }
}

fn scene_sprite_depth_from_sprite(depth: SpriteDepthMode) -> SceneSpriteDepthMode {
    match depth {
        SpriteDepthMode::World => SceneSpriteDepthMode::World,
        SpriteDepthMode::Overlay => SceneSpriteDepthMode::Overlay,
    }
}

/// Validates and spawns every root entity from a descriptor.
pub fn try_spawn_scene_descriptor(
    world: &mut World,
    scene: &SceneDescriptor,
) -> Result<Vec<Entity>, SceneValidationError> {
    try_spawn_scene_descriptor_instance(world, scene).map(|instance| instance.roots)
}

/// Validates and spawns a descriptor, returning its scene instance ID and roots.
pub fn try_spawn_scene_descriptor_instance(
    world: &mut World,
    scene: &SceneDescriptor,
) -> Result<SpawnedSceneInstance, SceneValidationError> {
    scene.validate()?;
    Ok(spawn_scene_descriptor_unchecked(world, scene))
}

pub fn spawn_scene_descriptor(world: &mut World, scene: &SceneDescriptor) -> Vec<Entity> {
    match try_spawn_scene_descriptor(world, scene) {
        Ok(roots) => roots,
        Err(err) => {
            let _ = err;
            Vec::new()
        }
    }
}

/// Spawns a descriptor, returning its scene instance ID and roots.
pub fn spawn_scene_descriptor_instance(
    world: &mut World,
    scene: &SceneDescriptor,
) -> Option<SpawnedSceneInstance> {
    match try_spawn_scene_descriptor_instance(world, scene) {
        Ok(instance) => Some(instance),
        Err(err) => {
            let _ = err;
            None
        }
    }
}

fn spawn_scene_descriptor_unchecked(
    world: &mut World,
    scene: &SceneDescriptor,
) -> SpawnedSceneInstance {
    register_scene_materials(world, scene);
    let instance_id = allocate_scene_instance_id(world);
    let prefabs = scene.prefab_lookup();
    let mut prefab_stack = Vec::new();
    let mut context = SceneSpawnContext {
        instance_id,
        prefabs: &prefabs,
        prefab_stack: &mut prefab_stack,
    };
    let roots = scene
        .entities
        .iter()
        .enumerate()
        .map(|(index, entity)| spawn_scene_entity(world, entity, index, "", None, &mut context))
        .collect();

    SpawnedSceneInstance {
        id: instance_id,
        roots,
    }
}

fn allocate_scene_instance_id(world: &mut World) -> SceneInstanceId {
    if !world.contains_resource::<SceneInstanceCounter>() {
        world.insert_resource(SceneInstanceCounter::default());
    }
    let counter = world.resource_mut::<SceneInstanceCounter>();
    let instance_id = SceneInstanceId(counter.next);
    counter.next = counter.next.saturating_add(1);
    instance_id
}

fn register_scene_materials(world: &mut World, scene: &SceneDescriptor) {
    if scene.materials.is_empty() {
        return;
    }

    if !world.contains_resource::<SceneMaterialLibrary>() {
        world.insert_resource(SceneMaterialLibrary::default());
    }

    let library = world.resource_mut::<SceneMaterialLibrary>();
    for material in &scene.materials {
        library.register(
            material.name.clone(),
            RenderMaterial::from(material.clone()).with_base_color(material.color),
        );
    }
}

/// Spawns one prefab instance from a scene descriptor.
///
/// The returned entity is the instance root. Prefab contents become children of
/// that root, so moving the instance root moves the whole prefab hierarchy.
pub fn try_spawn_scene_prefab(
    world: &mut World,
    scene: &SceneDescriptor,
    prefab_id: impl Into<String>,
    transform: SceneTransform,
) -> Result<Option<Entity>, SceneValidationError> {
    try_spawn_scene_prefab_instance(world, scene, prefab_id, transform)
        .map(|instance| instance.and_then(|instance| instance.root()))
}

/// Validates and spawns one prefab, returning its scene instance ID and root.
pub fn try_spawn_scene_prefab_instance(
    world: &mut World,
    scene: &SceneDescriptor,
    prefab_id: impl Into<String>,
    transform: SceneTransform,
) -> Result<Option<SpawnedSceneInstance>, SceneValidationError> {
    scene.validate()?;
    Ok(spawn_scene_prefab_unchecked(
        world, scene, prefab_id, transform,
    ))
}

/// Spawns one prefab instance from a scene descriptor, returning `None` when the
/// requested prefab ID does not exist.
pub fn spawn_scene_prefab(
    world: &mut World,
    scene: &SceneDescriptor,
    prefab_id: impl Into<String>,
    transform: SceneTransform,
) -> Option<Entity> {
    match try_spawn_scene_prefab(world, scene, prefab_id, transform) {
        Ok(entity) => entity,
        Err(err) => {
            let _ = err;
            None
        }
    }
}

/// Spawns one prefab instance, returning its scene instance ID and root.
pub fn spawn_scene_prefab_instance(
    world: &mut World,
    scene: &SceneDescriptor,
    prefab_id: impl Into<String>,
    transform: SceneTransform,
) -> Option<SpawnedSceneInstance> {
    match try_spawn_scene_prefab_instance(world, scene, prefab_id, transform) {
        Ok(instance) => instance,
        Err(err) => {
            let _ = err;
            None
        }
    }
}

fn spawn_scene_prefab_unchecked(
    world: &mut World,
    scene: &SceneDescriptor,
    prefab_id: impl Into<String>,
    transform: SceneTransform,
) -> Option<SpawnedSceneInstance> {
    register_scene_materials(world, scene);
    let prefab_id = prefab_id.into();
    scene.prefab(&prefab_id)?;

    let instance_id = allocate_scene_instance_id(world);
    let prefabs = scene.prefab_lookup();
    let mut prefab_stack = Vec::new();
    let mut context = SceneSpawnContext {
        instance_id,
        prefabs: &prefabs,
        prefab_stack: &mut prefab_stack,
    };
    let descriptor = SceneEntityDescriptor {
        name: Some(prefab_id.clone()),
        transform,
        kind: SceneEntityKind::Prefab {
            id: prefab_id,
            overrides: Vec::new(),
        },
        children: Vec::new(),
        ..Default::default()
    };
    let root = spawn_scene_entity(world, &descriptor, 0, "", None, &mut context);
    Some(SpawnedSceneInstance {
        id: instance_id,
        roots: vec![root],
    })
}

struct SceneSpawnContext<'a> {
    instance_id: SceneInstanceId,
    prefabs: &'a HashMap<&'a str, &'a ScenePrefabDescriptor>,
    prefab_stack: &'a mut Vec<String>,
}

fn spawn_scene_entity(
    world: &mut World,
    descriptor: &SceneEntityDescriptor,
    index: usize,
    parent_path: &str,
    parent: Option<Entity>,
    context: &mut SceneSpawnContext<'_>,
) -> Entity {
    let transform = Transform::from(descriptor.transform);
    let path = scene_entity_descriptor_path(descriptor, index, parent_path);
    let mut entity_mut = world.spawn((
        TransformComponent::new(transform),
        GlobalTransform::default(),
        SceneEntityPath(path.clone()),
        context.instance_id,
    ));

    if let Some(name) = &descriptor.name {
        entity_mut.insert(Name(name.clone()));
    }
    if !descriptor.tags.is_empty() {
        entity_mut.insert(Tags::new(descriptor.tags.clone()));
    }
    if !descriptor.visible {
        entity_mut.insert(Visibility::Hidden);
    }
    if let Some(mask) = descriptor.render_layers {
        entity_mut.insert(RenderLayers::from_mask(mask));
    }

    let entity = entity_mut.id();
    if let Some(parent) = parent {
        attach_child(world, parent, entity);
    }

    match &descriptor.kind {
        SceneEntityKind::Empty => {}
        SceneEntityKind::Camera {
            target,
            controller,
            order,
            active,
            viewport,
            clear_color,
        } => {
            let mut camera = CameraComponent::new();
            camera.0.position = transform.position;
            camera.0.target = vec3(*target);
            world.entity_mut(entity).insert((
                camera,
                CameraRenderView {
                    order: *order,
                    is_active: *active,
                    viewport: viewport
                        .map(|[x, y, width, height]| CameraViewport::new(x, y, width, height)),
                    clear_color: *clear_color,
                },
            ));
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
        SceneEntityKind::Sprite { sprite } => {
            world
                .entity_mut(entity)
                .insert(SpriteBillboard::from(sprite.clone()));
        }
        SceneEntityKind::Prefab { id, overrides } => {
            if !context.prefab_stack.iter().any(|active| active == id) {
                if let Some(prefab) = context.prefabs.get(id.as_str()) {
                    context.prefab_stack.push(id.clone());
                    for (index, child) in prefab.entities.iter().enumerate() {
                        let child = descriptor_with_prefab_overrides(
                            child,
                            index,
                            "",
                            overrides.as_slice(),
                        );
                        spawn_scene_entity(
                            world,
                            &child,
                            index,
                            path.as_str(),
                            Some(entity),
                            context,
                        );
                    }
                    let _ = context.prefab_stack.pop();
                }
            }
        }
    }

    for (index, child) in descriptor.children.iter().enumerate() {
        spawn_scene_entity(world, child, index, path.as_str(), Some(entity), context);
    }

    entity
}

fn descriptor_with_prefab_overrides(
    descriptor: &SceneEntityDescriptor,
    index: usize,
    parent_path: &str,
    overrides: &[ScenePrefabOverride],
) -> SceneEntityDescriptor {
    let path = prefab_entity_path(descriptor, index, parent_path);
    let mut descriptor = descriptor.clone();
    for prefab_override in overrides {
        if normalize_prefab_override_path(&prefab_override.path) == path {
            apply_prefab_override(&mut descriptor, prefab_override);
        }
    }
    descriptor.children = descriptor
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            descriptor_with_prefab_overrides(child, index, path.as_str(), overrides)
        })
        .collect();
    descriptor
}

fn apply_prefab_override(
    descriptor: &mut SceneEntityDescriptor,
    prefab_override: &ScenePrefabOverride,
) {
    if let Some(transform) = prefab_override.transform {
        descriptor.transform = transform;
    }
    if let Some(visible) = prefab_override.visible {
        descriptor.visible = visible;
    }
    if let Some(render_layers) = prefab_override.render_layers {
        descriptor.render_layers = Some(render_layers);
    }
    if let (SceneEntityKind::Mesh { material, .. }, Some(override_material)) =
        (&mut descriptor.kind, &prefab_override.material)
    {
        *material = override_material.clone();
    }
}

fn prefab_entity_path(
    descriptor: &SceneEntityDescriptor,
    index: usize,
    parent_path: &str,
) -> String {
    scene_entity_descriptor_path(descriptor, index, parent_path)
}

fn scene_entity_descriptor_path(
    descriptor: &SceneEntityDescriptor,
    index: usize,
    parent_path: &str,
) -> String {
    let segment = descriptor
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{index}"));
    if parent_path.is_empty() {
        segment
    } else {
        format!("{parent_path}/{segment}")
    }
}

fn normalize_scene_entity_path(path: &str) -> String {
    normalize_prefab_override_path(path)
}

fn normalize_prefab_override_path(path: &str) -> String {
    path.split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_scene_entity(
    descriptor: &SceneEntityDescriptor,
    path: String,
    prefabs: &HashMap<&str, &ScenePrefabDescriptor>,
    prefab_stack: &mut Vec<String>,
    diagnostics: &mut Vec<SceneValidationDiagnostic>,
) {
    validate_entity_tags(descriptor, path.as_str(), diagnostics);

    match &descriptor.kind {
        SceneEntityKind::Prefab { id, overrides } => {
            if id.trim().is_empty() {
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.id"),
                    "prefab references must not be empty",
                ));
            } else if !prefabs.contains_key(id.as_str()) {
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.id"),
                    format!("unknown prefab ID '{id}'"),
                ));
            } else if prefab_stack.iter().any(|active| active == id) {
                let mut chain = prefab_stack.join(" -> ");
                if !chain.is_empty() {
                    chain.push_str(" -> ");
                }
                chain.push_str(id);
                diagnostics.push(SceneValidationDiagnostic::new(
                    format!("{path}.id"),
                    format!("recursive prefab reference '{chain}'"),
                ));
            } else if let Some(prefab) = prefabs.get(id.as_str()) {
                validate_prefab_overrides(path.as_str(), prefab, overrides, diagnostics);
                prefab_stack.push(id.clone());
                for (index, entity) in prefab.entities.iter().enumerate() {
                    validate_scene_entity(
                        entity,
                        format!("{path}.prefab('{id}').entities[{index}]"),
                        prefabs,
                        prefab_stack,
                        diagnostics,
                    );
                }
                let _ = prefab_stack.pop();
            }
        }
        SceneEntityKind::Sprite { sprite } if sprite.sprite.trim().is_empty() => {
            diagnostics.push(SceneValidationDiagnostic::new(
                format!("{path}.sprite"),
                "sprite IDs must not be empty",
            ));
        }
        _ => {}
    }

    for (index, child) in descriptor.children.iter().enumerate() {
        validate_scene_entity(
            child,
            format!("{path}.children[{index}]"),
            prefabs,
            prefab_stack,
            diagnostics,
        );
    }
}

fn validate_entity_tags(
    descriptor: &SceneEntityDescriptor,
    path: &str,
    diagnostics: &mut Vec<SceneValidationDiagnostic>,
) {
    let mut seen = HashSet::new();
    for (index, tag) in descriptor.tags.iter().enumerate() {
        let tag = tag.trim();
        let diagnostic_path = format!("{path}.tags[{index}]");
        if tag.is_empty() {
            diagnostics.push(SceneValidationDiagnostic::new(
                diagnostic_path,
                "entity tags must not be empty",
            ));
            continue;
        }
        if !seen.insert(tag) {
            diagnostics.push(SceneValidationDiagnostic::new(
                diagnostic_path,
                format!("duplicate entity tag '{tag}'"),
            ));
        }
    }
}

fn validate_prefab_overrides(
    path: &str,
    prefab: &ScenePrefabDescriptor,
    overrides: &[ScenePrefabOverride],
    diagnostics: &mut Vec<SceneValidationDiagnostic>,
) {
    let mut targets = HashMap::new();
    let mut duplicate_targets = HashSet::new();
    for (index, entity) in prefab.entities.iter().enumerate() {
        collect_prefab_override_targets(entity, index, "", &mut targets, &mut duplicate_targets);
    }

    let mut seen_overrides = HashSet::new();
    for (index, prefab_override) in overrides.iter().enumerate() {
        let override_path = normalize_prefab_override_path(&prefab_override.path);
        let diagnostic_path = format!("{path}.overrides[{index}].path");
        if override_path.is_empty() {
            diagnostics.push(SceneValidationDiagnostic::new(
                diagnostic_path,
                "prefab override paths must not be empty",
            ));
            continue;
        }
        if !seen_overrides.insert(override_path.clone()) {
            diagnostics.push(SceneValidationDiagnostic::new(
                diagnostic_path,
                format!("duplicate prefab override path '{override_path}'"),
            ));
            continue;
        }
        if duplicate_targets.contains(override_path.as_str()) {
            diagnostics.push(SceneValidationDiagnostic::new(
                diagnostic_path,
                format!("ambiguous prefab override path '{override_path}'"),
            ));
            continue;
        }
        let Some(target) = targets.get(override_path.as_str()) else {
            diagnostics.push(SceneValidationDiagnostic::new(
                diagnostic_path,
                format!("unknown prefab override path '{override_path}'"),
            ));
            continue;
        };
        if prefab_override.material.is_some()
            && !matches!(target.kind, SceneEntityKind::Mesh { .. })
        {
            diagnostics.push(SceneValidationDiagnostic::new(
                format!("{path}.overrides[{index}].material"),
                format!("material overrides require a mesh target '{override_path}'"),
            ));
        }
    }
}

fn collect_prefab_override_targets<'a>(
    descriptor: &'a SceneEntityDescriptor,
    index: usize,
    parent_path: &str,
    targets: &mut HashMap<String, &'a SceneEntityDescriptor>,
    duplicate_targets: &mut HashSet<String>,
) {
    let path = prefab_entity_path(descriptor, index, parent_path);
    if targets.insert(path.clone(), descriptor).is_some() {
        duplicate_targets.insert(path.clone());
    }
    for (index, child) in descriptor.children.iter().enumerate() {
        collect_prefab_override_targets(child, index, path.as_str(), targets, duplicate_targets);
    }
}

fn vec3(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn default_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_visible() -> bool {
    true
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

fn default_sprite_size() -> [f32; 2] {
    [1.0, 1.0]
}

fn default_sprite_tint() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
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
    fn scene_descriptor_exports_world_hierarchy_for_authoring_save() {
        let mut world = World::new();
        let root = world
            .spawn((
                Name("Encounter".to_string()),
                TransformComponent::from_position(Vec3::new(1.0, 2.0, 3.0)),
                GlobalTransform::default(),
            ))
            .id();
        world.entity_mut(root).insert((
            Tags::new(["encounter", "edited"]),
            Visibility::Hidden,
            RenderLayers::layer(2),
            RenderMesh::new(
                MeshPrimitive::Cube,
                RenderMaterial::Named("crate".to_string()),
            )
            .with_tint([0.2, 0.4, 0.6, 1.0]),
        ));
        let child = world
            .spawn((
                Name("Marker".to_string()),
                TransformComponent::from_position(Vec3::new(0.0, 1.0, 0.0)),
                GlobalTransform::default(),
                SpriteBillboard::new("marker", Vec2::new(0.5, 0.75))
                    .with_tint([1.0, 0.5, 0.25, 1.0])
                    .with_facing(SpriteFacing::Camera)
                    .with_depth(SpriteDepthMode::Overlay),
            ))
            .id();
        attach_child(&mut world, root, child);

        let scene = scene_descriptor_from_roots(&world, [root]).unwrap();

        assert_eq!(scene.entities.len(), 1);
        let root_descriptor = &scene.entities[0];
        assert_eq!(root_descriptor.name.as_deref(), Some("Encounter"));
        assert_eq!(root_descriptor.tags, ["encounter", "edited"]);
        assert_eq!(root_descriptor.transform.position, [1.0, 2.0, 3.0]);
        assert!(!root_descriptor.visible);
        assert_eq!(
            root_descriptor.render_layers,
            Some(RenderLayers::layer(2).mask())
        );
        let SceneEntityKind::Mesh {
            primitive,
            material,
        } = &root_descriptor.kind
        else {
            panic!("expected exported mesh root");
        };
        assert!(matches!(primitive, SceneMeshPrimitive::Cube));
        assert_eq!(material.reference.as_deref(), Some("crate"));
        assert_eq!(material.color, [0.2, 0.4, 0.6, 1.0]);

        assert_eq!(root_descriptor.children.len(), 1);
        let SceneEntityKind::Sprite { sprite } = &root_descriptor.children[0].kind else {
            panic!("expected exported sprite child");
        };
        assert_eq!(sprite.sprite, "marker");
        assert_eq!(sprite.size, [0.5, 0.75]);
        assert_eq!(sprite.facing, SceneSpriteFacing::Camera);
        assert_eq!(sprite.depth, SceneSpriteDepthMode::Overlay);
        scene.validate().unwrap();

        let mut spawned_world = World::new();
        let spawned = spawn_scene_descriptor_instance(&mut spawned_world, &scene).unwrap();
        assert_eq!(
            entities_in_scene_instance(&mut spawned_world, spawned.id).len(),
            2
        );
        assert!(entity_has_tag(
            &spawned_world,
            spawned.root().unwrap(),
            "edited"
        ));
    }

    #[test]
    fn scene_descriptor_exports_cameras_and_lights() {
        let mut world = World::new();
        let mut camera = CameraComponent::new();
        camera.0.position = Vec3::new(0.0, 3.0, 5.0);
        camera.0.target = Vec3::ZERO;
        let camera_entity = world
            .spawn((
                Name("Gameplay Camera".to_string()),
                TransformComponent::from_position(Vec3::new(0.0, 3.0, 5.0)),
                GlobalTransform::default(),
            ))
            .id();
        world.entity_mut(camera_entity).insert((
            camera,
            CameraController::new(),
            CameraRenderView::new()
                .with_order(-2)
                .with_viewport(CameraViewport::new(0.1, 0.2, 0.3, 0.4))
                .with_clear_color([0.1, 0.2, 0.3, 1.0]),
        ));
        let light_entity = world
            .spawn((
                Name("Sun".to_string()),
                TransformComponent::default(),
                GlobalTransform::default(),
                DirectionalLight::new(Vec3::new(1.0, -1.0, 0.0), Vec3::new(1.0, 0.9, 0.8), 2.0),
            ))
            .id();

        let scene = scene_descriptor_from_roots(&world, [camera_entity, light_entity]).unwrap();

        let SceneEntityKind::Camera {
            target,
            controller,
            order,
            active,
            viewport,
            clear_color,
        } = &scene.entities[0].kind
        else {
            panic!("expected exported camera");
        };
        assert_eq!(*target, [0.0, 0.0, 0.0]);
        assert!(*controller);
        assert_eq!(*order, -2);
        assert!(*active);
        assert_eq!(*viewport, Some([0.1, 0.2, 0.3, 0.4]));
        assert_eq!(*clear_color, Some([0.1, 0.2, 0.3, 1.0]));

        let SceneEntityKind::DirectionalLight {
            color, intensity, ..
        } = &scene.entities[1].kind
        else {
            panic!("expected exported directional light");
        };
        assert_eq!(*color, [1.0, 0.9, 0.8]);
        assert_eq!(*intensity, 2.0);
    }

    #[test]
    fn scene_descriptor_export_reports_unrepresentable_materials() {
        let mut world = World::new();
        let entity = world
            .spawn((
                TransformComponent::default(),
                GlobalTransform::default(),
                RenderMesh::new(
                    MeshPrimitive::Cube,
                    RenderMaterial::Builtin {
                        shader: oxide_renderer::shader::BuiltinShader::Basic,
                        material_type: oxide_renderer::descriptor::MaterialType::Basic,
                        name: "basic".to_string(),
                        base_color: [1.0; 4],
                    },
                ),
            ))
            .id();

        let err = scene_descriptor_from_roots(&world, [entity]).unwrap_err();
        assert_eq!(
            err,
            SceneExportError::UnsupportedMaterialShader {
                entity,
                shader: "Basic".to_string()
            }
        );
    }

    #[test]
    fn scene_prefab_exports_selection_roots_for_reuse() {
        let mut world = World::new();
        let root = world
            .spawn((
                Name("Crate Pair".to_string()),
                TransformComponent::default(),
                GlobalTransform::default(),
            ))
            .id();
        let child = world
            .spawn((
                Name("Crate".to_string()),
                TransformComponent::from_position(Vec3::new(1.0, 0.0, 0.0)),
                GlobalTransform::default(),
                RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            ))
            .id();
        attach_child(&mut world, root, child);

        let prefab = scene_prefab_from_roots(&world, " crate_pair ", [root]).unwrap();

        assert_eq!(prefab.id, "crate_pair");
        assert_eq!(prefab.entities.len(), 1);
        assert_eq!(prefab.entities[0].name.as_deref(), Some("Crate Pair"));
        assert_eq!(prefab.entities[0].children.len(), 1);

        let scene = SceneDescriptor {
            prefabs: vec![prefab],
            entities: vec![SceneEntityDescriptor {
                name: Some("Instance".to_string()),
                kind: SceneEntityKind::Prefab {
                    id: "crate_pair".to_string(),
                    overrides: Vec::new(),
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        scene.validate().unwrap();

        let mut spawned_world = World::new();
        let spawned = spawn_scene_descriptor_instance(&mut spawned_world, &scene).unwrap();
        assert_eq!(
            entities_in_scene_instance(&mut spawned_world, spawned.id).len(),
            3
        );
        assert!(entity_by_scene_path_in_instance(
            &mut spawned_world,
            spawned.id,
            "Instance/Crate Pair/Crate"
        )
        .is_some());
    }

    #[test]
    fn scene_prefab_export_rejects_empty_ids() {
        let world = World::new();
        let err = scene_prefab_from_roots(&world, "  ", []).unwrap_err();
        assert_eq!(err, SceneExportError::EmptyPrefabId);
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

    #[test]
    fn prefab_entities_spawn_as_hierarchies() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let roots = spawn_scene_descriptor(&mut world, &scene);
        assert_eq!(roots.len(), 1);

        let instance_children = world.get::<oxide_transform::Children>(roots[0]).unwrap();
        assert_eq!(instance_children.len(), 2);

        let prefab_root = instance_children.iter().next().unwrap();
        let parent = world.get::<oxide_transform::Parent>(prefab_root).unwrap();
        assert_eq!(parent.0, roots[0]);

        let prefab_children = world.get::<oxide_transform::Children>(prefab_root).unwrap();
        assert_eq!(prefab_children.len(), 1);

        let mut meshes = world.query::<&RenderMesh>();
        assert_eq!(meshes.iter(&world).count(), 2);
    }

    #[test]
    fn spawn_scene_prefab_returns_instance_root() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let root = spawn_scene_prefab(
            &mut world,
            &scene,
            "crate_pair",
            SceneTransform::from_position([4.0, 0.0, -2.0]),
        )
        .unwrap();

        assert_eq!(world.get::<Name>(root).unwrap().0, "crate_pair");
        assert!(world.get::<oxide_transform::Children>(root).is_some());
        assert!(spawn_scene_prefab(
            &mut world,
            &scene,
            "missing_prefab",
            SceneTransform::default()
        )
        .is_none());
    }

    #[test]
    fn scene_descriptor_instance_spawn_returns_id_and_roots() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let instance = spawn_scene_descriptor_instance(&mut world, &scene).unwrap();

        assert_eq!(instance.roots.len(), 1);
        assert_eq!(
            scene_instance_id(&world, instance.root().unwrap()),
            Some(instance.id)
        );
        assert_eq!(entities_in_scene_instance(&mut world, instance.id).len(), 4);
    }

    #[test]
    fn scene_prefab_instance_spawn_returns_id_and_root() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let instance = spawn_scene_prefab_instance(
            &mut world,
            &scene,
            "crate_pair",
            SceneTransform::from_position([4.0, 0.0, -2.0]),
        )
        .unwrap();

        assert_eq!(instance.roots.len(), 1);
        assert_eq!(
            world.get::<Name>(instance.root().unwrap()).unwrap().0,
            "crate_pair"
        );
        assert_eq!(
            scene_instance_id(&world, instance.root().unwrap()),
            Some(instance.id)
        );
        assert_eq!(entities_in_scene_instance(&mut world, instance.id).len(), 3);
        assert!(spawn_scene_prefab_instance(
            &mut world,
            &scene,
            "missing_prefab",
            SceneTransform::default()
        )
        .is_none());
    }

    #[test]
    fn scene_entities_spawn_authored_tags() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Enemy Spawn".to_string()),
                tags: vec!["enemy".to_string(), "spawn_point".to_string()],
                kind: SceneEntityKind::Empty,
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        let tags = world.get::<Tags>(roots[0]).unwrap();

        assert!(tags.contains("enemy"));
        assert!(tags.contains("spawn_point"));
        assert!(tags.contains(" enemy "));
        assert_eq!(
            tags.iter().collect::<Vec<_>>(),
            vec!["enemy", "spawn_point"]
        );
    }

    #[test]
    fn scene_tag_helpers_find_authored_entities() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![
                SceneEntityDescriptor {
                    name: Some("Enemy Spawn".to_string()),
                    tags: vec!["enemy".to_string(), "spawn_point".to_string()],
                    kind: SceneEntityKind::Empty,
                    ..Default::default()
                },
                SceneEntityDescriptor {
                    name: Some("Boss Spawn".to_string()),
                    tags: vec!["enemy".to_string(), "boss".to_string()],
                    kind: SceneEntityKind::Empty,
                    ..Default::default()
                },
                SceneEntityDescriptor {
                    name: Some("Camera Marker".to_string()),
                    tags: vec!["marker".to_string()],
                    kind: SceneEntityKind::Empty,
                    ..Default::default()
                },
            ],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);

        let enemies = entities_with_tag(&mut world, "enemy");
        assert_eq!(enemies.len(), 2);
        assert!(enemies.contains(&roots[0]));
        assert!(enemies.contains(&roots[1]));
        assert_eq!(first_entity_with_tag(&mut world, "boss"), Some(roots[1]));
        assert!(entity_has_tag(&world, roots[0], " spawn_point "));
        assert!(!entity_has_tag(&world, roots[2], "enemy"));
        assert!(entities_with_tag(&mut world, "missing").is_empty());
    }

    #[test]
    fn scene_instance_ids_scope_tag_lookup() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Marker".to_string()),
                tags: vec!["marker".to_string()],
                kind: SceneEntityKind::Empty,
                ..Default::default()
            }],
        };
        let mut world = World::new();

        let first_roots = spawn_scene_descriptor(&mut world, &scene);
        let second_roots = spawn_scene_descriptor(&mut world, &scene);
        let first_instance = scene_instance_id(&world, first_roots[0]).unwrap();
        let second_instance = scene_instance_id(&world, second_roots[0]).unwrap();

        assert_eq!(entities_with_tag(&mut world, "marker").len(), 2);
        assert_eq!(
            first_entity_with_tag_in_instance(&mut world, first_instance, "marker"),
            Some(first_roots[0])
        );
        assert_eq!(
            entities_with_tag_in_instance(&mut world, second_instance, "marker"),
            vec![second_roots[0]]
        );
    }

    #[test]
    fn scene_entities_spawn_authored_paths_for_hierarchies_and_prefabs() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let roots = spawn_scene_descriptor(&mut world, &scene);
        let instance = roots[0];
        let base = entity_by_scene_path(&mut world, "Crate Pair Instance/Crate Base").unwrap();
        let top =
            entity_by_scene_path(&mut world, "Crate Pair Instance/Crate Base/Crate Top").unwrap();
        let marker =
            entity_by_scene_path(&mut world, " Crate Pair Instance / Instance Marker ").unwrap();

        assert_eq!(
            scene_entity_path(&world, instance),
            Some("Crate Pair Instance")
        );
        assert_eq!(
            scene_instance_id(&world, base),
            scene_instance_id(&world, instance)
        );
        assert_eq!(
            scene_entity_path(&world, top),
            Some("Crate Pair Instance/Crate Base/Crate Top")
        );
        assert_eq!(
            entities_under_scene_path(&mut world, "Crate Pair Instance/Crate Base").len(),
            2
        );
        assert_eq!(
            world.get::<oxide_transform::Parent>(base).unwrap().0,
            instance
        );
        assert_eq!(
            world.get::<oxide_transform::Parent>(marker).unwrap().0,
            instance
        );
    }

    #[test]
    fn scene_instance_ids_scope_authored_path_lookup() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let first_roots = spawn_scene_descriptor(&mut world, &scene);
        let second_roots = spawn_scene_descriptor(&mut world, &scene);
        let first_instance = scene_instance_id(&world, first_roots[0]).unwrap();
        let second_instance = scene_instance_id(&world, second_roots[0]).unwrap();

        assert_ne!(first_instance, second_instance);

        let first_base = entity_by_scene_path_in_instance(
            &mut world,
            first_instance,
            "Crate Pair Instance/Crate Base",
        )
        .unwrap();
        let second_base = entity_by_scene_path_in_instance(
            &mut world,
            second_instance,
            "Crate Pair Instance/Crate Base",
        )
        .unwrap();

        assert_ne!(first_base, second_base);
        assert_eq!(
            world.get::<oxide_transform::Parent>(first_base).unwrap().0,
            first_roots[0]
        );
        assert_eq!(
            world.get::<oxide_transform::Parent>(second_base).unwrap().0,
            second_roots[0]
        );
        assert_eq!(
            entities_under_scene_path(&mut world, "Crate Pair Instance/Crate Base").len(),
            4
        );
        assert_eq!(
            entities_under_scene_path_in_instance(
                &mut world,
                first_instance,
                "Crate Pair Instance/Crate Base"
            )
            .len(),
            2
        );
    }

    #[test]
    fn despawn_scene_instance_removes_only_matching_instance() {
        let scene = prefab_test_scene();
        let mut world = World::new();

        let first_roots = spawn_scene_descriptor(&mut world, &scene);
        let second_roots = spawn_scene_descriptor(&mut world, &scene);
        let first_instance = scene_instance_id(&world, first_roots[0]).unwrap();
        let second_instance = scene_instance_id(&world, second_roots[0]).unwrap();

        let despawned = despawn_scene_instance(&mut world, first_instance);

        assert_eq!(despawned.len(), 4);
        assert!(despawned.iter().all(|entity| !world.contains(*entity)));
        assert!(!world.contains(first_roots[0]));
        assert!(world.contains(second_roots[0]));
        assert_eq!(
            entities_in_scene_instance(&mut world, first_instance).len(),
            0
        );
        assert_eq!(
            entities_in_scene_instance(&mut world, second_instance).len(),
            4
        );
    }

    #[test]
    fn despawn_scene_instance_detaches_external_hierarchy_links() {
        let scene = prefab_test_scene();
        let mut world = World::new();
        let external_parent = world.reserve_entity();
        let external_child = world.spawn(TransformComponent::default()).id();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        let instance = scene_instance_id(&world, roots[0]).unwrap();

        attach_child(&mut world, external_parent, roots[0]);
        attach_child(&mut world, roots[0], external_child);

        let despawned = despawn_scene_instance(&mut world, instance);

        assert!(despawned.contains(&roots[0]));
        assert!(world.contains(external_parent));
        assert!(world.contains(external_child));
        assert!(world.get::<Parent>(external_child).is_none());
        assert!(!world
            .get::<Children>(external_parent)
            .map(|children| children.iter().any(|child| child == roots[0]))
            .unwrap_or(false));
    }

    #[test]
    fn unnamed_scene_entities_use_index_path_segments() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: None,
                kind: SceneEntityKind::Empty,
                children: vec![SceneEntityDescriptor {
                    name: None,
                    kind: SceneEntityKind::Empty,
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        let child = entity_by_scene_path(&mut world, "#0/#0").unwrap();

        assert_eq!(scene_entity_path(&world, roots[0]), Some("#0"));
        assert_eq!(scene_entity_path(&world, child), Some("#0/#0"));
    }

    #[test]
    fn prefab_instance_overrides_apply_to_named_prefab_entities() {
        let mut scene = prefab_test_scene();
        scene.entities[0].kind = SceneEntityKind::Prefab {
            id: "crate_pair".to_string(),
            overrides: vec![ScenePrefabOverride {
                path: "Crate Base/Crate Top".to_string(),
                transform: Some(SceneTransform::from_position([0.0, 2.0, 0.0])),
                visible: Some(false),
                render_layers: Some(RenderLayers::layer(4).mask()),
                material: Some(SceneMaterialDescriptor {
                    reference: Some("materials.highlight".to_string()),
                    color: [1.0, 0.2, 0.1, 1.0],
                    ..Default::default()
                }),
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        let base = child_named(&world, roots[0], "Crate Base").unwrap();
        let top = child_named(&world, base, "Crate Top").unwrap();

        let transform = world.get::<TransformComponent>(top).unwrap();
        assert_eq!(transform.transform.position, Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(world.get::<Visibility>(top), Some(&Visibility::Hidden));
        assert_eq!(
            world.get::<RenderLayers>(top),
            Some(&RenderLayers::layer(4))
        );

        let mesh = world.get::<RenderMesh>(top).unwrap();
        assert!(matches!(
            &mesh.material,
            RenderMaterial::Named(name) if name == "materials.highlight"
        ));
        assert_eq!(mesh.tint, [1.0, 0.2, 0.1, 1.0]);
    }

    #[test]
    fn sprite_scene_entities_spawn_billboards() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Sprite Actor".to_string()),
                kind: SceneEntityKind::Sprite {
                    sprite: SceneSpriteDescriptor {
                        sprite: "actor.hero".to_string(),
                        size: [1.5, 2.0],
                        tint: [0.5, 0.75, 1.0, 1.0],
                        facing: SceneSpriteFacing::Fixed,
                        depth: SceneSpriteDepthMode::Overlay,
                    },
                },
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        assert_eq!(roots.len(), 1);

        let sprite = world.get::<SpriteBillboard>(roots[0]).unwrap();
        assert_eq!(sprite.sprite.as_str(), "actor.hero");
        assert_eq!(sprite.size, Vec2::new(1.5, 2.0));
        assert_eq!(sprite.tint, [0.5, 0.75, 1.0, 1.0]);
        assert_eq!(sprite.facing, SpriteFacing::Fixed);
        assert_eq!(sprite.depth, SpriteDepthMode::Overlay);
    }

    #[test]
    fn hidden_scene_entities_spawn_visibility_component() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Hidden Mesh".to_string()),
                visible: false,
                kind: SceneEntityKind::Mesh {
                    primitive: SceneMeshPrimitive::Cube,
                    material: SceneMaterialDescriptor::default(),
                },
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        assert_eq!(roots.len(), 1);
        assert_eq!(world.get::<Visibility>(roots[0]), Some(&Visibility::Hidden));
    }

    #[test]
    fn scene_entities_spawn_render_layer_masks() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Layered Mesh".to_string()),
                render_layers: Some(RenderLayers::layer(2).mask()),
                kind: SceneEntityKind::Mesh {
                    primitive: SceneMeshPrimitive::Cube,
                    material: SceneMaterialDescriptor::default(),
                },
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        assert_eq!(roots.len(), 1);
        assert_eq!(
            world.get::<RenderLayers>(roots[0]),
            Some(&RenderLayers::layer(2))
        );
    }

    #[test]
    fn mesh_scene_entities_can_reference_named_materials() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Named Material Mesh".to_string()),
                kind: SceneEntityKind::Mesh {
                    primitive: SceneMeshPrimitive::Cube,
                    material: SceneMaterialDescriptor {
                        reference: Some("materials.crate".to_string()),
                        color: [0.25, 0.5, 0.75, 1.0],
                        ..Default::default()
                    },
                },
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        let mesh = world.get::<RenderMesh>(roots[0]).unwrap();
        assert!(matches!(
            &mesh.material,
            RenderMaterial::Named(name) if name == "materials.crate"
        ));
        assert_eq!(mesh.tint, [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn scene_materials_register_into_world_library_before_spawning() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: vec![SceneMaterialDescriptor {
                name: "materials.crate".to_string(),
                shader: SceneBuiltinShader::Lit,
                color: [0.9, 0.7, 0.45, 1.0],
                ..Default::default()
            }],
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Scene Material Mesh".to_string()),
                kind: SceneEntityKind::Mesh {
                    primitive: SceneMeshPrimitive::Cube,
                    material: SceneMaterialDescriptor {
                        reference: Some("materials.crate".to_string()),
                        color: [0.25, 0.5, 0.75, 1.0],
                        ..Default::default()
                    },
                },
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        assert_eq!(roots.len(), 1);

        let library = world.get_resource::<SceneMaterialLibrary>().unwrap();
        assert!(matches!(
            library.get("materials.crate"),
            Some(RenderMaterial::Builtin { name, .. }) if name == "materials.crate"
        ));

        let mesh = world.get::<RenderMesh>(roots[0]).unwrap();
        assert!(matches!(
            &mesh.material,
            RenderMaterial::Named(name) if name == "materials.crate"
        ));
        assert_eq!(mesh.tint, [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn camera_entities_spawn_render_view_metadata() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                name: Some("Debug Camera".to_string()),
                render_layers: Some(RenderLayers::layer(3).mask()),
                kind: SceneEntityKind::Camera {
                    target: [0.0, 1.0, 0.0],
                    controller: false,
                    order: -5,
                    active: false,
                    viewport: Some([0.5, 0.0, 0.5, 0.5]),
                    clear_color: Some([0.2, 0.3, 0.4, 1.0]),
                },
                ..Default::default()
            }],
        };

        let mut world = World::new();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        assert_eq!(roots.len(), 1);

        let view = world.get::<CameraRenderView>(roots[0]).unwrap();
        assert_eq!(view.order, -5);
        assert!(!view.is_active);
        assert_eq!(view.viewport, Some(CameraViewport::new(0.5, 0.0, 0.5, 0.5)));
        assert_eq!(view.clear_color, Some([0.2, 0.3, 0.4, 1.0]));
        assert_eq!(
            world.get::<RenderLayers>(roots[0]),
            Some(&RenderLayers::layer(3))
        );
    }

    #[test]
    fn load_scene_descriptor_accepts_sprite_entity_defaults() {
        let path = temp_path("sprite_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 1,
                "scene": {
                    "entities": [
                        {
                            "name": "Sprite Actor",
                            "type": "sprite",
                            "sprite": "actor.hero"
                        }
                    ]
                }
            }"#,
        )
        .unwrap();

        let scene = load_scene_descriptor(&path).unwrap();
        let SceneEntityKind::Sprite { sprite } = &scene.entities[0].kind else {
            panic!("expected sprite entity");
        };
        assert_eq!(sprite.sprite, "actor.hero");
        assert_eq!(sprite.size, [1.0, 1.0]);
        assert_eq!(sprite.tint, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(sprite.facing, SceneSpriteFacing::YBillboard);
        assert_eq!(sprite.depth, SceneSpriteDepthMode::World);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_scene_descriptor_accepts_declared_dependencies() {
        let path = temp_path("dependency_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 1,
                "scene": {
                    "dependencies": [
                        "materials/stone.oxmat",
                        "sprites/hud.png"
                    ],
                    "entities": []
                }
            }"#,
        )
        .unwrap();

        let scene = load_scene_descriptor(&path).unwrap();
        assert_eq!(
            scene.dependencies,
            vec![
                "materials/stone.oxmat".to_string(),
                "sprites/hud.png".to_string()
            ]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_scene_descriptor_accepts_prefab_instance_overrides() {
        let path = temp_path("prefab_override_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 1,
                "scene": {
                    "prefabs": [
                        {
                            "id": "crate_pair",
                            "entities": [
                                {
                                    "name": "Crate Base",
                                    "type": "mesh",
                                    "children": [
                                        {
                                            "name": "Crate Top",
                                            "type": "mesh"
                                        }
                                    ]
                                }
                            ]
                        }
                    ],
                    "entities": [
                        {
                            "type": "prefab",
                            "id": "crate_pair",
                            "overrides": [
                                {
                                    "path": "Crate Base/Crate Top",
                                    "visible": false,
                                    "material": {
                                        "ref": "materials.highlight",
                                        "color": [1.0, 0.2, 0.1, 1.0]
                                    }
                                }
                            ]
                        }
                    ]
                }
            }"#,
        )
        .unwrap();

        let scene = load_scene_descriptor(&path).unwrap();
        let SceneEntityKind::Prefab { id, overrides } = &scene.entities[0].kind else {
            panic!("expected prefab entity");
        };
        assert_eq!(id, "crate_pair");
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].path, "Crate Base/Crate Top");
        assert_eq!(overrides[0].visible, Some(false));
        assert!(matches!(
            overrides[0]
                .material
                .as_ref()
                .and_then(|material| material.reference.as_deref()),
            Some("materials.highlight")
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_scene_descriptor_accepts_entity_tags() {
        let path = temp_path("tagged_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 1,
                "scene": {
                    "entities": [
                        {
                            "name": "Tagged Spawn",
                            "tags": ["enemy", "spawn_point"],
                            "type": "empty"
                        }
                    ]
                }
            }"#,
        )
        .unwrap();

        let scene = load_scene_descriptor(&path).unwrap();
        assert_eq!(
            scene.entities[0].tags,
            vec!["enemy".to_string(), "spawn_point".to_string()]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn scene_validation_reports_prefab_authoring_errors() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: vec![
                ScenePrefabDescriptor {
                    id: "loop".to_string(),
                    entities: vec![SceneEntityDescriptor {
                        kind: SceneEntityKind::Prefab {
                            id: "loop".to_string(),
                            overrides: Vec::new(),
                        },
                        ..Default::default()
                    }],
                },
                ScenePrefabDescriptor {
                    id: "loop".to_string(),
                    entities: Vec::new(),
                },
            ],
            entities: vec![
                SceneEntityDescriptor {
                    kind: SceneEntityKind::Prefab {
                        id: "missing".to_string(),
                        overrides: Vec::new(),
                    },
                    ..Default::default()
                },
                SceneEntityDescriptor {
                    kind: SceneEntityKind::Sprite {
                        sprite: SceneSpriteDescriptor {
                            sprite: String::new(),
                            size: [1.0, 1.0],
                            tint: [1.0, 1.0, 1.0, 1.0],
                            facing: SceneSpriteFacing::YBillboard,
                            depth: SceneSpriteDepthMode::World,
                        },
                    },
                    ..Default::default()
                },
            ],
        };

        let err = scene.validate().unwrap_err();
        let messages: Vec<_> = err
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert!(messages
            .iter()
            .any(|message| message.contains("duplicate prefab ID")));
        assert!(messages
            .iter()
            .any(|message| message.contains("recursive prefab reference")));
        assert!(messages
            .iter()
            .any(|message| message.contains("unknown prefab ID")));
        assert!(messages
            .iter()
            .any(|message| message.contains("sprite IDs must not be empty")));
    }

    #[test]
    fn scene_validation_reports_entity_tag_authoring_errors() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: vec![ScenePrefabDescriptor {
                id: "tagged_prefab".to_string(),
                entities: vec![SceneEntityDescriptor {
                    tags: vec!["spawn".to_string(), " spawn ".to_string(), String::new()],
                    ..Default::default()
                }],
            }],
            entities: vec![SceneEntityDescriptor {
                tags: vec!["enemy".to_string(), "enemy".to_string(), "   ".to_string()],
                ..Default::default()
            }],
        };

        let err = scene.validate().unwrap_err();
        let diagnostics = err.diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "entities[0].tags[1]"
                && diagnostic.message.contains("duplicate entity tag")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "entities[0].tags[2]"
                && diagnostic.message == "entity tags must not be empty"
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "prefabs['tagged_prefab'].entities[0].tags[1]"
                && diagnostic.message.contains("duplicate entity tag")
        }));
    }

    #[test]
    fn scene_validation_reports_prefab_override_authoring_errors() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: vec![ScenePrefabDescriptor {
                id: "crate_pair".to_string(),
                entities: vec![
                    SceneEntityDescriptor {
                        name: Some("Duplicate".to_string()),
                        kind: SceneEntityKind::Empty,
                        ..Default::default()
                    },
                    SceneEntityDescriptor {
                        name: Some("Duplicate".to_string()),
                        kind: SceneEntityKind::Empty,
                        ..Default::default()
                    },
                ],
            }],
            entities: vec![SceneEntityDescriptor {
                kind: SceneEntityKind::Prefab {
                    id: "crate_pair".to_string(),
                    overrides: vec![
                        ScenePrefabOverride {
                            path: "missing".to_string(),
                            ..Default::default()
                        },
                        ScenePrefabOverride {
                            path: "Duplicate".to_string(),
                            ..Default::default()
                        },
                        ScenePrefabOverride {
                            path: String::new(),
                            ..Default::default()
                        },
                    ],
                },
                ..Default::default()
            }],
        };

        let err = scene.validate().unwrap_err();
        let messages: Vec<_> = err
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert!(messages
            .iter()
            .any(|message| message.contains("unknown prefab override path")));
        assert!(messages
            .iter()
            .any(|message| message.contains("ambiguous prefab override path")));
        assert!(messages
            .iter()
            .any(|message| message.contains("prefab override paths must not be empty")));
    }

    #[test]
    fn scene_validation_reports_material_authoring_errors() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: vec![
                SceneMaterialDescriptor {
                    name: "crate".to_string(),
                    ..Default::default()
                },
                SceneMaterialDescriptor {
                    name: "crate".to_string(),
                    ..Default::default()
                },
                SceneMaterialDescriptor {
                    name: "   ".to_string(),
                    ..Default::default()
                },
            ],
            prefabs: Vec::new(),
            entities: Vec::new(),
        };

        let err = scene.validate().unwrap_err();
        let diagnostics = err.diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "materials[1].name"
                && diagnostic.message.contains("duplicate material name")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "materials[2].name"
                && diagnostic.message == "material names must not be empty"
        }));
    }

    #[test]
    fn scene_validation_reports_dependency_authoring_errors() {
        let scene = SceneDescriptor {
            dependencies: vec![
                "materials/stone.oxmat".to_string(),
                " materials/stone.oxmat ".to_string(),
                String::new(),
            ],
            ..Default::default()
        };

        let err = scene.validate().unwrap_err();
        assert!(err.diagnostics().iter().any(|diagnostic| {
            diagnostic.path == "dependencies[1]"
                && diagnostic
                    .message
                    .contains("duplicate scene dependency path")
        }));
        assert!(err.diagnostics().iter().any(|diagnostic| {
            diagnostic.path == "dependencies[2]"
                && diagnostic.message == "scene dependency paths must not be empty"
        }));
    }

    #[test]
    fn load_scene_descriptor_rejects_invalid_prefab_reference() {
        let path = temp_path("invalid_prefab_scene", "oxscene");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxscene",
                "version": 1,
                "scene": {
                    "entities": [
                        {
                            "type": "prefab",
                            "id": "missing"
                        }
                    ]
                }
            }"#,
        )
        .unwrap();

        let err = load_scene_descriptor(&path).unwrap_err();
        let SceneDescriptorError::Validation { source, .. } = err else {
            panic!("expected validation error");
        };
        assert_eq!(source.diagnostics().len(), 1);
        assert_eq!(source.diagnostics()[0].path, "entities[0].id");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn try_spawn_scene_descriptor_rejects_invalid_scene_without_spawning() {
        let scene = SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
            entities: vec![SceneEntityDescriptor {
                kind: SceneEntityKind::Prefab {
                    id: "missing".to_string(),
                    overrides: Vec::new(),
                },
                ..Default::default()
            }],
        };
        let mut world = World::new();

        let err = try_spawn_scene_descriptor(&mut world, &scene).unwrap_err();
        assert_eq!(err.diagnostics()[0].message, "unknown prefab ID 'missing'");
        assert!(world
            .query::<&TransformComponent>()
            .iter(&world)
            .next()
            .is_none());
        assert!(spawn_scene_descriptor(&mut world, &scene).is_empty());
    }

    fn prefab_test_scene() -> SceneDescriptor {
        SceneDescriptor {
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: vec![ScenePrefabDescriptor {
                id: "crate_pair".to_string(),
                entities: vec![SceneEntityDescriptor {
                    name: Some("Crate Base".to_string()),
                    kind: SceneEntityKind::Mesh {
                        primitive: SceneMeshPrimitive::Cube,
                        material: SceneMaterialDescriptor::default(),
                    },
                    children: vec![SceneEntityDescriptor {
                        name: Some("Crate Top".to_string()),
                        transform: SceneTransform::from_position([0.0, 1.2, 0.0]),
                        kind: SceneEntityKind::Mesh {
                            primitive: SceneMeshPrimitive::Cube,
                            material: SceneMaterialDescriptor::default(),
                        },
                        children: Vec::new(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
            }],
            entities: vec![SceneEntityDescriptor {
                name: Some("Crate Pair Instance".to_string()),
                transform: SceneTransform::from_position([2.0, 0.0, -3.0]),
                kind: SceneEntityKind::Prefab {
                    id: "crate_pair".to_string(),
                    overrides: Vec::new(),
                },
                children: vec![SceneEntityDescriptor {
                    name: Some("Instance Marker".to_string()),
                    kind: SceneEntityKind::Empty,
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }
    }

    fn child_named(world: &World, parent: Entity, name: &str) -> Option<Entity> {
        world
            .get::<oxide_transform::Children>(parent)?
            .iter()
            .find(|entity| world.get::<Name>(*entity).map(|n| n.0.as_str()) == Some(name))
    }

    fn temp_path(name: &str, extension: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{stamp}.{extension}"))
    }
}
