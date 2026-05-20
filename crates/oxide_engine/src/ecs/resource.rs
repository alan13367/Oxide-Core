//! Engine resources

use std::time::{Duration, Instant};

use oxide_ecs::Resource;
use oxide_renderer::Renderer;

/// App lifecycle control resource used by systems to request a clean shutdown.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct AppExit {
    requested: bool,
    code: i32,
}

impl AppExit {
    /// Requests a successful app shutdown.
    pub fn request(&mut self) {
        self.request_with_code(0);
    }

    /// Requests app shutdown with an application-defined exit code.
    pub fn request_with_code(&mut self, code: i32) {
        self.requested = true;
        self.code = code;
    }

    /// Clears a pending shutdown request.
    pub fn clear(&mut self) {
        self.requested = false;
        self.code = 0;
    }

    /// Returns true when a system has requested app shutdown.
    pub fn is_requested(&self) -> bool {
        self.requested
    }

    /// Returns the application-defined exit code.
    pub fn code(&self) -> i32 {
        self.code
    }
}

#[derive(Resource)]
pub struct Time {
    pub delta: Duration,
    pub elapsed: Duration,
    last_frame: Instant,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            last_frame: Instant::now(),
        }
    }
}

impl Time {
    pub fn update(&mut self) {
        let now = Instant::now();
        self.delta = now - self.last_frame;
        self.elapsed += self.delta;
        self.last_frame = now;
    }

    pub fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }

    #[cfg(test)]
    pub(crate) fn set_delta_for_tests(&mut self, delta: Duration) {
        self.delta = delta;
        self.elapsed = delta;
    }
}

/// Fixed-step frame accumulator for deterministic gameplay schedules.
///
/// `FixedTime` is advanced once per rendered frame by the app runner. Systems
/// registered in `AppStage::FixedUpdate` run once for each due fixed tick, up
/// to `max_steps_per_frame`.
#[derive(Resource, Clone, Debug)]
pub struct FixedTime {
    timestep: Duration,
    accumulator: Duration,
    max_steps_per_frame: u32,
}

impl Default for FixedTime {
    fn default() -> Self {
        Self {
            timestep: Duration::from_secs_f64(1.0 / 60.0),
            accumulator: Duration::ZERO,
            max_steps_per_frame: 5,
        }
    }
}

impl FixedTime {
    /// Creates a fixed-step clock with a custom timestep.
    pub fn new(timestep: Duration) -> Self {
        Self {
            timestep,
            ..Self::default()
        }
    }

    /// Returns the duration of one fixed tick.
    pub fn timestep(&self) -> Duration {
        self.timestep
    }

    /// Returns the fixed tick duration in seconds.
    pub fn timestep_secs(&self) -> f32 {
        self.timestep.as_secs_f32()
    }

    /// Sets the fixed tick duration. Zero is ignored to avoid infinite loops.
    pub fn set_timestep(&mut self, timestep: Duration) {
        if !timestep.is_zero() {
            self.timestep = timestep;
        }
    }

    /// Returns the currently accumulated, not-yet-simulated frame time.
    pub fn accumulator(&self) -> Duration {
        self.accumulator
    }

    /// Returns the maximum fixed ticks the runner may execute in one frame.
    pub fn max_steps_per_frame(&self) -> u32 {
        self.max_steps_per_frame
    }

    /// Sets the per-frame fixed tick cap. Values below one are clamped to one.
    pub fn set_max_steps_per_frame(&mut self, max_steps: u32) {
        self.max_steps_per_frame = max_steps.max(1);
    }

    /// Advances the accumulator by a rendered-frame delta and returns due ticks.
    pub fn advance(&mut self, delta: Duration) -> u32 {
        if self.timestep.is_zero() {
            return 0;
        }

        self.accumulator += delta;
        let mut steps = 0;
        while self.accumulator >= self.timestep && steps < self.max_steps_per_frame {
            self.accumulator -= self.timestep;
            steps += 1;
        }

        if steps == self.max_steps_per_frame && self.accumulator >= self.timestep {
            self.accumulator = Duration::ZERO;
        }

        steps
    }

    /// Clears accumulated time without changing configuration.
    pub fn reset_accumulator(&mut self) {
        self.accumulator = Duration::ZERO;
    }
}

#[derive(Resource)]
pub struct RendererResource {
    pub renderer: Renderer,
}

impl RendererResource {
    pub fn new(renderer: Renderer) -> Self {
        Self { renderer }
    }
}

#[derive(Resource)]
pub struct WindowResource {
    pub width: u32,
    pub height: u32,
}

impl WindowResource {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn aspect_ratio(&self) -> f32 {
        if self.height > 0 {
            self.width as f32 / self.height as f32
        } else {
            1.0
        }
    }

    pub fn update(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_exit_tracks_requested_code_and_clear() {
        let mut exit = AppExit::default();
        assert!(!exit.is_requested());
        assert_eq!(exit.code(), 0);

        exit.request();
        assert!(exit.is_requested());
        assert_eq!(exit.code(), 0);

        exit.request_with_code(7);
        assert!(exit.is_requested());
        assert_eq!(exit.code(), 7);

        exit.clear();
        assert!(!exit.is_requested());
        assert_eq!(exit.code(), 0);
    }

    #[test]
    fn fixed_time_accumulates_until_step_is_due() {
        let mut fixed = FixedTime::new(Duration::from_millis(10));

        assert_eq!(fixed.advance(Duration::from_millis(4)), 0);
        assert_eq!(fixed.advance(Duration::from_millis(6)), 1);
        assert_eq!(fixed.accumulator(), Duration::ZERO);
    }

    #[test]
    fn fixed_time_caps_steps_and_drops_excess_backlog() {
        let mut fixed = FixedTime::new(Duration::from_millis(10));
        fixed.set_max_steps_per_frame(3);

        assert_eq!(fixed.advance(Duration::from_millis(100)), 3);
        assert_eq!(fixed.accumulator(), Duration::ZERO);
    }
}
