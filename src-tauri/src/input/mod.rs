//! Raw keyboard input via evdev.
//!
//! Reading the Linux input devices directly gives true key-down / key-up events
//! with no OS autorepeat interference — essential for an instrument, where a
//! held key must sustain a note and releasing it must stop the note.
//!
//! Every keyboard (built-in and external) is grabbed exclusively for the
//! lifetime of the app, so keystrokes never leak through to the desktop while
//! kenagen is running.

use std::io;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use evdev::{Device, EventSummary, KeyCode};

/// How long the reader thread sleeps when no events are pending.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// A key transition from some keyboard. Autorepeat is filtered out upstream.
pub enum KeyEvent {
    Pressed(KeyCode),
    Released(KeyCode),
}

/// Open every keyboard, grab it exclusively, and spawn a reader thread per
/// device that forwards key transitions onto the returned channel.
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
        if let Err(e) = device.grab() {
            eprintln!("input: grab {name} failed: {e}");
        }
        if let Err(e) = device.set_nonblocking(true) {
            eprintln!("input: could not set nonblocking on {name}: {e}");
        }
        println!("listening: {name}");

        let tx = tx.clone();
        thread::spawn(move || loop {
            match device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        // evdev value: 1 = press, 0 = release, 2 = autorepeat.
                        let EventSummary::Key(_, code, value) = event.destructure() else {
                            continue;
                        };
                        let msg = match value {
                            1 => KeyEvent::Pressed(code),
                            0 => KeyEvent::Released(code),
                            _ => continue,
                        };
                        if tx.send(msg).is_err() {
                            return; // receiver dropped — shutting down
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(POLL_INTERVAL);
                }
                Err(e) => {
                    eprintln!("input device {} stopped: {e}", path.display());
                    return;
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
