//! Lightweight animation systems for gameplay-facing transform tweens.

use std::collections::HashMap;
use std::time::Duration;

use glam::{Mat4, Quat, Vec3};
use oxide_asset::{Assets, Handle};
use oxide_ecs::{Component, Resource};
use oxide_math::transform::Transform;
use oxide_transform::{GlobalTransform, TransformComponent};

use crate::app::{App, AppBuilder, AppStage, Plugin, TRANSFORM_PROPAGATE_SYSTEM};
use crate::ecs::{Entity, Query, Res, Time, World};

/// Stable label for the built-in transform tween update system.
pub const TRANSFORM_TWEEN_SYSTEM: &str = "oxide.animation.transform_tween";
/// Stable label for the built-in transform animation clip playback system.
pub const TRANSFORM_ANIMATION_SYSTEM: &str = "oxide.animation.transform_clips";
/// Stable label for the built-in skin joint matrix update system.
pub const SKIN_JOINT_MATRICES_SYSTEM: &str = "oxide.animation.skin_joint_matrices";

/// Typed handle for transform animation clips.
pub type TransformAnimationClipHandle = Handle<TransformAnimationClip>;
/// Typed handle for skeletal skin bind data.
pub type SkeletonSkinHandle = Handle<SkeletonSkin>;

/// Resource storing imported or authored transform animation clips.
#[derive(Resource, Default)]
pub struct TransformAnimationClipAssets {
    /// Handle-indexed transform animation clip storage.
    pub assets: Assets<TransformAnimationClip>,
}

/// Resource storing imported or authored skeletal skin bind data.
#[derive(Resource, Default)]
pub struct SkeletonSkinAssets {
    /// Handle-indexed skeletal skin storage.
    pub assets: Assets<SkeletonSkin>,
}

/// Skeletal skin bind data for a skinned mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonSkin {
    /// Human-readable skin name or imported label.
    pub name: String,
    /// Joint targets used by this skin.
    pub joints: Vec<TransformAnimationTarget>,
    /// Inverse bind matrices aligned with [`Self::joints`].
    pub inverse_bind_matrices: Vec<Mat4>,
    /// Optional skeleton root target.
    pub skeleton_root: Option<TransformAnimationTarget>,
}

impl SkeletonSkin {
    /// Creates skeletal skin bind data.
    pub fn new(
        name: impl Into<String>,
        joints: Vec<TransformAnimationTarget>,
        inverse_bind_matrices: Vec<Mat4>,
    ) -> Self {
        Self {
            name: name.into(),
            joints,
            inverse_bind_matrices,
            skeleton_root: None,
        }
    }

    /// Sets the skeleton root target.
    pub fn with_skeleton_root(mut self, skeleton_root: TransformAnimationTarget) -> Self {
        self.skeleton_root = Some(skeleton_root);
        self
    }
}

/// Component that links an entity to skeletal skin bind data.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkinFilter {
    /// Skin asset handle used by this entity.
    pub skin: SkeletonSkinHandle,
}

impl SkinFilter {
    /// Creates a skin filter from a skin handle.
    pub fn new(skin: SkeletonSkinHandle) -> Self {
        Self { skin }
    }
}

/// Component storing the current joint matrices for a skinned entity.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SkinJointMatrices {
    /// Joint matrices aligned with the entity's [`SkeletonSkin`] asset.
    pub matrices: Vec<Mat4>,
}

impl SkinJointMatrices {
    /// Creates joint matrices from precomputed values.
    pub fn new(matrices: Vec<Mat4>) -> Self {
        Self { matrices }
    }

    /// Returns the number of joint matrices.
    pub fn len(&self) -> usize {
        self.matrices.len()
    }

    /// Returns true when no joint matrices are stored.
    pub fn is_empty(&self) -> bool {
        self.matrices.is_empty()
    }
}

/// Stable target identity for transform animation tracks.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransformAnimationTarget(pub u64);

/// Interpolation curve for transform animation keyframes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransformAnimationInterpolation {
    /// Linearly interpolates vectors and spherically interpolates rotations.
    #[default]
    Linear,
    /// Holds the previous keyframe value until the next keyframe is reached.
    Step,
}

/// Transform property animated by a clip channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformAnimationProperty {
    /// Local translation channel.
    Translation,
    /// Local rotation channel.
    Rotation,
    /// Local scale channel.
    Scale,
}

/// Translation or scale keyframe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3Keyframe {
    /// Timestamp relative to the start of the clip.
    pub time: Duration,
    /// Keyframe translation or scale value.
    pub value: Vec3,
}

/// Rotation keyframe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuatKeyframe {
    /// Timestamp relative to the start of the clip.
    pub time: Duration,
    /// Keyframe rotation value.
    pub value: Quat,
}

/// A single transform animation channel targeting one entity target ID.
#[derive(Clone, Debug, PartialEq)]
pub enum TransformAnimationChannel {
    /// Local translation keyframes for one target.
    Translation {
        /// Entity target identifier animated by this channel.
        target: TransformAnimationTarget,
        /// Interpolation used between keyframes.
        interpolation: TransformAnimationInterpolation,
        /// Ordered translation keyframes.
        keyframes: Vec<Vec3Keyframe>,
    },
    /// Local rotation keyframes for one target.
    Rotation {
        /// Entity target identifier animated by this channel.
        target: TransformAnimationTarget,
        /// Interpolation used between keyframes.
        interpolation: TransformAnimationInterpolation,
        /// Ordered rotation keyframes.
        keyframes: Vec<QuatKeyframe>,
    },
    /// Local scale keyframes for one target.
    Scale {
        /// Entity target identifier animated by this channel.
        target: TransformAnimationTarget,
        /// Interpolation used between keyframes.
        interpolation: TransformAnimationInterpolation,
        /// Ordered scale keyframes.
        keyframes: Vec<Vec3Keyframe>,
    },
}

impl TransformAnimationChannel {
    /// Returns the target identifier animated by this channel.
    pub fn target(&self) -> TransformAnimationTarget {
        match self {
            Self::Translation { target, .. }
            | Self::Rotation { target, .. }
            | Self::Scale { target, .. } => *target,
        }
    }

    /// Returns the transform property animated by this channel.
    pub fn property(&self) -> TransformAnimationProperty {
        match self {
            Self::Translation { .. } => TransformAnimationProperty::Translation,
            Self::Rotation { .. } => TransformAnimationProperty::Rotation,
            Self::Scale { .. } => TransformAnimationProperty::Scale,
        }
    }
}

/// Lightweight transform animation clip made of independent transform channels.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformAnimationClip {
    /// Human-readable clip name or imported label.
    pub name: String,
    /// Clip duration.
    pub duration: Duration,
    /// Transform channels contained by this clip.
    pub channels: Vec<TransformAnimationChannel>,
}

impl TransformAnimationClip {
    /// Creates an empty transform animation clip.
    pub fn new(name: impl Into<String>, duration: Duration) -> Self {
        Self {
            name: name.into(),
            duration,
            channels: Vec::new(),
        }
    }

    /// Adds a channel to the clip.
    pub fn with_channel(mut self, channel: TransformAnimationChannel) -> Self {
        self.channels.push(channel);
        self
    }

    /// Samples the clip for one target, using `fallback` for properties with no channel.
    pub fn sample_target(
        &self,
        target: TransformAnimationTarget,
        time: Duration,
        fallback: Transform,
    ) -> Transform {
        let local_time = self.local_time(time);
        let mut sampled = fallback;
        for channel in self
            .channels
            .iter()
            .filter(|channel| channel.target() == target)
        {
            match channel {
                TransformAnimationChannel::Translation {
                    interpolation,
                    keyframes,
                    ..
                } => {
                    if let Some(value) =
                        sample_vec3_keyframes(keyframes, *interpolation, local_time)
                    {
                        sampled.position = value;
                    }
                }
                TransformAnimationChannel::Rotation {
                    interpolation,
                    keyframes,
                    ..
                } => {
                    if let Some(value) =
                        sample_quat_keyframes(keyframes, *interpolation, local_time)
                    {
                        sampled.rotation = value;
                    }
                }
                TransformAnimationChannel::Scale {
                    interpolation,
                    keyframes,
                    ..
                } => {
                    if let Some(value) =
                        sample_vec3_keyframes(keyframes, *interpolation, local_time)
                    {
                        sampled.scale = value;
                    }
                }
            }
        }
        sampled
    }

    fn local_time(&self, time: Duration) -> Duration {
        if self.duration.is_zero() || time <= self.duration {
            return time;
        }
        Duration::from_secs_f64(time.as_secs_f64() % self.duration.as_secs_f64())
    }
}

/// Component that plays a transform animation clip on an entity.
#[derive(Component, Clone, Debug)]
pub struct AnimationPlayer {
    /// Clip asset handle to sample.
    pub clip: TransformAnimationClipHandle,
    /// Target channel ID to sample from the clip.
    pub target: TransformAnimationTarget,
    elapsed: Duration,
    speed: f32,
    repeat: TweenRepeat,
    playing: bool,
    finished: bool,
}

impl AnimationPlayer {
    /// Creates a looping player for `clip` and `target`.
    pub fn new(clip: TransformAnimationClipHandle, target: TransformAnimationTarget) -> Self {
        Self {
            clip,
            target,
            elapsed: Duration::ZERO,
            speed: 1.0,
            repeat: TweenRepeat::Loop,
            playing: true,
            finished: false,
        }
    }

    /// Sets repeat behavior.
    pub fn with_repeat(mut self, repeat: TweenRepeat) -> Self {
        self.repeat = repeat;
        self
    }

    /// Sets playback speed. Negative values are clamped to zero.
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.0);
        self
    }

    /// Returns elapsed playback time before repeat wrapping.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns true when playback is active.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Returns true when one-shot playback has reached the end.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Starts or resumes playback.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pauses playback.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Resets elapsed time and starts playback.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.playing = true;
    }

    fn advance(&mut self, delta: Duration, clip_duration: Duration) -> Duration {
        if self.playing && !self.finished {
            self.elapsed += delta.mul_f32(self.speed);
            if self.repeat == TweenRepeat::Once && self.elapsed >= clip_duration {
                self.elapsed = clip_duration;
                self.finished = true;
                self.playing = false;
            }
        }

        if clip_duration.is_zero() || self.elapsed <= clip_duration {
            self.elapsed
        } else {
            match self.repeat {
                TweenRepeat::Once => clip_duration,
                TweenRepeat::Loop => Duration::from_secs_f64(
                    self.elapsed.as_secs_f64() % clip_duration.as_secs_f64(),
                ),
                TweenRepeat::PingPong => {
                    let elapsed = self.elapsed.as_secs_f64();
                    let duration = clip_duration.as_secs_f64();
                    let cycle = (elapsed / duration).floor() as u64;
                    let local = elapsed % duration;
                    if cycle.is_multiple_of(2) {
                        Duration::from_secs_f64(local)
                    } else {
                        Duration::from_secs_f64(duration - local)
                    }
                }
            }
        }
    }
}

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

fn sample_vec3_keyframes(
    keyframes: &[Vec3Keyframe],
    interpolation: TransformAnimationInterpolation,
    time: Duration,
) -> Option<Vec3> {
    let first = keyframes.first()?;
    if time <= first.time {
        return Some(first.value);
    }
    for window in keyframes.windows(2) {
        let [from, to] = window else {
            continue;
        };
        if time <= to.time {
            if interpolation == TransformAnimationInterpolation::Step || to.time <= from.time {
                return Some(from.value);
            }
            let t = duration_lerp_factor(from.time, to.time, time);
            return Some(from.value.lerp(to.value, t));
        }
    }
    keyframes.last().map(|keyframe| keyframe.value)
}

fn sample_quat_keyframes(
    keyframes: &[QuatKeyframe],
    interpolation: TransformAnimationInterpolation,
    time: Duration,
) -> Option<Quat> {
    let first = keyframes.first()?;
    if time <= first.time {
        return Some(first.value);
    }
    for window in keyframes.windows(2) {
        let [from, to] = window else {
            continue;
        };
        if time <= to.time {
            if interpolation == TransformAnimationInterpolation::Step || to.time <= from.time {
                return Some(from.value);
            }
            let t = duration_lerp_factor(from.time, to.time, time);
            return Some(from.value.slerp(to.value, t));
        }
    }
    keyframes.last().map(|keyframe| keyframe.value)
}

fn duration_lerp_factor(from: Duration, to: Duration, time: Duration) -> f32 {
    let span = (to - from).as_secs_f32();
    if span <= f32::EPSILON {
        0.0
    } else {
        ((time - from).as_secs_f32() / span).clamp(0.0, 1.0)
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

/// System that advances [`AnimationPlayer`] components using frame [`Time`].
pub fn transform_animation_system(
    time: Res<Time>,
    clips: Res<TransformAnimationClipAssets>,
    mut query: Query<(&mut TransformComponent, &mut AnimationPlayer)>,
) {
    let delta = time.delta;
    for (transform, player) in query.iter_mut() {
        let Some(clip) = clips.assets.get(&player.clip) else {
            continue;
        };
        let sample_time = player.advance(delta, clip.duration);
        let sampled = clip.sample_target(player.target, sample_time, transform.transform);
        transform.set_transform(sampled);
    }
}

/// Updates [`SkinJointMatrices`] from current joint [`GlobalTransform`] values.
pub fn skin_joint_matrices_system(world: &mut World) {
    if !world.contains_resource::<SkeletonSkinAssets>() {
        return;
    }

    let target_globals = {
        let mut query = world.query::<(&TransformAnimationTarget, &GlobalTransform)>();
        query
            .iter(world)
            .map(|(target, global)| (*target, global.matrix))
            .collect::<HashMap<_, _>>()
    };
    if target_globals.is_empty() {
        return;
    }

    let skinned_entities = {
        let mut query = world.query::<(Entity, &SkinFilter)>();
        query
            .iter(world)
            .filter_map(|(entity, filter)| {
                world
                    .get::<GlobalTransform>(entity)
                    .map(|global| (entity, *filter, global.matrix))
            })
            .collect::<Vec<_>>()
    };

    let skin_assets = world.resource::<SkeletonSkinAssets>();
    let mut updates = Vec::new();
    for (entity, filter, model_matrix) in skinned_entities {
        let Some(skin) = skin_assets.assets.get(&filter.skin) else {
            continue;
        };
        let inverse_model = model_matrix.inverse();
        let matrices = skin
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| {
                let inverse_bind = skin
                    .inverse_bind_matrices
                    .get(index)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                let joint_matrix = target_globals.get(joint).copied().unwrap_or(Mat4::IDENTITY);
                inverse_model * joint_matrix * inverse_bind
            })
            .collect::<Vec<_>>();
        updates.push((entity, SkinJointMatrices::new(matrices)));
    }

    for (entity, matrices) in updates {
        world.entity_mut(entity).insert(matrices);
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
        app.add_labeled_system_mut(
            AppStage::Update,
            TRANSFORM_ANIMATION_SYSTEM,
            transform_animation_system,
        );
        app.add_labeled_system_after_mut(
            AppStage::PostUpdate,
            SKIN_JOINT_MATRICES_SYSTEM,
            TRANSFORM_PROPAGATE_SYSTEM,
            skin_joint_matrices_system,
        );
    }
}

fn initialize_animation_resources(world: &mut World, _window: &crate::window::Window) {
    if !world.contains_resource::<Time>() {
        world.init_resource::<Time>();
    }
    if !world.contains_resource::<TransformAnimationClipAssets>() {
        world.insert_resource(TransformAnimationClipAssets::default());
    }
    if !world.contains_resource::<SkeletonSkinAssets>() {
        world.insert_resource(SkeletonSkinAssets::default());
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

    #[test]
    fn transform_animation_clip_samples_target_channels() {
        let target = TransformAnimationTarget(7);
        let clip = TransformAnimationClip::new("move", Duration::from_secs(1)).with_channel(
            TransformAnimationChannel::Translation {
                target,
                interpolation: TransformAnimationInterpolation::Linear,
                keyframes: vec![
                    Vec3Keyframe {
                        time: Duration::ZERO,
                        value: Vec3::ZERO,
                    },
                    Vec3Keyframe {
                        time: Duration::from_secs(1),
                        value: Vec3::new(10.0, 0.0, 0.0),
                    },
                ],
            },
        );

        let sampled = clip.sample_target(
            target,
            Duration::from_millis(250),
            Transform::from_position(Vec3::Y),
        );

        assert_eq!(sampled.position, Vec3::new(2.5, 0.0, 0.0));
        assert_eq!(sampled.scale, Vec3::ONE);
    }

    #[test]
    fn transform_animation_system_updates_player_transform() {
        let target = TransformAnimationTarget(3);
        let clip = TransformAnimationClip::new("rise", Duration::from_secs(1)).with_channel(
            TransformAnimationChannel::Translation {
                target,
                interpolation: TransformAnimationInterpolation::Linear,
                keyframes: vec![
                    Vec3Keyframe {
                        time: Duration::ZERO,
                        value: Vec3::ZERO,
                    },
                    Vec3Keyframe {
                        time: Duration::from_secs(1),
                        value: Vec3::Y,
                    },
                ],
            },
        );
        let handle = TransformAnimationClipHandle::new(99);
        let mut world = World::new();
        let mut time = Time::default();
        time.set_delta_for_tests(Duration::from_millis(500));
        world.insert_resource(time);
        let mut clips = TransformAnimationClipAssets::default();
        clips.assets.insert(handle, clip);
        world.insert_resource(clips);
        let entity = world
            .spawn((
                TransformComponent::default(),
                AnimationPlayer::new(handle, target).with_repeat(TweenRepeat::Once),
            ))
            .id();

        let mut queue = CommandQueue::default();
        let mut system = transform_animation_system.into_system();
        system.run(&mut world, &mut queue);

        let transform = world.get::<TransformComponent>(entity).unwrap();
        assert_eq!(transform.transform.position, Vec3::new(0.0, 0.5, 0.0));
        assert!(transform.is_dirty);
    }

    #[test]
    fn skin_joint_matrices_system_builds_model_relative_joint_matrices() {
        let skin_handle = SkeletonSkinHandle::new(5);
        let joint_target = TransformAnimationTarget(42);
        let mut skin_assets = SkeletonSkinAssets::default();
        skin_assets.assets.insert(
            skin_handle,
            SkeletonSkin::new("arm", vec![joint_target], vec![Mat4::IDENTITY]),
        );
        let mut world = World::new();
        world.insert_resource(skin_assets);
        let skinned = world
            .spawn((
                SkinFilter::new(skin_handle),
                GlobalTransform::from_matrix(Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0))),
            ))
            .id();
        world.spawn((
            joint_target,
            GlobalTransform::from_matrix(Mat4::from_translation(Vec3::new(2.0, 3.0, 0.0))),
        ));

        skin_joint_matrices_system(&mut world);

        let matrices = world
            .get::<SkinJointMatrices>(skinned)
            .expect("skin matrices should be inserted");
        assert_eq!(matrices.len(), 1);
        assert_eq!(
            matrices.matrices[0].transform_point3(Vec3::ZERO),
            Vec3::new(0.0, 3.0, 0.0)
        );
    }
}
