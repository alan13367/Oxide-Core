//! Joint constraints for physics simulation.
//!
//! Provides various joint types to constrain rigid body motion:
//! - Fixed: Locks all relative motion between bodies
//! - Hinge: Allows rotation around a single axis
//! - BallSocket: Allows rotation around a point
//! - Spring: Applies spring forces between anchor points

use glam::{Mat3, Vec3};
use oxide_ecs::Component;

use crate::components::BodyId;

/// Types of joint constraints.
#[derive(Clone, Copy, Debug)]
pub enum JointType {
    /// Fixed joint - locks all relative motion.
    Fixed,
    /// Hinge (revolute) joint - allows rotation around a single axis.
    Hinge {
        /// Local axis on body A around which rotation is allowed.
        axis: Vec3,
        /// Optional angle limits in radians (min, max).
        limits: Option<(f32, f32)>,
    },
    /// Ball socket (spherical) joint - allows rotation around a point.
    BallSocket,
    /// Spring joint - applies spring forces between anchor points.
    Spring {
        /// Spring stiffness (N/m).
        stiffness: f32,
        /// Damping coefficient.
        damping: f32,
    },
}

impl Default for JointType {
    fn default() -> Self {
        Self::Fixed
    }
}

/// A joint constraint between two bodies.
#[derive(Component, Clone, Debug)]
pub struct JointComponent {
    /// The type of joint.
    pub joint_type: JointType,
    /// Body A identifier.
    pub body_a: BodyId,
    /// Body B identifier.
    pub body_b: BodyId,
    /// Anchor point on body A (local space).
    pub anchor_a: Vec3,
    /// Anchor point on body B (local space).
    pub anchor_b: Vec3,
    /// Whether the joint is enabled.
    pub enabled: bool,
    /// Break force threshold (0 = unbreakable).
    pub break_force: f32,
}

impl JointComponent {
    /// Create a new fixed joint between two bodies.
    pub fn fixed(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            joint_type: JointType::Fixed,
            body_a,
            body_b,
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
            enabled: true,
            break_force: 0.0,
        }
    }

    /// Create a hinge joint between two bodies.
    pub fn hinge(body_a: BodyId, body_b: BodyId, axis: Vec3, limits: Option<(f32, f32)>) -> Self {
        Self {
            joint_type: JointType::Hinge { axis, limits },
            body_a,
            body_b,
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
            enabled: true,
            break_force: 0.0,
        }
    }

    /// Create a ball socket joint between two bodies.
    pub fn ball_socket(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            joint_type: JointType::BallSocket,
            body_a,
            body_b,
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
            enabled: true,
            break_force: 0.0,
        }
    }

    /// Create a spring joint between two bodies.
    pub fn spring(body_a: BodyId, body_b: BodyId, stiffness: f32, damping: f32) -> Self {
        Self {
            joint_type: JointType::Spring { stiffness, damping },
            body_a,
            body_b,
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
            enabled: true,
            break_force: 0.0,
        }
    }

    /// Set the anchor points for the joint.
    pub fn with_anchors(mut self, anchor_a: Vec3, anchor_b: Vec3) -> Self {
        self.anchor_a = anchor_a;
        self.anchor_b = anchor_b;
        self
    }

    /// Set the break force threshold.
    pub fn with_break_force(mut self, force: f32) -> Self {
        self.break_force = force;
        self
    }
}

/// Internal representation of a joint for the solver.
#[derive(Clone, Debug)]
pub struct JointConstraint {
    pub joint: JointComponent,
    /// Jacobian for position constraint (linear part).
    pub linear_jacobian_a: Vec3,
    pub linear_jacobian_b: Vec3,
    /// Jacobian for angular constraint.
    pub angular_jacobian_a: Vec3,
    pub angular_jacobian_b: Vec3,
    /// Effective mass for the constraint.
    pub effective_mass: f32,
    /// Bias for Baumgarte stabilization.
    pub bias: Vec3,
    /// Accumulated impulse for warm starting.
    pub accumulated_impulse: Vec3,
}

impl JointConstraint {
    /// Create a joint constraint from a joint component and body data.
    pub fn from_joint(
        joint: JointComponent,
        pos_a: Vec3,
        rot_a: glam::Quat,
        inv_mass_a: f32,
        inv_inertia_a: Mat3,
        pos_b: Vec3,
        rot_b: glam::Quat,
        inv_mass_b: f32,
        inv_inertia_b: Mat3,
    ) -> Self {
        // Transform anchors to world space
        let anchor_world_a = pos_a + rot_a * joint.anchor_a;
        let anchor_world_b = pos_b + rot_b * joint.anchor_b;

        // Compute relative positions
        let r_a = anchor_world_a - pos_a;
        let r_b = anchor_world_b - pos_b;

        // Compute error (distance between anchors)
        let error = anchor_world_b - anchor_world_a;

        // Compute Jacobians for position constraint
        // For a fixed joint, we want anchor_a == anchor_b
        let linear_jacobian_a = -Vec3::X; // Simplified
        let linear_jacobian_b = Vec3::X;

        let angular_jacobian_a = r_a.cross(-Vec3::X);
        let angular_jacobian_b = r_b.cross(Vec3::X);

        // Compute effective mass
        let k = inv_mass_a
            + inv_mass_b
            + angular_jacobian_a.dot(inv_inertia_a * angular_jacobian_a)
            + angular_jacobian_b.dot(inv_inertia_b * angular_jacobian_b);

        let effective_mass = if k > 0.0 { 1.0 / k } else { 0.0 };

        // Baumgarte bias
        let baumgarte = 0.3;
        let bias = error * baumgarte;

        Self {
            joint,
            linear_jacobian_a,
            linear_jacobian_b,
            angular_jacobian_a,
            angular_jacobian_b,
            effective_mass,
            bias,
            accumulated_impulse: Vec3::ZERO,
        }
    }

    /// Solve the joint constraint.
    pub fn solve(
        &mut self,
        vel_a: &mut Vec3,
        ang_vel_a: &mut Vec3,
        inv_mass_a: f32,
        inv_inertia_a: Mat3,
        vel_b: &mut Vec3,
        ang_vel_b: &mut Vec3,
        inv_mass_b: f32,
        inv_inertia_b: Mat3,
    ) {
        // Compute relative velocity at anchor points
        let rel_vel = *vel_b - *vel_a;

        // Compute velocity along constraint
        let vel_error = rel_vel + self.bias;

        // Compute impulse
        let impulse = vel_error * self.effective_mass;

        // Apply impulse
        *vel_a -= impulse * inv_mass_a;
        *vel_b += impulse * inv_mass_b;

        // Apply angular impulse
        let ang_impulse_a = inv_inertia_a * self.angular_jacobian_a * impulse.x;
        let ang_impulse_b = inv_inertia_b * self.angular_jacobian_b * impulse.x;

        *ang_vel_a -= ang_impulse_a;
        *ang_vel_b += ang_impulse_b;

        // Accumulate impulse for warm starting
        self.accumulated_impulse += impulse;
    }
}

/// Solve all joint constraints in the physics world.
pub fn solve_joints(physics: &mut crate::resources::PhysicsWorld, joints: &[JointComponent]) {
    const ANCHOR_BAUMGARTE: f32 = 6.0;
    const ANGULAR_BAUMGARTE: f32 = 4.0;
    const ANGULAR_DAMPING: f32 = 0.3;

    for joint in joints {
        if !joint.enabled {
            continue;
        }

        // Get body data first
        let body_a_data = physics.body(joint.body_a);
        let body_b_data = physics.body(joint.body_b);

        let Some(body_a) = body_a_data else { continue };
        let Some(body_b) = body_b_data else { continue };

        // Skip if both bodies are static
        if !body_a.is_dynamic() && !body_b.is_dynamic() {
            continue;
        }

        match joint.joint_type {
            JointType::Spring { stiffness, damping } => {
                let anchor_world_a = body_a.position + body_a.rotation * joint.anchor_a;
                let anchor_world_b = body_b.position + body_b.rotation * joint.anchor_b;

                let delta = anchor_world_b - anchor_world_a;
                let distance = delta.length();

                if distance > 0.001 {
                    let direction = delta / distance;

                    // Spring force: F = -k * x
                    let spring_force = direction * (-stiffness * distance);

                    // Damping force: F = -c * v
                    let rel_vel = body_b.linear_velocity - body_a.linear_velocity;
                    let damping_force = direction * (-damping * rel_vel.dot(direction));

                    let total_force = spring_force + damping_force;

                    let a_is_dynamic = body_a.is_dynamic();
                    let b_is_dynamic = body_b.is_dynamic();
                    if a_is_dynamic {
                        if let Some(a) = physics.body_mut(joint.body_a) {
                            a.apply_force_at_point(-total_force, anchor_world_a);
                        }
                    }
                    if b_is_dynamic {
                        if let Some(b) = physics.body_mut(joint.body_b) {
                            b.apply_force_at_point(total_force, anchor_world_b);
                        }
                    }
                }
            }
            JointType::BallSocket => {
                solve_anchor_constraint(physics, joint, ANCHOR_BAUMGARTE);
            }
            JointType::Fixed => {
                solve_anchor_constraint(physics, joint, ANCHOR_BAUMGARTE);

                let Some(a) = physics.body(joint.body_a) else {
                    continue;
                };
                let Some(b) = physics.body(joint.body_b) else {
                    continue;
                };
                let orientation_error_local =
                    (a.rotation.conjugate() * b.rotation).to_scaled_axis();
                let orientation_error_world = a.rotation * orientation_error_local;
                let rel_ang_vel = b.angular_velocity - a.angular_velocity;
                let correction = rel_ang_vel + orientation_error_world * ANGULAR_BAUMGARTE;
                solve_angular_constraint(
                    physics,
                    joint.body_a,
                    joint.body_b,
                    correction,
                    ANGULAR_DAMPING,
                );
            }
            JointType::Hinge { axis, limits } => {
                solve_anchor_constraint(physics, joint, ANCHOR_BAUMGARTE);

                let axis = if axis.length_squared() > f32::EPSILON {
                    axis.normalize()
                } else {
                    Vec3::Y
                };

                let Some(a) = physics.body(joint.body_a) else {
                    continue;
                };
                let Some(b) = physics.body(joint.body_b) else {
                    continue;
                };
                let world_axis_a = (a.rotation * axis).normalize_or_zero();
                let world_axis_b = (b.rotation * axis).normalize_or_zero();

                // Keep non-hinge angular motion near zero and keep hinge axes aligned.
                let rel_ang_vel = b.angular_velocity - a.angular_velocity;
                let rel_ang_perp = rel_ang_vel - world_axis_a * rel_ang_vel.dot(world_axis_a);
                let axis_alignment_error = world_axis_b.cross(world_axis_a);
                let mut correction = rel_ang_perp + axis_alignment_error * ANGULAR_BAUMGARTE;

                // Approximate limit correction around hinge axis.
                if let Some((min_angle, max_angle)) = limits {
                    let rel_local = (a.rotation.conjugate() * b.rotation).to_scaled_axis();
                    let hinge_angle = rel_local.dot(axis);
                    if hinge_angle < min_angle {
                        let error = hinge_angle - min_angle;
                        correction += world_axis_a * error * ANGULAR_BAUMGARTE;
                    } else if hinge_angle > max_angle {
                        let error = hinge_angle - max_angle;
                        correction += world_axis_a * error * ANGULAR_BAUMGARTE;
                    }
                }

                solve_angular_constraint(
                    physics,
                    joint.body_a,
                    joint.body_b,
                    correction,
                    ANGULAR_DAMPING,
                );
            }
        }
    }
}

fn solve_anchor_constraint(
    physics: &mut crate::resources::PhysicsWorld,
    joint: &JointComponent,
    baumgarte: f32,
) {
    let Some(a) = physics.body(joint.body_a) else {
        return;
    };
    let Some(b) = physics.body(joint.body_b) else {
        return;
    };

    let inv_mass_a = a.inverse_mass;
    let inv_mass_b = b.inverse_mass;
    let inv_inertia_a = a.world_inverse_inertia;
    let inv_inertia_b = b.world_inverse_inertia;
    let pos_a = a.position;
    let pos_b = b.position;
    let rot_a = a.rotation;
    let rot_b = b.rotation;
    let mut vel_a = a.linear_velocity;
    let mut vel_b = b.linear_velocity;
    let mut ang_a = a.angular_velocity;
    let mut ang_b = b.angular_velocity;

    let anchor_world_a = pos_a + rot_a * joint.anchor_a;
    let anchor_world_b = pos_b + rot_b * joint.anchor_b;
    let r_a = anchor_world_a - pos_a;
    let r_b = anchor_world_b - pos_b;
    let error = anchor_world_b - anchor_world_a;

    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        let rel_vel = (vel_b + ang_b.cross(r_b) - vel_a - ang_a.cross(r_a)).dot(axis);
        let positional_bias = error.dot(axis) * baumgarte;
        let c_dot = rel_vel + positional_bias;

        let r_a_cross_n = r_a.cross(axis);
        let r_b_cross_n = r_b.cross(axis);
        let k = inv_mass_a
            + inv_mass_b
            + (inv_inertia_a * r_a_cross_n).cross(r_a).dot(axis)
            + (inv_inertia_b * r_b_cross_n).cross(r_b).dot(axis);
        if k <= f32::EPSILON {
            continue;
        }

        let lambda = -c_dot / k;
        let impulse = axis * lambda;
        vel_a -= impulse * inv_mass_a;
        vel_b += impulse * inv_mass_b;
        ang_a -= inv_inertia_a * r_a.cross(impulse);
        ang_b += inv_inertia_b * r_b.cross(impulse);
    }

    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(joint.body_a) {
            body.wake();
            body.linear_velocity = vel_a;
            body.angular_velocity = ang_a;
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(joint.body_b) {
            body.wake();
            body.linear_velocity = vel_b;
            body.angular_velocity = ang_b;
        }
    }
}

fn solve_angular_constraint(
    physics: &mut crate::resources::PhysicsWorld,
    body_a: BodyId,
    body_b: BodyId,
    correction: Vec3,
    damping: f32,
) {
    if !correction.is_finite() {
        return;
    }

    let Some(a) = physics.body(body_a) else {
        return;
    };
    let Some(b) = physics.body(body_b) else {
        return;
    };
    let inv_mass_a = a.inverse_mass;
    let inv_mass_b = b.inverse_mass;
    let inv_inertia_a = a.world_inverse_inertia;
    let inv_inertia_b = b.world_inverse_inertia;
    let mut ang_a = a.angular_velocity;
    let mut ang_b = b.angular_velocity;

    let mut correction = correction;
    let correction_len = correction.length();
    if correction_len > 20.0 {
        correction *= 20.0 / correction_len;
    }

    let lambda = correction * damping;
    if !lambda.is_finite() {
        return;
    }
    ang_a += inv_inertia_a * lambda;
    ang_b -= inv_inertia_b * lambda;

    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(body_a) {
            body.wake();
            body.angular_velocity = ang_a;
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(body_b) {
            body.wake();
            body.angular_velocity = ang_b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn create_fixed_joint() {
        let joint = JointComponent::fixed(BodyId(1), BodyId(2));
        assert!(matches!(joint.joint_type, JointType::Fixed));
        assert!(joint.enabled);
    }

    #[test]
    fn create_hinge_joint_with_limits() {
        let joint = JointComponent::hinge(
            BodyId(1),
            BodyId(2),
            Vec3::Y,
            Some((-std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4)),
        );
        if let JointType::Hinge { axis, limits } = joint.joint_type {
            assert_eq!(axis, Vec3::Y);
            assert!(limits.is_some());
        } else {
            panic!("Expected Hinge joint");
        }
    }

    #[test]
    fn create_spring_joint() {
        let joint = JointComponent::spring(BodyId(1), BodyId(2), 100.0, 5.0);
        if let JointType::Spring { stiffness, damping } = joint.joint_type {
            assert_eq!(stiffness, 100.0);
            assert_eq!(damping, 5.0);
        } else {
            panic!("Expected Spring joint");
        }
    }

    #[test]
    fn joint_with_anchors() {
        let joint = JointComponent::ball_socket(BodyId(1), BodyId(2))
            .with_anchors(Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(joint.anchor_a, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(joint.anchor_b, Vec3::new(-1.0, 0.0, 0.0));
    }
}
