//! Minimal FFI bindings to the system FluidSynth C library (libfluidsynth-dev),
//! plus a safe `Synth` wrapper.
//!
//! There is no maintained FluidSynth 2.x crate, so we bind the handful of
//! functions the engine needs ourselves. The library is located and linked by
//! `build.rs` via pkg-config.

use std::ffi::CString;
use std::os::raw::{c_char, c_double, c_int, c_void};
use std::path::Path;

use anyhow::{bail, Context, Result};

// FluidSynth's opaque handle types — we only ever hold pointers to them.
#[repr(C)]
struct FluidSettings {
    _private: [u8; 0],
}
#[repr(C)]
struct FluidSynth {
    _private: [u8; 0],
}

const FLUID_OK: c_int = 0;
const FLUID_FAILED: c_int = -1;

extern "C" {
    fn new_fluid_settings() -> *mut FluidSettings;
    fn delete_fluid_settings(settings: *mut FluidSettings);
    fn fluid_settings_setnum(
        settings: *mut FluidSettings,
        name: *const c_char,
        val: c_double,
    ) -> c_int;

    fn new_fluid_synth(settings: *mut FluidSettings) -> *mut FluidSynth;
    fn delete_fluid_synth(synth: *mut FluidSynth);

    fn fluid_synth_sfload(
        synth: *mut FluidSynth,
        filename: *const c_char,
        reset_presets: c_int,
    ) -> c_int;
    fn fluid_synth_program_select(
        synth: *mut FluidSynth,
        chan: c_int,
        sfont_id: c_int,
        bank_num: c_int,
        preset_num: c_int,
    ) -> c_int;
    fn fluid_synth_noteon(synth: *mut FluidSynth, chan: c_int, key: c_int, vel: c_int) -> c_int;
    fn fluid_synth_noteoff(synth: *mut FluidSynth, chan: c_int, key: c_int) -> c_int;
    fn fluid_synth_write_float(
        synth: *mut FluidSynth,
        len: c_int,
        lout: *mut c_void,
        loff: c_int,
        lincr: c_int,
        rout: *mut c_void,
        roff: c_int,
        rincr: c_int,
    ) -> c_int;
}

/// Owns a FluidSynth instance and its settings. Renders audio on demand via
/// `render_interleaved_stereo` and accepts note events from any thread.
pub struct Synth {
    synth: *mut FluidSynth,
    settings: *mut FluidSettings,
}

// FluidSynth's public `fluid_synth_*` API is internally synchronised, so it is
// safe to render on the audio thread while sending note on/off from another.
unsafe impl Send for Synth {}
unsafe impl Sync for Synth {}

impl Synth {
    /// Create a synth that renders at `sample_rate` Hz (matched to the audio
    /// device so playback pitch and tempo are correct).
    pub fn new(sample_rate: u32) -> Result<Self> {
        unsafe {
            let settings = new_fluid_settings();
            if settings.is_null() {
                bail!("new_fluid_settings() returned null");
            }
            set_num(settings, "synth.sample-rate", sample_rate as f64);
            // Headroom so chords and styles don't clip once they're added.
            set_num(settings, "synth.gain", 0.6);

            let synth = new_fluid_synth(settings);
            if synth.is_null() {
                delete_fluid_settings(settings);
                bail!("new_fluid_synth() returned null");
            }
            Ok(Synth { synth, settings })
        }
    }

    /// Load a SoundFont (.sf2) file, returning its FluidSynth font id.
    pub fn load_soundfont(&self, path: &Path) -> Result<i32> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .context("soundfont path contains an interior null byte")?;
        let id = unsafe { fluid_synth_sfload(self.synth, c_path.as_ptr(), 1) };
        if id == FLUID_FAILED {
            bail!("fluid_synth_sfload failed for {}", path.display());
        }
        Ok(id)
    }

    /// Bind a MIDI channel to a bank/preset within a loaded SoundFont.
    pub fn program_select(&self, chan: u8, sfont_id: i32, bank: u32, preset: u32) -> Result<()> {
        let rc = unsafe {
            fluid_synth_program_select(
                self.synth,
                chan as c_int,
                sfont_id as c_int,
                bank as c_int,
                preset as c_int,
            )
        };
        if rc != FLUID_OK {
            bail!("fluid_synth_program_select failed (chan {chan}, bank {bank}, preset {preset})");
        }
        Ok(())
    }

    /// Start a note. `chan` 0-15, `key` MIDI note 0-127, `vel` 1-127.
    pub fn note_on(&self, chan: u8, key: u8, vel: u8) {
        unsafe { fluid_synth_noteon(self.synth, chan as c_int, key as c_int, vel as c_int) };
    }

    /// Stop a sounding note. FluidSynth returns FLUID_FAILED if the note was not
    /// playing, which is harmless here, so the result is ignored.
    pub fn note_off(&self, chan: u8, key: u8) {
        unsafe { fluid_synth_noteoff(self.synth, chan as c_int, key as c_int) };
    }

    /// Fill an interleaved stereo `[L, R, L, R, ...]` f32 buffer with synth
    /// output. Intended to be called from the audio callback.
    pub fn render_interleaved_stereo(&self, buf: &mut [f32]) {
        let frames = (buf.len() / 2) as c_int;
        let ptr = buf.as_mut_ptr() as *mut c_void;
        // One buffer, two channels: L at offset 0, R at offset 1, stride 2.
        unsafe {
            fluid_synth_write_float(self.synth, frames, ptr, 0, 2, ptr, 1, 2);
        }
    }
}

impl Drop for Synth {
    fn drop(&mut self) {
        // The synth must be deleted before the settings it was created from.
        unsafe {
            delete_fluid_synth(self.synth);
            delete_fluid_settings(self.settings);
        }
    }
}

/// Set a numeric FluidSynth setting, ignoring the return code (a bad setting
/// name simply leaves the default in place).
unsafe fn set_num(settings: *mut FluidSettings, name: &str, val: f64) {
    let c_name = CString::new(name).expect("setting name has no interior null byte");
    fluid_settings_setnum(settings, c_name.as_ptr(), val);
}
