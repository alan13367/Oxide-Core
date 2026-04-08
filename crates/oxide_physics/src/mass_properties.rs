//! Mass and inertia tensor calculations for rigid body physics.
//!
//! Provides functions to compute mass properties from collider shapes and density.
//! Inertia tensors are computed in local space and can be transformed to world space.

use glam::{Mat3, Quat, Vec3};

use crate::components::ColliderShape;

/// Default density for rigid bodies (kg/m³).
/// Water is approximately 1000 kg/m³, making this a reasonable default.
pub const DEFAULT_DENSITY: f32 = 1000.0;

/// Mass properties for a rigid body.
#[derive(Clone, Copy, Debug)]
pub struct MassProperties {
    /// Total mass in kilograms.
    pub mass: f32,
    /// Inverse mass (1.0 / mass). Zero for non-dynamic bodies.
    pub inverse_mass: f32,
    /// Local-space inertia tensor (diagonal matrix for symmetric shapes).
    /// For spheres and cuboids, this is a diagonal matrix.
    pub local_inertia: Mat3,
    /// Inverse of the local inertia tensor.
    pub local_inverse_inertia: Mat3,
}

impl Default for MassProperties {
    fn default() -> Self {
        Self {
            mass: 1.0,
            inverse_mass: 1.0,
            local_inertia: Mat3::IDENTITY,
            local_inverse_inertia: Mat3::IDENTITY,
        }
    }
}

impl MassProperties {
    /// Create mass properties for a non-dynamic body (infinite mass).
    pub fn infinite() -> Self {
        Self {
            mass: 0.0,
            inverse_mass: 0.0,
            local_inertia: Mat3::ZERO,
            local_inverse_inertia: Mat3::ZERO,
        }
    }

    /// Create mass properties from mass and local inertia tensor.
    pub fn new(mass: f32, local_inertia: Mat3) -> Self {
        let inverse_mass = if mass > f32::EPSILON { 1.0 / mass } else { 0.0 };
        let local_inverse_inertia = invert_inertia_tensor(&local_inertia);
        Self {
            mass,
            inverse_mass,
            local_inertia,
            local_inverse_inertia,
        }
    }

    /// Create mass properties from a single shape and density.
    pub fn from_shape(shape: ColliderShape, density: f32) -> Self {
        let mass = calculate_mass(shape, density);
        let local_inertia = calculate_inertia_tensor(shape, mass);
        Self::new(mass, local_inertia)
    }

    /// Transform the local inverse inertia tensor to world space.
    ///
    /// The world-space inverse inertia is: R * I_local^{-1} * R^T
    /// where R is the rotation matrix derived from the quaternion.
    pub fn world_inverse_inertia(&self, rotation: Quat) -> Mat3 {
        if self.mass <= f32::EPSILON {
            return Mat3::ZERO;
        }

        let rot_mat = Mat3::from_quat(rotation);
        rot_mat * self.local_inverse_inertia * rot_mat.transpose()
    }
}

/// Calculate the mass of a shape given its density.
///
/// # Arguments
/// * `shape` - The collider shape
/// * `density` - Density in kg/m³
///
/// # Returns
/// Mass in kilograms
pub fn calculate_mass(shape: ColliderShape, density: f32) -> f32 {
    let volume = match shape {
        ColliderShape::Sphere { radius } => {
            // Volume = (4/3) * π * r³
            (4.0 / 3.0) * std::f32::consts::PI * radius * radius * radius
        }
        ColliderShape::Cuboid { half_extents } => {
            // Volume = (2*ex) * (2*ey) * (2*ez) = 8 * ex * ey * ez
            8.0 * half_extents.x * half_extents.y * half_extents.z
        }
    };

    volume * density
}

/// Calculate the local-space inertia tensor for a shape.
///
/// The inertia tensor describes how mass is distributed relative to the center of mass.
/// For symmetric shapes (sphere, cuboid), the tensor is diagonal in local space.
///
/// # Arguments
/// * `shape` - The collider shape
/// * `mass` - Total mass in kilograms
///
/// # Returns
/// 3x3 inertia tensor matrix in local space (diagonal for symmetric shapes)
pub fn calculate_inertia_tensor(shape: ColliderShape, mass: f32) -> Mat3 {
    if mass <= f32::EPSILON {
        return Mat3::ZERO;
    }

    match shape {
        ColliderShape::Sphere { radius } => {
            // I = (2/5) * m * r² for a solid sphere
            // The inertia tensor is a scalar times identity matrix
            let i = (2.0 / 5.0) * mass * radius * radius;
            Mat3::from_diagonal(Vec3::splat(i))
        }
        ColliderShape::Cuboid { half_extents } => {
            // For a solid cuboid with dimensions (2*ex, 2*ey, 2*ez):
            // I_xx = (m/12) * ((2*ey)² + (2*ez)²) = (m/3) * (ey² + ez²)
            // I_yy = (m/12) * ((2*ex)² + (2*ez)²) = (m/3) * (ex² + ez²)
            // I_zz = (m/12) * ((2*ex)² + (2*ey)²) = (m/3) * (ex² + ey²)
            // where ex, ey, ez are half-extents.
            let ex = half_extents.x;
            let ey = half_extents.y;
            let ez = half_extents.z;

            let factor = mass / 3.0;
            let i_xx = factor * (ey * ey + ez * ez);
            let i_yy = factor * (ex * ex + ez * ez);
            let i_zz = factor * (ex * ex + ey * ey);

            Mat3::from_diagonal(Vec3::new(i_xx, i_yy, i_zz))
        }
    }
}

/// Invert an inertia tensor, handling near-zero diagonal elements.
///
/// For diagonal or near-diagonal tensors, this simply inverts each diagonal element.
/// Returns a zero matrix if the tensor is singular.
fn invert_inertia_tensor(tensor: &Mat3) -> Mat3 {
    // Check if tensor is approximately diagonal
    let diag = tensor.to_cols_array();
    let off_diag = [
        diag[1], // (0,1)
        diag[2], // (0,2)
        diag[5], // (1,2)
        diag[3], // (1,0)
        diag[6], // (2,0)
        diag[7], // (2,1)
    ];

    let is_diagonal = off_diag.iter().all(|&v| v.abs() < 1e-6);

    if is_diagonal {
        // Invert diagonal elements
        let cols = tensor.to_cols_array();
        let diag_vals = [cols[0], cols[4], cols[8]];

        let inv_diag = [
            if diag_vals[0].abs() > 1e-10 {
                1.0 / diag_vals[0]
            } else {
                0.0
            },
            if diag_vals[1].abs() > 1e-10 {
                1.0 / diag_vals[1]
            } else {
                0.0
            },
            if diag_vals[2].abs() > 1e-10 {
                1.0 / diag_vals[2]
            } else {
                0.0
            },
        ];

        Mat3::from_diagonal(Vec3::new(inv_diag[0], inv_diag[1], inv_diag[2]))
    } else {
        // Full matrix inverse (shouldn't be needed for our symmetric shapes)
        tensor.inverse()
    }
}

/// Aggregate mass properties from multiple colliders.
///
/// This is used when a single body has multiple collider shapes.
/// The combined inertia is computed in the body's local space.
pub fn aggregate_mass_properties(shapes: &[(ColliderShape, f32)]) -> MassProperties {
    if shapes.is_empty() {
        return MassProperties::default();
    }

    if shapes.len() == 1 {
        return MassProperties::from_shape(shapes[0].0, shapes[0].1);
    }

    // Compute total mass
    let mut total_mass = 0.0;
    let mut total_inertia = Mat3::ZERO;

    for (shape, density) in shapes {
        let props = MassProperties::from_shape(*shape, *density);
        total_mass += props.mass;
        // For colliders at the body origin, inertia tensors simply add
        total_inertia += props.local_inertia;
    }

    MassProperties::new(total_mass, total_inertia)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn sphere_mass_is_volume_times_density() {
        let radius = 1.0;
        let density = 1000.0;
        let expected_volume = (4.0 / 3.0) * PI * radius * radius * radius;
        let expected_mass = expected_volume * density;

        let mass = calculate_mass(ColliderShape::sphere(radius), density);
        assert!(
            (mass - expected_mass).abs() < 1e-5,
            "expected {expected_mass}, got {mass}"
        );
    }

    #[test]
    fn cuboid_mass_is_volume_times_density() {
        let half_extents = Vec3::new(1.0, 2.0, 3.0);
        let density = 500.0;
        let expected_volume = 8.0 * 1.0 * 2.0 * 3.0;
        let expected_mass = expected_volume * density;

        let mass = calculate_mass(ColliderShape::cuboid(half_extents), density);
        assert!(
            (mass - expected_mass).abs() < 1e-5,
            "expected {expected_mass}, got {mass}"
        );
    }

    #[test]
    fn sphere_inertia_is_correct() {
        let radius = 2.0;
        let mass = 10.0;
        let expected_i = (2.0 / 5.0) * mass * radius * radius;

        let inertia = calculate_inertia_tensor(ColliderShape::sphere(radius), mass);
        let diag = inertia.to_cols_array();

        assert!((diag[0] - expected_i).abs() < 1e-5, "I_xx mismatch");
        assert!((diag[4] - expected_i).abs() < 1e-5, "I_yy mismatch");
        assert!((diag[8] - expected_i).abs() < 1e-5, "I_zz mismatch");

        // Off-diagonal elements should be zero
        assert!(diag[1].abs() < 1e-5, "I_xy should be zero");
        assert!(diag[2].abs() < 1e-5, "I_xz should be zero");
        assert!(diag[5].abs() < 1e-5, "I_yz should be zero");
    }

    #[test]
    fn cuboid_inertia_is_correct() {
        let half_extents = Vec3::new(1.0, 2.0, 3.0);
        let mass = 24.0; // Makes math clean: volume = 8*1*2*3 = 48, density = 0.5

        let expected_i_xx = (mass / 3.0) * (2.0 * 2.0 + 3.0 * 3.0);
        let expected_i_yy = (mass / 3.0) * (1.0 * 1.0 + 3.0 * 3.0);
        let expected_i_zz = (mass / 3.0) * (1.0 * 1.0 + 2.0 * 2.0);

        let inertia = calculate_inertia_tensor(ColliderShape::cuboid(half_extents), mass);
        let diag = inertia.to_cols_array();

        assert!((diag[0] - expected_i_xx).abs() < 1e-5, "I_xx mismatch");
        assert!((diag[4] - expected_i_yy).abs() < 1e-5, "I_yy mismatch");
        assert!((diag[8] - expected_i_zz).abs() < 1e-5, "I_zz mismatch");
    }

    #[test]
    fn mass_properties_inverse_inertia_is_correct() {
        let props = MassProperties::from_shape(ColliderShape::sphere(1.0), 1000.0);

        // The product of inertia and inverse inertia should be identity
        let product = props.local_inertia * props.local_inverse_inertia;
        let expected = Mat3::IDENTITY;

        for i in 0..3 {
            for j in 0..3 {
                let val = product.col(i)[j];
                let exp = expected.col(i)[j];
                assert!(
                    (val - exp).abs() < 1e-4,
                    "Product should be identity at ({i},{j}): got {val}, expected {exp}"
                );
            }
        }
    }

    #[test]
    fn world_inverse_inertia_transforms_correctly() {
        let props = MassProperties::from_shape(ColliderShape::cuboid(Vec3::splat(1.0)), 1000.0);

        // Test with identity rotation
        let identity_result = props.world_inverse_inertia(Quat::IDENTITY);
        let diag_local = props.local_inverse_inertia.to_cols_array();
        let diag_world = identity_result.to_cols_array();

        assert!(
            (diag_local[0] - diag_world[0]).abs() < 1e-5,
            "Identity rotation should preserve diagonal"
        );

        // Test with 90-degree rotation around Y
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let world_inv = props.world_inverse_inertia(rotation);

        // For a cube (symmetric), rotation shouldn't change the diagonal
        let diag_rotated = world_inv.to_cols_array();
        assert!(
            (diag_rotated[0] - diag_local[0]).abs() < 1e-5,
            "Cube inertia should be rotation-invariant"
        );
    }

    #[test]
    fn infinite_mass_properties() {
        let props = MassProperties::infinite();

        assert_eq!(props.mass, 0.0);
        assert_eq!(props.inverse_mass, 0.0);

        let world_inv = props.world_inverse_inertia(Quat::IDENTITY);
        assert_eq!(world_inv, Mat3::ZERO);
    }
}
