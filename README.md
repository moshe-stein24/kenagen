# kenagen

A Yamaha PSR-style keyboard emulator played on a standard computer keyboard —
left hand plays chords, right hand plays melody, with auto-accompaniment styles.
Built in Rust for Linux (Ubuntu).

Status: **early development** — engine proof of concept.

## Architecture

| Module      | Role                                                        |
|-------------|-------------------------------------------------------------|
| `engine`    | CPAL audio stream + FluidSynth; `note_on` / `note_off`      |
| `melody`    | single key → semitone → note (right hand)                   |
| `chords`    | key combos → chord voicing (left hand)                      |
| `metronome` | BPM beat clock                                              |
| `styles`    | auto-accompaniment rhythm patterns, driven by the metronome |
| `voices`    | instrument voice / soundfont preset selection               |

## Building

Requires the FluidSynth development library:

```
sudo apt install libfluidsynth-dev
```

Then:

```
cargo run
```

The engine loads a General MIDI soundfont — by default
`/usr/share/sounds/sf2/default-GM.sf2`. Override with the `KENAGEN_SOUNDFONT`
environment variable.
