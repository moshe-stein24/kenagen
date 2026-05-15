//! Melody input — the right hand. Each key is one chromatic semitone; holding a
//! key sustains the note and releasing it stops the note.
//!
//! Layout (see data/melody/status.md):
//!   home row    H J K L ; '   = C  C# D  D# E  F
//!   bottom row  N M , . /      = F# G  G# A  A#
//!   ]  octave up      \  octave down
//! H sounds C4 (MIDI 60) at octave shift 0.

use std::collections::HashMap;

use evdev::KeyCode;

use crate::engine::Engine;

/// MIDI channel the melody plays on.
const MELODY_CHANNEL: u8 = 0;
/// MIDI note for the lowest melody key (H) at octave shift 0 — middle C.
const BASE_NOTE: i32 = 60;
/// Note-on velocity for melody keys.
const VELOCITY: u8 = 100;
/// How far the octave shift can travel in either direction.
const OCTAVE_LIMIT: i32 = 3;

/// What just happened in the melody handler. The caller forwards this to the
/// UI; audio is already side-effecting through `Engine`.
pub enum MelodyEvent {
    NoteOn { midi: u8, keycode: u16 },
    NoteOff { midi: u8, keycode: u16 },
    OctaveShift(i32),
}

pub struct Melody {
    octave_shift: i32,
    /// Physical key → the MIDI note it started. Lets release stop the exact note
    /// that was started, even if the octave shifted while the key was held.
    sounding: HashMap<KeyCode, u8>,
}

impl Melody {
    pub fn new() -> Self {
        Melody {
            octave_shift: 0,
            sounding: HashMap::new(),
        }
    }

    pub fn octave_shift(&self) -> i32 {
        self.octave_shift
    }

    /// Route a key transition. Note keys play/stop notes; `]` and `\` shift the
    /// octave; anything else is ignored. Returns what happened so the caller
    /// can forward it to the UI.
    pub fn handle(
        &mut self,
        key: KeyCode,
        pressed: bool,
        engine: &Engine,
    ) -> Option<MelodyEvent> {
        match key {
            KeyCode::KEY_RIGHTBRACE if pressed => self.shift_octave(1),
            KeyCode::KEY_BACKSLASH if pressed => self.shift_octave(-1),
            _ => {
                let semitone = semitone_for(key)?;
                if pressed {
                    self.press(key, semitone, engine)
                } else {
                    self.release(key, engine)
                }
            }
        }
    }

    fn press(&mut self, key: KeyCode, semitone: i32, engine: &Engine) -> Option<MelodyEvent> {
        // evdev gives a clean press/release pair, but guard against a stray
        // double-press so we never leak a note that release can't stop.
        if self.sounding.contains_key(&key) {
            return None;
        }
        let note = (BASE_NOTE + semitone + 12 * self.octave_shift).clamp(0, 127) as u8;
        engine.note_on(MELODY_CHANNEL, note, VELOCITY);
        self.sounding.insert(key, note);
        Some(MelodyEvent::NoteOn {
            midi: note,
            keycode: key.code(),
        })
    }

    fn release(&mut self, key: KeyCode, engine: &Engine) -> Option<MelodyEvent> {
        let note = self.sounding.remove(&key)?;
        engine.note_off(MELODY_CHANNEL, note);
        Some(MelodyEvent::NoteOff {
            midi: note,
            keycode: key.code(),
        })
    }

    /// Send note-off for every currently sounding note and clear our tracking.
    /// Returns the MIDI notes that were silenced so the caller can update the
    /// UI. Used when entering the quit dialog so notes don't sustain forever.
    pub fn release_all(&mut self, engine: &Engine) -> Vec<u8> {
        let notes: Vec<u8> = self.sounding.values().copied().collect();
        for &note in &notes {
            engine.note_off(MELODY_CHANNEL, note);
        }
        self.sounding.clear();
        notes
    }

    fn shift_octave(&mut self, delta: i32) -> Option<MelodyEvent> {
        let shifted = (self.octave_shift + delta).clamp(-OCTAVE_LIMIT, OCTAVE_LIMIT);
        if shifted == self.octave_shift {
            return None;
        }
        self.octave_shift = shifted;
        Some(MelodyEvent::OctaveShift(self.octave_shift))
    }
}

impl Default for Melody {
    fn default() -> Self {
        Self::new()
    }
}

/// Semitone offset above the base note for a melody key, or `None` if the key
/// isn't part of the melody zone.
fn semitone_for(key: KeyCode) -> Option<i32> {
    Some(match key {
        // Upper row (home row, right half) — unchanged, 6 notes
        KeyCode::KEY_H => 0,          // C
        KeyCode::KEY_J => 1,          // C#
        KeyCode::KEY_K => 2,          // D
        KeyCode::KEY_L => 3,          // D#
        KeyCode::KEY_SEMICOLON => 4,  // E
        KeyCode::KEY_APOSTROPHE => 5, // F
        // Lower row (bottom row, right half) — shifted one position right so
        // the freed B key (from the chord section) joins as F#. Six notes.
        KeyCode::KEY_B => 6,          // F#  (new — joins the melody family)
        KeyCode::KEY_N => 7,          // G
        KeyCode::KEY_M => 8,          // G#
        KeyCode::KEY_COMMA => 9,      // A
        KeyCode::KEY_DOT => 10,       // A#
        KeyCode::KEY_SLASH => 11,     // B (Si)
        _ => return None,
    })
}
