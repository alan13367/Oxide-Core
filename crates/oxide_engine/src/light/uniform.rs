//! GPU-aligned light uniform structures
//!
//! WGSL requires arrays in uniform buffers to be 16-byte aligned.
//! Each struct uses `#[repr(C, align(16))]` to ensure proper padding.

use bytemuck::{Pod, Zeroable};

use super::{AmbientLight, DirectionalLight, PointLight};

/// Maximum number of directional lights supported.
pub const MAX_DIRECTIONAL_LIGHTS: usize = 4;
/// Maximum number of point lights supported.
pub const MAX_POINT_LIGHTS: usize = 8;

/// GPU-aligned directional light data.
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuDirectionalLight {
    /// Direction (vec3f + padding to 16 bytes).
    pub direction: [f32; 4],
    /// Color in RGB, intensity stored in the alpha channel.
    pub color_intensity: [f32; 4],
}

impl From<&DirectionalLight> for GpuDirectionalLight {
    fn from(light: &DirectionalLight) -> Self {
        Self {
            direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
            color_intensity: [light.color.x, light.color.y, light.color.z, light.intensity],
        }
    }
}

/// GPU-aligned point light data.
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuPointLight {
    /// Position (vec3f + padding to 16 bytes).
    pub position: [f32; 4],
    /// Color in RGB, intensity stored in the alpha channel.
    pub color_intensity: [f32; 4],
    /// Radius (f32 + padding to 16 bytes).
    pub radius: [f32; 4],
}

impl From<&PointLight> for GpuPointLight {
    fn from(light: &PointLight) -> Self {
        Self {
            position: [light.position.x, light.position.y, light.position.z, 0.0],
            color_intensity: [light.color.x, light.color.y, light.color.z, light.intensity],
            radius: [light.radius, 0.0, 0.0, 0.0],
        }
    }
}

/// Main light uniform buffer structure.
/// This is bound to the shader at @group(2) @binding(0).
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LightUniform {
    /// Ambient light color (RGB + intensity in alpha).
    pub ambient_color_intensity: [f32; 4],
    /// Number of directional lights currently active.
    pub directional_count: u32,
    /// Number of point lights currently active.
    pub point_count: u32,
    /// Padding to align to 16 bytes.
    pub _padding: [u32; 2],
    /// Array of directional lights.
    pub directional_lights: [GpuDirectionalLight; MAX_DIRECTIONAL_LIGHTS],
    /// Array of point lights.
    pub point_lights: [GpuPointLight; MAX_POINT_LIGHTS],
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            ambient_color_intensity: [0.2, 0.2, 0.2, 0.2], // Default dim ambient
            directional_count: 0,
            point_count: 0,
            _padding: [0; 2],
            directional_lights: [GpuDirectionalLight::zeroed(); MAX_DIRECTIONAL_LIGHTS],
            point_lights: [GpuPointLight::zeroed(); MAX_POINT_LIGHTS],
        }
    }
}

impl LightUniform {
    /// Creates a new light uniform from light components.
    pub fn new(
        ambient: Option<&AmbientLight>,
        directional_lights: &[&DirectionalLight],
        point_lights: &[&PointLight],
    ) -> Self {
        let mut uniform = Self::default();

        // Set ambient light
        if let Some(ambient) = ambient {
            uniform.ambient_color_intensity = [
                ambient.color.x,
                ambient.color.y,
                ambient.color.z,
                ambient.intensity,
            ];
        }

        // Set directional lights (clamped to max)
        uniform.directional_count = (directional_lights.len() as u32).min(MAX_DIRECTIONAL_LIGHTS as u32);
        for (i, light) in directional_lights.iter().take(MAX_DIRECTIONAL_LIGHTS).enumerate() {
            uniform.directional_lights[i] = GpuDirectionalLight::from(*light);
        }

        // Set point lights (clamped to max)
        uniform.point_count = (point_lights.len() as u32).min(MAX_POINT_LIGHTS as u32);
        for (i, light) in point_lights.iter().take(MAX_POINT_LIGHTS).enumerate() {
            uniform.point_lights[i] = GpuPointLight::from(*light);
        }

        uniform
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pod<T: Pod>() {}

    #[test]
    fn test_gpu_directional_light_alignment() {
        assert_eq!(
            std::mem::size_of::<GpuDirectionalLight>() % 16,
            0,
            "GpuDirectionalLight must be 16-byte aligned"
        );
    }

    #[test]
    fn test_gpu_point_light_alignment() {
        assert_eq!(
            std::mem::size_of::<GpuPointLight>() % 16,
            0,
            "GpuPointLight must be 16-byte aligned"
        );
    }

    #[test]
    fn test_light_uniform_alignment() {
        assert_eq!(
            std::mem::size_of::<LightUniform>() % 16,
            0,
            "LightUniform must be 16-byte aligned"
        );
    }

    #[test]
    fn test_bytemuck_and_alignment_contracts() {
        assert_pod::<GpuDirectionalLight>();
        assert_pod::<GpuPointLight>();
        assert_pod::<LightUniform>();

        assert_eq!(std::mem::align_of::<GpuDirectionalLight>(), 16);
        assert_eq!(std::mem::align_of::<GpuPointLight>(), 16);
        assert_eq!(std::mem::align_of::<LightUniform>(), 16);
    }

    #[test]
    fn test_light_uniform_size() {
        // Verify expected size for debugging
        println!("GpuDirectionalLight size: {}", std::mem::size_of::<GpuDirectionalLight>());
        println!("GpuPointLight size: {}", std::mem::size_of::<GpuPointLight>());
        println!("LightUniform size: {}", std::mem::size_of::<LightUniform>());
    }
}