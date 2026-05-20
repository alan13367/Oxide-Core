//! Lightweight runtime diagnostics for engine and gameplay tooling.

use std::collections::{HashMap, VecDeque};

use oxide_ecs::Resource;

use crate::app::{App, AppBuilder, AppStage, Plugin};
use crate::ecs::{Res, ResMut, Time, World};

/// Frame delta time in milliseconds.
pub const FRAME_TIME_MS: &str = "oxide.frame_time_ms";
/// Approximate frames per second from the latest frame delta.
pub const FPS: &str = "oxide.fps";
/// Frame delta time in seconds.
pub const DELTA_SECONDS: &str = "oxide.delta_seconds";

/// A rolling scalar diagnostic stream.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    label: String,
    samples: VecDeque<f64>,
    max_samples: usize,
    sum: f64,
}

impl Diagnostic {
    /// Creates a diagnostic stream with a bounded sample history.
    pub fn new(label: impl Into<String>, max_samples: usize) -> Self {
        Self {
            label: label.into(),
            samples: VecDeque::new(),
            max_samples: max_samples.max(1),
            sum: 0.0,
        }
    }

    /// Returns the stable diagnostic label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the configured rolling sample capacity.
    pub fn max_samples(&self) -> usize {
        self.max_samples
    }

    /// Records a new scalar sample.
    pub fn record(&mut self, value: f64) {
        self.samples.push_back(value);
        self.sum += value;

        while self.samples.len() > self.max_samples {
            if let Some(removed) = self.samples.pop_front() {
                self.sum -= removed;
            }
        }
    }

    /// Returns the newest sample.
    pub fn latest(&self) -> Option<f64> {
        self.samples.back().copied()
    }

    /// Returns the average across retained samples.
    pub fn average(&self) -> Option<f64> {
        if self.samples.is_empty() {
            None
        } else {
            Some(self.sum / self.samples.len() as f64)
        }
    }

    /// Returns retained samples from oldest to newest.
    pub fn samples(&self) -> impl Iterator<Item = f64> + '_ {
        self.samples.iter().copied()
    }

    /// Returns the number of retained samples.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns true when no samples have been recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Collection of named runtime diagnostic streams.
#[derive(Resource, Clone, Debug)]
pub struct Diagnostics {
    default_max_samples: usize,
    diagnostics: HashMap<String, Diagnostic>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new(120)
    }
}

impl Diagnostics {
    /// Creates a diagnostics collection with a default rolling sample capacity.
    pub fn new(default_max_samples: usize) -> Self {
        Self {
            default_max_samples: default_max_samples.max(1),
            diagnostics: HashMap::new(),
        }
    }

    /// Returns the default rolling sample capacity for newly-created streams.
    pub fn default_max_samples(&self) -> usize {
        self.default_max_samples
    }

    /// Registers a diagnostic stream if it does not already exist.
    pub fn register(&mut self, label: impl Into<String>) -> &mut Diagnostic {
        let label = label.into();
        self.diagnostics
            .entry(label.clone())
            .or_insert_with(|| Diagnostic::new(label, self.default_max_samples))
    }

    /// Records a sample, registering the stream on first use.
    pub fn record(&mut self, label: impl Into<String>, value: f64) {
        self.register(label).record(value);
    }

    /// Returns a diagnostic stream by label.
    pub fn get(&self, label: &str) -> Option<&Diagnostic> {
        self.diagnostics.get(label)
    }

    /// Returns the newest value for `label`.
    pub fn latest(&self, label: &str) -> Option<f64> {
        self.get(label).and_then(Diagnostic::latest)
    }

    /// Returns the rolling average for `label`.
    pub fn average(&self, label: &str) -> Option<f64> {
        self.get(label).and_then(Diagnostic::average)
    }

    /// Iterates all diagnostics sorted by label for stable tooling output.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        let mut diagnostics: Vec<_> = self.diagnostics.values().collect();
        diagnostics.sort_by(|a, b| a.label().cmp(b.label()));
        diagnostics.into_iter()
    }

    /// Returns the number of registered diagnostic streams.
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns true when no streams are registered.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Installs frame-time diagnostics.
pub struct FrameDiagnosticsPlugin;

impl<T: App> Plugin<T> for FrameDiagnosticsPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_diagnostics);
        app.add_system_mut(AppStage::PreUpdate, frame_diagnostics_system);
    }
}

/// Inserts the diagnostics resource when missing.
pub fn initialize_diagnostics(world: &mut World, _window: &crate::window::Window) {
    if !world.contains_resource::<Diagnostics>() {
        let mut diagnostics = Diagnostics::default();
        diagnostics.register(FRAME_TIME_MS);
        diagnostics.register(FPS);
        diagnostics.register(DELTA_SECONDS);
        world.insert_resource(diagnostics);
    }
}

/// Records built-in frame timing diagnostics from [`Time`].
pub fn frame_diagnostics_system(time: Res<Time>, mut diagnostics: ResMut<Diagnostics>) {
    let delta_secs = time.delta_secs() as f64;
    let frame_ms = delta_secs * 1000.0;
    let fps = if delta_secs > f64::EPSILON {
        1.0 / delta_secs
    } else {
        0.0
    };

    diagnostics.record(DELTA_SECONDS, delta_secs);
    diagnostics.record(FRAME_TIME_MS, frame_ms);
    diagnostics.record(FPS, fps);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn diagnostic_retains_bounded_samples_and_average() {
        let mut diagnostic = Diagnostic::new("test", 3);
        diagnostic.record(1.0);
        diagnostic.record(2.0);
        diagnostic.record(3.0);
        diagnostic.record(4.0);

        assert_eq!(diagnostic.latest(), Some(4.0));
        assert_eq!(diagnostic.average(), Some(3.0));
        assert_eq!(
            diagnostic.samples().collect::<Vec<_>>(),
            vec![2.0, 3.0, 4.0]
        );
    }

    #[test]
    fn diagnostics_registers_streams_on_record() {
        let mut diagnostics = Diagnostics::new(2);
        diagnostics.record("a", 4.0);
        diagnostics.record("a", 6.0);
        diagnostics.record("a", 8.0);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.latest("a"), Some(8.0));
        assert_eq!(diagnostics.average("a"), Some(7.0));
    }

    #[test]
    fn frame_diagnostics_system_records_time_metrics() {
        let mut world = World::new();
        world.insert_resource(Diagnostics::default());
        let mut time = Time::default();
        time.set_delta_for_tests(Duration::from_millis(20));
        world.insert_resource(time);

        let mut schedule = oxide_ecs::prelude::Schedule::new();
        schedule.add_system(frame_diagnostics_system);
        schedule.run(&mut world);

        let diagnostics = world.resource::<Diagnostics>();
        assert_close(diagnostics.latest(FRAME_TIME_MS).unwrap(), 20.0);
        assert_close(diagnostics.latest(DELTA_SECONDS).unwrap(), 0.02);
        assert_close(diagnostics.latest(FPS).unwrap(), 50.0);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }
}
