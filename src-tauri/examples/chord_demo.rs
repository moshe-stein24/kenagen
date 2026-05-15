//! Chord input demonstration.
//!
//! Plays the chords from Moshe's chord-combo design (see data/chords/status.md)
//! with a strings voice, so the chord *vocabulary* can be heard before the
//! evdev key-combo input handler exists. Not interactive yet — canned playback.
//!
//! Run with: `cargo run --example chord_demo`

use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;
use kenagen::engine::Engine;

/// GM bank 0, preset 48 = String Ensemble 1.
const STRINGS: u32 = 48;

fn main() -> Result<()> {
    println!("kenagen — chord demonstration (strings voice)");

    let engine = Engine::new()?;
    engine.select_program(0, 0, STRINGS)?;

    // Base combo S-F-B = A major. Modifier keys reshape the chord quality:
    //   CTRL  -> minor (3rd down a semitone)
    //   SHIFT -> 7th
    //   ALT   -> 7th (used when building m7-family chords)
    // Modifiers stack. Exact SHIFT-vs-ALT semantics are still being pinned
    // down — see data/chords/status.md. Voicings below are music-standard.
    let a_family = [
        ("A     (S-F-B)", &[57, 61, 64][..]),
        ("Am    (CTRL + S-F-B)", &[57, 60, 64][..]),
        ("A7    (SHIFT + S-F-B)", &[57, 61, 64, 67][..]),
        ("Am7   (CTRL ALT + S-F-B)", &[57, 60, 64, 67][..]),
        ("AmM7  (CTRL ALT SHIFT + S-F-B)", &[57, 60, 64, 68][..]),
    ];

    // Same shapes, shifted: base combo S-F-H = F.
    let f_family = [
        ("F     (S-F-H)", &[53, 57, 60][..]),
        ("Fm    (CTRL + S-F-H)", &[53, 56, 60][..]),
        ("F7    (SHIFT + S-F-H)", &[53, 57, 60, 63][..]),
        ("Fm7   (CTRL ALT + S-F-H)", &[53, 56, 60, 63][..]),
        ("FmM7  (CTRL ALT SHIFT + S-F-H)", &[53, 56, 60, 64][..]),
    ];

    println!("\nA family — base combo S-F-B:");
    for (name, notes) in a_family {
        play_chord(&engine, name, notes);
    }

    println!("\nF family — same shapes, base combo S-F-H:");
    for (name, notes) in f_family {
        play_chord(&engine, name, notes);
    }

    println!("\ndone.");
    Ok(())
}

/// Sound all the notes of a chord together, hold, then release them all.
fn play_chord(engine: &Engine, name: &str, notes: &[u8]) {
    println!("  {name:<32} {notes:?}");
    for &n in notes {
        engine.note_on(0, n, 90);
    }
    sleep(Duration::from_millis(1300));
    for &n in notes {
        engine.note_off(0, n);
    }
    sleep(Duration::from_millis(250));
}
