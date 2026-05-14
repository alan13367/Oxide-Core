//! Camera-locked game UI rendered through scene primitives.

use std::collections::{HashMap, HashSet};

use glam::{Mat3, Vec3};
use oxide_camera::CameraComponent;
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::Resource;
use oxide_math::transform::Transform;
use oxide_scene::{GlobalTransform, MeshPrimitive, Name, RenderMaterial, RenderMesh};
use oxide_transform::TransformComponent;

use super::text::{GameTextStyle, GameUiText};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameUiAnchor {
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Debug)]
pub enum GameUiWidget {
    Panel(GameUiRect),
    Button(GameUiButton),
    Bar(GameUiBar),
    Counter(GameUiCounter),
    Reticle(GameUiReticle),
    Text(GameUiText),
}

#[derive(Clone, Debug)]
pub struct GameUiRect {
    pub id: String,
    pub anchor: GameUiAnchor,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct GameUiButton {
    pub id: String,
    pub anchor: GameUiAnchor,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub selected_color: [f32; 4],
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct GameUiBar {
    pub id: String,
    pub anchor: GameUiAnchor,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub value: f32,
    pub background_color: [f32; 4],
    pub fill_color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct GameUiCounter {
    pub id: String,
    pub anchor: GameUiAnchor,
    pub offset: [f32; 2],
    pub pip_size: [f32; 2],
    pub gap: f32,
    pub value: u32,
    pub max: u32,
    pub active_color: [f32; 4],
    pub inactive_color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct GameUiReticle {
    pub id: String,
    pub size: f32,
    pub thickness: f32,
    pub color: [f32; 4],
}

#[derive(Resource)]
pub struct GameUi {
    widgets: Vec<GameUiWidget>,
    entities: HashMap<String, Vec<Entity>>,
    distance: f32,
}

impl Default for GameUi {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            entities: HashMap::new(),
            distance: 0.9,
        }
    }
}

impl GameUi {
    pub fn clear(&mut self) {
        self.widgets.clear();
    }

    pub fn set_distance(&mut self, distance: f32) {
        self.distance = distance.max(0.2);
    }

    pub fn panel(
        &mut self,
        id: impl Into<String>,
        anchor: GameUiAnchor,
        offset: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
    ) {
        self.widgets.push(GameUiWidget::Panel(GameUiRect {
            id: id.into(),
            anchor,
            offset,
            size,
            color,
        }));
    }

    pub fn button(
        &mut self,
        id: impl Into<String>,
        anchor: GameUiAnchor,
        offset: [f32; 2],
        size: [f32; 2],
        selected: bool,
    ) {
        self.widgets.push(GameUiWidget::Button(GameUiButton {
            id: id.into(),
            anchor,
            offset,
            size,
            color: [0.16, 0.18, 0.22, 1.0],
            selected_color: [0.86, 0.62, 0.18, 1.0],
            selected,
        }));
    }

    pub fn bar(
        &mut self,
        id: impl Into<String>,
        anchor: GameUiAnchor,
        offset: [f32; 2],
        size: [f32; 2],
        value: f32,
        fill_color: [f32; 4],
    ) {
        self.widgets.push(GameUiWidget::Bar(GameUiBar {
            id: id.into(),
            anchor,
            offset,
            size,
            value: value.clamp(0.0, 1.0),
            background_color: [0.06, 0.07, 0.08, 1.0],
            fill_color,
        }));
    }

    pub fn counter(
        &mut self,
        id: impl Into<String>,
        anchor: GameUiAnchor,
        offset: [f32; 2],
        value: u32,
        max: u32,
        active_color: [f32; 4],
    ) {
        self.widgets.push(GameUiWidget::Counter(GameUiCounter {
            id: id.into(),
            anchor,
            offset,
            pip_size: [0.014, 0.026],
            gap: 0.006,
            value,
            max,
            active_color,
            inactive_color: [0.08, 0.09, 0.1, 1.0],
        }));
    }

    pub fn reticle(&mut self, id: impl Into<String>, size: f32, thickness: f32, color: [f32; 4]) {
        self.widgets.push(GameUiWidget::Reticle(GameUiReticle {
            id: id.into(),
            size,
            thickness,
            color,
        }));
    }

    pub fn text(
        &mut self,
        id: impl Into<String>,
        anchor: GameUiAnchor,
        offset: [f32; 2],
        text: impl Into<String>,
        style: GameTextStyle,
    ) {
        self.widgets.push(GameUiWidget::Text(GameUiText {
            id: id.into(),
            anchor,
            offset,
            text: text.into(),
            style,
        }));
    }

    pub fn label(
        &mut self,
        id: impl Into<String>,
        anchor: GameUiAnchor,
        offset: [f32; 2],
        text: impl Into<String>,
    ) {
        self.text(id, anchor, offset, text, GameTextStyle::default());
    }

    pub(crate) fn text_widgets(&self) -> impl Iterator<Item = &GameUiText> {
        self.widgets.iter().filter_map(|widget| match widget {
            GameUiWidget::Text(text) => Some(text),
            _ => None,
        })
    }
}

pub fn initialize_game_ui(world: &mut World) {
    if !world.contains_resource::<GameUi>() {
        world.insert_resource(GameUi::default());
    }
}

pub fn game_ui_sync_system(world: &mut World, aspect_ratio: f32) {
    let Some(mut game_ui) = world.remove_resource::<GameUi>() else {
        return;
    };

    sync_game_ui(world, &mut game_ui, aspect_ratio);
    world.insert_resource(game_ui);
}

fn sync_game_ui(world: &mut World, game_ui: &mut GameUi, aspect_ratio: f32) {
    let Some(camera) = active_ui_camera(world, aspect_ratio) else {
        remove_stale_widgets(world, game_ui, HashSet::new());
        return;
    };

    let mut active_ids = HashSet::new();
    let widgets = game_ui.widgets.clone();
    for widget in widgets {
        match widget {
            GameUiWidget::Panel(rect) => {
                active_ids.insert(rect.id.clone());
                sync_parts(world, game_ui, &camera, &rect.id, 1, |index| {
                    rect_part(&rect, index, 0.0)
                });
            }
            GameUiWidget::Button(button) => {
                active_ids.insert(button.id.clone());
                sync_parts(world, game_ui, &camera, &button.id, 1, |index| {
                    button_part(&button, index)
                });
            }
            GameUiWidget::Bar(bar) => {
                active_ids.insert(bar.id.clone());
                sync_parts(world, game_ui, &camera, &bar.id, 2, |index| {
                    bar_part(&bar, index)
                });
            }
            GameUiWidget::Counter(counter) => {
                active_ids.insert(counter.id.clone());
                let count = counter.max.min(64) as usize;
                sync_parts(world, game_ui, &camera, &counter.id, count, |index| {
                    counter_part(&counter, index)
                });
            }
            GameUiWidget::Reticle(reticle) => {
                active_ids.insert(reticle.id.clone());
                sync_parts(world, game_ui, &camera, &reticle.id, 2, |index| {
                    reticle_part(&reticle, index)
                });
            }
            GameUiWidget::Text(_) => {}
        }
    }

    remove_stale_widgets(world, game_ui, active_ids);
}

#[derive(Clone, Copy)]
struct UiCamera {
    position: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    rotation: glam::Quat,
    fov: f32,
    aspect: f32,
}

#[derive(Clone, Copy)]
struct UiPart {
    anchor: GameUiAnchor,
    offset: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    depth_bias: f32,
}

fn active_ui_camera(world: &mut World, aspect: f32) -> Option<UiCamera> {
    let camera = {
        let mut query = world.query::<&CameraComponent>();
        query.iter(world).next().copied()?
    };

    let forward = camera.0.forward().normalize_or_zero();
    if forward.length_squared() <= f32::EPSILON {
        return None;
    }

    let mut up = camera.0.up.normalize_or_zero();
    if up.length_squared() <= f32::EPSILON {
        up = Vec3::Y;
    }
    let right = forward.cross(up).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    let rotation = glam::Quat::from_mat3(&Mat3::from_cols(right, up, -forward));

    Some(UiCamera {
        position: camera.0.position,
        forward,
        right,
        up,
        rotation,
        fov: camera.0.fov,
        aspect,
    })
}

fn sync_parts(
    world: &mut World,
    game_ui: &mut GameUi,
    camera: &UiCamera,
    id: &str,
    count: usize,
    mut part_at: impl FnMut(usize) -> UiPart,
) {
    let existing = game_ui.entities.entry(id.to_string()).or_default();

    while existing.len() < count {
        existing.push(spawn_ui_entity(world, id));
    }

    while existing.len() > count {
        if let Some(entity) = existing.pop() {
            let _ = world.despawn(entity);
        }
    }

    for (index, entity) in existing.iter().copied().enumerate() {
        apply_ui_part(world, entity, camera, game_ui.distance, part_at(index));
    }
}

fn spawn_ui_entity(world: &mut World, id: &str) -> Entity {
    world
        .spawn((
            Name(format!("Game UI {id}")),
            TransformComponent::default(),
            GlobalTransform::default(),
            RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
        ))
        .id()
}

fn apply_ui_part(
    world: &mut World,
    entity: Entity,
    camera: &UiCamera,
    distance: f32,
    part: UiPart,
) {
    let (position, scale) = ui_transform(camera, distance + part.depth_bias, part);

    if let Some(transform) = world.get_mut::<TransformComponent>(entity) {
        transform.set_transform(Transform {
            position,
            rotation: camera.rotation,
            scale,
        });
    }

    if let Some(render_mesh) = world.get_mut::<RenderMesh>(entity) {
        render_mesh.tint = part.color;
    }
}

fn ui_transform(camera: &UiCamera, distance: f32, part: UiPart) -> (Vec3, Vec3) {
    let screen_height = 2.0 * distance * (camera.fov * 0.5).tan();
    let screen_width = screen_height * camera.aspect;
    let anchor = anchor_position(part.anchor);

    let center_x = (anchor[0] + part.offset[0]) * screen_width;
    let center_y = (anchor[1] + part.offset[1]) * screen_height;
    let width = part.size[0].max(0.001) * screen_width;
    let height = part.size[1].max(0.001) * screen_height;

    let position = camera.position
        + camera.forward * distance
        + camera.right * center_x
        + camera.up * center_y;
    (position, Vec3::new(width, height, 0.006))
}

fn anchor_position(anchor: GameUiAnchor) -> [f32; 2] {
    match anchor {
        GameUiAnchor::Center => [0.0, 0.0],
        GameUiAnchor::TopLeft => [-0.5, 0.5],
        GameUiAnchor::TopCenter => [0.0, 0.5],
        GameUiAnchor::TopRight => [0.5, 0.5],
        GameUiAnchor::BottomLeft => [-0.5, -0.5],
        GameUiAnchor::BottomCenter => [0.0, -0.5],
        GameUiAnchor::BottomRight => [0.5, -0.5],
    }
}

fn rect_part(rect: &GameUiRect, _index: usize, depth_bias: f32) -> UiPart {
    UiPart {
        anchor: rect.anchor,
        offset: rect.offset,
        size: rect.size,
        color: rect.color,
        depth_bias,
    }
}

fn button_part(button: &GameUiButton, _index: usize) -> UiPart {
    UiPart {
        anchor: button.anchor,
        offset: button.offset,
        size: button.size,
        color: if button.selected {
            button.selected_color
        } else {
            button.color
        },
        depth_bias: -0.02,
    }
}

fn bar_part(bar: &GameUiBar, index: usize) -> UiPart {
    if index == 0 {
        return UiPart {
            anchor: bar.anchor,
            offset: bar.offset,
            size: bar.size,
            color: bar.background_color,
            depth_bias: 0.0,
        };
    }

    let value = bar.value.clamp(0.0, 1.0);
    UiPart {
        anchor: bar.anchor,
        offset: [
            bar.offset[0] - bar.size[0] * (1.0 - value) * 0.5,
            bar.offset[1],
        ],
        size: [bar.size[0] * value.max(0.001), bar.size[1] * 0.72],
        color: bar.fill_color,
        depth_bias: -0.02,
    }
}

fn counter_part(counter: &GameUiCounter, index: usize) -> UiPart {
    let index = index as u32;
    UiPart {
        anchor: counter.anchor,
        offset: [
            counter.offset[0] + index as f32 * (counter.pip_size[0] + counter.gap),
            counter.offset[1],
        ],
        size: counter.pip_size,
        color: if index < counter.value {
            counter.active_color
        } else {
            counter.inactive_color
        },
        depth_bias: -0.02,
    }
}

fn reticle_part(reticle: &GameUiReticle, index: usize) -> UiPart {
    let size = if index == 0 {
        [reticle.size, reticle.thickness]
    } else {
        [reticle.thickness, reticle.size]
    };

    UiPart {
        anchor: GameUiAnchor::Center,
        offset: [0.0, 0.0],
        size,
        color: reticle.color,
        depth_bias: -0.04,
    }
}

fn remove_stale_widgets(world: &mut World, game_ui: &mut GameUi, active_ids: HashSet<String>) {
    let stale: Vec<String> = game_ui
        .entities
        .keys()
        .filter(|id| !active_ids.contains(*id))
        .cloned()
        .collect();

    for id in stale {
        if let Some(entities) = game_ui.entities.remove(&id) {
            for entity in entities {
                let _ = world.despawn(entity);
            }
        }
    }
}
