//! Chord input — the left hand.
//!
//! Chord-section keys are the 16 left-hand QWERTY keys (top/home/bottom rows,
//! left half). Each can be assigned any pitch class via a runtime map; nothing
//! is hard-wired. The default map matches the Yamaha-style "letter = chord
//! name" layout, with B moved to V (the B key is now a melody note).
//!
//! The chord *quality* follows this modifier matrix:
//!   no modifiers              → major
//!   Alt                       → dominant 7 (major triad + b7)
//!   Alt + Shift               → minor 7    (shortcut for m7, no Ctrl needed)
//!   Ctrl                      → minor      (lowers the 3rd)
//!   Ctrl + Shift              → minor 7    (adds the b7)
//!   Ctrl + Shift + Alt        → minor-major 7 (raises the 7 to M7)
//!
//! Multiple chord-root keys may be physically held; the most recently pressed
//! one wins (release pops back to whatever's still held).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use evdev::KeyCode;
use serde::Serialize;

use crate::engine::Engine;

/// MIDI channel the chord voice plays on. Channel 0 belongs to melody.
pub const CHORD_CHANNEL: u8 = 1;
const CHORD_VELOCITY: u8 = 85;

/// Place chord roots in the octave starting at C3 (MIDI 48), so chord notes
/// sit just below middle C — out of the way of the melody.
const ROOT_BASE: u8 = 48;

/// The 16 physical keys that make up the chord-section visual keyboard. The
/// user can assign any chord (or none) to each of these via the Function
/// settings panel.
pub const CHORD_SECTION_KEYS: &[KeyCode] = &[
    // Top row
    KeyCode::KEY_Q,
    KeyCode::KEY_W,
    KeyCode::KEY_E,
    KeyCode::KEY_R,
    KeyCode::KEY_T,
    KeyCode::KEY_Y,
    // Home row (left half)
    KeyCode::KEY_A,
    KeyCode::KEY_S,
    KeyCode::KEY_D,
    KeyCode::KEY_F,
    KeyCode::KEY_G,
    // Bottom row (left half)
    KeyCode::KEY_Z,
    KeyCode::KEY_X,
    KeyCode::KEY_C,
    KeyCode::KEY_V,
    KeyCode::KEY_B,
];

pub fn is_chord_section_key(key: KeyCode) -> bool {
    CHORD_SECTION_KEYS.contains(&key)
}

/// Live key-code → pitch-class mapping. Shared between the instrument thread
/// (reads on every key press) and the Tauri commands (writes when the user
/// remaps a key from the settings UI).
pub type SharedChordMap = Arc<RwLock<HashMap<u16, u8>>>;

/// Photo-2 default layout: letters spell their chord, except B (on V).
pub fn default_root_map() -> HashMap<u16, u8> {
    let pairs: [(KeyCode, u8); 12] = [
        (KeyCode::KEY_C, 0),
        (KeyCode::KEY_D, 2),
        (KeyCode::KEY_E, 4),
        (KeyCode::KEY_F, 5),
        (KeyCode::KEY_G, 7),
        (KeyCode::KEY_A, 9),
        (KeyCode::KEY_V, 11),
        (KeyCode::KEY_X, 1),
        (KeyCode::KEY_R, 3),
        (KeyCode::KEY_T, 6),
        (KeyCode::KEY_Y, 8),
        (KeyCode::KEY_W, 10),
    ];
    pairs.iter().map(|(k, v)| (k.code(), *v)).collect()
}

#[derive(Clone, Copy, Default, Serialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Major,
    Minor,
    Dom7,
    Min7,
    MinMaj7,
}

impl Quality {
    fn suffix(self) -> &'static str {
        match self {
            Quality::Major => "",
            Quality::Minor => "m",
            Quality::Dom7 => "7",
            Quality::Min7 => "m7",
            Quality::MinMaj7 => "mM7",
        }
    }
    fn intervals(self) -> &'static [u8] {
        match self {
            Quality::Major => &[0, 4, 7],
            Quality::Minor => &[0, 3, 7],
            Quality::Dom7 => &[0, 4, 7, 10],
            Quality::Min7 => &[0, 3, 7, 10],
            Quality::MinMaj7 => &[0, 3, 7, 11],
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Chord {
    pub name: String,
    pub quality: Quality,
    pub root_pitch_class: u8,
    pub notes: Vec<u8>,
}

#[derive(Clone, Serialize)]
pub struct ChordState {
    pub chord: Option<Chord>,
    pub modifiers: Modifiers,
}

pub struct Chords {
    root_map: SharedChordMap,
    held_stack: Vec<KeyCode>,
    held_set: HashSet<KeyCode>,
    modifiers: Modifiers,
    sounding: Vec<u8>,
}

impl Chords {
    pub fn new(root_map: SharedChordMap) -> Self {
        Chords {
            root_map,
            held_stack: Vec::new(),
            held_set: HashSet::new(),
            modifiers: Modifiers::default(),
            sounding: Vec::new(),
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    fn root_pitch_class(&self, key: KeyCode) -> Option<u8> {
        self.root_map.read().ok()?.get(&key.code()).copied()
    }

    pub fn current_chord(&self) -> Option<Chord> {
        let &root_key = self.held_stack.last()?;
        let root_pc = self.root_pitch_class(root_key)?;
        Some(build_chord(root_pc, quality_for(self.modifiers)))
    }

    pub fn state(&self) -> ChordState {
        ChordState {
            chord: self.current_chord(),
            modifiers: self.modifiers,
        }
    }

    pub fn handle(&mut self, key: KeyCode, pressed: bool, engine: &Engine) -> bool {
        let changed_mod = self.update_modifier(key, pressed);
        let changed_root = self.update_root(key, pressed);
        if !changed_mod && !changed_root {
            return false;
        }
        self.resound(engine);
        true
    }

    /// Re-resolve the current chord against the latest map. Called after the
    /// user remaps a key while a chord is held, so the audio follows.
    pub fn refresh(&mut self, engine: &Engine) {
        self.resound(engine);
    }

    pub fn release_all(&mut self, engine: &Engine) {
        for &n in &self.sounding {
            engine.note_off(CHORD_CHANNEL, n);
        }
        self.sounding.clear();
        self.held_stack.clear();
        self.held_set.clear();
    }

    fn update_modifier(&mut self, key: KeyCode, pressed: bool) -> bool {
        let target = match key {
            KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => &mut self.modifiers.ctrl,
            KeyCode::KEY_LEFTALT | KeyCode::KEY_RIGHTALT => &mut self.modifiers.alt,
            KeyCode::KEY_LEFTSHIFT => &mut self.modifiers.shift,
            _ => return false,
        };
        if *target == pressed {
            return false;
        }
        *target = pressed;
        true
    }

    fn update_root(&mut self, key: KeyCode, pressed: bool) -> bool {
        if self.root_pitch_class(key).is_none() {
            return false;
        }
        if pressed {
            if self.held_set.insert(key) {
                self.held_stack.push(key);
                return true;
            }
            false
        } else if self.held_set.remove(&key) {
            self.held_stack.retain(|&k| k != key);
            true
        } else {
            false
        }
    }

    fn resound(&mut self, engine: &Engine) {
        let target: Vec<u8> = self.current_chord().map(|c| c.notes).unwrap_or_default();
        for &n in &self.sounding {
            if !target.contains(&n) {
                engine.note_off(CHORD_CHANNEL, n);
            }
        }
        for &n in &target {
            if !self.sounding.contains(&n) {
                engine.note_on(CHORD_CHANNEL, n, CHORD_VELOCITY);
            }
        }
        self.sounding = target;
    }
}

fn quality_for(m: Modifiers) -> Quality {
    if m.ctrl {
        if !m.shift {
            return Quality::Minor;
        }
        if !m.alt {
            return Quality::Min7;
        }
        return Quality::MinMaj7;
    }
    if m.alt && m.shift {
        return Quality::Min7;
    }
    if m.alt {
        return Quality::Dom7;
    }
    Quality::Major
}

fn build_chord(root_pc: u8, quality: Quality) -> Chord {
    let root_midi = ROOT_BASE + root_pc;
    let notes: Vec<u8> = quality
        .intervals()
        .iter()
        .map(|i| root_midi + i)
        .collect();
    let name = format!("{}{}", pitch_class_name(root_pc), quality.suffix());
    Chord {
        name,
        quality,
        root_pitch_class: root_pc,
        notes,
    }
}

fn pitch_class_name(pc: u8) -> &'static str {
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][pc as usize]
}
