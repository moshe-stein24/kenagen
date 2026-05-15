//! Persistent user settings — loaded on startup, saved when the user confirms
//! "Save settings? Yes" from the Function menu.
//!
//! Files live in the standard XDG config directory:
//!   $XDG_CONFIG_HOME/kenagen/settings.json   (volumes + sensitivity)
//!   $XDG_CONFIG_HOME/kenagen/banks.json      (8 registration slots)
//!
//! Factory reset deletes both files. Banks store local per-slot choices
//! (drum kit / voice / style match) and are intentionally low-cost to lose.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::chords;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Volumes {
    pub melody: u8,
    pub chord: u8,
    pub styles: u8,
    pub master: u8,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Sensitivity {
    /// Pitch-bend wheel range in semitones (1..=12). 2 is the MIDI default.
    pub pitch_bend_semitones: u8,
}

/// One chord-key mapping (key code → pitch class). Stored as a list of pairs
/// because JSON object keys are strings, and we'd rather keep `u16` semantics.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ChordKeyMapping {
    pub key_code: u16,
    pub pitch_class: u8,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub volumes: Volumes,
    pub sensitivity: Sensitivity,
    /// User-configurable assignment of chord-section keys to pitch classes.
    /// Missing in older saved files → falls back to the factory layout.
    #[serde(default = "default_chord_mappings")]
    pub chord_map: Vec<ChordKeyMapping>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volumes: Volumes {
                melody: 100,
                chord: 55,
                styles: 90,
                master: 60,
            },
            sensitivity: Sensitivity {
                pitch_bend_semitones: 2,
            },
            chord_map: default_chord_mappings(),
        }
    }
}

pub fn default_chord_mappings() -> Vec<ChordKeyMapping> {
    chords::default_root_map()
        .into_iter()
        .map(|(key_code, pitch_class)| ChordKeyMapping {
            key_code,
            pitch_class,
        })
        .collect()
}

/// Materialize a chord map into the HashMap form the chord engine uses.
pub fn chord_map_to_hashmap(mappings: &[ChordKeyMapping]) -> HashMap<u16, u8> {
    mappings
        .iter()
        .map(|m| (m.key_code, m.pitch_class))
        .collect()
}

pub fn chord_map_from_hashmap(map: &HashMap<u16, u8>) -> Vec<ChordKeyMapping> {
    map.iter()
        .map(|(&key_code, &pitch_class)| ChordKeyMapping {
            key_code,
            pitch_class,
        })
        .collect()
}

/// A single registration slot. All fields are optional — an empty slot is just
/// `Bank::default()`. None of these are wired up yet, but the schema is here
/// so the file format is stable from day one.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Bank {
    pub voice: Option<String>,
    pub style: Option<String>,
    pub drum_kit: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Banks {
    pub slots: [Bank; 8],
}

impl Default for Banks {
    fn default() -> Self {
        Self {
            slots: Default::default(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("kenagen");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("kenagen");
    }
    PathBuf::from(".kenagen")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn banks_path() -> PathBuf {
    config_dir().join("banks.json")
}

pub fn load_settings() -> Settings {
    match fs::read_to_string(settings_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    fs::write(dir.join("settings.json"), json)
}

pub fn delete_settings() -> io::Result<()> {
    let path = settings_path();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn load_banks() -> Banks {
    match fs::read_to_string(banks_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Banks::default(),
    }
}

pub fn save_banks(banks: &Banks) -> io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(banks)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    fs::write(dir.join("banks.json"), json)
}

pub fn delete_banks() -> io::Result<()> {
    let path = banks_path();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}
