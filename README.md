# kenagen

A Yamaha PSR-style keyboard emulator played on a standard computer keyboard —
left hand plays chords, right hand plays melody, with auto-accompaniment styles.
Built in Rust for Linux (Ubuntu), with a Tauri 2 GUI.

Status: **early development** — engine + melody + chord input + GUI scaffolding.

## Architecture

| Module      | Role                                                        |
|-------------|-------------------------------------------------------------|
| `engine`    | CPAL audio stream + FluidSynth; `note_on` / `note_off`      |
| `input`     | evdev raw keyboard reader (grabs all keyboards)             |
| `melody`    | single key → semitone → note (right hand)                   |
| `chords`    | single-key root + Ctrl/Alt/Shift quality (left hand)        |
| `metronome` | BPM beat clock                                              |
| `styles`    | auto-accompaniment rhythm patterns, driven by the metronome |
| `voices`    | instrument voice / soundfont preset selection               |

Layout: `src-tauri/` is the Rust crate (instrument + Tauri backend). `ui/` is
the plain HTML/CSS/JS frontend. Tauri 2 wires them together.

## Display

Designed and tested for **1920×1080 (FHD) on a 15" laptop** — the keyboard
proportions, font sizes, and panel widths assume FHD. **Larger 4K-and-up
screens are expected to work** (the layout scales by viewport units). Smaller
resolutions (1366×768 and below) are not yet supported but will be a target
later — for now expect cramping or overflow.

The app launches fullscreen; press `Esc` to bring up the quit dialog.

## Building

System dependencies (one-time):

```
sudo apt install libfluidsynth-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libgtk-3-dev libsoup-3.0-dev
```

Tauri 2 CLI:

```
cargo install tauri-cli --version "^2"
```

Run:

```
cargo tauri dev      # from kenagen/
```

Release build:

```
cargo tauri build --no-bundle
```

The engine loads a General MIDI soundfont — by default
`/usr/share/sounds/sf2/default-GM.sf2`. Override with the `KENAGEN_SOUNDFONT`
environment variable.

The user must be in the `input` group so evdev can grab keyboards.
