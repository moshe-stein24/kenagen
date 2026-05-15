//! Raw keyboard input via evdev.
//!
//! Reading the Linux input devices directly gives true key-down / key-up events
//! with no OS autorepeat interference — essential for an instrument, where a
//! held key must sustain a note and releasing it must stop the note.
//!
//! Every connected keyboard is opened and *grabbed* (exclusive access), so
//! keystrokes don't leak into the terminal while playing. The grab is released
//! automatically when the process exits.

use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::{anyhow, Result};
use evdev::{Device, EventSummary, KeyCode};

/// A key transition from some keyboard. Autorepeat is filtered out upstream.
pub enum KeyEvent {
    Pressed(KeyCode),
    Released(KeyCode),
}

/// Open and grab every keyboard, spawning a reader thread per device. Returns a
/// receiver that yields `KeyEvent`s from all of them, merged.
pub fn keyboards() -> Result<Receiver<KeyEvent>> {
    let devices: Vec<(_, Device)> = evdev::enumerate()
        .filter(|(_, device)| is_keyboard(device))
        .collect();
    if devices.is_empty() {
        return Err(anyhow!(
            "no keyboard input devices found — is this user in the `input` group?"
        ));
    }

    let (tx, rx) = mpsc::channel();
    for (path, mut device) in devices {
        let name = device.name().unwrap_or("unknown").to_string();
        match device.grab() {
            Ok(()) => println!("listening: {name}"),
            Err(e) => println!("listening: {name} (not grabbed: {e})"),
        }

        let tx = tx.clone();
        thread::spawn(move || {
            loop {
                let events = match device.fetch_events() {
                    Ok(events) => events,
                    Err(e) => {
                        eprintln!("input device {} stopped: {e}", path.display());
                        return;
                    }
                };
                for event in events {
                    // evdev value: 1 = press, 0 = release, 2 = autorepeat (ignored).
                    let EventSummary::Key(_, code, value) = event.destructure() else {
                        continue;
                    };
                    let msg = match value {
                        1 => KeyEvent::Pressed(code),
                        0 => KeyEvent::Released(code),
                        _ => continue,
                    };
                    if tx.send(msg).is_err() {
                        return; // receiver dropped — instrument is shutting down
                    }
                }
            }
        });
    }
    Ok(rx)
}

/// A real keyboard: supports the letter keys and Escape (rules out mice,
/// power buttons, and other input devices).
fn is_keyboard(device: &Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::KEY_H) && keys.contains(KeyCode::KEY_ESC))
}
