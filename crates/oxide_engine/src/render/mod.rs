//! Engine-managed render frame utilities.

use std::collections::{HashMap, HashSet};

use crate::ecs::World;

/// Stable anchor for the built-in scene render pass.
pub const RENDER_PASS_SCENE: &str = "oxide.render.scene";
/// Stable anchor for the built-in game text render pass.
pub const RENDER_PASS_GAME_TEXT: &str = "oxide.render.game_text";
/// Stable anchor for [`App::queue`](crate::app::App::queue).
pub const RENDER_PASS_APP_QUEUE: &str = "oxide.render.app_queue";
/// Stable anchor for the built-in egui render pass.
pub const RENDER_PASS_EGUI: &str = "oxide.render.egui";

/// A render callback registered with [`RenderPassSchedule`].
///
/// Render passes run after the app `Prepare` stage and receive mutable access
/// to the world plus the active frame encoder/view. Long-lived GPU resources
/// should be stored as non-send resources and updated during `Prepare`.
pub type RenderPassFn = fn(&mut World, &mut RenderFrame);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderPassAnchor {
    Scene,
    GameText,
    AppQueue,
    Egui,
}

struct RenderPassEntry {
    label: String,
    sets: Vec<String>,
    before: Vec<String>,
    after: Vec<String>,
    insertion_index: usize,
    callback: RenderPassEntryKind,
}

enum RenderPassEntryKind {
    Anchor(RenderPassAnchor),
    Pass(RenderPassFn),
}

#[derive(Clone, Debug)]
struct RenderPassSetConstraint {
    set: String,
    before: Vec<String>,
    after: Vec<String>,
}

/// A non-fatal ordering issue found in a [`RenderPassSchedule`].
///
/// Diagnostics do not prevent rendering. Missing labels are ignored and cycles
/// fall back to insertion order for the cyclic subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderPassOrderDiagnostic {
    pub message: String,
}

impl RenderPassOrderDiagnostic {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderPassStep {
    Anchor(RenderPassAnchor),
    Pass(usize),
}

/// Ordered render pass registry for engine and plugin frame callbacks.
///
/// This is intentionally lighter than a full render graph: passes are retained,
/// named, and ordered by labels or sets, but resource lifetime stays explicit in
/// Oxide-owned resources. The default schedule contains anchors for the built-in
/// scene, game text, app queue, and egui passes so plugins can target stable
/// insertion points.
pub struct RenderPassSchedule {
    entries: Vec<RenderPassEntry>,
    set_constraints: Vec<RenderPassSetConstraint>,
    next_insertion_index: usize,
}

impl Default for RenderPassSchedule {
    fn default() -> Self {
        let mut schedule = Self {
            entries: Vec::new(),
            set_constraints: Vec::new(),
            next_insertion_index: 0,
        };
        schedule.push_anchor(RENDER_PASS_SCENE, RenderPassAnchor::Scene);
        schedule.push_anchor_after(
            RENDER_PASS_GAME_TEXT,
            RenderPassAnchor::GameText,
            RENDER_PASS_SCENE,
        );
        schedule.push_anchor_after(
            RENDER_PASS_APP_QUEUE,
            RenderPassAnchor::AppQueue,
            RENDER_PASS_GAME_TEXT,
        );
        schedule.push_anchor_after(
            RENDER_PASS_EGUI,
            RenderPassAnchor::Egui,
            RENDER_PASS_APP_QUEUE,
        );
        schedule
    }
}

impl RenderPassSchedule {
    /// Creates a render pass schedule with the default Oxide render anchors.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a render pass to the default custom-pass position.
    ///
    /// Without explicit ordering, custom passes run after built-in game text
    /// and before `App::queue`.
    pub fn add_pass(&mut self, label: impl Into<String>, pass: RenderPassFn) -> &mut Self {
        self.push_pass(
            label.into(),
            Vec::new(),
            vec![RENDER_PASS_APP_QUEUE.to_string()],
            vec![RENDER_PASS_GAME_TEXT.to_string()],
            pass,
        );
        self
    }

    /// Adds a render pass to a named ordering set.
    pub fn add_pass_to_set(
        &mut self,
        label: impl Into<String>,
        set: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.push_pass(
            label.into(),
            vec![set.into()],
            vec![RENDER_PASS_APP_QUEUE.to_string()],
            vec![RENDER_PASS_GAME_TEXT.to_string()],
            pass,
        );
        self
    }

    /// Adds a render pass that should run before `before_label`.
    pub fn add_pass_before(
        &mut self,
        label: impl Into<String>,
        before_label: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.push_pass(
            label.into(),
            Vec::new(),
            vec![before_label.into()],
            Vec::new(),
            pass,
        );
        self
    }

    /// Adds a render pass that should run after `after_label`.
    pub fn add_pass_after(
        &mut self,
        label: impl Into<String>,
        after_label: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.push_pass(
            label.into(),
            Vec::new(),
            Vec::new(),
            vec![after_label.into()],
            pass,
        );
        self
    }

    /// Orders every pass in `set` before passes matched by `before_label`.
    pub fn configure_set_before(
        &mut self,
        set: impl Into<String>,
        before_label: impl Into<String>,
    ) -> &mut Self {
        self.set_constraints.push(RenderPassSetConstraint {
            set: set.into(),
            before: vec![before_label.into()],
            after: Vec::new(),
        });
        self
    }

    /// Orders every pass in `set` after passes matched by `after_label`.
    pub fn configure_set_after(
        &mut self,
        set: impl Into<String>,
        after_label: impl Into<String>,
    ) -> &mut Self {
        self.set_constraints.push(RenderPassSetConstraint {
            set: set.into(),
            before: Vec::new(),
            after: vec![after_label.into()],
        });
        self
    }

    /// Returns the number of entries, including built-in anchors.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true when no entries are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns non-fatal ordering diagnostics for this schedule.
    pub fn ordering_diagnostics(&self) -> Vec<RenderPassOrderDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut labels = HashSet::new();
        for entry in &self.entries {
            if !labels.insert(entry.label.as_str()) {
                diagnostics.push(RenderPassOrderDiagnostic::new(format!(
                    "duplicate render pass label '{}'",
                    entry.label
                )));
            }
        }

        let targets = self.target_lookup();
        for entry in &self.entries {
            for before in &entry.before {
                if !targets.contains_key(before.as_str()) {
                    diagnostics.push(RenderPassOrderDiagnostic::new(format!(
                        "render pass ordering references missing before-label '{before}'"
                    )));
                }
            }
            for after in &entry.after {
                if !targets.contains_key(after.as_str()) {
                    diagnostics.push(RenderPassOrderDiagnostic::new(format!(
                        "render pass ordering references missing after-label '{after}'"
                    )));
                }
            }
        }
        for constraint in &self.set_constraints {
            if !targets.contains_key(constraint.set.as_str()) {
                diagnostics.push(RenderPassOrderDiagnostic::new(format!(
                    "render pass set ordering references missing set '{}'",
                    constraint.set
                )));
            }
            for before in &constraint.before {
                if !targets.contains_key(before.as_str()) {
                    diagnostics.push(RenderPassOrderDiagnostic::new(format!(
                        "render pass set ordering references missing before-label '{before}'"
                    )));
                }
            }
            for after in &constraint.after {
                if !targets.contains_key(after.as_str()) {
                    diagnostics.push(RenderPassOrderDiagnostic::new(format!(
                        "render pass set ordering references missing after-label '{after}'"
                    )));
                }
            }
        }

        if self.has_ordering_cycle() {
            diagnostics.push(RenderPassOrderDiagnostic::new(
                "render pass ordering contains a cycle",
            ));
        }

        diagnostics
    }

    /// Returns labels in the order they will execute.
    pub fn ordered_labels(&self) -> Vec<&str> {
        self.run_order()
            .into_iter()
            .map(|index| self.entries[index].label.as_str())
            .collect()
    }

    pub(crate) fn ordered_steps(&self) -> Vec<RenderPassStep> {
        self.run_order()
            .into_iter()
            .map(|index| match self.entries[index].callback {
                RenderPassEntryKind::Anchor(anchor) => RenderPassStep::Anchor(anchor),
                RenderPassEntryKind::Pass(_) => RenderPassStep::Pass(index),
            })
            .collect()
    }

    pub(crate) fn run_pass_at(&self, index: usize, world: &mut World, frame: &mut RenderFrame) {
        if let RenderPassEntryKind::Pass(pass) = self.entries[index].callback {
            pass(world, frame);
        }
    }

    fn push_anchor(&mut self, label: impl Into<String>, anchor: RenderPassAnchor) {
        self.entries.push(RenderPassEntry {
            label: label.into(),
            sets: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            insertion_index: self.next_insertion_index,
            callback: RenderPassEntryKind::Anchor(anchor),
        });
        self.next_insertion_index += 1;
    }

    fn push_anchor_after(
        &mut self,
        label: impl Into<String>,
        anchor: RenderPassAnchor,
        after_label: impl Into<String>,
    ) {
        self.entries.push(RenderPassEntry {
            label: label.into(),
            sets: Vec::new(),
            before: Vec::new(),
            after: vec![after_label.into()],
            insertion_index: self.next_insertion_index,
            callback: RenderPassEntryKind::Anchor(anchor),
        });
        self.next_insertion_index += 1;
    }

    fn push_pass(
        &mut self,
        label: String,
        sets: Vec<String>,
        before: Vec<String>,
        after: Vec<String>,
        pass: RenderPassFn,
    ) {
        self.entries.push(RenderPassEntry {
            label,
            sets,
            before,
            after,
            insertion_index: self.next_insertion_index,
            callback: RenderPassEntryKind::Pass(pass),
        });
        self.next_insertion_index += 1;
    }

    fn target_lookup(&self) -> HashMap<&str, Vec<usize>> {
        let mut lookup: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            lookup.entry(entry.label.as_str()).or_default().push(index);
            for set in &entry.sets {
                lookup.entry(set.as_str()).or_default().push(index);
            }
        }
        lookup
    }

    fn ordering_edges(&self) -> Vec<(usize, usize)> {
        let lookup = self.target_lookup();
        let mut edges = Vec::new();
        let mut seen = HashSet::new();

        for (index, entry) in self.entries.iter().enumerate() {
            for before in &entry.before {
                if let Some(targets) = lookup.get(before.as_str()) {
                    for &target in targets {
                        if index != target && seen.insert((index, target)) {
                            edges.push((index, target));
                        }
                    }
                }
            }
            for after in &entry.after {
                if let Some(sources) = lookup.get(after.as_str()) {
                    for &source in sources {
                        if source != index && seen.insert((source, index)) {
                            edges.push((source, index));
                        }
                    }
                }
            }
        }

        for constraint in &self.set_constraints {
            let Some(members) = lookup.get(constraint.set.as_str()) else {
                continue;
            };
            for before in &constraint.before {
                if let Some(targets) = lookup.get(before.as_str()) {
                    for &member in members {
                        for &target in targets {
                            if member != target && seen.insert((member, target)) {
                                edges.push((member, target));
                            }
                        }
                    }
                }
            }
            for after in &constraint.after {
                if let Some(sources) = lookup.get(after.as_str()) {
                    for &source in sources {
                        for &member in members {
                            if source != member && seen.insert((source, member)) {
                                edges.push((source, member));
                            }
                        }
                    }
                }
            }
        }

        edges
    }

    fn has_ordering_cycle(&self) -> bool {
        self.topological_order(false).len() != self.entries.len()
    }

    fn run_order(&self) -> Vec<usize> {
        let mut order = self.topological_order(true);
        if order.len() < self.entries.len() {
            let emitted: HashSet<_> = order.iter().copied().collect();
            let mut remaining: Vec<_> = (0..self.entries.len())
                .filter(|index| !emitted.contains(index))
                .collect();
            remaining.sort_by_key(|index| self.entries[*index].insertion_index);
            order.extend(remaining);
        }
        order
    }

    fn topological_order(&self, _allow_partial: bool) -> Vec<usize> {
        let edges = self.ordering_edges();
        let mut incoming = vec![0usize; self.entries.len()];
        let mut outgoing = vec![Vec::new(); self.entries.len()];
        for (source, target) in edges {
            incoming[target] += 1;
            outgoing[source].push(target);
        }

        let mut emitted = vec![false; self.entries.len()];
        let mut order = Vec::new();

        loop {
            let next = (0..self.entries.len())
                .filter(|index| !emitted[*index] && incoming[*index] == 0)
                .min_by_key(|index| self.entries[*index].insertion_index);

            let Some(index) = next else {
                break;
            };

            emitted[index] = true;
            order.push(index);
            for target in &outgoing[index] {
                incoming[*target] -= 1;
            }
        }

        order
    }
}

pub struct RenderFrame {
    pub view: wgpu::TextureView,
    pub encoder: wgpu::CommandEncoder,
    surface_texture: wgpu::SurfaceTexture,
}

impl RenderFrame {
    pub fn new(device: &wgpu::Device, surface_texture: wgpu::SurfaceTexture) -> Self {
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        Self {
            view,
            encoder,
            surface_texture,
        }
    }

    pub fn present(self, queue: &wgpu::Queue) {
        queue.submit(std::iter::once(self.encoder.finish()));
        self.surface_texture.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_pass(_world: &mut World, _frame: &mut RenderFrame) {}

    #[test]
    fn render_pass_schedule_orders_custom_passes_between_builtin_anchors_by_default() {
        let mut schedule = RenderPassSchedule::new();
        schedule.add_pass("custom.overlay", noop_pass);

        assert_eq!(
            schedule.ordered_labels(),
            vec![
                RENDER_PASS_SCENE,
                RENDER_PASS_GAME_TEXT,
                "custom.overlay",
                RENDER_PASS_APP_QUEUE,
                RENDER_PASS_EGUI,
            ]
        );
    }

    #[test]
    fn render_pass_schedule_can_target_builtin_anchors() {
        let mut schedule = RenderPassSchedule::new();
        schedule.add_pass_before("custom.background", RENDER_PASS_SCENE, noop_pass);
        schedule.add_pass_before("custom.final_overlay", RENDER_PASS_EGUI, noop_pass);

        assert_eq!(
            schedule.ordered_labels(),
            vec![
                "custom.background",
                RENDER_PASS_SCENE,
                RENDER_PASS_GAME_TEXT,
                RENDER_PASS_APP_QUEUE,
                "custom.final_overlay",
                RENDER_PASS_EGUI,
            ]
        );
    }

    #[test]
    fn render_pass_schedule_reports_ordering_diagnostics() {
        let mut schedule = RenderPassSchedule::new();
        schedule.add_pass_after("a", "b", noop_pass);
        schedule.add_pass_after("b", "a", noop_pass);
        schedule.add_pass_before("missing_target", "nope", noop_pass);

        let diagnostics = schedule.ordering_diagnostics();
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("contains a cycle")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing before-label")));
    }
}
