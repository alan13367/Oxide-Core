//! GPU-aligned light structures for uniform/storage buffer bindings.

use bytemuck::{Pod, Zeroable};

use super::{AmbientLight, DirectionalLight};

pub const MAX_DIRECTIONAL_LIGHTS: usize = 4;
pub const MAX_POINT_LIGHTS: usize = 4096;

#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuDirectionalLight {
    pub direction: [f32; 4],
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

/// Storage-buffer-friendly point light with explicit 16-byte alignment.
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuPointLight {
    pub position_radius: [f32; 4],
    pub color_intensity: [f32; 4],
    pub _padding: [f32; 4],
}

impl GpuPointLight {
    pub fn from_values(
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        radius: f32,
    ) -> Self {
        Self {
            position_radius: [position.x, position.y, position.z, radius],
            color_intensity: [color.x, color.y, color.z, intensity],
            _padding: [0.0; 4],
        }
    }
}

#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LightUniform {
    pub ambient_color_intensity: [f32; 4],
    pub directional_count: u32,
    pub point_count: u32,
    pub _padding: [u32; 2],
    pub directional_lights: [GpuDirectionalLight; MAX_DIRECTIONAL_LIGHTS],
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            ambient_color_intensity: [0.2, 0.2, 0.2, 0.2],
            directional_count: 0,
            point_count: 0,
            _padding: [0; 2],
            directional_lights: [GpuDirectionalLight::zeroed(); MAX_DIRECTIONAL_LIGHTS],
        }
    }
}

impl LightUniform {
    pub fn new(
        ambient: Option<&AmbientLight>,
        directional_lights: &[&DirectionalLight],
        point_count: usize,
    ) -> Self {
        let mut uniform = Self::default();

        if let Some(ambient) = ambient {
            uniform.ambient_color_intensity = [
                ambient.color.x,
                ambient.color.y,
                ambient.color.z,
                ambient.intensity,
            ];
        }

        uniform.directional_count =
            (directional_lights.len() as u32).min(MAX_DIRECTIONAL_LIGHTS as u32);
        for (i, light) in directional_lights
            .iter()
            .take(MAX_DIRECTIONAL_LIGHTS)
            .enumerate()
        {
            uniform.directional_lights[i] = GpuDirectionalLight::from(*light);
        }

        uniform.point_count = (point_count as u32).min(MAX_POINT_LIGHTS as u32);
        uniform
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pod<T: Pod>() {}

    #[test]
    fn storage_and_uniform_alignment_contracts() {
        assert_pod::<GpuDirectionalLight>();
        assert_pod::<GpuPointLight>();
        assert_pod::<LightUniform>();

        assert_eq!(std::mem::size_of::<GpuDirectionalLight>() % 16, 0);
        assert_eq!(std::mem::size_of::<GpuPointLight>() % 16, 0);
        assert_eq!(std::mem::size_of::<LightUniform>() % 16, 0);
        assert_eq!(std::mem::size_of::<GpuPointLight>(), 48);

        assert_eq!(std::mem::align_of::<GpuDirectionalLight>(), 16);
        assert_eq!(std::mem::align_of::<GpuPointLight>(), 16);
        assert_eq!(std::mem::align_of::<LightUniform>(), 16);
    }
}
