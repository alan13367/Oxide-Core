//! Fallback textures for materials without textures

use wgpu::{Device, Queue};

use super::Texture;

/// A 1x1 white texture used as a fallback when no texture is provided.
#[derive(Debug)]
pub struct FallbackTexture {
    pub texture: Texture,
}

impl FallbackTexture {
    /// Creates a new 1x1 white pixel texture.
    pub fn new(device: &Device, queue: &Queue) -> Self {
        // 1x1 RGBA white pixel
        let bytes: [u8; 4] = [255, 255, 255, 255];

        let texture = Texture::from_bytes(
            device,
            queue,
            &bytes,
            (1, 1),
            Some("Fallback White Texture"),
        );

        Self { texture }
    }
}
