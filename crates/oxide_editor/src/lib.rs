//! Runtime scene editor model and egui surface.

use glam::Vec3;
use oxide_camera::CameraComponent;
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::Resource;
use oxide_light::{AmbientLight, DirectionalLight, PointLight};
use oxide_math::transform::Transform;
use oxide_scene::{
    Children, GlobalTransform, MeshPrimitive, Name, Parent, RenderMaterial, RenderMesh,
    TransformComponent,
};
use oxide_ui::{DevOverlay, DevOverlaySnapshot, RuntimeUi};

#[derive(Resource, Clone, Debug)]
pub struct SceneEditor {
    pub visible: bool,
    selected: Option<Entity>,
    next_spawn_index: u32,
}

impl Default for SceneEditor {
    fn default() -> Self {
        Self {
            visible: true,
            selected: None,
            next_spawn_index: 1,
        }
    }
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

impl SceneEditor {
    pub fn selected(&self) -> Option<Entity> {
        self.selected
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
}

fn drag_value(ui: &mut egui::Ui, value: &mut f32, label: &str) -> bool {
    ui.label(label);
    ui.add(egui::DragValue::new(value).speed(0.05)).changed()
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
}
