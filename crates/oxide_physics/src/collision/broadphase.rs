//! Spatial hashing broadphase for O(n) collision candidate generation.
//!
//! Uses a 3D uniform grid with hashing to efficiently find potential
//! collision pairs without O(n²) comparisons.

use std::collections::{HashMap, HashSet};

use glam::IVec3;

use crate::components::{BodyId, ColliderId};
use crate::resources::{Aabb, PhysicsWorld};

/// A cell in the spatial hash grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellKey(pub IVec3);

impl CellKey {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self(IVec3::new(x, y, z))
    }
}

/// Proxy for a collider in the spatial hash.
#[derive(Clone, Debug)]
pub struct SpatialProxy {
    pub collider_id: ColliderId,
    pub body_id: BodyId,
    pub is_dynamic: bool,
    pub is_sensor: bool,
    pub aabb: Aabb,
}

/// Spatial hash grid for broadphase collision detection.
#[derive(Clone, Debug)]
pub struct SpatialHash {
    /// Cell size in world units.
    pub cell_size: f32,
    /// Inverse cell size for faster computation.
    inv_cell_size: f32,
    /// Map from cell key to list of proxies in that cell.
    cells: HashMap<CellKey, Vec<SpatialProxy>>,
    /// All dynamic proxies for quick iteration.
    dynamic_proxies: Vec<SpatialProxy>,
    /// All sensor proxies for trigger/event processing.
    sensor_proxies: Vec<SpatialProxy>,
}

impl Default for SpatialHash {
    fn default() -> Self {
        Self::new(2.0)
    }
}

impl SpatialHash {
    /// Create a new spatial hash with the given cell size.
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(0.1),
            inv_cell_size: 1.0 / cell_size.max(0.1),
            cells: HashMap::new(),
            dynamic_proxies: Vec::new(),
            sensor_proxies: Vec::new(),
        }
    }

    /// Clear all proxies from the grid.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.dynamic_proxies.clear();
        self.sensor_proxies.clear();
    }

    /// Convert a world position to a cell key.
    pub fn world_to_cell(&self, pos: glam::Vec3) -> CellKey {
        CellKey(IVec3::new(
            (pos.x * self.inv_cell_size).floor() as i32,
            (pos.y * self.inv_cell_size).floor() as i32,
            (pos.z * self.inv_cell_size).floor() as i32,
        ))
    }

    /// Insert a proxy into the grid.
    pub fn insert(&mut self, proxy: SpatialProxy) {
        let min_cell = self.world_to_cell(proxy.aabb.min);
        let max_cell = self.world_to_cell(proxy.aabb.max);

        // Add to all cells the AABB overlaps
        for x in min_cell.0.x..=max_cell.0.x {
            for y in min_cell.0.y..=max_cell.0.y {
                for z in min_cell.0.z..=max_cell.0.z {
                    let key = CellKey::new(x, y, z);
                    self.cells.entry(key).or_default().push(proxy.clone());
                }
            }
        }

        // Track dynamic proxies separately.
        if proxy.is_dynamic {
            self.dynamic_proxies.push(proxy.clone());
        }

        // Track sensor proxies so static trigger pairs still get evaluated.
        if proxy.is_sensor {
            self.sensor_proxies.push(proxy);
        }
    }

    /// Generate all potential collision pairs.
    /// Returns pairs as (collider_a, collider_b) where at least one is dynamic.
    pub fn find_pairs(&self) -> Vec<(ColliderId, ColliderId)> {
        let mut pairs = Vec::new();
        let mut visited: HashSet<(ColliderId, ColliderId)> = HashSet::new();

        // For each active proxy, check cells it overlaps
        for query_proxy in self
            .dynamic_proxies
            .iter()
            .chain(self.sensor_proxies.iter())
        {
            let min_cell = self.world_to_cell(query_proxy.aabb.min);
            let max_cell = self.world_to_cell(query_proxy.aabb.max);

            for x in min_cell.0.x..=max_cell.0.x {
                for y in min_cell.0.y..=max_cell.0.y {
                    for z in min_cell.0.z..=max_cell.0.z {
                        let key = CellKey::new(x, y, z);
                        if let Some(proxies) = self.cells.get(&key) {
                            for other in proxies {
                                // Skip same body
                                if other.body_id == query_proxy.body_id {
                                    continue;
                                }
                                // Skip static-static unless a sensor is involved.
                                if !other.is_dynamic
                                    && !query_proxy.is_dynamic
                                    && !other.is_sensor
                                    && !query_proxy.is_sensor
                                {
                                    continue;
                                }
                                // Skip if AABBs don't overlap
                                if !query_proxy.aabb.intersects(&other.aabb) {
                                    continue;
                                }

                                // Create ordered pair to avoid duplicates
                                let pair = if query_proxy.collider_id.0 <= other.collider_id.0 {
                                    (query_proxy.collider_id, other.collider_id)
                                } else {
                                    (other.collider_id, query_proxy.collider_id)
                                };

                                visited.insert(pair);
                            }
                        }
                    }
                }
            }
        }

        pairs.extend(visited.iter().copied());
        pairs
    }

    /// Get the number of cells in the grid.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Get statistics about the grid.
    pub fn stats(&self) -> SpatialHashStats {
        let mut total_proxies = 0;
        let mut max_proxies_per_cell = 0;

        for proxies in self.cells.values() {
            total_proxies += proxies.len();
            max_proxies_per_cell = max_proxies_per_cell.max(proxies.len());
        }

        SpatialHashStats {
            cell_count: self.cells.len(),
            total_proxy_entries: total_proxies,
            max_proxies_per_cell,
            dynamic_proxy_count: self.dynamic_proxies.len(),
        }
    }
}

/// Statistics about the spatial hash grid.
#[derive(Clone, Copy, Debug)]
pub struct SpatialHashStats {
    pub cell_count: usize,
    pub total_proxy_entries: usize,
    pub max_proxies_per_cell: usize,
    pub dynamic_proxy_count: usize,
}

/// Build a spatial hash from a physics world.
pub fn build_spatial_hash(physics: &PhysicsWorld, cell_size: f32) -> SpatialHash {
    let mut hash = SpatialHash::new(cell_size);

    for collider in physics.colliders.values() {
        let Some(body) = physics.body(collider.body_id) else {
            continue;
        };

        let aabb = super::shape_to_aabb(collider.shape, body.position, body.rotation);
        let proxy = SpatialProxy {
            collider_id: collider.id,
            body_id: collider.body_id,
            is_dynamic: body.is_dynamic(),
            is_sensor: collider.is_sensor,
            aabb,
        };

        hash.insert(proxy);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn make_proxy(
        id: u64,
        pos: Vec3,
        size: f32,
        is_dynamic: bool,
        is_sensor: bool,
    ) -> SpatialProxy {
        SpatialProxy {
            collider_id: ColliderId(id),
            body_id: BodyId(id),
            is_dynamic,
            is_sensor,
            aabb: Aabb {
                min: pos - Vec3::splat(size),
                max: pos + Vec3::splat(size),
            },
        }
    }

    #[test]
    fn empty_hash_has_no_pairs() {
        let hash = SpatialHash::new(2.0);
        let pairs = hash.find_pairs();
        assert!(pairs.is_empty());
    }

    #[test]
    fn single_dynamic_no_pairs() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::ZERO, 0.5, true, false));
        let pairs = hash.find_pairs();
        assert!(pairs.is_empty());
    }

    #[test]
    fn overlapping_bodies_generate_pair() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::new(-0.5, 0.0, 0.0), 0.5, true, false));
        hash.insert(make_proxy(2, Vec3::new(0.5, 0.0, 0.0), 0.5, true, false));

        let pairs = hash.find_pairs();
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn separated_bodies_no_pair() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::new(-10.0, 0.0, 0.0), 0.5, true, false));
        hash.insert(make_proxy(2, Vec3::new(10.0, 0.0, 0.0), 0.5, true, false));

        let pairs = hash.find_pairs();
        assert!(pairs.is_empty());
    }

    #[test]
    fn static_static_no_pair() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::new(-0.5, 0.0, 0.0), 0.5, false, false));
        hash.insert(make_proxy(2, Vec3::new(0.5, 0.0, 0.0), 0.5, false, false));

        let pairs = hash.find_pairs();
        assert!(pairs.is_empty());
    }

    #[test]
    fn dynamic_static_generates_pair() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::new(-0.5, 0.0, 0.0), 0.5, true, false));
        hash.insert(make_proxy(2, Vec3::new(0.5, 0.0, 0.0), 0.5, false, false));

        let pairs = hash.find_pairs();
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn cell_key_conversion() {
        let hash = SpatialHash::new(2.0);

        // Origin should be cell (0, 0, 0)
        let cell = hash.world_to_cell(Vec3::ZERO);
        assert_eq!(cell, CellKey::new(0, 0, 0));

        // Point at (1.9, 1.9, 1.9) should still be cell (0, 0, 0)
        let cell = hash.world_to_cell(Vec3::new(1.9, 1.9, 1.9));
        assert_eq!(cell, CellKey::new(0, 0, 0));

        // Point at (2.0, 2.0, 2.0) should be cell (1, 1, 1)
        let cell = hash.world_to_cell(Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(cell, CellKey::new(1, 1, 1));

        // Negative coordinates
        let cell = hash.world_to_cell(Vec3::new(-2.0, -2.0, -2.0));
        assert_eq!(cell, CellKey::new(-1, -1, -1));
    }

    #[test]
    fn no_duplicate_pairs() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::ZERO, 1.0, true, false));
        hash.insert(make_proxy(2, Vec3::ZERO, 1.0, true, false));

        // Even though they overlap multiple cells, should only generate one pair
        let pairs = hash.find_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (ColliderId(1), ColliderId(2)));
    }

    #[test]
    fn static_sensor_pairs_generate_pairs() {
        let mut hash = SpatialHash::new(2.0);
        hash.insert(make_proxy(1, Vec3::ZERO, 0.5, false, true));
        hash.insert(make_proxy(2, Vec3::new(0.25, 0.0, 0.0), 0.5, false, true));

        let pairs = hash.find_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (ColliderId(1), ColliderId(2)));
    }
}
