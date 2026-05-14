//! Terrain heightfields and configurable world descriptors.

use glam::{Vec2, Vec3};
use oxide_ecs::world::World;
use oxide_ecs::{Component, Resource};
use oxide_math::transform::Transform;
use oxide_transform::{GlobalTransform, TransformComponent};
use serde::{Deserialize, Serialize};

use crate::{Name, RenderMaterial};

#[derive(Component, Clone, Debug)]
pub struct Terrain {
    pub width: f32,
    pub depth: f32,
    pub columns: u32,
    pub rows: u32,
    pub heights: Vec<f32>,
    pub tint: [f32; 4],
    pub material: RenderMaterial,
    revision: u64,
}

impl Terrain {
    pub fn flat(width: f32, depth: f32, columns: u32, rows: u32) -> Self {
        let columns = columns.clamp(1, 254);
        let rows = rows.clamp(1, 254);
        Self {
            width: width.max(0.1),
            depth: depth.max(0.1),
            columns,
            rows,
            heights: vec![0.0; (columns + 1) as usize * (rows + 1) as usize],
            tint: [0.36, 0.42, 0.34, 1.0],
            material: RenderMaterial::default(),
            revision: 1,
        }
    }

    pub fn from_height_fn(
        width: f32,
        depth: f32,
        columns: u32,
        rows: u32,
        mut height_at: impl FnMut(f32, f32) -> f32,
    ) -> Self {
        let mut terrain = Self::flat(width, depth, columns, rows);
        for row in 0..=terrain.rows {
            for column in 0..=terrain.columns {
                let x = column as f32 / terrain.columns as f32 - 0.5;
                let z = row as f32 / terrain.rows as f32 - 0.5;
                let index = terrain.index(column, row);
                terrain.heights[index] = height_at(x, z);
            }
        }
        terrain.revision = terrain.revision.saturating_add(1);
        terrain
    }

    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    pub fn with_material(mut self, material: RenderMaterial) -> Self {
        self.material = material;
        self
    }

    pub fn set_height(&mut self, column: u32, row: u32, height: f32) {
        if column > self.columns || row > self.rows {
            return;
        }
        let index = self.index(column, row);
        self.heights[index] = height;
        self.revision = self.revision.saturating_add(1);
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn sample_height(&self, local_x: f32, local_z: f32) -> f32 {
        let u = ((local_x / self.width) + 0.5).clamp(0.0, 1.0) * self.columns as f32;
        let v = ((local_z / self.depth) + 0.5).clamp(0.0, 1.0) * self.rows as f32;

        let x0 = u.floor() as u32;
        let z0 = v.floor() as u32;
        let x1 = (x0 + 1).min(self.columns);
        let z1 = (z0 + 1).min(self.rows);
        let tx = u - x0 as f32;
        let tz = v - z0 as f32;

        let h00 = self.height_at(x0, z0);
        let h10 = self.height_at(x1, z0);
        let h01 = self.height_at(x0, z1);
        let h11 = self.height_at(x1, z1);
        let h0 = h00 + (h10 - h00) * tx;
        let h1 = h01 + (h11 - h01) * tx;
        h0 + (h1 - h0) * tz
    }

    pub fn height_at(&self, column: u32, row: u32) -> f32 {
        self.heights[self.index(column.min(self.columns), row.min(self.rows))]
    }

    fn index(&self, column: u32, row: u32) -> usize {
        row as usize * (self.columns + 1) as usize + column as usize
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerrainDescriptor {
    #[serde(default = "default_terrain_width")]
    pub width: f32,
    #[serde(default = "default_terrain_depth")]
    pub depth: f32,
    #[serde(default = "default_terrain_columns")]
    pub columns: u32,
    #[serde(default = "default_terrain_rows")]
    pub rows: u32,
    #[serde(default = "default_terrain_height_scale")]
    pub height_scale: f32,
    #[serde(default)]
    pub waves: Vec<TerrainWaveDescriptor>,
    #[serde(default = "default_terrain_tint")]
    pub tint: [f32; 4],
}

impl Default for TerrainDescriptor {
    fn default() -> Self {
        Self {
            width: default_terrain_width(),
            depth: default_terrain_depth(),
            columns: default_terrain_columns(),
            rows: default_terrain_rows(),
            height_scale: default_terrain_height_scale(),
            waves: Vec::new(),
            tint: default_terrain_tint(),
        }
    }
}

impl TerrainDescriptor {
    pub fn build(&self) -> Terrain {
        Terrain::from_height_fn(self.width, self.depth, self.columns, self.rows, |x, z| {
            let mut height = 0.0;
            for wave in &self.waves {
                let direction = Vec2::from_array(wave.direction).normalize_or_zero();
                let phase = (x * direction.x + z * direction.y) * wave.frequency + wave.phase;
                height += phase.sin() * wave.amplitude;
            }
            height * self.height_scale
        })
        .with_tint(self.tint)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TerrainWaveDescriptor {
    #[serde(default = "default_wave_direction")]
    pub direction: [f32; 2],
    #[serde(default = "default_wave_frequency")]
    pub frequency: f32,
    #[serde(default = "default_wave_amplitude")]
    pub amplitude: f32,
    #[serde(default)]
    pub phase: f32,
}

impl Default for TerrainWaveDescriptor {
    fn default() -> Self {
        Self {
            direction: default_wave_direction(),
            frequency: default_wave_frequency(),
            amplitude: default_wave_amplitude(),
            phase: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneWorldDescriptor {
    #[serde(default)]
    pub terrain: TerrainDescriptor,
    #[serde(default)]
    pub objects: Vec<WorldObjectDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldObjectDescriptor {
    pub name: String,
    pub position: [f32; 3],
    pub size: [f32; 3],
    #[serde(default = "default_object_tint")]
    pub tint: [f32; 4],
    #[serde(default)]
    pub solid: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct SceneWorldSpawnResult {
    pub terrain: Option<oxide_ecs::entity::Entity>,
    pub objects: Vec<oxide_ecs::entity::Entity>,
}

pub fn spawn_world_descriptor(
    world: &mut World,
    descriptor: &SceneWorldDescriptor,
) -> SceneWorldSpawnResult {
    let terrain_entity = world
        .spawn((
            Name("Terrain".to_string()),
            TransformComponent::new(Transform::default()),
            GlobalTransform::default(),
            descriptor.terrain.build(),
        ))
        .id();

    let objects = descriptor
        .objects
        .iter()
        .map(|object| {
            world
                .spawn((
                    Name(object.name.clone()),
                    TransformComponent::new(Transform {
                        position: Vec3::from_array(object.position),
                        scale: Vec3::from_array(object.size),
                        ..Default::default()
                    }),
                    GlobalTransform::default(),
                    crate::RenderMesh::new(crate::MeshPrimitive::Cube, RenderMaterial::default())
                        .with_tint(object.tint),
                ))
                .id()
        })
        .collect();

    SceneWorldSpawnResult {
        terrain: Some(terrain_entity),
        objects,
    }
}

fn default_terrain_width() -> f32 {
    48.0
}

fn default_terrain_depth() -> f32 {
    48.0
}

fn default_terrain_columns() -> u32 {
    48
}

fn default_terrain_rows() -> u32 {
    48
}

fn default_terrain_height_scale() -> f32 {
    1.0
}

fn default_terrain_tint() -> [f32; 4] {
    [0.34, 0.4, 0.32, 1.0]
}

fn default_wave_direction() -> [f32; 2] {
    [1.0, 0.0]
}

fn default_wave_frequency() -> f32 {
    3.0
}

fn default_wave_amplitude() -> f32 {
    0.08
}

fn default_object_tint() -> [f32; 4] {
    [0.38, 0.33, 0.27, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_samples_interpolated_height() {
        let mut terrain = Terrain::flat(10.0, 10.0, 1, 1);
        terrain.set_height(0, 0, 0.0);
        terrain.set_height(1, 0, 2.0);
        terrain.set_height(0, 1, 2.0);
        terrain.set_height(1, 1, 4.0);

        assert!((terrain.sample_height(0.0, 0.0) - 2.0).abs() < 0.001);
    }

    #[test]
    fn world_descriptor_spawns_terrain_and_objects() {
        let mut world = World::new();
        let result = spawn_world_descriptor(
            &mut world,
            &SceneWorldDescriptor {
                terrain: TerrainDescriptor::default(),
                objects: vec![WorldObjectDescriptor {
                    name: "crate".to_string(),
                    position: [0.0, 0.5, 0.0],
                    size: [1.0, 1.0, 1.0],
                    tint: [1.0, 0.0, 0.0, 1.0],
                    solid: true,
                }],
            },
        );

        assert!(result.terrain.is_some());
        assert_eq!(result.objects.len(), 1);
    }
}
