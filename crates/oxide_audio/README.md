# oxide_audio

Audio playback and software mixing for Oxide Core.

The crate owns Oxide's runtime audio types: `Audio`, `AudioClip`, generated
tones, WAV decoding, playback settings, and sound instance IDs. `oxide_engine`
provides `AudioPlugin` to install `Audio` into the ECS world.
