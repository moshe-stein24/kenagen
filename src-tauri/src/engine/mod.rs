//! Audio engine: owns the CPAL output stream and the FluidSynth instance.
//!
//! Architecture (per the project's design decision): CPAL opens the output
//! stream and, in its audio callback, pulls rendered samples from FluidSynth via
//! `fluid_synth_write_float`. This gives us our own mixing point for the
//! metronome and styles modules later, rather than relying on FluidSynth's
//! built-in audio driver.

mod fluid;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub use fluid::Synth;

/// Default General MIDI soundfont shipped on Ubuntu. Override with the
/// `KENAGEN_SOUNDFONT` environment variable.
const DEFAULT_SOUNDFONT: &str = "/usr/share/sounds/sf2/default-GM.sf2";

pub struct Engine {
    synth: Arc<Synth>,
    sfont_id: i32,
    _stream: cpal::Stream,
}

impl Engine {
    /// Open the default output device, start the audio stream, and load the
    /// soundfont. Audio is running by the time this returns.
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default audio output device")?;
        let supported = device
            .default_output_config()
            .context("querying default output config")?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        if config.channels != 2 {
            bail!(
                "engine currently expects a stereo output device, got {} channel(s)",
                config.channels
            );
        }
        let sample_rate = config.sample_rate;

        let synth = Arc::new(Synth::new(sample_rate)?);
        let soundfont = soundfont_path();
        let sfont_id = synth
            .load_soundfont(&soundfont)
            .with_context(|| format!("loading soundfont {}", soundfont.display()))?;
        // Channel 0, bank 0, preset 0 = Acoustic Grand Piano in General MIDI.
        synth.program_select(0, sfont_id, 0, 0)?;

        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream(&device, &config, Arc::clone(&synth))?,
            other => bail!(
                "unsupported sample format {other:?}; the engine currently requires f32 output"
            ),
        };
        stream.play().context("starting audio stream")?;

        println!(
            "engine: {} Hz stereo, soundfont {}",
            sample_rate,
            soundfont.display()
        );
        Ok(Engine {
            synth,
            sfont_id,
            _stream: stream,
        })
    }

    /// Start a note. `chan` 0-15, `key` MIDI note 0-127, `vel` 1-127.
    pub fn note_on(&self, chan: u8, key: u8, vel: u8) {
        self.synth.note_on(chan, key, vel);
    }

    /// Stop a note that was started with `note_on`.
    pub fn note_off(&self, chan: u8, key: u8) {
        self.synth.note_off(chan, key);
    }

    /// Select the instrument voice for a MIDI channel — a bank/preset within
    /// the loaded soundfont. In General MIDI, bank 0 preset 0 is Acoustic Grand
    /// Piano and preset 48 is String Ensemble 1.
    pub fn select_program(&self, chan: u8, bank: u32, preset: u32) -> Result<()> {
        self.synth.program_select(chan, self.sfont_id, bank, preset)
    }

    /// Set a MIDI channel's volume (CC 7, 0-127). Scales how loud notes played
    /// on this channel sound, independently of velocity.
    pub fn set_channel_volume(&self, chan: u8, value: u8) {
        self.synth.cc(chan, 7, value.min(127));
    }

    /// Master output gain. We start at 0.6 to leave headroom for chord + style
    /// mixing; the UI can push it back up.
    pub fn set_master_gain(&self, gain: f32) {
        self.synth.set_gain(gain.clamp(0.0, 10.0));
    }

    /// Apply pitch-bend wheel sensitivity to a channel (1..=12 semitones).
    pub fn set_pitch_bend_range(&self, chan: u8, semitones: u8) {
        self.synth.pitch_wheel_sens(chan, semitones.clamp(1, 12));
    }

    /// Borrow the shared synth handle. The Tauri commands take a clone so they
    /// can call CC/gain/pitch bend from the UI thread.
    pub fn synth(&self) -> Arc<Synth> {
        Arc::clone(&self.synth)
    }
}

fn soundfont_path() -> PathBuf {
    std::env::var_os("KENAGEN_SOUNDFONT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOUNDFONT))
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    synth: Arc<Synth>,
) -> Result<cpal::Stream> {
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                synth.render_interleaved_stereo(data);
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .context("building output stream")?;
    Ok(stream)
}
