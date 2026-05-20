//! Texture loading and management

mod fallback;
mod sampler;

use std::path::Path;

use wgpu::{Device, Queue};

pub use fallback::FallbackTexture;
pub use sampler::SamplerDescriptor;

#[derive(thiserror::Error, Debug)]
pub enum TextureError {
    #[cfg(feature = "image-import")]
    #[error("Failed to load image '{path}': {source}")]
    ImageLoad {
        path: String,
        source: image::ImageError,
    },
    #[cfg(feature = "image-import")]
    #[error("Failed to read image file '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("Texture file loading requires the 'image-import' feature")]
    ImageImportDisabled,
    #[error("Invalid texture dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("Invalid RGBA byte length {actual}; expected {expected} for {width}x{height}")]
    InvalidRgbaLength {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
}

/// CPU-side RGBA image data ready to upload into a GPU texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TextureImage {
    /// Creates a CPU texture image from tightly packed RGBA8 bytes.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, TextureError> {
        if width == 0 || height == 0 {
            return Err(TextureError::InvalidDimensions { width, height });
        }
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(TextureError::InvalidRgbaLength {
                width,
                height,
                expected,
                actual: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

/// A GPU texture with its view and sampler.
#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    /// Uploads a CPU-side texture image to the GPU.
    pub fn from_image(
        device: &Device,
        queue: &Queue,
        image: &TextureImage,
        label: Option<&str>,
    ) -> Self {
        Self::from_bytes(
            device,
            queue,
            &image.rgba,
            (image.width, image.height),
            label,
        )
    }

    /// Creates a texture from raw bytes (RGBA format).
    pub fn from_bytes(
        device: &Device,
        queue: &Queue,
        bytes: &[u8],
        dimensions: (u32, u32),
        label: Option<&str>,
    ) -> Self {
        let (width, height) = dimensions;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            texture.as_image_copy(),
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
        }
    }

    /// Loads a texture from a file (PNG or JPEG).
    #[cfg(feature = "image-import")]
    pub fn from_file(
        device: &Device,
        queue: &Queue,
        path: impl AsRef<Path>,
    ) -> Result<Self, TextureError> {
        let path = path.as_ref();
        let path_str = path.display().to_string();

        let img = image::open(path).map_err(|source| TextureError::ImageLoad {
            path: path_str.clone(),
            source,
        })?;

        let rgba = img.to_rgba8();
        let dimensions = rgba.dimensions();

        Ok(Self::from_bytes(
            device,
            queue,
            &rgba,
            dimensions,
            Some(&path_str),
        ))
    }

    /// Loads a texture from a file (PNG or JPEG).
    #[cfg(not(feature = "image-import"))]
    pub fn from_file(
        _device: &Device,
        _queue: &Queue,
        _path: impl AsRef<Path>,
    ) -> Result<Self, TextureError> {
        Err(TextureError::ImageImportDisabled)
    }

    /// Creates a texture with a custom sampler.
    pub fn with_sampler(mut self, device: &Device, descriptor: &SamplerDescriptor) -> Self {
        self.sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: descriptor.address_mode_u,
            address_mode_v: descriptor.address_mode_v,
            address_mode_w: descriptor.address_mode_w,
            mag_filter: descriptor.mag_filter,
            min_filter: descriptor.min_filter,
            mipmap_filter: descriptor.mipmap_filter,
            ..Default::default()
        });
        self
    }
}
