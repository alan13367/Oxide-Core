//! Lightweight animation systems for gameplay-facing transform tweens.

use std::time::Duration;

use oxide_ecs::Component;
use oxide_math::transform::Transform;
use oxide_transform::TransformComponent;

use crate::app::{App, AppBuilder, AppStage, Plugin};
use crate::ecs::{Query, Res, Time, World};

/// Stable label for the built-in transform tween update system.
pub const TRANSFORM_TWEEN_SYSTEM: &str = "oxide.animation.transform_tween";

/// Interpolation curve used by [`TransformTween`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TweenEasing {
    #[default]
    Linear,
    SmoothStep,
    EaseIn,
    EaseOut,
}

impl TweenEasing {
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::SmoothStep => t * t * (3.0 - 2.0 * t),
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

/// Repeat behavior for [`TransformTween`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TweenRepeat {
    #[default]
    Once,
    Loop,
    PingPong,
}

/// Component that animates an entity's local transform over time.
#[derive(Component, Clone, Debug)]
pub struct TransformTween {
    from: Transform,
    to: Transform,
    duration: Duration,
    elapsed: Duration,
    easing: TweenEasing,
    repeat: TweenRepeat,
    playing: bool,
    finished: bool,
}

impl TransformTween {
    /// Creates a one-shot transform tween.
    pub fn once(from: Transform, to: Transform, duration: Duration) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: Duration::ZERO,
            easing: TweenEasing::Linear,
            repeat: TweenRepeat::Once,
            playing: true,
            finished: false,
        }
    }

    /// Creates a looping transform tween.
    pub fn looping(from: Transform, to: Transform, duration: Duration) -> Self {
        Self::once(from, to, duration).with_repeat(TweenRepeat::Loop)
    }

    /// Creates a forward/backward looping transform tween.
    pub fn ping_pong(from: Transform, to: Transform, duration: Duration) -> Self {
        Self::once(from, to, duration).with_repeat(TweenRepeat::PingPong)
    }

    pub fn with_easing(mut self, easing: TweenEasing) -> Self {
        self.easing = easing;
        self
    }

    pub fn with_repeat(mut self, repeat: TweenRepeat) -> Self {
        self.repeat = repeat;
        self
    }

    pub fn from(&self) -> Transform {
        self.from
    }

    pub fn to(&self) -> Transform {
        self.to
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn repeat(&self) -> TweenRepeat {
        self.repeat
    }

    pub fn easing(&self) -> TweenEasing {
        self.easing
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.playing = true;
    }

    /// Advances the tween and returns the sampled transform.
    pub fn tick(&mut self, delta: Duration) -> Transform {
        if self.playing && !self.finished {
            self.elapsed += delta;
            if self.repeat == TweenRepeat::Once && self.elapsed >= self.duration {
                self.elapsed = self.duration;
                self.finished = true;
                self.playing = false;
            }
        }
        self.sample()
    }

    /// Samples the current tween value without advancing time.
    pub fn sample(&self) -> Transform {
        let progress = self.progress();
        sample_transform(self.from, self.to, self.easing.sample(progress))
    }

    /// Returns normalized progress after repeat behavior is applied.
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }

        let elapsed = self.elapsed.as_secs_f64();
        let duration = self.duration.as_secs_f64();
        match self.repeat {
            TweenRepeat::Once => (elapsed / duration).clamp(0.0, 1.0) as f32,
            TweenRepeat::Loop => ((elapsed / duration) % 1.0) as f32,
            TweenRepeat::PingPong => {
                let cycle = (elapsed / duration).floor() as u64;
                let local = ((elapsed / duration) % 1.0) as f32;
                if cycle.is_multiple_of(2) {
                    local
                } else {
                    1.0 - local
                }
            }
        }
    }
}

fn sample_transform(from: Transform, to: Transform, t: f32) -> Transform {
    Transform {
        position: from.position.lerp(to.position, t),
        rotation: from.rotation.slerp(to.rotation, t),
        scale: from.scale.lerp(to.scale, t),
    }
}

/// System that advances [`TransformTween`] components using the frame [`Time`].
pub fn transform_tween_system(
    time: Res<Time>,
    mut query: Query<(&mut TransformComponent, &mut TransformTween)>,
) {
    let delta = time.delta;
    for (transform, tween) in query.iter_mut() {
        transform.set_transform(tween.tick(delta));
    }
}

/// Plugin that installs transform tween animation support.
pub struct AnimationPlugin;

impl<T: App> Plugin<T> for AnimationPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_animation_resources);
        app.add_labeled_system_mut(
            AppStage::Update,
            TRANSFORM_TWEEN_SYSTEM,
            transform_tween_system,
        );
    }
}

fn initialize_animation_resources(world: &mut World, _window: &crate::window::Window) {
    if !world.contains_resource::<Time>() {
        world.init_resource::<Time>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};
    use std::time::Duration;

    use crate::ecs::{CommandQueue, IntoSystem};

    #[test]
    fn transform_tween_interpolates_transform_components() {
        let from = Transform::from_position(Vec3::ZERO);
        let to = Transform {
            position: Vec3::new(10.0, 0.0, 0.0),
            rotation: Quat::from_rotation_y(std::f32::consts::PI),
            scale: Vec3::splat(2.0),
        };
        let mut world = World::new();
        let mut time = Time::default();
        time.set_delta_for_tests(Duration::from_millis(500));
        world.insert_resource(time);
        let entity = world
            .spawn((
                TransformComponent::new(from),
                TransformTween::once(from, to, Duration::from_secs(1)),
            ))
            .id();

        let mut queue = CommandQueue::default();
        let mut system = transform_tween_system.into_system();
        system.run(&mut world, &mut queue);

        let transform = world.get::<TransformComponent>(entity).unwrap();
        assert_eq!(transform.transform.position, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(transform.transform.scale, Vec3::splat(1.5));
        assert!(transform.is_dirty);
    }

    #[test]
    fn transform_tween_supports_ping_pong_progress() {
        let from = Transform::from_position(Vec3::ZERO);
        let to = Transform::from_position(Vec3::X);
        let mut tween = TransformTween::ping_pong(from, to, Duration::from_secs(1));

        tween.tick(Duration::from_millis(250));
        assert!((tween.progress() - 0.25).abs() < f32::EPSILON);

        tween.tick(Duration::from_secs(1));
        assert!((tween.progress() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn one_shot_transform_tween_finishes_and_pauses() {
        let from = Transform::from_position(Vec3::ZERO);
        let to = Transform::from_position(Vec3::X);
        let mut tween = TransformTween::once(from, to, Duration::from_secs(1));

        tween.tick(Duration::from_secs(2));

        assert!(tween.is_finished());
        assert!(!tween.is_playing());
        assert_eq!(tween.progress(), 1.0);
        assert_eq!(tween.sample().position, Vec3::X);
    }
}
