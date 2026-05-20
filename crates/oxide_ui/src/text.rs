//! Native overlay text rendering for game UI.

use std::collections::HashMap;
use std::path::Path;

use ab_glyph::{point, Font, FontArc, GlyphId, ScaleFont};
use bytemuck::{Pod, Zeroable};
use oxide_ecs::world::World;
use oxide_ecs::Resource;

use crate::{GameUi, GameUiAnchor};

pub const BUILTIN_GAME_FONT: &str = "oxide:builtin";

const BUILTIN_FONT_WIDTH: usize = 5;
const BUILTIN_FONT_HEIGHT: usize = 7;
const BUILTIN_FIRST_GLYPH: u8 = 32;
const BUILTIN_LAST_GLYPH: u8 = 126;
const TEXT_ATLAS_WIDTH: u32 = 2048;
const TEXT_ATLAS_HEIGHT: u32 = 2048;
const TEXT_ATLAS_PADDING: u32 = 1;

const TEXT_SHADER: &str = r#"
@group(0) @binding(0)
var glyph_atlas: texture_2d<f32>;

@group(0) @binding(1)
var glyph_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_atlas, glyph_sampler, input.uv).r;
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}
"#;

const TEXT_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x4
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextHorizontalAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextVerticalAlign {
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GameFontId(String);

impl GameFontId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn builtin() -> Self {
        Self(BUILTIN_GAME_FONT.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for GameFontId {
    fn default() -> Self {
        Self::builtin()
    }
}

impl From<&str> for GameFontId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for GameFontId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for GameFontId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum GameFontError {
    #[error("failed to read font '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("font '{id}' is not a valid TrueType/OpenType font")]
    InvalidFont { id: String },
}

#[derive(Clone)]
enum GameFontSource {
    BuiltinBitmap,
    TrueType(FontArc),
}

#[derive(Clone)]
pub struct GameFont {
    id: GameFontId,
    source: GameFontSource,
}

impl GameFont {
    pub fn id(&self) -> &GameFontId {
        &self.id
    }

    fn builtin() -> Self {
        Self {
            id: GameFontId::builtin(),
            source: GameFontSource::BuiltinBitmap,
        }
    }

    fn true_type(id: GameFontId, font: FontArc) -> Self {
        Self {
            id,
            source: GameFontSource::TrueType(font),
        }
    }
}

#[derive(Resource, Clone)]
pub struct GameFonts {
    fonts: HashMap<GameFontId, GameFont>,
}

impl Default for GameFonts {
    fn default() -> Self {
        let mut fonts = HashMap::new();
        let builtin = GameFont::builtin();
        fonts.insert(builtin.id.clone(), builtin);
        Self { fonts }
    }
}

impl GameFonts {
    pub fn register_truetype_bytes(
        &mut self,
        id: impl Into<GameFontId>,
        bytes: impl AsRef<[u8]>,
    ) -> Result<GameFontId, GameFontError> {
        let id = id.into();
        let font = FontArc::try_from_vec(bytes.as_ref().to_vec()).map_err(|_| {
            GameFontError::InvalidFont {
                id: id.as_str().to_string(),
            }
        })?;
        self.fonts
            .insert(id.clone(), GameFont::true_type(id.clone(), font));
        Ok(id)
    }

    pub fn load_truetype_file(
        &mut self,
        id: impl Into<GameFontId>,
        path: impl AsRef<Path>,
    ) -> Result<GameFontId, GameFontError> {
        let id = id.into();
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| GameFontError::Io {
            path: path.display().to_string(),
            source,
        })?;
        self.register_truetype_bytes(id, bytes)
    }

    pub fn alias_builtin(&mut self, id: impl Into<GameFontId>) -> GameFontId {
        let id = id.into();
        self.fonts.insert(
            id.clone(),
            GameFont {
                id: id.clone(),
                source: GameFontSource::BuiltinBitmap,
            },
        );
        id
    }

    pub fn contains(&self, id: &GameFontId) -> bool {
        self.fonts.contains_key(id)
    }

    fn resolve(&self, id: &GameFontId) -> &GameFont {
        self.fonts
            .get(id)
            .or_else(|| self.fonts.get(&GameFontId::builtin()))
            .expect("GameFonts always stores the builtin font")
    }
}

pub fn initialize_game_fonts(world: &mut World) {
    if !world.contains_resource::<GameFonts>() {
        world.insert_resource(GameFonts::default());
    }
}

pub fn register_game_font_bytes(
    world: &mut World,
    id: impl Into<GameFontId>,
    bytes: impl AsRef<[u8]>,
) -> Result<GameFontId, GameFontError> {
    ensure_game_fonts(world);
    world
        .resource_mut::<GameFonts>()
        .register_truetype_bytes(id, bytes)
}

pub fn load_game_font(
    world: &mut World,
    id: impl Into<GameFontId>,
    path: impl AsRef<Path>,
) -> Result<GameFontId, GameFontError> {
    ensure_game_fonts(world);
    world
        .resource_mut::<GameFonts>()
        .load_truetype_file(id, path)
}

fn ensure_game_fonts(world: &mut World) {
    if !world.contains_resource::<GameFonts>() {
        world.insert_resource(GameFonts::default());
    }
}

#[derive(Clone, Debug)]
pub struct GameTextStyle {
    pub font: GameFontId,
    /// Glyph height as a fraction of viewport height.
    pub size: f32,
    pub color: [f32; 4],
    /// Extra advance as a fraction of glyph height.
    pub tracking: f32,
    /// Line step as a multiplier of glyph height.
    pub line_height: f32,
    /// Optional wrapping width as a fraction of viewport width.
    pub max_width: Option<f32>,
    pub horizontal_align: TextHorizontalAlign,
    pub vertical_align: TextVerticalAlign,
}

impl Default for GameTextStyle {
    fn default() -> Self {
        Self {
            font: GameFontId::builtin(),
            size: 0.035,
            color: [0.94, 0.96, 0.9, 1.0],
            tracking: 0.08,
            line_height: 1.25,
            max_width: None,
            horizontal_align: TextHorizontalAlign::Center,
            vertical_align: TextVerticalAlign::Center,
        }
    }
}

impl GameTextStyle {
    pub fn new(size: f32, color: [f32; 4]) -> Self {
        Self {
            size,
            color,
            ..Default::default()
        }
    }

    pub fn with_font(mut self, font: impl Into<GameFontId>) -> Self {
        self.font = font.into();
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    pub fn with_tracking(mut self, tracking: f32) -> Self {
        self.tracking = tracking;
        self
    }

    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn with_wrap_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    pub fn without_wrap(mut self) -> Self {
        self.max_width = None;
        self
    }

    pub fn with_horizontal_align(mut self, align: TextHorizontalAlign) -> Self {
        self.horizontal_align = align;
        self
    }

    pub fn with_vertical_align(mut self, align: TextVerticalAlign) -> Self {
        self.vertical_align = align;
        self
    }
}

#[derive(Clone, Debug)]
pub struct GameUiText {
    pub id: String,
    pub anchor: GameUiAnchor,
    pub offset: [f32; 2],
    pub text: String,
    pub style: GameTextStyle,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

impl TextVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &TEXT_VERTEX_ATTRIBUTES,
        }
    }
}

#[derive(Debug)]
struct TextBatch {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    atlas_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

pub struct GameTextRenderer {
    pipeline: wgpu::RenderPipeline,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    batch: Option<TextBatch>,
}

impl GameTextRenderer {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Game UI Text Shader"),
            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Game UI Text Atlas Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Game UI Text Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let pipeline = create_text_pipeline(device, &shader, format, &atlas_layout);

        Self {
            pipeline,
            atlas_layout,
            sampler,
            batch: None,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &World,
        viewport_size: (f32, f32),
    ) {
        let (width, height) = viewport_size;
        if width == 0.0 || height == 0.0 {
            self.batch = None;
            return;
        }

        if !world.contains_resource::<GameUi>() {
            self.batch = None;
            return;
        }
        let game_ui = world.resource::<GameUi>();
        let default_fonts;
        let fonts = if world.contains_resource::<GameFonts>() {
            world.resource::<GameFonts>()
        } else {
            default_fonts = GameFonts::default();
            &default_fonts
        };

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut atlas = GlyphAtlasBuilder::new(TEXT_ATLAS_WIDTH, TEXT_ATLAS_HEIGHT);
        for text in game_ui.text_widgets() {
            push_text_widget(
                text,
                fonts,
                &mut atlas,
                width,
                height,
                &mut vertices,
                &mut indices,
            );
        }

        self.batch = create_text_batch(
            device,
            queue,
            TextBatchInput {
                atlas_layout: &self.atlas_layout,
                sampler: &self.sampler,
                vertices: &vertices,
                indices: &indices,
                atlas_width: atlas.width,
                atlas_height: atlas.height,
                atlas_pixels: &atlas.pixels,
            },
        );
    }

    pub fn queue(&self, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        let Some(batch) = &self.batch else {
            return;
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Game UI Text Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &batch.atlas_bind_group, &[]);
        render_pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
        render_pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..batch.index_count, 0, 0..1);
    }
}

fn create_text_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    atlas_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Game UI Text Pipeline Layout"),
        bind_group_layouts: &[Some(atlas_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Game UI Text Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[TextVertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn push_text_widget(
    text: &GameUiText,
    fonts: &GameFonts,
    atlas: &mut GlyphAtlasBuilder,
    viewport_width: f32,
    viewport_height: f32,
    vertices: &mut Vec<TextVertex>,
    indices: &mut Vec<u32>,
) {
    let style = &text.style;
    let font_height = (style.size.max(0.001) * viewport_height).max(1.0);
    let line_height = font_height * style.line_height.max(0.5);
    let max_width = style
        .max_width
        .map(|width| (width.max(0.001) * viewport_width).max(font_height));
    let font = fonts.resolve(&style.font);
    let layout = layout_text(&text.text, font, font_height, style.tracking, max_width);
    if layout.lines.is_empty() {
        return;
    }

    let anchor = anchor_pixel(text.anchor, text.offset, viewport_width, viewport_height);
    let total_height = font_height + (layout.lines.len().saturating_sub(1) as f32 * line_height);
    let top = match style.vertical_align {
        TextVerticalAlign::Top => anchor[1],
        TextVerticalAlign::Center => anchor[1] - total_height * 0.5,
        TextVerticalAlign::Bottom => anchor[1] - total_height,
    };

    for (line_index, line) in layout.lines.iter().enumerate() {
        let line_top = top + line_index as f32 * line_height;
        let line_left = match style.horizontal_align {
            TextHorizontalAlign::Left => anchor[0],
            TextHorizontalAlign::Center => anchor[0] - line.width * 0.5,
            TextHorizontalAlign::Right => anchor[0] - line.width,
        };

        for glyph in &line.glyphs {
            let Some(packed_glyph) = atlas.get_or_insert(font, glyph.ch, glyph.pixel_height) else {
                continue;
            };

            push_glyph_quad(
                vertices,
                indices,
                [
                    line_left + glyph.x + packed_glyph.offset[0],
                    line_top + packed_glyph.offset[1],
                ],
                packed_glyph.size,
                packed_glyph.uv,
                style.color,
                [viewport_width, viewport_height],
            );
        }
    }
}

struct TextBatchInput<'a> {
    atlas_layout: &'a wgpu::BindGroupLayout,
    sampler: &'a wgpu::Sampler,
    vertices: &'a [TextVertex],
    indices: &'a [u32],
    atlas_width: u32,
    atlas_height: u32,
    atlas_pixels: &'a [u8],
}

fn create_text_batch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: TextBatchInput<'_>,
) -> Option<TextBatch> {
    if input.vertices.is_empty() || input.indices.is_empty() {
        return None;
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Game UI Text Dynamic Atlas"),
        size: wgpu::Extent3d {
            width: input.atlas_width,
            height: input.atlas_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        input.atlas_pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(input.atlas_width),
            rows_per_image: Some(input.atlas_height),
        },
        wgpu::Extent3d {
            width: input.atlas_width,
            height: input.atlas_height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Game UI Text Atlas Bind Group"),
        layout: input.atlas_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(input.sampler),
            },
        ],
    });

    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Game UI Text Vertices"),
        size: std::mem::size_of_val(input.vertices) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(input.vertices));
    vertex_buffer.unmap();

    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Game UI Text Indices"),
        size: std::mem::size_of_val(input.indices) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::INDEX,
        mapped_at_creation: true,
    });
    index_buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(input.indices));
    index_buffer.unmap();

    Some(TextBatch {
        _texture: texture,
        _view: view,
        atlas_bind_group,
        vertex_buffer,
        index_buffer,
        index_count: input.indices.len() as u32,
    })
}

#[derive(Clone, Debug, Default)]
struct TextLayout {
    lines: Vec<TextLine>,
}

#[derive(Clone, Debug, Default)]
struct TextLine {
    glyphs: Vec<LayoutGlyph>,
    width: f32,
}

#[derive(Clone, Copy, Debug)]
struct LayoutGlyph {
    ch: char,
    x: f32,
    pixel_height: u32,
}

fn layout_text(
    text: &str,
    font: &GameFont,
    font_height: f32,
    tracking: f32,
    max_width: Option<f32>,
) -> TextLayout {
    let mut layout = TextLayout::default();
    let mut line = TextLine::default();
    let mut previous = None;

    for ch in text.chars() {
        if ch == '\n' {
            layout.lines.push(line);
            line = TextLine::default();
            previous = None;
            continue;
        }

        let mut metrics = glyph_metrics(font, ch, font_height, tracking, previous);
        if should_wrap(&line, metrics.advance, max_width) {
            layout.lines.push(line);
            line = TextLine::default();
            previous = None;
            if ch == ' ' {
                continue;
            }
            metrics = glyph_metrics(font, ch, font_height, tracking, previous);
        }

        line.glyphs.push(LayoutGlyph {
            ch: metrics.ch,
            x: line.width + metrics.kern,
            pixel_height: metrics.pixel_height,
        });
        line.width += metrics.kern + metrics.advance;
        previous = metrics.glyph_id;
    }

    layout.lines.push(line);
    layout
}

fn should_wrap(line: &TextLine, advance: f32, max_width: Option<f32>) -> bool {
    let Some(max_width) = max_width else {
        return false;
    };
    !line.glyphs.is_empty() && line.width + advance > max_width
}

#[derive(Clone, Copy, Debug)]
struct GlyphMetrics {
    ch: char,
    glyph_id: Option<GlyphId>,
    kern: f32,
    advance: f32,
    pixel_height: u32,
}

fn glyph_metrics(
    font: &GameFont,
    ch: char,
    font_height: f32,
    tracking: f32,
    previous: Option<GlyphId>,
) -> GlyphMetrics {
    let tracking = tracking.max(0.0) * font_height;
    match &font.source {
        GameFontSource::BuiltinBitmap => {
            let ch = normalize_builtin_glyph(ch);
            let scale = font_height / BUILTIN_FONT_HEIGHT as f32;
            let cell_width = if ch == ' ' {
                3.0
            } else {
                BUILTIN_FONT_WIDTH as f32 + 1.0
            };
            GlyphMetrics {
                ch,
                glyph_id: None,
                kern: 0.0,
                advance: cell_width * scale + tracking,
                pixel_height: font_height.ceil().max(1.0) as u32,
            }
        }
        GameFontSource::TrueType(font) => {
            let scaled = font.as_scaled(font_height);
            let glyph_id = scaled.glyph_id(ch);
            let kern = previous.map_or(0.0, |previous| scaled.kern(previous, glyph_id));
            GlyphMetrics {
                ch,
                glyph_id: Some(glyph_id),
                kern,
                advance: scaled.h_advance(glyph_id) + tracking,
                pixel_height: font_height.ceil().max(1.0) as u32,
            }
        }
    }
}

fn push_glyph_quad(
    vertices: &mut Vec<TextVertex>,
    indices: &mut Vec<u32>,
    position: [f32; 2],
    size: [f32; 2],
    uv: ([f32; 2], [f32; 2]),
    color: [f32; 4],
    viewport: [f32; 2],
) {
    let [x, y] = position;
    let [width, height] = size;
    let [viewport_width, viewport_height] = viewport;
    let (uv_min, uv_max) = uv;
    let base = vertices.len() as u32;

    vertices.extend_from_slice(&[
        TextVertex {
            position: ndc_position(x, y, viewport_width, viewport_height),
            uv: [uv_min[0], uv_min[1]],
            color,
        },
        TextVertex {
            position: ndc_position(x + width, y, viewport_width, viewport_height),
            uv: [uv_max[0], uv_min[1]],
            color,
        },
        TextVertex {
            position: ndc_position(x + width, y + height, viewport_width, viewport_height),
            uv: [uv_max[0], uv_max[1]],
            color,
        },
        TextVertex {
            position: ndc_position(x, y + height, viewport_width, viewport_height),
            uv: [uv_min[0], uv_max[1]],
            color,
        },
    ]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn ndc_position(x: f32, y: f32, viewport_width: f32, viewport_height: f32) -> [f32; 2] {
    [
        x / viewport_width * 2.0 - 1.0,
        1.0 - y / viewport_height * 2.0,
    ]
}

fn anchor_pixel(
    anchor: GameUiAnchor,
    offset: [f32; 2],
    viewport_width: f32,
    viewport_height: f32,
) -> [f32; 2] {
    let base = match anchor {
        GameUiAnchor::Center => [viewport_width * 0.5, viewport_height * 0.5],
        GameUiAnchor::TopLeft => [0.0, 0.0],
        GameUiAnchor::TopCenter => [viewport_width * 0.5, 0.0],
        GameUiAnchor::TopRight => [viewport_width, 0.0],
        GameUiAnchor::BottomLeft => [0.0, viewport_height],
        GameUiAnchor::BottomCenter => [viewport_width * 0.5, viewport_height],
        GameUiAnchor::BottomRight => [viewport_width, viewport_height],
    };

    [
        base[0] + offset[0] * viewport_width,
        base[1] - offset[1] * viewport_height,
    ]
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GlyphCacheKey {
    font: GameFontId,
    ch: char,
    pixel_height: u32,
}

#[derive(Clone, Copy, Debug)]
struct PackedGlyph {
    uv: ([f32; 2], [f32; 2]),
    size: [f32; 2],
    offset: [f32; 2],
}

struct GlyphAtlasBuilder {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    glyphs: HashMap<GlyphCacheKey, Option<PackedGlyph>>,
}

impl GlyphAtlasBuilder {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
            cursor_x: TEXT_ATLAS_PADDING,
            cursor_y: TEXT_ATLAS_PADDING,
            row_height: 0,
            glyphs: HashMap::new(),
        }
    }

    fn get_or_insert(
        &mut self,
        font: &GameFont,
        ch: char,
        pixel_height: u32,
    ) -> Option<PackedGlyph> {
        let key = GlyphCacheKey {
            font: font.id.clone(),
            ch,
            pixel_height,
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return *glyph;
        }

        let glyph =
            rasterize_glyph(font, ch, pixel_height).and_then(|glyph| self.pack_glyph(glyph));
        self.glyphs.insert(key, glyph);
        glyph
    }

    fn pack_glyph(&mut self, glyph: GlyphBitmap) -> Option<PackedGlyph> {
        if glyph.width == 0 || glyph.height == 0 {
            return None;
        }

        let padded_width = glyph.width + TEXT_ATLAS_PADDING;
        let padded_height = glyph.height + TEXT_ATLAS_PADDING;
        if padded_width >= self.width || padded_height >= self.height {
            return None;
        }

        if self.cursor_x + padded_width >= self.width {
            self.cursor_x = TEXT_ATLAS_PADDING;
            self.cursor_y += self.row_height + TEXT_ATLAS_PADDING;
            self.row_height = 0;
        }
        if self.cursor_y + padded_height >= self.height {
            return None;
        }

        let atlas_x = self.cursor_x;
        let atlas_y = self.cursor_y;
        for y in 0..glyph.height {
            let dst_start = ((atlas_y + y) * self.width + atlas_x) as usize;
            let src_start = (y * glyph.width) as usize;
            let count = glyph.width as usize;
            self.pixels[dst_start..dst_start + count]
                .copy_from_slice(&glyph.pixels[src_start..src_start + count]);
        }

        self.cursor_x += padded_width;
        self.row_height = self.row_height.max(glyph.height);
        let uv_min = [
            atlas_x as f32 / self.width as f32,
            atlas_y as f32 / self.height as f32,
        ];
        let uv_max = [
            (atlas_x + glyph.width) as f32 / self.width as f32,
            (atlas_y + glyph.height) as f32 / self.height as f32,
        ];

        Some(PackedGlyph {
            uv: (uv_min, uv_max),
            size: [glyph.width as f32, glyph.height as f32],
            offset: glyph.offset,
        })
    }
}

struct GlyphBitmap {
    width: u32,
    height: u32,
    offset: [f32; 2],
    pixels: Vec<u8>,
}

fn rasterize_glyph(font: &GameFont, ch: char, pixel_height: u32) -> Option<GlyphBitmap> {
    match &font.source {
        GameFontSource::BuiltinBitmap => rasterize_builtin_glyph(ch, pixel_height),
        GameFontSource::TrueType(font) => rasterize_truetype_glyph(font, ch, pixel_height),
    }
}

fn rasterize_builtin_glyph(ch: char, pixel_height: u32) -> Option<GlyphBitmap> {
    let ch = normalize_builtin_glyph(ch);
    if ch == ' ' {
        return None;
    }

    let scale = (pixel_height as f32 / BUILTIN_FONT_HEIGHT as f32).max(1.0);
    let width = (BUILTIN_FONT_WIDTH as f32 * scale).ceil().max(1.0) as u32;
    let height = pixel_height.max(1);
    let bitmap = glyph_bitmap(ch);
    let mut pixels = vec![0; (width * height) as usize];

    for y in 0..height {
        let source_y = ((y as f32 / scale).floor() as usize).min(BUILTIN_FONT_HEIGHT - 1);
        let bits = bitmap[source_y];
        for x in 0..width {
            let source_x = ((x as f32 / scale).floor() as usize).min(BUILTIN_FONT_WIDTH - 1);
            if bits & (1 << (BUILTIN_FONT_WIDTH - 1 - source_x)) != 0 {
                pixels[(y * width + x) as usize] = 255;
            }
        }
    }

    Some(GlyphBitmap {
        width,
        height,
        offset: [0.0, 0.0],
        pixels,
    })
}

fn rasterize_truetype_glyph(font: &FontArc, ch: char, pixel_height: u32) -> Option<GlyphBitmap> {
    if ch == ' ' {
        return None;
    }

    let scaled = font.as_scaled(pixel_height as f32);
    let glyph_id = scaled.glyph_id(ch);
    let baseline = scaled.ascent();
    let glyph = glyph_id.with_scale_and_position(pixel_height as f32, point(0.0, baseline));
    let outlined = scaled.outline_glyph(glyph).or_else(|| {
        let fallback = scaled
            .glyph_id('?')
            .with_scale_and_position(pixel_height as f32, point(0.0, baseline));
        scaled.outline_glyph(fallback)
    })?;
    let bounds = outlined.px_bounds();
    let width = bounds.width().ceil().max(0.0) as u32;
    let height = bounds.height().ceil().max(0.0) as u32;
    if width == 0 || height == 0 {
        return None;
    }

    let mut pixels = vec![0; (width * height) as usize];
    outlined.draw(|x, y, coverage| {
        if x < width && y < height {
            pixels[(y * width + x) as usize] = (coverage.clamp(0.0, 1.0) * 255.0) as u8;
        }
    });

    Some(GlyphBitmap {
        width,
        height,
        offset: [bounds.min.x, bounds.min.y],
        pixels,
    })
}

fn normalize_builtin_glyph(ch: char) -> char {
    let ch = ch.to_ascii_uppercase();
    let code = ch as u32;
    if (BUILTIN_FIRST_GLYPH as u32..=BUILTIN_LAST_GLYPH as u32).contains(&code) {
        ch
    } else {
        '?'
    }
}

fn glyph_bitmap(ch: char) -> [u8; BUILTIN_FONT_HEIGHT] {
    match normalize_builtin_glyph(ch) {
        ' ' => [0, 0, 0, 0, 0, 0, 0],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        '"' => [0b01010, 0b01010, 0b01010, 0, 0, 0, 0],
        '#' => [0b01010, 0b11111, 0b01010, 0b01010, 0b11111, 0b01010, 0],
        '$' => [
            0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100,
        ],
        '%' => [0b11001, 0b11010, 0b00100, 0b01000, 0b10110, 0b00110, 0],
        '&' => [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
        '\'' => [0b00100, 0b00100, 0b01000, 0, 0, 0, 0],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '*' => [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        ',' => [0, 0, 0, 0, 0b00100, 0b00100, 0b01000],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b00100, 0b00100],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ':' => [0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0],
        ';' => [0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0b01000],
        '<' => [
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ],
        '=' => [0, 0, 0b11111, 0, 0b11111, 0, 0],
        '>' => [
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '@' => [
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b01010, 0b01010, 0b00100, 0b01010, 0b01010, 0b10001,
        ],
        'Y' => [
            0b10001, 0b01010, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '[' => [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
        '\\' => [
            0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001,
        ],
        ']' => [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
        '^' => [0b00100, 0b01010, 0b10001, 0, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '`' => [0b01000, 0b00100, 0b00010, 0, 0, 0, 0],
        '{' => [
            0b00010, 0b00100, 0b00100, 0b01000, 0b00100, 0b00100, 0b00010,
        ],
        '|' => [0b00100, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0b00100],
        '}' => [
            0b01000, 0b00100, 0b00100, 0b00010, 0b00100, 0b00100, 0b01000,
        ],
        '~' => [0, 0, 0b01000, 0b10101, 0b00010, 0, 0],
        _ => glyph_bitmap('?'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_case_normalizes_to_uppercase() {
        assert_eq!(normalize_builtin_glyph('a'), 'A');
        assert_eq!(normalize_builtin_glyph('z'), 'Z');
    }

    #[test]
    fn text_layout_wraps_when_max_width_is_reached() {
        let font = GameFont::builtin();
        let layout = layout_text("ABCDE", &font, 70.0, 0.0, Some(130.0));
        assert!(layout.lines.len() > 1);
    }

    #[test]
    fn builtin_glyph_rasterizes_alpha_for_known_glyph() {
        let glyph = rasterize_builtin_glyph('A', 28).unwrap();
        assert!(glyph.pixels.contains(&255));
    }

    #[test]
    fn game_fonts_can_alias_builtin_font() {
        let mut fonts = GameFonts::default();
        let id = fonts.alias_builtin("menu");
        assert!(fonts.contains(&id));
    }
}
