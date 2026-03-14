//! Transform components for scene graph

use oxide_ecs::Component;
use glam::{Mat4, Quat, Vec3};

use oxide_math::transform::Transform;

/// Local transform component.
/// Represents the position, rotation, and scale relative to the parent entity.
#[derive(Component, Clone, Debug)]
pub struct TransformComponent(pub Transform);

impl Default for TransformComponent {
    fn default() -> Self {
        Self(Transform {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        })
    }
}

impl TransformComponent {
    /// Creates a new transform component.
    pub fn new(transform: Transform) -> Self {
        Self(transform)
    }

    /// Creates a transform from position only.
    pub fn from_position(position: Vec3) -> Self {
        Self(Transform {
            position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        })
    }

    /// Creates a transform from position and rotation.
    pub fn from_position_rotation(position: Vec3, rotation: Quat) -> Self {
        Self(Transform {
            position,
            rotation,
            scale: Vec3::ONE,
        })
    }

    /// Returns the local-to-parent matrix.
    pub fn to_matrix(&self) -> Mat4 {
        self.0.to_matrix()
    }
}

impl From<Transform> for TransformComponent {
    fn from(transform: Transform) -> Self {
        Self(transform)
    }
}

impl From<TransformComponent> for Transform {
    fn from(component: TransformComponent) -> Self {
        component.0
    }
}

/// Global transform component.
/// Represents the world-space transform computed from the hierarchy.
#[derive(Component, Clone, Copy, Debug)]
pub struct GlobalTransform {
    /// The world-space transformation matrix.
    pub matrix: Mat4,
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self {
            matrix: Mat4::IDENTITY,
        }
    }
}

impl GlobalTransform {
    /// Creates a new global transform from a matrix.
    pub fn from_matrix(matrix: Mat4) -> Self {
        Self { matrix }
    }

    /// Creates an identity global transform.
    pub fn identity() -> Self {
        Self {
            matrix: Mat4::IDENTITY,
        }
    }

    /// Extracts the world position from the matrix.
    pub fn position(&self) -> Vec3 {
        self.matrix.col(3).truncate()
    }

    /// Multiplies this transform by another.
    pub fn mul(&self, other: &GlobalTransform) -> GlobalTransform {
        GlobalTransform {
            matrix: self.matrix * other.matrix,
        }
    }
}
