//! Kenagen — a Yamaha-style keyboard emulator played on a computer keyboard.
//!
//! Currently the playable right hand: the melody zone. Left-hand chords, the
//! metronome, and auto-accompaniment styles come next.

use anyhow::Result;
use evdev::KeyCode;

use kenagen::engine::Engine;
use kenagen::input::{self, KeyEvent};
use kenagen::melody::Melody;

fn main() -> Result<()> {
    println!("kenagen");

    let engine = Engine::new()?;
    let mut melody = Melody::new();
    let events = input::keyboards()?;

    println!();
    println!("  melody   H J K L ; '   N M , . /     (C C# D D# E F  F# G G# A A#)");
    println!("  octave   ]  up      \\  down");
    println!("  quit     ESC");
    println!();

    for event in events {
        match event {
            KeyEvent::Pressed(KeyCode::KEY_ESC) => break,
            KeyEvent::Pressed(key) => melody.handle(key, true, &engine),
            KeyEvent::Released(key) => melody.handle(key, false, &engine),
        }
    }

    println!("bye.");
    Ok(())
}
