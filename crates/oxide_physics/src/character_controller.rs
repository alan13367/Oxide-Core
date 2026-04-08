//! Kinematic Character Controller for player movement.
//!
//! Provides a capsule-based character controller that handles:
//! - Walking and sliding against obstacles
//! - Step-up for small obstacles
//! - Slope limiting
//! - Ground detection

use glam::Vec3;
use oxide_ecs::Component;

/// Character controller component for kinematic movement.
///
/// Attach this to an entity with a capsule collider for player-style movement.
/// The controller moves kinematically and doesn't apply forces to other bodies.
#[derive(Component, Clone, Debug)]
pub struct CharacterControllerComponent {
    /// Maximum height the character can step up (default: 0.5m).
    pub step_offset: f32,
    /// Maximum slope angle in radians (default: 45 degrees).
    pub max_slope_angle: f32,
    /// Whether the character is currently on the ground.
    pub grounded: bool,
    /// Up direction for the character (default: Y-up).
    pub up_vector: Vec3,
    /// Radius of the capsule collider.
    pub radius: f32,
    /// Half-height of the capsule (from center to end cap).
    pub half_height: f32,
    /// Current velocity of the character.
    pub velocity: Vec3,
    /// Whether the character touched a wall this frame.
    pub touching_wall: bool,
    /// Whether the character touched the ceiling this frame.
    pub touching_ceiling: bool,
    /// Normal of the ground the character is standing on.
    pub ground_normal: Vec3,
}

impl Default for CharacterControllerComponent {
    fn default() -> Self {
        Self {
            step_offset: 0.5,
            max_slope_angle: std::f32::consts::FRAC_PI_4, // 45 degrees
            grounded: false,
            up_vector: Vec3::Y,
            radius: 0.5,
            half_height: 1.0,
            velocity: Vec3::ZERO,
            touching_wall: false,
            touching_ceiling: false,
            ground_normal: Vec3::Y,
        }
    }
}

impl CharacterControllerComponent {
    /// Create a new character controller with the given capsule dimensions.
    pub fn new(radius: f32, half_height: f32) -> Self {
        Self {
            radius,
            half_height,
            ..Default::default()
        }
    }

    /// Set the step offset (maximum climb height).
    pub fn with_step_offset(mut self, offset: f32) -> Self {
        self.step_offset = offset;
        self
    }

    /// Set the maximum slope angle.
    pub fn with_max_slope(mut self, angle_radians: f32) -> Self {
        self.max_slope_angle = angle_radians;
        self
    }

    /// Check if the character is on a walkable slope.
    pub fn is_on_walkable_slope(&self) -> bool {
        if !self.grounded {
            return false;
        }

        // Angle between up vector and ground normal
        let slope_angle = self.up_vector.angle_between(self.ground_normal);
        slope_angle <= self.max_slope_angle
    }

    /// Move the character and handle collision response.
    ///
    /// Returns the actual displacement after collision.
    pub fn move_and_slide(
        &mut self,
        physics: &crate::resources::PhysicsWorld,
        position: Vec3,
        desired_velocity: Vec3,
        dt: f32,
    ) -> Vec3 {
        let mut current_pos = position;
        let mut remaining_velocity = desired_velocity * dt;
        let mut total_displacement = Vec3::ZERO;

        // Reset state
        self.grounded = false;
        self.touching_wall = false;
        self.touching_ceiling = false;
        self.ground_normal = self.up_vector;

        // Iterate up to 4 times for sliding
        for _ in 0..4 {
            if remaining_velocity.length_squared() < 0.0001 {
                break;
            }

            // Cast capsule along velocity
            let cast_dir = remaining_velocity.normalize();
            let cast_dist = remaining_velocity.length();

            if let Some(hit) = self.cast_capsule(physics, current_pos, cast_dir, cast_dist) {
                // Hit something
                let hit_distance = hit.distance;
                let hit_normal = hit.normal;

                // Move to just before the hit
                let safe_dist = (hit_distance - 0.001).max(0.0);
                let safe_move = cast_dir * safe_dist;
                current_pos += safe_move;
                total_displacement += safe_move;

                // Determine what we hit
                let hit_angle = self.up_vector.angle_between(hit_normal);
                if hit_angle <= self.max_slope_angle {
                    // Walkable ground
                    self.grounded = true;
                    self.ground_normal = hit_normal;
                } else if hit_normal.dot(self.up_vector) < 0.0 {
                    // Ceiling
                    self.touching_ceiling = true;
                } else {
                    // Wall
                    self.touching_wall = true;
                }

                // Slide along the surface
                remaining_velocity =
                    remaining_velocity - hit_normal * remaining_velocity.dot(hit_normal);
            } else {
                // No hit, move freely
                current_pos += remaining_velocity;
                total_displacement += remaining_velocity;
                break;
            }
        }

        // Try to step up if we hit a wall and not grounded
        if self.touching_wall && !self.grounded {
            let step_up = self.up_vector * self.step_offset;
            let stepped_pos = current_pos + step_up;

            // Check if we can move forward from the stepped position
            let mut horizontal_dir = desired_velocity * dt;
            horizontal_dir.y = 0.0;

            if horizontal_dir.length_squared() > 0.0001 {
                let horizontal_move = horizontal_dir.normalize() * horizontal_dir.length();

                if self
                    .cast_capsule(
                        physics,
                        stepped_pos,
                        horizontal_move.normalize(),
                        horizontal_move.length(),
                    )
                    .is_none()
                {
                    // Can step up! Move to stepped position
                    current_pos = stepped_pos + horizontal_move;
                    total_displacement = step_up + horizontal_move;

                    // Step down to find ground
                    if let Some(hit) = self.cast_capsule(
                        physics,
                        current_pos,
                        -self.up_vector,
                        self.step_offset * 2.0,
                    ) {
                        current_pos += -self.up_vector * (hit.distance - 0.001);
                        total_displacement += -self.up_vector * (hit.distance - 0.001);
                        self.grounded = true;
                        self.ground_normal = hit.normal;
                    }
                }
            }
        }

        // Store velocity for external use
        self.velocity = total_displacement / dt.max(0.001);

        total_displacement
    }

    /// Cast a capsule shape to detect collisions.
    fn cast_capsule(
        &self,
        physics: &crate::resources::PhysicsWorld,
        position: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<CapsuleCastHit> {
        // Approximate the capsule by sweeping spheres along its central axis.
        let up = if self.up_vector.length_squared() > f32::EPSILON {
            self.up_vector.normalize()
        } else {
            Vec3::Y
        };
        let center = position + up * self.half_height;
        let sample_centers = [
            center - up * self.half_height,
            center,
            center + up * self.half_height,
        ];

        let mut best_hit: Option<CapsuleCastHit> = None;
        for sample_center in sample_centers {
            if let Some(hit) =
                physics.sphere_cast(sample_center, direction, self.radius, max_distance)
            {
                let candidate = CapsuleCastHit {
                    normal: hit.normal,
                    distance: hit.time_of_impact * max_distance,
                };
                if best_hit
                    .as_ref()
                    .map(|best| candidate.distance < best.distance)
                    .unwrap_or(true)
                {
                    best_hit = Some(candidate);
                }
            }
        }

        best_hit
    }

    /// Apply a jump impulse.
    pub fn jump(&mut self, jump_force: f32) {
        if self.grounded {
            self.velocity.y = jump_force;
            self.grounded = false;
        }
    }

    /// Check if the character is on the ground.
    pub fn is_on_floor(&self) -> bool {
        self.grounded
    }

    /// Check if the character is touching a wall.
    pub fn is_on_wall(&self) -> bool {
        self.touching_wall
    }

    /// Check if the character is touching the ceiling.
    pub fn is_on_ceiling(&self) -> bool {
        self.touching_ceiling
    }
}

/// Result of a capsule cast.
#[derive(Clone, Copy, Debug)]
struct CapsuleCastHit {
    normal: Vec3,
    distance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_character_controller() {
        let controller = CharacterControllerComponent::new(0.5, 1.0);
        assert_eq!(controller.radius, 0.5);
        assert_eq!(controller.half_height, 1.0);
        assert!(!controller.grounded);
    }

    #[test]
    fn character_controller_defaults() {
        let controller = CharacterControllerComponent::default();
        assert_eq!(controller.step_offset, 0.5);
        assert_eq!(controller.max_slope_angle, std::f32::consts::FRAC_PI_4);
    }

    #[test]
    fn is_on_walkable_slope() {
        let mut controller = CharacterControllerComponent::default();
        controller.grounded = true;
        controller.ground_normal = Vec3::Y;

        // Flat ground is walkable
        assert!(controller.is_on_walkable_slope());

        // 45 degree slope is still walkable (matches max_slope_angle)
        let slope_45 = Vec3::new(1.0, 1.0, 0.0).normalize();
        controller.ground_normal = slope_45;
        assert!(controller.is_on_walkable_slope());

        // 60 degree slope is too steep
        controller.max_slope_angle = std::f32::consts::FRAC_PI_4; // 45 degrees
        let slope_60 = Vec3::new(1.0, 0.577, 0.0).normalize(); // ~60 degrees from vertical
        controller.ground_normal = slope_60;
        assert!(!controller.is_on_walkable_slope());
    }

    #[test]
    fn jump_when_grounded() {
        let mut controller = CharacterControllerComponent::default();
        controller.grounded = true;

        controller.jump(10.0);
        assert_eq!(controller.velocity.y, 10.0);
        assert!(!controller.grounded);
    }

    #[test]
    fn no_jump_when_airborne() {
        let mut controller = CharacterControllerComponent::default();
        controller.grounded = false;
        controller.velocity = Vec3::ZERO;

        controller.jump(10.0);
        assert_eq!(controller.velocity.y, 0.0);
    }
}
