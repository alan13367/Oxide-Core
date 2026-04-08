//! Collision detection types and algorithms.
//!
//! Provides contact manifold generation, Sutherland-Hodgman clipping,
//! and contact point caching for warm starting.

pub mod broadphase;

use glam::{Quat, Vec3};

use crate::components::{BodyId, ColliderId, ColliderShape};
use crate::resources::Aabb;

pub use broadphase::*;

/// Unique identifier for a contact point, used for warm start matching.
/// Derived from the feature pair (face/edge indices) that generated this contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactId(pub u32);

impl ContactId {
    /// Create a contact ID from two feature IDs.
    /// The ordering ensures the same pair always produces the same ID.
    pub fn from_features(feature_a: u16, feature_b: u16) -> Self {
        let (lo, hi) = if feature_a <= feature_b {
            (feature_a, feature_b)
        } else {
            (feature_b, feature_a)
        };
        ContactId((lo as u32) << 16 | hi as u32)
    }
}

/// A single contact point between two bodies.
#[derive(Clone, Copy, Debug)]
pub struct ContactPoint {
    /// Unique ID for this contact, used for warm start matching.
    pub id: ContactId,
    /// World-space position of the contact point.
    pub position: Vec3,
    /// Normal direction (from body A to body B).
    pub normal: Vec3,
    /// Penetration depth (positive means overlapping).
    pub penetration: f32,
    /// Accumulated normal impulse (cached for warm starting).
    pub normal_impulse: f32,
    /// Accumulated tangent impulse (cached for warm starting).
    pub tangent_impulse: Vec3,
}

impl Default for ContactPoint {
    fn default() -> Self {
        Self {
            id: ContactId(0),
            position: Vec3::ZERO,
            normal: Vec3::Y,
            penetration: 0.0,
            normal_impulse: 0.0,
            tangent_impulse: Vec3::ZERO,
        }
    }
}

/// A contact manifold containing all contact points between two bodies.
#[derive(Clone, Debug)]
pub struct ContactManifold {
    /// Body A identifier.
    pub body_a: BodyId,
    /// Body B identifier.
    pub body_b: BodyId,
    /// Collider A identifier.
    pub collider_a: ColliderId,
    /// Collider B identifier.
    pub collider_b: ColliderId,
    /// Contact points (up to 4 for box-box collision).
    pub contacts: Vec<ContactPoint>,
    /// Combined friction coefficient.
    pub friction: f32,
    /// Combined restitution coefficient.
    pub restitution: f32,
}

impl ContactManifold {
    /// Maximum number of contact points in a manifold.
    pub const MAX_CONTACTS: usize = 4;

    /// Create a new empty manifold.
    pub fn new(
        body_a: BodyId,
        body_b: BodyId,
        collider_a: ColliderId,
        collider_b: ColliderId,
    ) -> Self {
        Self {
            body_a,
            body_b,
            collider_a,
            collider_b,
            contacts: Vec::with_capacity(Self::MAX_CONTACTS),
            friction: 0.5,
            restitution: 0.0,
        }
    }

    /// Add a contact point to the manifold.
    /// If the manifold is full, removes the contact with least penetration.
    pub fn add_contact(&mut self, contact: ContactPoint) {
        if self.contacts.len() < Self::MAX_CONTACTS {
            self.contacts.push(contact);
        } else {
            // Find and replace the contact with least penetration
            let min_idx = self
                .contacts
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.penetration
                        .partial_cmp(&b.penetration)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i);

            if let Some(idx) = min_idx {
                if contact.penetration > self.contacts[idx].penetration {
                    self.contacts[idx] = contact;
                }
            }
        }
    }

    /// Transfer cached impulses from a previous manifold for warm starting.
    /// Matches contacts by ID.
    pub fn warm_start_from(&mut self, previous: &ContactManifold) {
        for contact in &mut self.contacts {
            if let Some(prev_contact) = previous.contacts.iter().find(|c| c.id == contact.id) {
                contact.normal_impulse = prev_contact.normal_impulse;
                contact.tangent_impulse = prev_contact.tangent_impulse;
            }
        }
    }

    /// Check if the manifold has any contacts.
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
    }
}

/// Clip a polygon against a plane using Sutherland-Hodgman algorithm.
///
/// # Arguments
/// * `vertices` - Input polygon vertices
/// * `plane_normal` - Normal of the clipping plane
/// * `plane_point` - A point on the clipping plane
///
/// # Returns
/// Clipped polygon vertices
pub fn clip_polygon_against_plane(
    vertices: &[Vec3],
    plane_normal: Vec3,
    plane_point: Vec3,
) -> Vec<Vec3> {
    if vertices.is_empty() {
        return Vec::new();
    }

    let mut output = Vec::with_capacity(vertices.len() + 1);

    for i in 0..vertices.len() {
        let current = vertices[i];
        let next = vertices[(i + 1) % vertices.len()];

        let current_dist = (current - plane_point).dot(plane_normal);
        let next_dist = (next - plane_point).dot(plane_normal);

        let current_inside = current_dist >= 0.0;
        let next_inside = next_dist >= 0.0;

        if current_inside {
            output.push(current);
        }

        // If edge crosses the plane, compute intersection point
        if current_inside != next_inside {
            let t = current_dist / (current_dist - next_dist);
            let intersection = current + t * (next - current);
            output.push(intersection);
        }
    }

    output
}

/// Generate contact points for box-box collision using clipping.
///
/// Uses the reference/incident face approach:
/// 1. Find the reference face (face with most aligned normal)
/// 2. Find the incident face on the other box
/// 3. Clip the incident face against the reference face's side planes
/// 4. Keep points that are behind the reference face
pub fn generate_box_box_contacts(
    center_a: Vec3,
    axes_a: [Vec3; 3],
    half_extents_a: Vec3,
    center_b: Vec3,
    axes_b: [Vec3; 3],
    half_extents_b: Vec3,
    normal: Vec3,
    penetration: f32,
) -> Vec<ContactPoint> {
    // Determine reference and incident faces
    // The reference face is the one most aligned with the collision normal
    let (
        ref_center,
        ref_axes,
        ref_extents,
        incident_center,
        incident_axes,
        incident_extents,
        flip_normal,
    ) = if normal
        .abs()
        .dot(axes_a[0].abs())
        .max(normal.abs().dot(axes_a[1].abs()))
        .max(normal.abs().dot(axes_a[2].abs()))
        >= normal
            .abs()
            .dot(axes_b[0].abs())
            .max(normal.abs().dot(axes_b[1].abs()))
            .max(normal.abs().dot(axes_b[2].abs()))
    {
        // A is reference
        (
            center_a,
            axes_a,
            half_extents_a,
            center_b,
            axes_b,
            half_extents_b,
            false,
        )
    } else {
        // B is reference, flip everything
        (
            center_b,
            axes_b,
            half_extents_b,
            center_a,
            axes_a,
            half_extents_a,
            true,
        )
    };

    // Find the reference face index
    let ref_face_idx = find_most_aligned_face(&normal, &ref_axes);
    let ref_face_normal = ref_axes[ref_face_idx];
    let ref_face_sign = if normal.dot(ref_face_normal) > 0.0 {
        1.0
    } else {
        -1.0
    };

    // Get the incident face vertices
    let incident_direction = -ref_face_normal * ref_face_sign;
    let incident_face_idx = find_most_aligned_face(&incident_direction, &incident_axes);
    let incident_face_sign = if incident_direction.dot(incident_axes[incident_face_idx]) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let incident_vertices = get_face_vertices(
        incident_center,
        &incident_axes,
        incident_extents,
        incident_face_idx,
        incident_face_sign,
    );

    // Clip against reference face's side planes
    let mut clipped = incident_vertices;

    // Get the four side planes of the reference face
    let tangent_axes = [
        ref_axes[(ref_face_idx + 1) % 3],
        ref_axes[(ref_face_idx + 2) % 3],
    ];

    let ref_face_center = ref_center + ref_face_normal * ref_face_sign * ref_extents[ref_face_idx];

    // Clip against each of the 4 side planes
    for &tangent in &tangent_axes {
        for sign in [-1.0, 1.0] {
            let plane_normal = tangent * sign;
            let plane_point = ref_face_center
                + tangent
                    * sign
                    * ref_extents[(ref_face_idx + 1) % 3].max(ref_extents[(ref_face_idx + 2) % 3]);
            clipped = clip_polygon_against_plane(&clipped, plane_normal, plane_point);
        }
    }

    // Keep points that are behind the reference face
    let mut contacts = Vec::new();
    for vertex in clipped {
        let dist = (vertex - ref_face_center).dot(ref_face_normal * ref_face_sign);
        if dist <= 0.0 {
            let contact_normal = if flip_normal { -normal } else { normal };
            let contact = ContactPoint {
                id: ContactId::from_features(
                    (ref_face_idx as u16) << 8 | (incident_face_idx as u16),
                    vertex.to_array().iter().map(|&v| v.to_bits() as u16).sum(),
                ),
                position: vertex,
                normal: contact_normal,
                penetration: penetration,
                normal_impulse: 0.0,
                tangent_impulse: Vec3::ZERO,
            };
            contacts.push(contact);
        }
    }

    // Limit to 4 contacts
    if contacts.len() > 4 {
        // Keep the deepest contacts
        contacts.sort_by(|a, b| {
            b.penetration
                .partial_cmp(&a.penetration)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        contacts.truncate(4);
    }

    contacts
}

/// Find the face index most aligned with a given direction.
fn find_most_aligned_face(direction: &Vec3, axes: &[Vec3; 3]) -> usize {
    let mut best_idx = 0;
    let mut best_dot = 0.0;

    for (i, axis) in axes.iter().enumerate() {
        let dot = direction.dot(*axis).abs();
        if dot > best_dot {
            best_dot = dot;
            best_idx = i;
        }
    }

    best_idx
}

/// Get the 4 vertices of a face on an OBB.
fn get_face_vertices(
    center: Vec3,
    axes: &[Vec3; 3],
    half_extents: Vec3,
    face_idx: usize,
    face_sign: f32,
) -> Vec<Vec3> {
    let face_normal = axes[face_idx];

    let tangent1 = axes[(face_idx + 1) % 3];
    let tangent2 = axes[(face_idx + 2) % 3];

    let ext1 = half_extents[(face_idx + 1) % 3];
    let ext2 = half_extents[(face_idx + 2) % 3];
    let face_dist = half_extents[face_idx];

    let face_center = center + face_normal * face_sign * face_dist;

    vec![
        face_center + tangent1 * ext1 + tangent2 * ext2,
        face_center + tangent1 * ext1 - tangent2 * ext2,
        face_center - tangent1 * ext1 - tangent2 * ext2,
        face_center - tangent1 * ext1 + tangent2 * ext2,
    ]
}

/// Compute AABB for a shape at a given position and rotation.
pub fn shape_to_aabb(shape: ColliderShape, position: Vec3, rotation: Quat) -> Aabb {
    match shape {
        ColliderShape::Sphere { radius } => {
            let extents = Vec3::splat(radius);
            Aabb {
                min: position - extents,
                max: position + extents,
            }
        }
        ColliderShape::Cuboid { half_extents } => {
            let axis_x = rotation * Vec3::X;
            let axis_y = rotation * Vec3::Y;
            let axis_z = rotation * Vec3::Z;
            let extents = Vec3::new(
                axis_x.x.abs() * half_extents.x
                    + axis_y.x.abs() * half_extents.y
                    + axis_z.x.abs() * half_extents.z,
                axis_x.y.abs() * half_extents.x
                    + axis_y.y.abs() * half_extents.y
                    + axis_z.y.abs() * half_extents.z,
                axis_x.z.abs() * half_extents.x
                    + axis_y.z.abs() * half_extents.y
                    + axis_z.z.abs() * half_extents.z,
            );
            Aabb {
                min: position - extents,
                max: position + extents,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_id_is_order_independent() {
        let id1 = ContactId::from_features(1, 2);
        let id2 = ContactId::from_features(2, 1);
        assert_eq!(id1, id2, "contact ID should be order-independent");
    }

    #[test]
    fn manifold_adds_contacts() {
        let mut manifold = ContactManifold::new(BodyId(1), BodyId(2), ColliderId(1), ColliderId(2));

        let contact = ContactPoint {
            id: ContactId(1),
            position: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::Y,
            penetration: 0.1,
            normal_impulse: 0.0,
            tangent_impulse: Vec3::ZERO,
        };

        manifold.add_contact(contact);
        assert_eq!(manifold.contacts.len(), 1);
    }

    #[test]
    fn manifold_limits_contacts() {
        let mut manifold = ContactManifold::new(BodyId(1), BodyId(2), ColliderId(1), ColliderId(2));

        // Add 6 contacts with varying penetration
        for i in 0..6 {
            manifold.add_contact(ContactPoint {
                id: ContactId(i as u32),
                position: Vec3::ZERO,
                normal: Vec3::Y,
                penetration: i as f32 * 0.1,
                normal_impulse: 0.0,
                tangent_impulse: Vec3::ZERO,
            });
        }

        assert_eq!(manifold.contacts.len(), 4);
    }

    #[test]
    fn clip_polygon_retains_inside_points() {
        let vertices = vec![
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
        ];

        // Clip against Y=0 plane (should keep all points)
        let clipped = clip_polygon_against_plane(&vertices, Vec3::Y, Vec3::ZERO);
        assert_eq!(clipped.len(), 4);
    }

    #[test]
    fn clip_polygon_removes_outside_points() {
        let vertices = vec![
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(-1.0, -1.0, 0.0),
        ];

        // Clip against X=0 plane (keeps right half)
        let clipped = clip_polygon_against_plane(&vertices, Vec3::X, Vec3::ZERO);
        assert!(clipped.len() >= 3, "should have at least 3 vertices");
    }

    #[test]
    fn warm_start_transfers_impulses() {
        let mut prev_manifold =
            ContactManifold::new(BodyId(1), BodyId(2), ColliderId(1), ColliderId(2));

        prev_manifold.add_contact(ContactPoint {
            id: ContactId(42),
            position: Vec3::ZERO,
            normal: Vec3::Y,
            penetration: 0.1,
            normal_impulse: 5.0,
            tangent_impulse: Vec3::new(1.0, 0.0, 0.0),
        });

        let mut new_manifold =
            ContactManifold::new(BodyId(1), BodyId(2), ColliderId(1), ColliderId(2));

        new_manifold.add_contact(ContactPoint {
            id: ContactId(42),
            position: Vec3::ZERO,
            normal: Vec3::Y,
            penetration: 0.15,
            normal_impulse: 0.0,
            tangent_impulse: Vec3::ZERO,
        });

        new_manifold.warm_start_from(&prev_manifold);

        assert_eq!(new_manifold.contacts[0].normal_impulse, 5.0);
        assert_eq!(
            new_manifold.contacts[0].tangent_impulse,
            Vec3::new(1.0, 0.0, 0.0)
        );
    }
}
