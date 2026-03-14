//! Sampler creation utilities

use wgpu::{AddressMode, FilterMode, MipmapFilterMode};

/// Descriptor for creating a sampler.
#[derive(Clone, Copy, Debug)]
pub struct SamplerDescriptor {
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
    pub address_mode_w: AddressMode,
    pub mag_filter: FilterMode,
    pub min_filter: FilterMode,
    pub mipmap_filter: MipmapFilterMode,
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Nearest,
        }
    }
}

impl SamplerDescriptor {
    /// Creates a linear-filtered sampler with clamp-to-edge addressing.
    pub fn linear() -> Self {
        Self::default()
    }

    /// Creates a nearest-neighbor filtered sampler for pixel art.
    pub fn nearest() -> Self {
        Self {
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: MipmapFilterMode::Nearest,
            ..Self::default()
        }
    }

    /// Creates a repeating sampler.
    pub fn repeat() -> Self {
        Self {
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            ..Self::default()
        }
    }
}
