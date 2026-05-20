//! Engine plugin wiring for `oxide_audio`.

use crate::app::{App, AppBuilder, Plugin};
use crate::ecs::World;
use crate::window::Window;

pub use oxide_audio::{
    Audio, AudioClip, AudioClipError, AudioError, AudioTone, AudioWaveform, PlaySoundSettings,
    SoundInstanceId, SpatialSoundSettings,
};

pub struct AudioPlugin;

impl<T: App> Plugin<T> for AudioPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_audio);
    }
}

pub fn initialize_audio(world: &mut World, _window: &Window) {
    if world.contains_resource::<Audio>() {
        return;
    }

    match Audio::new_default() {
        Ok(audio) => world.insert_resource(audio),
        Err(err) => {
            tracing::warn!("Audio disabled: {err}");
            world.insert_resource(Audio::disabled());
        }
    }
}
