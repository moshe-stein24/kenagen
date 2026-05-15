//! Kenagen — Tauri entrypoint.
//!
//! The Tauri builder owns the GUI event loop on the main thread. A background
//! thread runs the instrument: open the audio engine, grab the keyboards,
//! route events through `Melody` and `Chords`, and emit payloads back to the
//! webview.
//!
//! ESC opens a quit-confirmation dialog instead of exiting directly. While the
//! dialog is open, key events are forwarded to the webview as `dialog-key`
//! events instead of playing notes; the webview owns the dialog state machine
//! and calls back into Rust via the `quit_app` / `resume_play` commands.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use evdev::KeyCode;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use kenagen::chords::{self, Chords, SharedChordMap, CHORD_CHANNEL};
use kenagen::engine::{Engine, Synth};
use kenagen::input::{self, KeyEvent};
use kenagen::melody::{Melody, MelodyEvent};
use kenagen::settings::{self, Settings};

const MODE_PLAY: u8 = 0;
const MODE_DIALOG: u8 = 1;

/// GM bank 0, preset 48 = String Ensemble 1 — the chord voice.
const CHORD_PRESET: u32 = 48;

/// MIDI channels we currently use.
const MELODY_CHAN: u8 = 0;
const STYLES_CHAN: u8 = 9; // GM convention: channel 10 (0-indexed 9) is drums

struct AppState {
    mode: Arc<AtomicU8>,
    /// Set once the instrument thread has constructed the Engine.
    synth: Mutex<Option<Arc<Synth>>>,
    /// Live chord-key → pitch-class map. Shared with the running `Chords`
    /// instance; mutated from the Function-menu chord-mapping UI.
    chord_map: SharedChordMap,
    /// Signals the instrument thread to refresh chord voicing after a remap
    /// (so notes that were already held get re-resolved against the new map).
    chord_refresh: Arc<std::sync::atomic::AtomicBool>,
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn resume_play(state: State<'_, AppState>) {
    state.mode.store(MODE_PLAY, Ordering::Relaxed);
}

/// Update a named output's volume. `percent` is 0..=100 from the UI slider.
#[tauri::command]
fn set_volume(state: State<'_, AppState>, channel: String, percent: u8) {
    let guard = state.synth.lock().unwrap();
    let Some(synth) = guard.as_ref() else { return };
    let p = percent.min(100);
    match channel.as_str() {
        "melody" => synth.cc(MELODY_CHAN, 7, percent_to_cc(p)),
        "chord" => synth.cc(CHORD_CHANNEL, 7, percent_to_cc(p)),
        "styles" => synth.cc(STYLES_CHAN, 7, percent_to_cc(p)),
        // Master: 0..100% maps to gain 0.0..1.0 (default 60% = 0.6).
        "master" => synth.set_gain(p as f32 / 100.0),
        _ => {}
    }
}

fn percent_to_cc(p: u8) -> u8 {
    ((p as u32 * 127) / 100).min(127) as u8
}

/// MIDI CC 1 (modulation) on the melody channel.
#[tauri::command]
fn set_modulation(state: State<'_, AppState>, value: u8) {
    let guard = state.synth.lock().unwrap();
    let Some(synth) = guard.as_ref() else { return };
    synth.cc(MELODY_CHAN, 1, value.min(127));
}

/// Pitch bend on the melody and chord channels. `value` is signed:
/// -8192..=+8191, with 0 being center (no bend). The mouse wheel drives this.
#[tauri::command]
fn set_pitch_bend(state: State<'_, AppState>, value: i32) {
    let guard = state.synth.lock().unwrap();
    let Some(synth) = guard.as_ref() else { return };
    let clamped = value.clamp(-8192, 8191);
    let midi = (clamped + 8192) as u16; // 0..=16383, 8192 = center
    synth.pitch_bend(MELODY_CHAN, midi);
    synth.pitch_bend(CHORD_CHANNEL, midi);
}

/// Pitch-bend wheel sensitivity in semitones (clamped to 1..=12). Applied to
/// melody and chord channels.
#[tauri::command]
fn set_pitch_bend_range(state: State<'_, AppState>, semitones: u8) {
    let guard = state.synth.lock().unwrap();
    let Some(synth) = guard.as_ref() else { return };
    let s = semitones.clamp(1, 12);
    synth.pitch_wheel_sens(MELODY_CHAN, s);
    synth.pitch_wheel_sens(CHORD_CHANNEL, s);
}

/// Update one chord-key assignment. `pitch_class` of None unassigns the key.
#[tauri::command]
fn set_chord_key(state: State<'_, AppState>, key_code: u16, pitch_class: Option<u8>) {
    {
        let mut map = state.chord_map.write().unwrap();
        match pitch_class {
            Some(pc) if pc < 12 => {
                map.insert(key_code, pc);
            }
            _ => {
                map.remove(&key_code);
            }
        }
    }
    state
        .chord_refresh
        .store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Persist the supplied settings to ~/.config/kenagen/settings.json. The
/// chord_map field is taken from the live runtime map (not from `new_settings`)
/// so the on-disk copy always matches what the user has been editing.
#[tauri::command]
fn save_settings(state: State<'_, AppState>, new_settings: Settings) -> Result<(), String> {
    let live_chord_map = state.chord_map.read().unwrap().clone();
    let full = Settings {
        volumes: new_settings.volumes,
        sensitivity: new_settings.sensitivity,
        chord_map: settings::chord_map_from_hashmap(&live_chord_map),
    };
    settings::save_settings(&full).map_err(|e| e.to_string())
}

/// Factory reset: delete the on-disk settings + banks, apply defaults to the
/// synth and chord map, and emit a `settings` event so the UI redraws.
#[tauri::command]
fn factory_reset(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    settings::delete_settings().map_err(|e| e.to_string())?;
    settings::delete_banks().map_err(|e| e.to_string())?;
    let defaults = Settings::default();
    apply_settings(&state, &defaults);
    *state.chord_map.write().unwrap() = chords::default_root_map();
    state
        .chord_refresh
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = app.emit("settings", json!(defaults));
    Ok(())
}

/// Apply a Settings struct to the running synth (volumes + pitch sensitivity).
/// Safe to call from any thread.
fn apply_settings(state: &State<'_, AppState>, s: &Settings) {
    let guard = state.synth.lock().unwrap();
    let Some(synth) = guard.as_ref() else { return };
    synth.cc(MELODY_CHAN, 7, percent_to_cc(s.volumes.melody));
    synth.cc(CHORD_CHANNEL, 7, percent_to_cc(s.volumes.chord));
    synth.cc(STYLES_CHAN, 7, percent_to_cc(s.volumes.styles));
    synth.set_gain(s.volumes.master as f32 / 100.0);
    synth.pitch_wheel_sens(MELODY_CHAN, s.sensitivity.pitch_bend_semitones.clamp(1, 12));
    synth.pitch_wheel_sens(CHORD_CHANNEL, s.sensitivity.pitch_bend_semitones.clamp(1, 12));
}

fn main() {
    let mode = Arc::new(AtomicU8::new(MODE_PLAY));
    let chord_map: SharedChordMap = Arc::new(RwLock::new(chords::default_root_map()));
    let chord_refresh = Arc::new(std::sync::atomic::AtomicBool::new(false));
    tauri::Builder::default()
        .manage(AppState {
            mode: Arc::clone(&mode),
            synth: Mutex::new(None),
            chord_map: Arc::clone(&chord_map),
            chord_refresh: Arc::clone(&chord_refresh),
        })
        .invoke_handler(tauri::generate_handler![
            quit_app,
            resume_play,
            set_volume,
            set_modulation,
            set_pitch_bend,
            set_pitch_bend_range,
            save_settings,
            factory_reset,
            set_chord_key
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let mode = Arc::clone(&mode);
            let chord_map = Arc::clone(&chord_map);
            let chord_refresh = Arc::clone(&chord_refresh);
            std::thread::spawn(move || {
                if let Err(e) = run_instrument(handle.clone(), mode, chord_map, chord_refresh) {
                    eprintln!("instrument thread error: {e:#}");
                    let _ = handle.emit("status", format!("error: {e}"));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn run_instrument(
    app: AppHandle,
    mode: Arc<AtomicU8>,
    chord_map: SharedChordMap,
    chord_refresh: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let engine = Engine::new()?;
    engine.select_program(CHORD_CHANNEL, 0, CHORD_PRESET)?;

    {
        let state = app.state::<AppState>();
        *state.synth.lock().unwrap() = Some(engine.synth());
    }

    // Load persisted settings (defaults on first run); apply volumes /
    // sensitivity to the synth and the chord_map into the shared RwLock.
    let loaded = settings::load_settings();
    {
        let state = app.state::<AppState>();
        apply_settings(&state, &loaded);
    }
    *chord_map.write().unwrap() = settings::chord_map_to_hashmap(&loaded.chord_map);

    let mut melody = Melody::new();
    let mut chords = Chords::new(Arc::clone(&chord_map));
    let events = input::keyboards()?;
    let _ = app.emit("status", "listening — press keys on any keyboard");
    let _ = app.emit("chord", json!(chords.state()));
    let _ = app.emit("settings", json!(loaded));

    for event in events {
        let current_mode = mode.load(Ordering::Relaxed);

        match (current_mode, event) {
            // ESC during play: open the quit dialog. Release any sounding
            // notes so they don't sustain while the user decides.
            (MODE_PLAY, KeyEvent::Pressed(KeyCode::KEY_ESC)) => {
                mode.store(MODE_DIALOG, Ordering::Relaxed);
                for note in melody.release_all(&engine) {
                    let _ = app.emit(
                        "note",
                        json!({
                            "action": "off",
                            "midi": note,
                            "keycode": 0,
                            "octave_shift": melody.octave_shift(),
                        }),
                    );
                }
                chords.release_all(&engine);
                let _ = app.emit("chord", json!(chords.state()));
                let _ = app.emit("request-quit", json!({}));
            }

            // Dialog mode: forward the keys the dialog cares about and ignore
            // everything else. Melody and chords are paused.
            (MODE_DIALOG, KeyEvent::Pressed(key)) => {
                let action = match key {
                    KeyCode::KEY_Y => Some("y"),
                    KeyCode::KEY_N => Some("n"),
                    KeyCode::KEY_C => Some("c"),
                    KeyCode::KEY_ESC => Some("esc"),
                    KeyCode::KEY_ENTER => Some("enter"),
                    KeyCode::KEY_LEFT => Some("left"),
                    KeyCode::KEY_RIGHT => Some("right"),
                    KeyCode::KEY_UP => Some("up"),
                    KeyCode::KEY_DOWN => Some("down"),
                    _ => None,
                };
                if let Some(a) = action {
                    let _ = app.emit("dialog-key", json!({ "key": a }));
                }
            }
            (MODE_DIALOG, KeyEvent::Released(_)) => {}

            (_, KeyEvent::Pressed(key)) => {
                if chords::is_chord_section_key(key) {
                    let _ = app.emit(
                        "chord-section-key",
                        json!({ "code": key.code(), "pressed": true }),
                    );
                }
                // Arrow up/down trigger a pitch-bend pulse (the JS side adds
                // the same spring-back animation as the mouse wheel).
                match key {
                    KeyCode::KEY_UP => {
                        let _ = app.emit("pitch-key", json!({ "direction": "up" }));
                    }
                    KeyCode::KEY_DOWN => {
                        let _ = app.emit("pitch-key", json!({ "direction": "down" }));
                    }
                    _ => {}
                }
                if let Some(me) = melody.handle(key, true, &engine) {
                    emit_melody(&app, me, melody.octave_shift());
                }
                if chords.handle(key, true, &engine) {
                    let _ = app.emit("chord", json!(chords.state()));
                }
                if chord_refresh.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    chords.refresh(&engine);
                    let _ = app.emit("chord", json!(chords.state()));
                }
            }
            (_, KeyEvent::Released(key)) => {
                if chords::is_chord_section_key(key) {
                    let _ = app.emit(
                        "chord-section-key",
                        json!({ "code": key.code(), "pressed": false }),
                    );
                }
                if let Some(me) = melody.handle(key, false, &engine) {
                    emit_melody(&app, me, melody.octave_shift());
                }
                if chords.handle(key, false, &engine) {
                    let _ = app.emit("chord", json!(chords.state()));
                }
                if chord_refresh.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    chords.refresh(&engine);
                    let _ = app.emit("chord", json!(chords.state()));
                }
            }
        }
    }
    Ok(())
}

fn emit_melody(app: &AppHandle, ev: MelodyEvent, shift: i32) {
    let payload = match ev {
        MelodyEvent::NoteOn { midi, keycode } => json!({
            "action": "on", "midi": midi, "keycode": keycode, "octave_shift": shift,
        }),
        MelodyEvent::NoteOff { midi, keycode } => json!({
            "action": "off", "midi": midi, "keycode": keycode, "octave_shift": shift,
        }),
        MelodyEvent::OctaveShift(o) => json!({
            "action": "octave", "octave_shift": o,
        }),
    };
    let _ = app.emit("note", payload);
}
