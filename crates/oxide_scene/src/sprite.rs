//! Native sprite assets and billboard components for scene rendering.

use std::collections::HashMap;

use glam::Vec2;
use oxide_ecs::{Component, Resource};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteId(String);

impl SpriteId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SpriteId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SpriteId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for SpriteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum SpriteImageError {
    #[error("sprite dimensions must be non-zero, got {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("sprite RGBA data has {actual} bytes, expected {expected}")]
    InvalidRgbaLength { expected: usize, actual: usize },
    #[error("sprite ASCII rows cannot be empty")]
    EmptyRows,
    #[error("sprite ASCII rows must all have the same width")]
    RaggedRows,
    #[error("sprite ASCII row contains unmapped character '{ch}'")]
    UnknownPaletteChar { ch: char },
}

#[derive(Clone, Debug)]
pub struct SpriteImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl SpriteImage {
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, SpriteImageError> {
        if width == 0 || height == 0 {
            return Err(SpriteImageError::InvalidDimensions { width, height });
        }

        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(SpriteImageError::InvalidRgbaLength {
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

    pub fn solid(width: u32, height: u32, color: [u8; 4]) -> Result<Self, SpriteImageError> {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width.saturating_mul(height) {
            rgba.extend_from_slice(&color);
        }
        Self::from_rgba(width, height, rgba)
    }

    pub fn from_ascii(
        rows: &[&str],
        palette: &[(char, [u8; 4])],
    ) -> Result<Self, SpriteImageError> {
        let first = rows.first().ok_or(SpriteImageError::EmptyRows)?;
        let width = first.chars().count();
        if width == 0 {
            return Err(SpriteImageError::InvalidDimensions {
                width: 0,
                height: rows.len() as u32,
            });
        }
        if rows.iter().any(|row| row.chars().count() != width) {
            return Err(SpriteImageError::RaggedRows);
        }

        let palette: HashMap<char, [u8; 4]> = palette.iter().copied().collect();
        let mut rgba = Vec::with_capacity(width * rows.len() * 4);
        for row in rows {
            for ch in row.chars() {
                let Some(color) = palette.get(&ch) else {
                    return Err(SpriteImageError::UnknownPaletteChar { ch });
                };
                rgba.extend_from_slice(color);
            }
        }

        Self::from_rgba(width as u32, rows.len() as u32, rgba)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

#[derive(Clone, Debug)]
pub struct SpriteAsset {
    pub image: SpriteImage,
    pub revision: u64,
}

#[derive(Resource, Default)]
pub struct SpriteAssets {
    sprites: HashMap<SpriteId, SpriteAsset>,
}

impl SpriteAssets {
    pub fn register(&mut self, id: impl Into<SpriteId>, image: SpriteImage) -> SpriteId {
        let id = id.into();
        let revision = self
            .sprites
            .get(&id)
            .map(|sprite| sprite.revision.saturating_add(1))
            .unwrap_or(1);
        self.sprites
            .insert(id.clone(), SpriteAsset { image, revision });
        id
    }

    pub fn get(&self, id: &SpriteId) -> Option<&SpriteAsset> {
        self.sprites.get(id)
    }

    pub fn contains(&self, id: &SpriteId) -> bool {
        self.sprites.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpriteFacing {
    /// Rotate around Y so upright sprites face the active camera.
    #[default]
    YBillboard,
    /// Fully face the active camera, including pitch.
    Camera,
    /// Use the entity transform rotation.
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpriteDepthMode {
    /// Test against scene depth for actors and world props.
    #[default]
    World,
    /// Draw over the scene using clip-space transform position/size.
    Overlay,
}

#[derive(Component, Clone, Debug)]
pub struct SpriteBillboard {
    pub sprite: SpriteId,
    pub size: Vec2,
    pub tint: [f32; 4],
    pub facing: SpriteFacing,
    pub depth: SpriteDepthMode,
}

impl SpriteBillboard {
    pub fn new(sprite: impl Into<SpriteId>, size: Vec2) -> Self {
        Self {
            sprite: sprite.into(),
            size,
            tint: [1.0, 1.0, 1.0, 1.0],
            facing: SpriteFacing::default(),
            depth: SpriteDepthMode::default(),
        }
    }

    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    pub fn with_facing(mut self, facing: SpriteFacing) -> Self {
        self.facing = facing;
        self
    }

    pub fn with_depth(mut self, depth: SpriteDepthMode) -> Self {
        self.depth = depth;
        self
    }
}

pub fn register_sprite(
    world: &mut oxide_ecs::world::World,
    id: impl Into<SpriteId>,
    image: SpriteImage,
) -> SpriteId {
    if !world.contains_resource::<SpriteAssets>() {
        world.insert_resource(SpriteAssets::default());
    }
    world.resource_mut::<SpriteAssets>().register(id, image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_sprite_maps_palette_to_rgba() {
        let image = SpriteImage::from_ascii(
            &[" A", "A "],
            &[(' ', [0, 0, 0, 0]), ('A', [255, 0, 0, 255])],
        )
        .expect("sprite should parse");

        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.rgba()[0..4], [0, 0, 0, 0]);
        assert_eq!(image.rgba()[4..8], [255, 0, 0, 255]);
    }

    #[test]
    fn sprite_assets_increment_revision_on_replace() {
        let mut assets = SpriteAssets::default();
        let id = assets.register(
            "actor",
            SpriteImage::solid(1, 1, [255, 255, 255, 255]).unwrap(),
        );
        let first = assets.get(&id).unwrap().revision;

        assets.register(
            id.clone(),
            SpriteImage::solid(1, 1, [0, 0, 0, 255]).unwrap(),
        );

        assert!(assets.get(&id).unwrap().revision > first);
    }
}
