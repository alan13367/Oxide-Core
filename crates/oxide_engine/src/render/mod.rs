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
    enabled: bool,
    insertion_index: usize,
    callback: RenderPassEntryKind,
}

impl RenderPassEntry {
    fn kind(&self) -> RenderPassKind {
        match self.callback {
            RenderPassEntryKind::Anchor(_) => RenderPassKind::Anchor,
            RenderPassEntryKind::Pass(_) => RenderPassKind::Pass,
        }
    }
}

enum RenderPassEntryKind {
    Anchor(RenderPassAnchor),
    Pass(RenderPassFn),
}

/// Public kind metadata for a registered render pass schedule entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderPassKind {
    /// Built-in runner anchor such as scene rendering, game text, app queue, or egui.
    Anchor,
    /// Plugin or app-provided render callback.
    Pass,
}

/// Snapshot metadata for one render pass schedule entry.
///
/// Use this for tooling, diagnostics, editor UIs, or tests that need to inspect
/// the frame pipeline without reaching into runner internals. The snapshot is
/// owned so it can be logged or stored independently of the schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderPassInfo {
    /// Stable label used for ordering and diagnostics.
    pub label: String,
    /// Whether this entry is a built-in anchor or a custom callback.
    pub kind: RenderPassKind,
    /// Named ordering sets this entry belongs to.
    pub sets: Vec<String>,
    /// Labels or sets this entry should run before.
    pub before: Vec<String>,
    /// Labels or sets this entry should run after.
    pub after: Vec<String>,
    /// Whether this entry currently participates in execution.
    pub enabled: bool,
    /// Current resolved execution index when enabled.
    ///
    /// Disabled entries are retained for inspection but return `None` because
    /// they are skipped by the runner.
    pub order_index: Option<usize>,
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

    /// Returns metadata for every registered entry in insertion order.
    ///
    /// Disabled custom passes remain visible here so editor/debug tooling can
    /// present the complete pipeline even when parts of it are temporarily
    /// skipped.
    pub fn pass_infos(&self) -> Vec<RenderPassInfo> {
        let mut order_lookup = HashMap::new();
        for (order_index, entry_index) in self.run_order().into_iter().enumerate() {
            order_lookup.insert(entry_index, order_index);
        }

        self.entries
            .iter()
            .enumerate()
            .map(|(entry_index, entry)| RenderPassInfo {
                label: entry.label.clone(),
                kind: entry.kind(),
                sets: entry.sets.clone(),
                before: entry.before.clone(),
                after: entry.after.clone(),
                enabled: entry.enabled,
                order_index: order_lookup.get(&entry_index).copied(),
            })
            .collect()
    }

    /// Returns metadata for enabled entries in resolved execution order.
    pub fn ordered_pass_infos(&self) -> Vec<RenderPassInfo> {
        let infos = self.pass_infos();
        let mut ordered: Vec<_> = infos
            .into_iter()
            .filter(|info| info.order_index.is_some())
            .collect();
        ordered.sort_by_key(|info| info.order_index);
        ordered
    }

    /// Sets enabled state for every custom pass matching `label`.
    ///
    /// Built-in anchors are intentionally not affected. Returns the number of
    /// custom passes whose state was updated.
    pub fn set_pass_enabled(&mut self, label: impl AsRef<str>, enabled: bool) -> usize {
        let label = label.as_ref();
        let mut updated = 0;
        for entry in &mut self.entries {
            if entry.label == label && matches!(entry.callback, RenderPassEntryKind::Pass(_)) {
                entry.enabled = enabled;
                updated += 1;
            }
        }
        updated
    }

    /// Sets enabled state for every custom pass in `set`.
    ///
    /// This is useful for plugin-owned pass groups such as capture,
    /// post-processing, or debug drawing stacks. Built-in anchors are not
    /// affected. Returns the number of custom passes whose state was updated.
    pub fn set_pass_set_enabled(&mut self, set: impl AsRef<str>, enabled: bool) -> usize {
        let set = set.as_ref();
        let mut updated = 0;
        for entry in &mut self.entries {
            if matches!(entry.callback, RenderPassEntryKind::Pass(_))
                && entry.sets.iter().any(|entry_set| entry_set == set)
            {
                entry.enabled = enabled;
                updated += 1;
            }
        }
        updated
    }

    /// Enables every custom pass matching `label`.
    pub fn enable_pass(&mut self, label: impl AsRef<str>) -> usize {
        self.set_pass_enabled(label, true)
    }

    /// Disables every custom pass matching `label` without unregistering it.
    pub fn disable_pass(&mut self, label: impl AsRef<str>) -> usize {
        self.set_pass_enabled(label, false)
    }

    /// Enables every custom pass in `set`.
    pub fn enable_set(&mut self, set: impl AsRef<str>) -> usize {
        self.set_pass_set_enabled(set, true)
    }

    /// Disables every custom pass in `set` without unregistering it.
    pub fn disable_set(&mut self, set: impl AsRef<str>) -> usize {
        self.set_pass_set_enabled(set, false)
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
            enabled: true,
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
            enabled: true,
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
            enabled: true,
            insertion_index: self.next_insertion_index,
            callback: RenderPassEntryKind::Pass(pass),
        });
        self.next_insertion_index += 1;
    }

    fn is_entry_enabled(&self, index: usize) -> bool {
        self.entries
            .get(index)
            .map(|entry| entry.enabled)
            .unwrap_or(false)
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
            if !entry.enabled {
                continue;
            }
            for before in &entry.before {
                if let Some(targets) = lookup.get(before.as_str()) {
                    for &target in targets {
                        if index != target
                            && self.is_entry_enabled(target)
                            && seen.insert((index, target))
                        {
                            edges.push((index, target));
                        }
                    }
                }
            }
            for after in &entry.after {
                if let Some(sources) = lookup.get(after.as_str()) {
                    for &source in sources {
                        if source != index
                            && self.is_entry_enabled(source)
                            && seen.insert((source, index))
                        {
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
                        if !self.is_entry_enabled(member) {
                            continue;
                        }
                        for &target in targets {
                            if member != target
                                && self.is_entry_enabled(target)
                                && seen.insert((member, target))
                            {
                                edges.push((member, target));
                            }
                        }
                    }
                }
            }
            for after in &constraint.after {
                if let Some(sources) = lookup.get(after.as_str()) {
                    for &source in sources {
                        if !self.is_entry_enabled(source) {
                            continue;
                        }
                        for &member in members {
                            if source != member
                                && self.is_entry_enabled(member)
                                && seen.insert((source, member))
                            {
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
        self.topological_order(false).len() != self.enabled_entry_count()
    }

    fn run_order(&self) -> Vec<usize> {
        let mut order = self.topological_order(true);
        let enabled_entry_count = self.enabled_entry_count();
        if order.len() < enabled_entry_count {
            let emitted: HashSet<_> = order.iter().copied().collect();
            let mut remaining: Vec<_> = (0..self.entries.len())
                .filter(|index| self.entries[*index].enabled && !emitted.contains(index))
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
                .filter(|index| {
                    self.entries[*index].enabled && !emitted[*index] && incoming[*index] == 0
                })
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

    fn enabled_entry_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.enabled).count()
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

    #[test]
    fn render_pass_schedule_can_disable_custom_passes_without_removing_metadata() {
        let mut schedule = RenderPassSchedule::new();
        schedule.add_pass("custom.overlay", noop_pass);

        assert_eq!(schedule.disable_pass("custom.overlay"), 1);
        assert_eq!(
            schedule.ordered_labels(),
            vec![
                RENDER_PASS_SCENE,
                RENDER_PASS_GAME_TEXT,
                RENDER_PASS_APP_QUEUE,
                RENDER_PASS_EGUI,
            ]
        );

        let info = schedule
            .pass_infos()
            .into_iter()
            .find(|info| info.label == "custom.overlay")
            .expect("custom pass info should remain available");
        assert_eq!(info.kind, RenderPassKind::Pass);
        assert!(!info.enabled);
        assert_eq!(info.order_index, None);

        assert_eq!(schedule.enable_pass("custom.overlay"), 1);
        assert!(schedule.ordered_labels().contains(&"custom.overlay"));
    }

    #[test]
    fn render_pass_schedule_can_disable_sets_without_removing_metadata() {
        let mut schedule = RenderPassSchedule::new();
        schedule.add_pass_to_set("capture.depth", "capture", noop_pass);
        schedule.add_pass_to_set("capture.color", "capture", noop_pass);
        schedule.add_pass_to_set("debug.lines", "debug", noop_pass);
        schedule.configure_set_before("capture", RENDER_PASS_EGUI);

        assert_eq!(schedule.disable_set("capture"), 2);
        assert_eq!(
            schedule.ordered_labels(),
            vec![
                RENDER_PASS_SCENE,
                RENDER_PASS_GAME_TEXT,
                "debug.lines",
                RENDER_PASS_APP_QUEUE,
                RENDER_PASS_EGUI,
            ]
        );

        let capture_infos: Vec<_> = schedule
            .pass_infos()
            .into_iter()
            .filter(|info| info.sets.iter().any(|set| set == "capture"))
            .collect();
        assert_eq!(capture_infos.len(), 2);
        assert!(capture_infos
            .iter()
            .all(|info| !info.enabled && info.order_index.is_none()));

        assert_eq!(schedule.enable_set("capture"), 2);
        let labels = schedule.ordered_labels();
        assert!(labels.contains(&"capture.depth"));
        assert!(labels.contains(&"capture.color"));
    }

    #[test]
    fn render_pass_schedule_reports_ordered_pass_infos() {
        let mut schedule = RenderPassSchedule::new();
        schedule.add_pass_to_set("capture.depth", "capture", noop_pass);
        schedule.configure_set_before("capture", RENDER_PASS_EGUI);

        let ordered_infos = schedule.ordered_pass_infos();
        let labels: Vec<_> = ordered_infos
            .iter()
            .map(|info| info.label.as_str())
            .collect();

        assert_eq!(
            labels,
            vec![
                RENDER_PASS_SCENE,
                RENDER_PASS_GAME_TEXT,
                "capture.depth",
                RENDER_PASS_APP_QUEUE,
                RENDER_PASS_EGUI,
            ]
        );
        assert_eq!(
            ordered_infos
                .iter()
                .find(|info| info.label == "capture.depth")
                .map(|info| info.sets.as_slice()),
            Some(&["capture".to_string()][..])
        );
    }
}
