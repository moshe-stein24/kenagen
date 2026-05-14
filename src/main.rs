//! Kenagen — a Yamaha-style keyboard emulator played on a computer keyboard.
//!
//! This binary is currently the engine proof of concept: it brings up the audio
//! engine and plays a few test notes to confirm that `note_on` starts sound and
//! `note_off` actually stops it.

mod engine;

use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;

use engine::Engine;

fn main() -> Result<()> {
    println!("kenagen — engine proof of concept");

    let engine = Engine::new()?;

    // Test 1: a held note. Proves note_on starts sound and, crucially, that
    // note_off stops it — the teshura piano project's bug was that releasing a
    // key never stopped the note.
    println!("  middle C — hold 1s, then release (expect silence after)");
    engine.note_on(0, 60, 100);
    sleep(Duration::from_millis(1000));
    engine.note_off(0, 60);
    sleep(Duration::from_millis(700));

    // Test 2: a short C-major arpeggio.
    println!("  C major arpeggio");
    for &key in &[60u8, 64, 67, 72] {
        engine.note_on(0, key, 100);
        sleep(Duration::from_millis(220));
        engine.note_off(0, key);
    }
    sleep(Duration::from_millis(400));

    println!("done — engine works.");
    Ok(())
}
