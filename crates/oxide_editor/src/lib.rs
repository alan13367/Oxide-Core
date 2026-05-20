//! Runtime scene editor model and egui surface.

use glam::{Mat4, Vec2, Vec3, Vec4};
use oxide_camera::CameraComponent;
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::Resource;
use oxide_input::MouseInput;
use oxide_light::{AmbientLight, DirectionalLight, PointLight};
use oxide_math::transform::Transform;
use oxide_scene::{
    Children, GlobalTransform, MeshPrimitive, Name, Parent, RenderMaterial, RenderMesh,
    SceneGizmoLines, TransformComponent,
};
use oxide_ui::{DevOverlay, DevOverlaySnapshot, RuntimeUi};

#[derive(Resource, Clone, Debug)]
pub struct SceneEditor {
    pub visible: bool,
    selected: Option<Entity>,
    active_tool: SceneEditorTool,
    active_axis: GizmoAxis,
    drag: Option<GizmoDrag>,
    last_left_pressed: bool,
    next_spawn_index: u32,
}

impl Default for SceneEditor {
    fn default() -> Self {
        Self {
            visible: true,
            selected: None,
            active_tool: SceneEditorTool::Select,
            active_axis: GizmoAxis::X,
            drag: None,
            last_left_pressed: false,
            next_spawn_index: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneEditorTool {
    Select,
    Translate,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoAxis {
    X,
    Y,
    Z,
    Uniform,
}

#[derive(Clone, Copy, Debug)]
struct GizmoDrag {
    entity: Entity,
    start_cursor: Vec2,
    start_transform: Transform,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePickRay {
    pub origin: Vec3,
    pub direction: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePickHit {
    pub entity: Entity,
    pub distance: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneEntitySummary {
    pub entity: Entity,
    pub name: Option<String>,
    pub parent: Option<Entity>,
    pub child_count: usize,
    pub has_render_mesh: bool,
    pub has_camera: bool,
    pub has_light: bool,
}

pub fn initialize_scene_editor(world: &mut World) {
    if !world.contains_resource::<SceneEditor>() {
        world.insert_resource(SceneEditor::default());
    }
}

pub fn show_scene_editor_egui(world: &mut World, ctx: &egui::Context) {
    with_scene_editor(world, |editor, world| editor.show_egui(ctx, world));
}

pub fn show_scene_authoring_egui(world: &mut World, ctx: &egui::Context) {
    show_scene_editor_egui(world, ctx);

    if let Some(mut runtime_ui) = world.remove_resource::<RuntimeUi>() {
        runtime_ui.show_egui(ctx);
        world.insert_resource(runtime_ui);
    }

    if world.contains_resource::<DevOverlay>() {
        let overlay = world.resource::<DevOverlay>().clone();
        overlay.show_egui(ctx, DevOverlaySnapshot::from_world(world));
    }
}

pub fn with_scene_editor<R>(
    world: &mut World,
    f: impl FnOnce(&mut SceneEditor, &mut World) -> R,
) -> Option<R> {
    let mut editor = world.remove_resource::<SceneEditor>()?;
    let result = f(&mut editor, world);
    world.insert_resource(editor);
    Some(result)
}

pub fn scene_editor_viewport_system(world: &mut World, viewport_size: [f32; 2]) {
    if !world.contains_resource::<SceneEditor>() || !world.contains_resource::<MouseInput>() {
        return;
    }
    if !world.contains_resource::<SceneGizmoLines>() {
        world.insert_resource(SceneGizmoLines::default());
    }

    let mut editor = world.remove_resource::<SceneEditor>().unwrap();
    editor.update_viewport_interaction(world, viewport_size);
    world.insert_resource(editor);
}

impl SceneEditor {
    pub fn selected(&self) -> Option<Entity> {
        self.selected
    }

    pub fn active_tool(&self) -> SceneEditorTool {
        self.active_tool
    }

    pub fn set_active_tool(&mut self, tool: SceneEditorTool) {
        self.active_tool = tool;
        if tool == SceneEditorTool::Select {
            self.drag = None;
        }
    }

    pub fn active_axis(&self) -> GizmoAxis {
        self.active_axis
    }

    pub fn set_active_axis(&mut self, axis: GizmoAxis) {
        self.active_axis = axis;
    }

    pub fn select(&mut self, world: &World, entity: Option<Entity>) -> bool {
        match entity {
            Some(entity) if world.contains(entity) => {
                self.selected = Some(entity);
                true
            }
            Some(_) => false,
            None => {
                self.selected = None;
                true
            }
        }
    }

    pub fn entities(&self, world: &mut World) -> Vec<SceneEntitySummary> {
        let mut query = world.query::<(Entity, &TransformComponent)>();
        let entities: Vec<Entity> = query.iter(world).map(|(entity, _)| entity).collect();

        entities
            .into_iter()
            .map(|entity| SceneEntitySummary {
                entity,
                name: world.get::<Name>(entity).map(|name| name.0.clone()),
                parent: world.get::<Parent>(entity).map(|parent| parent.0),
                child_count: world
                    .get::<Children>(entity)
                    .map(|children| children.len())
                    .unwrap_or_default(),
                has_render_mesh: world.get::<RenderMesh>(entity).is_some(),
                has_camera: world.get::<CameraComponent>(entity).is_some(),
                has_light: world.get::<AmbientLight>(entity).is_some()
                    || world.get::<DirectionalLight>(entity).is_some()
                    || world.get::<PointLight>(entity).is_some(),
            })
            .collect()
    }

    pub fn spawn_cube(&mut self, world: &mut World) -> Entity {
        self.spawn_mesh(world, MeshPrimitive::Cube)
    }

    pub fn spawn_sphere(&mut self, world: &mut World) -> Entity {
        self.spawn_mesh(
            world,
            MeshPrimitive::Sphere {
                segments: 16,
                rings: 16,
            },
        )
    }

    pub fn spawn_mesh(&mut self, world: &mut World, primitive: MeshPrimitive) -> Entity {
        let name = match primitive {
            MeshPrimitive::Cube => format!("Cube {}", self.next_spawn_index),
            MeshPrimitive::Sphere { .. } => format!("Sphere {}", self.next_spawn_index),
        };
        self.next_spawn_index += 1;

        let entity = world
            .spawn((
                Name(name),
                TransformComponent::from_position(Vec3::new(0.0, 0.0, 0.0)),
                GlobalTransform::default(),
                RenderMesh::new(primitive, RenderMaterial::default()),
            ))
            .id();
        self.selected = Some(entity);
        entity
    }

    pub fn duplicate_selected(&mut self, world: &mut World) -> Option<Entity> {
        let source = self.selected?;
        if !world.contains(source) {
            self.selected = None;
            return None;
        }

        let transform = world.get::<TransformComponent>(source)?.clone();
        let render_mesh = world.get::<RenderMesh>(source).cloned();
        let name = world
            .get::<Name>(source)
            .map(|name| format!("{} Copy", name.0))
            .unwrap_or_else(|| "Entity Copy".to_string());

        let entity = world
            .spawn((Name(name), transform, GlobalTransform::default()))
            .id();

        if let Some(render_mesh) = render_mesh {
            world.entity_mut(entity).insert(render_mesh);
        }

        self.selected = Some(entity);
        Some(entity)
    }

    pub fn delete_selected(&mut self, world: &mut World) -> bool {
        let Some(entity) = self.selected.take() else {
            return false;
        };

        delete_entity_recursive(world, entity)
    }

    pub fn set_selected_name(&mut self, world: &mut World, name: impl Into<String>) -> bool {
        let Some(entity) = self.selected.filter(|entity| world.contains(*entity)) else {
            return false;
        };

        if let Some(existing) = world.get_mut::<Name>(entity) {
            existing.0 = name.into();
        } else {
            world.entity_mut(entity).insert(Name(name.into()));
        }
        true
    }

    pub fn set_selected_transform(&mut self, world: &mut World, transform: Transform) -> bool {
        let Some(entity) = self.selected.filter(|entity| world.contains(*entity)) else {
            return false;
        };

        if let Some(component) = world.get_mut::<TransformComponent>(entity) {
            component.set_transform(transform);
        } else {
            world
                .entity_mut(entity)
                .insert(TransformComponent::new(transform));
        }
        true
    }

    pub fn set_selected_tint(&mut self, world: &mut World, tint: [f32; 4]) -> bool {
        let Some(entity) = self.selected.filter(|entity| world.contains(*entity)) else {
            return false;
        };

        if let Some(render_mesh) = world.get_mut::<RenderMesh>(entity) {
            render_mesh.tint = tint;
            true
        } else {
            false
        }
    }

    pub fn show_egui(&mut self, ctx: &egui::Context, world: &mut World) {
        if !self.visible {
            return;
        }

        let summaries = self.entities(world);

        egui::SidePanel::left("oxide_scene_hierarchy")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Scene");
                ui.horizontal(|ui| {
                    if ui.button("Cube").clicked() {
                        self.spawn_cube(world);
                    }
                    if ui.button("Sphere").clicked() {
                        self.spawn_sphere(world);
                    }
                });
                ui.horizontal(|ui| {
                    tool_button(ui, &mut self.active_tool, SceneEditorTool::Select, "Select");
                    tool_button(
                        ui,
                        &mut self.active_tool,
                        SceneEditorTool::Translate,
                        "Move",
                    );
                    tool_button(ui, &mut self.active_tool, SceneEditorTool::Rotate, "Rotate");
                    tool_button(ui, &mut self.active_tool, SceneEditorTool::Scale, "Scale");
                });
                ui.horizontal(|ui| {
                    axis_button(ui, &mut self.active_axis, GizmoAxis::X, "X");
                    axis_button(ui, &mut self.active_axis, GizmoAxis::Y, "Y");
                    axis_button(ui, &mut self.active_axis, GizmoAxis::Z, "Z");
                    axis_button(ui, &mut self.active_axis, GizmoAxis::Uniform, "All");
                });
                ui.separator();

                for summary in &summaries {
                    let label = summary
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Entity {}", summary.entity.index()));
                    let selected = self.selected == Some(summary.entity);
                    if ui.selectable_label(selected, label).clicked() {
                        self.selected = Some(summary.entity);
                    }
                }
            });

        egui::Window::new("Inspector").show(ctx, |ui| {
            let Some(entity) = self.selected.filter(|entity| world.contains(*entity)) else {
                ui.label("No entity selected");
                return;
            };

            ui.label(format!("Entity {}", entity.index()));

            let mut name = world
                .get::<Name>(entity)
                .map(|name| name.0.clone())
                .unwrap_or_default();
            if ui.text_edit_singleline(&mut name).changed() {
                self.set_selected_name(world, name);
            }

            if let Some(transform) = world.get::<TransformComponent>(entity).cloned() {
                let mut position = transform.transform.position.to_array();
                let mut scale = transform.transform.scale.to_array();

                ui.separator();
                ui.label("Position");
                let mut position_changed = false;
                ui.horizontal(|ui| {
                    position_changed |= drag_value(ui, &mut position[0], "x");
                    position_changed |= drag_value(ui, &mut position[1], "y");
                    position_changed |= drag_value(ui, &mut position[2], "z");
                });

                ui.label("Scale");
                let mut scale_changed = false;
                ui.horizontal(|ui| {
                    scale_changed |= drag_value(ui, &mut scale[0], "x");
                    scale_changed |= drag_value(ui, &mut scale[1], "y");
                    scale_changed |= drag_value(ui, &mut scale[2], "z");
                });

                if position_changed || scale_changed {
                    let mut edited = transform.transform;
                    edited.position = Vec3::from_array(position);
                    edited.scale = Vec3::from_array(scale);
                    self.set_selected_transform(world, edited);
                }
            }

            if let Some(render_mesh) = world.get::<RenderMesh>(entity).cloned() {
                let mut tint = render_mesh.tint;
                ui.separator();
                ui.label("Tint");
                let mut tint_changed = false;
                ui.horizontal(|ui| {
                    tint_changed |= drag_value(ui, &mut tint[0], "r");
                    tint_changed |= drag_value(ui, &mut tint[1], "g");
                    tint_changed |= drag_value(ui, &mut tint[2], "b");
                    tint_changed |= drag_value(ui, &mut tint[3], "a");
                });
                if tint_changed {
                    self.set_selected_tint(world, tint);
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Duplicate").clicked() {
                    self.duplicate_selected(world);
                }
                if ui.button("Delete").clicked() {
                    self.delete_selected(world);
                }
            });
        });
    }

    fn update_viewport_interaction(&mut self, world: &mut World, viewport_size: [f32; 2]) {
        {
            let gizmos = world.resource_mut::<SceneGizmoLines>();
            gizmos.clear();
        }
        self.draw_selected_gizmo(world);

        let (cursor, left_pressed) = {
            let mouse = world.resource::<MouseInput>();
            (
                mouse
                    .position
                    .map(|position| Vec2::new(position.x as f32, position.y as f32)),
                mouse.left_pressed,
            )
        };
        let just_pressed = left_pressed && !self.last_left_pressed;
        self.last_left_pressed = left_pressed;

        let Some(cursor) = cursor else {
            return;
        };

        if !left_pressed {
            self.drag = None;
            return;
        }

        match self.active_tool {
            SceneEditorTool::Select => {
                if just_pressed {
                    let picked = active_camera(world)
                        .and_then(|camera| viewport_pick_ray(&camera.0, cursor, viewport_size))
                        .and_then(|ray| pick_render_mesh(world, ray));
                    let _ = self.select(world, picked.map(|hit| hit.entity));
                }
            }
            SceneEditorTool::Translate | SceneEditorTool::Rotate | SceneEditorTool::Scale => {
                if just_pressed {
                    if let Some(entity) = self.selected.filter(|entity| world.contains(*entity)) {
                        if let Some(transform) = world.get::<TransformComponent>(entity).cloned() {
                            self.drag = Some(GizmoDrag {
                                entity,
                                start_cursor: cursor,
                                start_transform: transform.transform,
                            });
                        }
                    }
                }

                if let Some(drag) = self.drag {
                    if !world.contains(drag.entity) {
                        self.drag = None;
                        return;
                    }
                    let delta = cursor - drag.start_cursor;
                    let edited = apply_gizmo_drag(
                        drag.start_transform,
                        self.active_tool,
                        self.active_axis,
                        delta,
                    );
                    if let Some(transform) = world.get_mut::<TransformComponent>(drag.entity) {
                        transform.set_transform(edited);
                    }
                }
            }
        }
    }

    fn draw_selected_gizmo(&self, world: &mut World) {
        let Some(entity) = self.selected.filter(|entity| world.contains(*entity)) else {
            return;
        };
        let matrix = entity_model_matrix(world, entity);
        let gizmos = world.resource_mut::<SceneGizmoLines>();
        gizmos.draw_axes(matrix, 1.5);
    }
}

fn drag_value(ui: &mut egui::Ui, value: &mut f32, label: &str) -> bool {
    ui.label(label);
    ui.add(egui::DragValue::new(value).speed(0.05)).changed()
}

fn tool_button(
    ui: &mut egui::Ui,
    active: &mut SceneEditorTool,
    tool: SceneEditorTool,
    label: &str,
) {
    if ui.selectable_label(*active == tool, label).clicked() {
        *active = tool;
    }
}

fn axis_button(ui: &mut egui::Ui, active: &mut GizmoAxis, axis: GizmoAxis, label: &str) {
    if ui.selectable_label(*active == axis, label).clicked() {
        *active = axis;
    }
}

pub fn viewport_pick_ray(
    camera: &oxide_math::prelude::Camera,
    cursor: Vec2,
    viewport_size: [f32; 2],
) -> Option<ScenePickRay> {
    if viewport_size[0] <= 0.0 || viewport_size[1] <= 0.0 {
        return None;
    }

    let ndc_x = (cursor.x / viewport_size[0]) * 2.0 - 1.0;
    let ndc_y = 1.0 - (cursor.y / viewport_size[1]) * 2.0;
    let view_proj = camera.view_projection_matrix(viewport_size[0] / viewport_size[1]);
    let inv = view_proj.inverse();
    let near = unproject(inv, Vec3::new(ndc_x, ndc_y, -1.0));
    let far = unproject(inv, Vec3::new(ndc_x, ndc_y, 1.0));
    let direction = (far - near).normalize_or_zero();

    (direction != Vec3::ZERO).then_some(ScenePickRay {
        origin: camera.position,
        direction,
    })
}

pub fn pick_render_mesh(world: &mut World, ray: ScenePickRay) -> Option<ScenePickHit> {
    let mut query = world.query::<(Entity, &RenderMesh)>();
    let entities: Vec<(Entity, MeshPrimitive)> = query
        .iter(world)
        .map(|(entity, mesh)| (entity, mesh.primitive))
        .collect();

    entities
        .into_iter()
        .filter_map(|(entity, primitive)| {
            let model = entity_model_matrix(world, entity);
            intersect_primitive(ray, model, primitive)
                .map(|distance| ScenePickHit { entity, distance })
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

pub fn apply_gizmo_drag(
    start: Transform,
    tool: SceneEditorTool,
    axis: GizmoAxis,
    cursor_delta: Vec2,
) -> Transform {
    let amount = (cursor_delta.x - cursor_delta.y) * 0.01;
    let mut edited = start;
    match tool {
        SceneEditorTool::Select => {}
        SceneEditorTool::Translate => {
            edited.position += axis_vector(axis) * amount;
        }
        SceneEditorTool::Rotate => {
            let rotation = match axis {
                GizmoAxis::X => glam::Quat::from_rotation_x(amount),
                GizmoAxis::Y => glam::Quat::from_rotation_y(amount),
                GizmoAxis::Z | GizmoAxis::Uniform => glam::Quat::from_rotation_z(amount),
            };
            edited.rotation = rotation * edited.rotation;
        }
        SceneEditorTool::Scale => {
            let factor = (1.0 + amount).max(0.05);
            match axis {
                GizmoAxis::X => edited.scale.x = (start.scale.x * factor).max(0.05),
                GizmoAxis::Y => edited.scale.y = (start.scale.y * factor).max(0.05),
                GizmoAxis::Z => edited.scale.z = (start.scale.z * factor).max(0.05),
                GizmoAxis::Uniform => {
                    edited.scale = (start.scale * factor).max(Vec3::splat(0.05));
                }
            }
        }
    }
    edited
}

fn active_camera(world: &mut World) -> Option<CameraComponent> {
    let mut query = world.query::<&CameraComponent>();
    query.iter(world).next().copied()
}

fn unproject(inv_view_proj: Mat4, ndc: Vec3) -> Vec3 {
    let point = inv_view_proj * Vec4::new(ndc.x, ndc.y, ndc.z, 1.0);
    point.truncate() / point.w
}

fn entity_model_matrix(world: &World, entity: Entity) -> Mat4 {
    if let Some(global) = world.get::<GlobalTransform>(entity) {
        global.matrix
    } else if let Some(local) = world.get::<TransformComponent>(entity) {
        local.to_matrix()
    } else {
        Mat4::IDENTITY
    }
}

fn intersect_primitive(ray: ScenePickRay, model: Mat4, primitive: MeshPrimitive) -> Option<f32> {
    let inv_model = model.inverse();
    let local_origin = inv_model.transform_point3(ray.origin);
    let local_direction = inv_model
        .transform_vector3(ray.direction)
        .normalize_or_zero();
    if local_direction == Vec3::ZERO {
        return None;
    }

    let local_t = match primitive {
        MeshPrimitive::Cube => intersect_unit_cube(local_origin, local_direction),
        MeshPrimitive::Sphere { .. } => intersect_unit_sphere(local_origin, local_direction),
    }?;
    let local_hit = local_origin + local_direction * local_t;
    let world_hit = model.transform_point3(local_hit);
    Some((world_hit - ray.origin).length())
}

fn intersect_unit_sphere(origin: Vec3, direction: Vec3) -> Option<f32> {
    let a = direction.length_squared();
    let b = 2.0 * origin.dot(direction);
    let c = origin.length_squared() - 0.25;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt = discriminant.sqrt();
    let near = (-b - sqrt) / (2.0 * a);
    let far = (-b + sqrt) / (2.0 * a);
    [near, far]
        .into_iter()
        .filter(|t| *t >= 0.0)
        .min_by(|a, b| a.total_cmp(b))
}

fn intersect_unit_cube(origin: Vec3, direction: Vec3) -> Option<f32> {
    let mut t_min = 0.0f32;
    let mut t_max = f32::INFINITY;

    for axis in 0..3 {
        let origin_axis = origin[axis];
        let direction_axis = direction[axis];
        if direction_axis.abs() < f32::EPSILON {
            if !(-0.5..=0.5).contains(&origin_axis) {
                return None;
            }
            continue;
        }

        let inv = 1.0 / direction_axis;
        let mut t0 = (-0.5 - origin_axis) * inv;
        let mut t1 = (0.5 - origin_axis) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_min = t_min.max(t0);
        t_max = t_max.min(t1);
        if t_max < t_min {
            return None;
        }
    }

    Some(t_min)
}

fn axis_vector(axis: GizmoAxis) -> Vec3 {
    match axis {
        GizmoAxis::X => Vec3::X,
        GizmoAxis::Y => Vec3::Y,
        GizmoAxis::Z => Vec3::Z,
        GizmoAxis::Uniform => Vec3::ONE.normalize(),
    }
}

fn delete_entity_recursive(world: &mut World, entity: Entity) -> bool {
    if !world.contains(entity) {
        return false;
    }

    let children = world
        .get::<Children>(entity)
        .map(|children| children.0.clone())
        .unwrap_or_default();
    for child in children {
        delete_entity_recursive(world, child);
    }

    if let Some(parent) = world.get::<Parent>(entity).copied() {
        if let Some(children) = world.get_mut::<Children>(parent.0) {
            children.remove(entity);
        }
    }

    world.despawn(entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_editor_can_spawn_select_and_delete_meshes() {
        let mut world = World::new();
        let mut editor = SceneEditor::default();

        let entity = editor.spawn_cube(&mut world);
        assert_eq!(editor.selected(), Some(entity));
        assert!(world.get::<RenderMesh>(entity).is_some());

        assert!(editor.set_selected_tint(&mut world, [1.0, 0.0, 0.0, 1.0]));
        assert_eq!(world.get::<RenderMesh>(entity).unwrap().tint[0], 1.0);

        assert!(editor.delete_selected(&mut world));
        assert!(!world.contains(entity));
        assert_eq!(editor.selected(), None);
    }

    #[test]
    fn scene_editor_duplicates_selected_mesh() {
        let mut world = World::new();
        let mut editor = SceneEditor::default();

        let source = editor.spawn_sphere(&mut world);
        let duplicated = editor
            .duplicate_selected(&mut world)
            .expect("selected mesh should duplicate");

        assert_ne!(source, duplicated);
        assert_eq!(editor.selected(), Some(duplicated));
        assert!(world.get::<RenderMesh>(duplicated).is_some());
    }

    #[test]
    fn picking_selects_nearest_render_mesh() {
        let mut world = World::new();
        let near = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 0.0, -2.0)),
                GlobalTransform::default(),
                RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            ))
            .id();
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, -5.0)),
            GlobalTransform::default(),
            RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
        ));
        oxide_scene::transform_propagate_system(&mut world);

        let hit = pick_render_mesh(
            &mut world,
            ScenePickRay {
                origin: Vec3::ZERO,
                direction: Vec3::NEG_Z,
            },
        )
        .expect("ray should hit a cube");
        assert_eq!(hit.entity, near);
    }

    #[test]
    fn gizmo_translate_rotate_and_scale_math_is_stable() {
        let start = Transform::default();
        let moved = apply_gizmo_drag(
            start,
            SceneEditorTool::Translate,
            GizmoAxis::X,
            Vec2::new(20.0, 0.0),
        );
        assert!(moved.position.x > start.position.x);

        let rotated = apply_gizmo_drag(
            start,
            SceneEditorTool::Rotate,
            GizmoAxis::Y,
            Vec2::new(20.0, 0.0),
        );
        assert_ne!(rotated.rotation, start.rotation);

        let scaled = apply_gizmo_drag(
            start,
            SceneEditorTool::Scale,
            GizmoAxis::Uniform,
            Vec2::new(20.0, 0.0),
        );
        assert!(scaled.scale.x > start.scale.x);

        let clamped = apply_gizmo_drag(
            start,
            SceneEditorTool::Scale,
            GizmoAxis::Uniform,
            Vec2::new(-1000.0, 0.0),
        );
        assert!(clamped.scale.min_element() >= 0.05);
    }
}
