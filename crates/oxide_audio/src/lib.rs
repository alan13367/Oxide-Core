//! Oxide audio playback, clip loading, and software mixing.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(thiserror::Error, Debug)]
pub enum AudioError {
    #[error("no default output audio device is available")]
    NoOutputDevice,
    #[error("failed to query default output audio config: {0}")]
    DefaultConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("unsupported output sample format: {0:?}")]
    UnsupportedSampleFormat(cpal::SampleFormat),
    #[error("failed to build output audio stream: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("failed to start output audio stream: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
}

#[derive(thiserror::Error, Debug)]
pub enum AudioClipError {
    #[error("failed to read audio clip '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("WAV data is missing RIFF/WAVE header")]
    InvalidHeader,
    #[error("WAV data is missing fmt chunk")]
    MissingFmt,
    #[error("WAV data is missing data chunk")]
    MissingData,
    #[error("unsupported WAV format code {format}, bits per sample {bits_per_sample}")]
    UnsupportedWavFormat { format: u16, bits_per_sample: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundInstanceId(u64);

impl SoundInstanceId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AudioWaveform {
    Sine,
    Square,
    Saw,
    Noise,
}

#[derive(Clone, Copy, Debug)]
pub struct AudioTone {
    pub waveform: AudioWaveform,
    pub frequency_hz: f32,
    pub duration_secs: f32,
    pub volume: f32,
}

impl AudioTone {
    pub fn sine(frequency_hz: f32, duration_secs: f32, volume: f32) -> Self {
        Self {
            waveform: AudioWaveform::Sine,
            frequency_hz,
            duration_secs,
            volume,
        }
    }

    pub fn noise(duration_secs: f32, volume: f32) -> Self {
        Self {
            waveform: AudioWaveform::Noise,
            frequency_hz: 1.0,
            duration_secs,
            volume,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlaySoundSettings {
    pub volume: f32,
    pub repeat: bool,
    pub spatial: Option<SpatialSoundSettings>,
}

impl Default for PlaySoundSettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            repeat: false,
            spatial: None,
        }
    }
}

impl PlaySoundSettings {
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    pub fn repeating(mut self, repeat: bool) -> Self {
        self.repeat = repeat;
        self
    }

    pub fn with_spatial(mut self, spatial: SpatialSoundSettings) -> Self {
        self.spatial = Some(spatial);
        self
    }

    pub fn at_position(mut self, position: [f32; 3]) -> Self {
        self.spatial = Some(SpatialSoundSettings::new(position));
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialSoundSettings {
    pub position: [f32; 3],
    pub max_distance: f32,
    pub rolloff: f32,
    pub pan_strength: f32,
}

impl SpatialSoundSettings {
    pub fn new(position: [f32; 3]) -> Self {
        Self {
            position,
            max_distance: 32.0,
            rolloff: 1.0,
            pan_strength: 1.0,
        }
    }

    pub fn with_max_distance(mut self, max_distance: f32) -> Self {
        self.max_distance = max_distance.max(0.0);
        self
    }

    pub fn with_rolloff(mut self, rolloff: f32) -> Self {
        self.rolloff = rolloff.max(0.0);
        self
    }

    pub fn with_pan_strength(mut self, pan_strength: f32) -> Self {
        self.pan_strength = pan_strength.max(0.0);
        self
    }
}

#[derive(Clone, Debug)]
pub struct AudioClip {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

impl AudioClip {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate: sample_rate.max(1),
            channels: channels.max(1),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn tone(
        waveform: AudioWaveform,
        frequency_hz: f32,
        duration_secs: f32,
        sample_rate: u32,
    ) -> Self {
        let sample_rate = sample_rate.max(1);
        let frame_count = (duration_secs.max(0.0) * sample_rate as f32).ceil() as usize;
        let frequency_hz = frequency_hz.max(1.0);
        let mut seed = 0x1234_5678_u32;
        let mut samples = Vec::with_capacity(frame_count);

        for frame in 0..frame_count {
            let t = frame as f32 / sample_rate as f32;
            let phase = (t * frequency_hz).fract();
            let raw = match waveform {
                AudioWaveform::Sine => (std::f32::consts::TAU * t * frequency_hz).sin(),
                AudioWaveform::Square => {
                    if phase < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                AudioWaveform::Saw => phase * 2.0 - 1.0,
                AudioWaveform::Noise => {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    ((seed >> 8) as f32 / 0x00ff_ffff as f32) * 2.0 - 1.0
                }
            };
            samples.push(raw * envelope(frame, frame_count, sample_rate));
        }

        Self::new(samples, sample_rate, 1)
    }

    pub fn from_wav_file(path: impl AsRef<Path>) -> Result<Self, AudioClipError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| AudioClipError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_wav_bytes(&bytes)
    }

    pub fn from_wav_bytes(bytes: &[u8]) -> Result<Self, AudioClipError> {
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(AudioClipError::InvalidHeader);
        }

        let mut cursor = 12usize;
        let mut format = None::<WavFormat>;
        let mut data = None::<&[u8]>;

        while cursor + 8 <= bytes.len() {
            let id = &bytes[cursor..cursor + 4];
            let size = read_u32(bytes, cursor + 4).unwrap_or(0) as usize;
            cursor += 8;
            if cursor + size > bytes.len() {
                break;
            }

            match id {
                b"fmt " => {
                    format = Some(parse_wav_format(&bytes[cursor..cursor + size])?);
                }
                b"data" => {
                    data = Some(&bytes[cursor..cursor + size]);
                }
                _ => {}
            }

            cursor += size + (size % 2);
        }

        let format = format.ok_or(AudioClipError::MissingFmt)?;
        let data = data.ok_or(AudioClipError::MissingData)?;
        decode_wav_data(format, data)
    }
}

#[derive(Clone, Copy, Debug)]
struct WavFormat {
    format: u16,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

fn parse_wav_format(bytes: &[u8]) -> Result<WavFormat, AudioClipError> {
    if bytes.len() < 16 {
        return Err(AudioClipError::MissingFmt);
    }

    Ok(WavFormat {
        format: read_u16(bytes, 0).ok_or(AudioClipError::MissingFmt)?,
        channels: read_u16(bytes, 2).ok_or(AudioClipError::MissingFmt)?.max(1),
        sample_rate: read_u32(bytes, 4).ok_or(AudioClipError::MissingFmt)?.max(1),
        bits_per_sample: read_u16(bytes, 14).ok_or(AudioClipError::MissingFmt)?,
    })
}

fn decode_wav_data(format: WavFormat, data: &[u8]) -> Result<AudioClip, AudioClipError> {
    let mut samples = Vec::new();
    match (format.format, format.bits_per_sample) {
        (1, 8) => {
            samples.reserve(data.len());
            for byte in data {
                samples.push((*byte as f32 - 128.0) / 128.0);
            }
        }
        (1, 16) => {
            samples.reserve(data.len() / 2);
            for chunk in data.chunks_exact(2) {
                samples.push(i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0);
            }
        }
        (1, 24) => {
            samples.reserve(data.len() / 3);
            for chunk in data.chunks_exact(3) {
                let raw =
                    ((chunk[0] as i32) | ((chunk[1] as i32) << 8) | ((chunk[2] as i32) << 16)) << 8
                        >> 8;
                samples.push(raw as f32 / 8_388_608.0);
            }
        }
        (1, 32) => {
            samples.reserve(data.len() / 4);
            for chunk in data.chunks_exact(4) {
                samples.push(
                    i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f32
                        / 2_147_483_648.0,
                );
            }
        }
        (3, 32) => {
            samples.reserve(data.len() / 4);
            for chunk in data.chunks_exact(4) {
                samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        _ => {
            return Err(AudioClipError::UnsupportedWavFormat {
                format: format.format,
                bits_per_sample: format.bits_per_sample,
            });
        }
    }

    Ok(AudioClip::new(samples, format.sample_rate, format.channels))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
        *bytes.get(offset + 2)?,
        *bytes.get(offset + 3)?,
    ]))
}

fn envelope(frame: usize, frame_count: usize, sample_rate: u32) -> f32 {
    if frame_count == 0 {
        return 0.0;
    }

    let fade_frames = ((sample_rate as f32 * 0.005) as usize).clamp(1, frame_count);
    let attack = (frame as f32 / fade_frames as f32).clamp(0.0, 1.0);
    let release = ((frame_count.saturating_sub(frame)) as f32 / fade_frames as f32).clamp(0.0, 1.0);
    attack.min(release)
}

struct ActiveSound {
    id: SoundInstanceId,
    clip: Arc<AudioClip>,
    cursor: f64,
    step: f64,
    volume: f32,
    repeat: bool,
    spatial: Option<SpatialSoundSettings>,
    finished: bool,
}

impl ActiveSound {
    fn sample(&self, frame: usize, channel: usize) -> f32 {
        let channels = self.clip.channels as usize;
        let source_channel = channel.min(channels.saturating_sub(1));
        self.clip
            .samples
            .get(frame * channels + source_channel)
            .copied()
            .unwrap_or(0.0)
    }
}

struct MixerState {
    active: Vec<ActiveSound>,
    sample_rate: u32,
    channels: u16,
    master_volume: f32,
    listener_position: [f32; 3],
    listener_right: [f32; 3],
    next_id: u64,
}

impl MixerState {
    fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            active: Vec::new(),
            sample_rate: sample_rate.max(1),
            channels: channels.max(1),
            master_volume: 1.0,
            listener_position: [0.0, 0.0, 0.0],
            listener_right: [1.0, 0.0, 0.0],
            next_id: 1,
        }
    }

    fn play(&mut self, clip: Arc<AudioClip>, settings: PlaySoundSettings) -> SoundInstanceId {
        let id = SoundInstanceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1).max(1);
        let step = clip.sample_rate as f64 / self.sample_rate as f64;
        self.active.push(ActiveSound {
            id,
            clip,
            cursor: 0.0,
            step,
            volume: settings.volume.max(0.0),
            repeat: settings.repeat,
            spatial: settings.spatial,
            finished: false,
        });
        id
    }

    fn set_listener_position(&mut self, position: [f32; 3]) {
        self.listener_position = position;
    }

    fn set_listener_right(&mut self, right: [f32; 3]) {
        let length = vec3_length(right);
        self.listener_right = if length > 0.0 {
            [right[0] / length, right[1] / length, right[2] / length]
        } else {
            [1.0, 0.0, 0.0]
        };
    }

    fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.max(0.0);
    }

    fn stop(&mut self, id: SoundInstanceId) {
        for sound in &mut self.active {
            if sound.id == id {
                sound.finished = true;
            }
        }
    }

    fn stop_all(&mut self) {
        self.active.clear();
    }

    fn mix_into(&mut self, output: &mut [f32]) {
        output.fill(0.0);
        let output_channels = self.channels as usize;
        let listener_position = self.listener_position;
        let listener_right = self.listener_right;

        for frame in output.chunks_mut(output_channels) {
            for sound in &mut self.active {
                let frame_count = sound.clip.frame_count();
                if frame_count == 0 || sound.finished {
                    sound.finished = true;
                    continue;
                }

                while sound.cursor >= frame_count as f64 {
                    if sound.repeat {
                        sound.cursor -= frame_count as f64;
                    } else {
                        sound.finished = true;
                        break;
                    }
                }
                if sound.finished {
                    continue;
                }

                let source_frame = sound.cursor as usize;
                let next_frame = (source_frame + 1).min(frame_count - 1);
                let mix = (sound.cursor - source_frame as f64) as f32;
                let spatial = sound
                    .spatial
                    .map(|spatial| spatial_gains(spatial, listener_position, listener_right))
                    .unwrap_or([1.0, 1.0]);

                for (channel, sample) in frame.iter_mut().enumerate() {
                    let a = sound.sample(source_frame, channel);
                    let b = sound.sample(next_frame, channel);
                    let spatial_gain = match output_channels {
                        1 => (spatial[0] + spatial[1]) * 0.5,
                        _ if channel == 0 => spatial[0],
                        _ if channel == 1 => spatial[1],
                        _ => 1.0,
                    };
                    *sample +=
                        (a + (b - a) * mix) * sound.volume * self.master_volume * spatial_gain;
                }

                sound.cursor += sound.step;
            }
        }

        for sample in output {
            *sample = sample.clamp(-1.0, 1.0);
        }
        self.active.retain(|sound| !sound.finished);
    }
}

fn spatial_gains(
    spatial: SpatialSoundSettings,
    listener_position: [f32; 3],
    listener_right: [f32; 3],
) -> [f32; 2] {
    let relative = [
        spatial.position[0] - listener_position[0],
        spatial.position[1] - listener_position[1],
        spatial.position[2] - listener_position[2],
    ];
    let distance = vec3_length(relative);
    let max_distance = spatial.max_distance.max(0.0001);
    let attenuation = (1.0 - distance / max_distance)
        .clamp(0.0, 1.0)
        .powf(spatial.rolloff.max(0.0001));
    let pan =
        (vec3_dot(relative, listener_right) / max_distance).clamp(-1.0, 1.0) * spatial.pan_strength;
    let pan = pan.clamp(-1.0, 1.0);
    let left = ((1.0 - pan) * 0.5).sqrt() * attenuation;
    let right = ((1.0 + pan) * 0.5).sqrt() * attenuation;
    [left, right]
}

fn vec3_dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn vec3_length(value: [f32; 3]) -> f32 {
    vec3_dot(value, value).sqrt()
}

pub struct Audio {
    mixer: Arc<Mutex<MixerState>>,
    _stream: Option<cpal::Stream>,
    enabled: bool,
}

impl Audio {
    pub fn new_default() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let supported_config = device.default_output_config()?;
        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let stream_config: cpal::StreamConfig = supported_config.clone().into();
        let mixer = Arc::new(Mutex::new(MixerState::new(sample_rate, channels)));
        let stream = build_stream(
            &device,
            &stream_config,
            supported_config.sample_format(),
            mixer.clone(),
        )?;
        stream.play()?;

        Ok(Self {
            mixer,
            _stream: Some(stream),
            enabled: true,
        })
    }

    pub fn disabled() -> Self {
        Self {
            mixer: Arc::new(Mutex::new(MixerState::new(48_000, 2))),
            _stream: None,
            enabled: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.mixer
            .lock()
            .map(|mixer| mixer.sample_rate)
            .unwrap_or(48_000)
    }

    pub fn play_clip(
        &self,
        clip: impl Into<Arc<AudioClip>>,
        settings: PlaySoundSettings,
    ) -> Option<SoundInstanceId> {
        if !self.enabled {
            return None;
        }

        self.mixer
            .lock()
            .ok()
            .map(|mut mixer| mixer.play(clip.into(), settings))
    }

    pub fn play_spatial_clip(
        &self,
        clip: impl Into<Arc<AudioClip>>,
        position: [f32; 3],
        settings: PlaySoundSettings,
    ) -> Option<SoundInstanceId> {
        self.play_clip(clip, settings.at_position(position))
    }

    pub fn play_tone(&self, tone: AudioTone) -> Option<SoundInstanceId> {
        let clip = AudioClip::tone(
            tone.waveform,
            tone.frequency_hz,
            tone.duration_secs,
            self.output_sample_rate(),
        );
        self.play_clip(clip, PlaySoundSettings::default().with_volume(tone.volume))
    }

    pub fn play_spatial_tone(
        &self,
        tone: AudioTone,
        position: [f32; 3],
    ) -> Option<SoundInstanceId> {
        let clip = AudioClip::tone(
            tone.waveform,
            tone.frequency_hz,
            tone.duration_secs,
            self.output_sample_rate(),
        );
        self.play_spatial_clip(
            clip,
            position,
            PlaySoundSettings::default().with_volume(tone.volume),
        )
    }

    pub fn set_master_volume(&self, volume: f32) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.set_master_volume(volume);
        }
    }

    pub fn set_listener_position(&self, position: [f32; 3]) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.set_listener_position(position);
        }
    }

    pub fn set_listener_right(&self, right: [f32; 3]) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.set_listener_right(right);
        }
    }

    pub fn stop(&self, id: SoundInstanceId) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.stop(id);
        }
    }

    pub fn stop_all(&self) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.stop_all();
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    mixer: Arc<Mutex<MixerState>>,
) -> Result<cpal::Stream, AudioError> {
    let err_fn = |err| tracing::warn!("Audio stream error: {err}");

    match sample_format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| write_f32_output(data, &mixer),
                err_fn,
                None,
            )
            .map_err(AudioError::from),
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                config,
                move |data: &mut [i16], _| write_i16_output(data, &mixer),
                err_fn,
                None,
            )
            .map_err(AudioError::from),
        cpal::SampleFormat::U16 => device
            .build_output_stream(
                config,
                move |data: &mut [u16], _| write_u16_output(data, &mixer),
                err_fn,
                None,
            )
            .map_err(AudioError::from),
        other => Err(AudioError::UnsupportedSampleFormat(other)),
    }
}

fn write_f32_output(output: &mut [f32], mixer: &Arc<Mutex<MixerState>>) {
    if let Ok(mut mixer) = mixer.lock() {
        mixer.mix_into(output);
    } else {
        output.fill(0.0);
    }
}

fn write_i16_output(output: &mut [i16], mixer: &Arc<Mutex<MixerState>>) {
    let mut scratch = vec![0.0; output.len()];
    write_f32_output(&mut scratch, mixer);
    for (dst, src) in output.iter_mut().zip(scratch) {
        *dst = (src.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
    }
}

fn write_u16_output(output: &mut [u16], mixer: &Arc<Mutex<MixerState>>) {
    let mut scratch = vec![0.0; output.len()];
    write_f32_output(&mut scratch, mixer);
    for (dst, src) in output.iter_mut().zip(scratch) {
        *dst = ((src.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_generates_samples() {
        let clip = AudioClip::tone(AudioWaveform::Sine, 440.0, 0.1, 48_000);
        assert_eq!(clip.sample_rate(), 48_000);
        assert_eq!(clip.channels(), 1);
        assert!(clip.frame_count() > 0);
        assert!(clip.samples().iter().any(|sample| sample.abs() > 0.0));
    }

    #[test]
    fn wav_pcm16_decodes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes());
        bytes.extend_from_slice(&i16::MAX.to_le_bytes());

        let clip = AudioClip::from_wav_bytes(&bytes).unwrap();
        assert_eq!(clip.sample_rate(), 48_000);
        assert_eq!(clip.channels(), 1);
        assert_eq!(clip.frame_count(), 2);
        assert!(clip.samples()[1] > 0.9);
    }

    #[test]
    fn spatial_settings_pan_mono_sound_to_listener_right() {
        let mut mixer = MixerState::new(48_000, 2);
        let clip = Arc::new(AudioClip::new(vec![1.0, 1.0], 48_000, 1));
        mixer.play(
            clip,
            PlaySoundSettings::default()
                .with_spatial(SpatialSoundSettings::new([8.0, 0.0, 0.0]).with_max_distance(16.0)),
        );

        let mut output = vec![0.0; 4];
        mixer.mix_into(&mut output);

        assert!(output[1] > output[0]);
        assert!(output[3] > output[2]);
    }

    #[test]
    fn spatial_settings_attenuate_distant_sounds() {
        let near = spatial_gains(
            SpatialSoundSettings::new([1.0, 0.0, 0.0]).with_max_distance(16.0),
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        );
        let far = spatial_gains(
            SpatialSoundSettings::new([12.0, 0.0, 0.0]).with_max_distance(16.0),
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        );

        assert!(near[0] > far[0]);
        assert!(near[1] > far[1]);
    }

    #[test]
    fn listener_right_controls_spatial_pan_axis() {
        let mut mixer = MixerState::new(48_000, 2);
        mixer.set_listener_right([0.0, 0.0, 1.0]);
        let clip = Arc::new(AudioClip::new(vec![1.0], 48_000, 1));
        mixer.play(
            clip,
            PlaySoundSettings::default()
                .at_position([0.0, 0.0, 8.0])
                .with_spatial(SpatialSoundSettings::new([0.0, 0.0, 8.0])),
        );

        let mut output = vec![0.0; 2];
        mixer.mix_into(&mut output);

        assert!(output[1] > output[0]);
    }
}
