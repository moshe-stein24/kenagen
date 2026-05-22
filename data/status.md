# Kenagen — project status

_Last updated: 2026-05-23_

Kenagen is a Yamaha-PSR-style keyboard emulator (Rust + Tauri 2, Linux). The
instrument is played on a PC keyboard via evdev; audio is FluidSynth.

## What works now

- **Melody (right hand)** — 12 chromatic notes, one key per note:
  `H J K L ; '` = C C# D D# E F, `B N M , . /` = F# G G# A A# B.
  `]` / `\` shift the octave.
- **Chords (left hand)** — 16 chord-section keys, runtime-remappable;
  chord quality from Ctrl / Alt / Shift modifiers.
- **Function menu** — volumes, pitch-bend range, chord-section layout
  editor, factory reset, and a **Melody on/off toggle**.
- Mod + pitch wheels, persistent settings (`~/.config/kenagen/`),
  quit-confirmation dialog, numpad-driven MKD soft buttons.
- Note readout uses Yamaha octave numbering — middle C is **C3**.

## In progress / parked

- **Flute fingering** — playing melody by key *combos* (cover and release
  holes like a recorder). An in-app version was built and then **reverted**
  (it did not work well in the app). The experiment now lives as a
  standalone POC at `../kenagen-night`: it proves multi-key combo detection
  works (`SPACE+H+J` = C3, release `J` = C#3; 6 tests pass). The Melody
  toggle exists so flute mode can later be added as the optional alternative.

## Next

1. **Styles** — a style-playback engine; import Yamaha `.sty` files;
   sections A / B / C / D with fills, intro and ending.
2. **Flute fingering** — carefully, as an optional mode, built on the
   kenagen-night POC's proven combo detection.

## Notes / gotchas

- The git repo is the `kenagen/` directory (not its parent).
- The old "only the first key is detected" bug was **software** (input
  wiring / device selection), not keyboard ghosting — the POC confirmed the
  keyboard reports multi-key combos cleanly.
- The dev machine runs `keyd` (a key remapper); input code must read the
  "keyd virtual keyboard" device, since keyd grabs the physical keyboards.
