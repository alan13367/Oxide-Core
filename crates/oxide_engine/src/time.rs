//! Time utilities

pub use std::time::{Duration, Instant};

/// Controls what happens when a timer reaches its duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerMode {
    Once,
    Repeating,
}

/// Small gameplay timer that can be stored in components/resources.
#[derive(Clone, Debug)]
pub struct Timer {
    duration: Duration,
    elapsed: Duration,
    mode: TimerMode,
    finished: bool,
    just_finished: bool,
}

impl Timer {
    pub fn once(duration: Duration) -> Self {
        Self::new(duration, TimerMode::Once)
    }

    pub fn repeating(duration: Duration) -> Self {
        Self::new(duration, TimerMode::Repeating)
    }

    pub fn new(duration: Duration, mode: TimerMode) -> Self {
        Self {
            duration,
            elapsed: Duration::ZERO,
            mode,
            finished: false,
            just_finished: false,
        }
    }

    pub fn tick(&mut self, delta: Duration) -> &mut Self {
        self.just_finished = false;
        if self.finished && self.mode == TimerMode::Once {
            return self;
        }

        self.elapsed += delta;
        if self.elapsed >= self.duration {
            self.finished = true;
            self.just_finished = true;
            if self.mode == TimerMode::Repeating && !self.duration.is_zero() {
                self.elapsed = Duration::from_secs_f32(
                    self.elapsed.as_secs_f32() % self.duration.as_secs_f32(),
                );
                self.finished = false;
            }
        }
        self
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.just_finished = false;
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn just_finished(&self) -> bool {
        self.just_finished
    }

    pub fn fraction(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_timer_finishes_once() {
        let mut timer = Timer::once(Duration::from_secs(1));
        timer.tick(Duration::from_millis(500));
        assert!(!timer.finished());
        timer.tick(Duration::from_millis(500));
        assert!(timer.finished());
        assert!(timer.just_finished());
        timer.tick(Duration::from_millis(500));
        assert!(timer.finished());
        assert!(!timer.just_finished());
    }

    #[test]
    fn repeating_timer_wraps_elapsed() {
        let mut timer = Timer::repeating(Duration::from_secs(1));
        timer.tick(Duration::from_millis(1250));
        assert!(!timer.finished());
        assert!(timer.just_finished());
        assert!(timer.elapsed() < Duration::from_secs(1));
    }
}
