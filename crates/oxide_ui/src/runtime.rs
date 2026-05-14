//! Runtime UI data model for HUDs, menus, and debug panels.

use std::collections::HashSet;

use oxide_ecs::world::World;
use oxide_ecs::Resource;

#[derive(Clone, Debug)]
pub enum UiElement {
    Label { id: String, text: String },
    Button { id: String, text: String },
    Separator,
}

#[derive(Resource, Default)]
pub struct RuntimeUi {
    elements: Vec<UiElement>,
    clicked: HashSet<String>,
}

impl RuntimeUi {
    pub fn clear(&mut self) {
        self.elements.clear();
        self.clicked.clear();
    }

    pub fn label(&mut self, id: impl Into<String>, text: impl Into<String>) {
        self.elements.push(UiElement::Label {
            id: id.into(),
            text: text.into(),
        });
    }

    pub fn button(&mut self, id: impl Into<String>, text: impl Into<String>) {
        self.elements.push(UiElement::Button {
            id: id.into(),
            text: text.into(),
        });
    }

    pub fn separator(&mut self) {
        self.elements.push(UiElement::Separator);
    }

    pub fn clicked(&self, id: &str) -> bool {
        self.clicked.contains(id)
    }

    pub fn elements(&self) -> &[UiElement] {
        &self.elements
    }

    pub fn show_egui(&mut self, ctx: &egui::Context) {
        self.clicked.clear();
        egui::Window::new("Runtime UI").show(ctx, |ui| {
            for element in &self.elements {
                match element {
                    UiElement::Label { text, .. } => {
                        ui.label(text);
                    }
                    UiElement::Button { id, text } => {
                        if ui.button(text).clicked() {
                            self.clicked.insert(id.clone());
                        }
                    }
                    UiElement::Separator => {
                        ui.separator();
                    }
                }
            }
        });
    }
}

pub fn initialize_runtime_ui(world: &mut World) {
    if !world.contains_resource::<RuntimeUi>() {
        world.insert_resource(RuntimeUi::default());
    }
}
