//! Light component definitions

use glam::Vec3;

use oxide_ecs::Component;

/// A directional light (like the sun) that affects all objects uniformly.
#[derive(Component, Clone, Copy, Debug)]
pub struct DirectionalLight {
    /// Direction the light is pointing (should be normalized).
    pub direction: Vec3,
    /// RGB color of the light.
    pub color: Vec3,
    /// Light intensity multiplier.
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: Vec3::NEG_Y,
            color: Vec3::ONE,
            intensity: 1.0,
        }
    }
}

impl DirectionalLight {
    /// Creates a new directional light.
    pub fn new(direction: Vec3, color: Vec3, intensity: f32) -> Self {
        Self {
            direction: direction.normalize(),
            color,
            intensity,
        }
    }
}

/// A point light that radiates from a position in all directions.
#[derive(Component, Clone, Copy, Debug)]
pub struct PointLight {
    /// World position of the light.
    pub position: Vec3,
    /// RGB color of the light.
    pub color: Vec3,
    /// Light intensity multiplier.
    pub intensity: f32,
    /// Maximum distance the light affects (attenuation).
    pub radius: f32,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            color: Vec3::ONE,
            intensity: 1.0,
            radius: 10.0,
        }
    }
}

impl PointLight {
    /// Creates a new point light.
    pub fn new(position: Vec3, color: Vec3, intensity: f32, radius: f32) -> Self {
        Self {
            position,
            color,
            intensity,
            radius,
        }
    }
}

/// Ambient lighting that affects all objects uniformly.
/// Only one ambient light should be active at a time.
#[derive(Component, Clone, Copy, Debug)]
pub struct AmbientLight {
    /// RGB color of the ambient light.
    pub color: Vec3,
    /// Light intensity multiplier.
    pub intensity: f32,
}

impl Default for AmbientLight {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 0.2,
        }
    }
}

impl AmbientLight {
    /// Creates a new ambient light.
    pub fn new(color: Vec3, intensity: f32) -> Self {
        Self { color, intensity }
    }
}
