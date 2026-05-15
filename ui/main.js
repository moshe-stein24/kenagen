// Kenagen frontend. Drives the melody display, chord pill, mod + pitch
// wheels, settings panel (with file-persistent settings + factory reset), and
// the quit / save dialogs.

const { listen } = window.__TAURI__.event;
const { invoke } = window.__TAURI__.core;

const heldEl = document.getElementById("held-notes");
const octaveEl = document.getElementById("octave-shift");
const statusEl = document.getElementById("status");
const pianoEl = document.getElementById("piano");
const chordSymbolEl = document.getElementById("chord-symbol");
const chordBgEl = document.getElementById("chord-symbol-bg");

// Function menu / settings view.
const viewDefault = document.getElementById("view-default");
const viewSettings = document.getElementById("view-settings");
const functionBtn = document.getElementById("function-btn");
const VOLUME_CHANNELS = ["melody", "chord", "styles", "master"];

// evdev key codes for the 16 chord-section keys, in keyboard layout order.
const CHORD_SECTION_LAYOUT = [
  { row: "top",    keys: [["Q",16],["W",17],["E",18],["R",19],["T",20],["Y",21]] },
  { row: "home",   keys: [["A",30],["S",31],["D",32],["F",33],["G",34]] },
  { row: "bottom", keys: [["Z",44],["X",45],["C",46],["V",47],["B",48]] },
];
const PITCH_NAMES = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
let chordKeyMap = new Map(); // u16 keycode → u8 pitch class
const sensPitchEl = document.getElementById("sens-pitch");
const sensPitchValEl = document.getElementById("sens-pitch-val");
const resetBtn = document.getElementById("reset-btn");

// Dialogs.
const saveBackdrop = document.getElementById("save-backdrop");
const saveYesBtn = document.getElementById("save-yes");
const saveNoBtn = document.getElementById("save-no");
const saveCancelBtn = document.getElementById("save-cancel");
const resetBackdrop = document.getElementById("reset-backdrop");
const resetYesBtn = document.getElementById("reset-yes");
const resetNoBtn = document.getElementById("reset-no");

// Quit dialog.
const backdropEl = document.getElementById("dialog-backdrop");
const confirmEl = document.getElementById("dialog-confirm");
const countdownEl = document.getElementById("dialog-countdown");
const countdownNEl = document.getElementById("countdown-n");
const btnYes = document.getElementById("btn-yes");
const btnNo = document.getElementById("btn-no");
const btnCancel1 = document.getElementById("btn-cancel-1");
const btnCancel = document.getElementById("btn-cancel");

// Wheels.
const modWheelEl = document.getElementById("mod-wheel");
const modHandleEl = document.getElementById("mod-wheel-handle");
const pitchWheelEl = document.getElementById("pitch-wheel");
const pitchHandleEl = document.getElementById("pitch-wheel-handle");

const held = new Map();
const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const BLACK_PITCH_CLASSES = new Set([1, 3, 6, 8, 10]);
const QUALITY_CLASSES = ["major", "minor", "dom7", "min7", "minmaj7"];

function noteName(midi) {
  return NOTE_NAMES[midi % 12] + (Math.floor(midi / 12) - 1);
}

/* ---------- piano rendering ---------- */
const PIANO_LOW = 36;
const PIANO_HIGH = 72;

function buildOctaveKeyboard(container, lowMidi, highMidi, { labelC = true } = {}) {
  let whiteCount = 0;
  for (let m = lowMidi; m <= highMidi; m++) {
    if (!BLACK_PITCH_CLASSES.has(m % 12)) whiteCount++;
  }
  container.style.setProperty("--white-count", whiteCount);

  const whiteByMidi = new Map();
  let whiteIndex = 0;
  for (let m = lowMidi; m <= highMidi; m++) {
    if (BLACK_PITCH_CLASSES.has(m % 12)) continue;
    const el = document.createElement("div");
    el.className = "key white";
    el.dataset.midi = m;
    container.appendChild(el);
    whiteByMidi.set(m, whiteIndex);
    if (labelC && m % 12 === 0) {
      const lbl = document.createElement("div");
      lbl.className = "octave-label";
      lbl.textContent = noteName(m);
      lbl.style.left = `${((whiteIndex + 0.5) / whiteCount) * 100}%`;
      container.appendChild(lbl);
    }
    whiteIndex++;
  }
  for (let m = lowMidi; m <= highMidi; m++) {
    if (!BLACK_PITCH_CLASSES.has(m % 12)) continue;
    const lowerWhite = whiteByMidi.get(m - 1);
    if (lowerWhite === undefined) continue;
    const el = document.createElement("div");
    el.className = "key black";
    el.dataset.midi = m;
    el.style.left = `${((lowerWhite + 1) / whiteCount) * 100}%`;
    container.appendChild(el);
  }
}

function setPianoKeyHeld(midi, on) {
  const el = pianoEl.querySelector(`.key[data-midi="${midi}"]`);
  if (el) el.classList.toggle("held", on);
}

function renderHeld() {
  if (held.size === 0) {
    heldEl.textContent = "—";
    heldEl.classList.add("empty");
    return;
  }
  const sorted = [...held.keys()].sort((a, b) => a - b);
  heldEl.textContent = sorted.map(noteName).join(" ");
  heldEl.classList.remove("empty");
}

function applyChordState(state) {
  QUALITY_CLASSES.forEach((q) => chordBgEl.classList.remove(q));
  if (state.chord) {
    chordBgEl.classList.add(state.chord.quality);
    chordSymbolEl.textContent = state.chord.name;
    chordSymbolEl.classList.remove("empty");
  } else {
    chordSymbolEl.textContent = "—";
    chordSymbolEl.classList.add("empty");
  }
}

/* ---------- wheels ---------- */
// Position a wheel handle so its bottom sits at `pct` (0..1) of the available
// travel inside the track. Uses measured track/handle heights so the handle
// never overshoots the track edges.
function placeHandle(wheelEl, handleEl, pct) {
  const trackEl = wheelEl.querySelector(".mod-wheel-track");
  if (!trackEl) return;
  const trackH = trackEl.offsetHeight;
  const handleH = handleEl.offsetHeight || 14;
  const range = Math.max(0, trackH - handleH);
  const p = Math.max(0, Math.min(1, pct));
  handleEl.style.bottom = `${p * range}px`;
  handleEl.style.transform = "";
}

// --- modulation wheel: click + drag, holds position ---
let modValue = 0;
let modDragging = false;

function setModFromY(clientY) {
  const trackRect = modWheelEl
    .querySelector(".mod-wheel-track")
    .getBoundingClientRect();
  let pct = 1 - (clientY - trackRect.top) / trackRect.height;
  pct = Math.max(0, Math.min(1, pct));
  const newValue = Math.round(pct * 127);
  if (newValue === modValue) return;
  modValue = newValue;
  placeHandle(modWheelEl, modHandleEl, modValue / 127);
  invoke("set_modulation", { value: modValue }).catch(() => {});
}

modWheelEl.addEventListener("mousedown", (e) => {
  modDragging = true;
  setModFromY(e.clientY);
  e.preventDefault();
});
window.addEventListener("mousemove", (e) => {
  if (modDragging) setModFromY(e.clientY);
});
window.addEventListener("mouseup", () => {
  modDragging = false;
});

// --- pitch wheel: mouse scroll wheel, spring-back to center ---
// --- modulation override: holding the left mouse button reroutes the wheel
//     to modulation instead of pitch bend.
let pitchValue = 0; // -8192..+8191, 0 = center
let pitchSpringTimer = null;
let pitchSpringFrame = null;
const PITCH_STEP = 900;          // bend units per scroll tick (out of 8192)
const PITCH_SPRING_DELAY = 450;  // ms idle before snapping back to center
const PITCH_SPRING_DURATION = 220;
const MOD_STEP = 8;              // modulation units per scroll tick (out of 127)
let leftMouseHeld = false;

function applyPitch() {
  const pct = (pitchValue + 8192) / 16383;
  placeHandle(pitchWheelEl, pitchHandleEl, pct);
  invoke("set_pitch_bend", { value: pitchValue }).catch(() => {});
}

function cancelPitchSpring() {
  if (pitchSpringTimer !== null) {
    clearTimeout(pitchSpringTimer);
    pitchSpringTimer = null;
  }
  if (pitchSpringFrame !== null) {
    cancelAnimationFrame(pitchSpringFrame);
    pitchSpringFrame = null;
  }
}

function schedulePitchSpring() {
  pitchSpringTimer = setTimeout(() => {
    pitchSpringTimer = null;
    const start = pitchValue;
    const t0 = performance.now();
    const step = (now) => {
      const t = Math.min(1, (now - t0) / PITCH_SPRING_DURATION);
      pitchValue = Math.round(start * (1 - t));
      applyPitch();
      if (t < 1) {
        pitchSpringFrame = requestAnimationFrame(step);
      } else {
        pitchSpringFrame = null;
      }
    };
    pitchSpringFrame = requestAnimationFrame(step);
  }, PITCH_SPRING_DELAY);
}

// Track left mouse button globally so the wheel handler can choose which
// performance control (pitch vs mod) to drive.
window.addEventListener("mousedown", (e) => {
  if (e.button === 0) leftMouseHeld = true;
});
window.addEventListener("mouseup", (e) => {
  if (e.button === 0) leftMouseHeld = false;
});

// Wheel events fire globally:
//   • plain scroll          → pitch bend (springs back to center)
//   • left-button + scroll  → modulation (stays where you leave it)
window.addEventListener(
  "wheel",
  (e) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? -1 : 1;
    if (leftMouseHeld) {
      const next = Math.max(0, Math.min(127, modValue + delta * MOD_STEP));
      if (next === modValue) return;
      modValue = next;
      placeHandle(modWheelEl, modHandleEl, modValue / 127);
      invoke("set_modulation", { value: modValue }).catch(() => {});
    } else {
      cancelPitchSpring();
      pitchValue = Math.max(-8192, Math.min(8191, pitchValue + delta * PITCH_STEP));
      applyPitch();
      schedulePitchSpring();
    }
  },
  { passive: false }
);

/* ---------- function menu + save-settings flow ---------- */
let inSettings = false;
let snapshotSettings = null;

function readSettings() {
  const volumes = {};
  VOLUME_CHANNELS.forEach((c) => {
    volumes[c] = Number(document.getElementById(`vol-${c}`).value);
  });
  const chord_map = [...chordKeyMap.entries()].map(([key_code, pitch_class]) => ({
    key_code,
    pitch_class,
  }));
  return {
    volumes,
    sensitivity: { pitch_bend_semitones: Number(sensPitchEl.value) },
    chord_map,
  };
}

function applySettings(s) {
  if (s.volumes) {
    VOLUME_CHANNELS.forEach((c) => {
      const slider = document.getElementById(`vol-${c}`);
      const val = document.getElementById(`vol-${c}-val`);
      if (s.volumes[c] === undefined || !slider) return;
      slider.value = s.volumes[c];
      val.textContent = `${s.volumes[c]}%`;
      invoke("set_volume", { channel: c, percent: s.volumes[c] }).catch(() => {});
    });
  }
  if (s.sensitivity && s.sensitivity.pitch_bend_semitones !== undefined) {
    const st = s.sensitivity.pitch_bend_semitones;
    sensPitchEl.value = st;
    sensPitchValEl.textContent = `${st} st`;
    invoke("set_pitch_bend_range", { semitones: st }).catch(() => {});
  }
  if (Array.isArray(s.chord_map)) {
    applyChordMap(s.chord_map);
  }
}

function openSettings() {
  inSettings = true;
  snapshotSettings = readSettings();
  viewDefault.hidden = true;
  viewSettings.hidden = false;
  functionBtn.classList.add("active");
  functionBtn.textContent = "← Back";
}

function closeSettings(commit) {
  inSettings = false;
  if (!commit && snapshotSettings) {
    applySettings(snapshotSettings);
    // applySettings re-renders the chord map locally but doesn't push it to
    // Rust — do that explicitly so the live engine matches the snapshot.
    pushChordMapToRust(snapshotSettings.chord_map || []);
  }
  snapshotSettings = null;
  viewDefault.hidden = false;
  viewSettings.hidden = true;
  functionBtn.classList.remove("active");
  functionBtn.textContent = "⚙ Function";
}

functionBtn.addEventListener("click", () => {
  if (!inSettings) openSettings();
  else openSaveDialog();
});

VOLUME_CHANNELS.forEach((channel) => {
  const slider = document.getElementById(`vol-${channel}`);
  const val = document.getElementById(`vol-${channel}-val`);
  if (!slider) return;
  slider.addEventListener("input", () => {
    const percent = Number(slider.value);
    val.textContent = `${percent}%`;
    invoke("set_volume", { channel, percent }).catch(() => {});
  });
});

sensPitchEl.addEventListener("input", () => {
  const st = Number(sensPitchEl.value);
  sensPitchValEl.textContent = `${st} st`;
  invoke("set_pitch_bend_range", { semitones: st }).catch(() => {});
});

/* ---------- chord-map mini-keyboard ---------- */
function buildChordMapGrid() {
  const grid = document.getElementById("chord-map-grid");
  grid.innerHTML = "";
  for (const row of CHORD_SECTION_LAYOUT) {
    const rowEl = document.createElement("div");
    rowEl.className = `chord-map-row row-${row.row}`;
    for (const [letter, code] of row.keys) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "chord-map-key";
      btn.dataset.code = String(code);
      btn.innerHTML =
        `<span class="chord-map-key-letter">${letter}</span>` +
        `<span class="chord-map-key-chord unassigned">—</span>`;
      btn.addEventListener("click", () => cycleChordKey(code));
      rowEl.appendChild(btn);
    }
    grid.appendChild(rowEl);
  }
  refreshChordMapGrid();
}

function refreshChordMapGrid() {
  document.querySelectorAll(".chord-map-key").forEach((btn) => {
    const code = Number(btn.dataset.code);
    const pc = chordKeyMap.get(code);
    const chordEl = btn.querySelector(".chord-map-key-chord");
    if (pc === undefined) {
      chordEl.textContent = "—";
      chordEl.classList.add("unassigned");
    } else {
      chordEl.textContent = PITCH_NAMES[pc];
      chordEl.classList.remove("unassigned");
    }
  });
}

// Click cycle: unassigned → C → C# → ... → B → unassigned.
function cycleChordKey(code) {
  const current = chordKeyMap.get(code);
  let next;
  if (current === undefined) next = 0;
  else if (current >= 11) next = null;
  else next = current + 1;

  if (next === null) chordKeyMap.delete(code);
  else chordKeyMap.set(code, next);
  refreshChordMapGrid();
  invoke("set_chord_key", { keyCode: code, pitchClass: next }).catch(() => {});
}

function applyChordMap(mappings) {
  chordKeyMap = new Map();
  if (Array.isArray(mappings)) {
    for (const m of mappings) {
      if (m && typeof m.key_code === "number" && typeof m.pitch_class === "number") {
        chordKeyMap.set(m.key_code, m.pitch_class);
      }
    }
  }
  refreshChordMapGrid();
}

// Push every entry of `target` to Rust so the live map matches (used when the
// user clicks "Save No" to revert their changes during this settings session).
function pushChordMapToRust(target) {
  const seen = new Set();
  for (const m of target) {
    invoke("set_chord_key", { keyCode: m.key_code, pitchClass: m.pitch_class }).catch(() => {});
    seen.add(m.key_code);
  }
  for (const code of chordKeyMap.keys()) {
    if (!seen.has(code)) {
      invoke("set_chord_key", { keyCode: code, pitchClass: null }).catch(() => {});
    }
  }
}

/* ---------- save-settings dialog ---------- */
function openSaveDialog() {
  saveBackdrop.hidden = false;
  saveYesBtn.focus();
}
function closeSaveDialog() {
  saveBackdrop.hidden = true;
}
saveYesBtn.addEventListener("click", () => {
  closeSaveDialog();
  const newSettings = readSettings();
  invoke("save_settings", { newSettings }).catch((err) => {
    statusEl.textContent = `save failed: ${err}`;
  });
  closeSettings(true);
});
saveNoBtn.addEventListener("click", () => {
  closeSaveDialog();
  closeSettings(false); // revert to snapshot
});
saveCancelBtn.addEventListener("click", () => {
  closeSaveDialog(); // stay in settings
});

/* ---------- factory reset ---------- */
resetBtn.addEventListener("click", () => {
  resetBackdrop.hidden = false;
  resetYesBtn.focus();
});
resetYesBtn.addEventListener("click", () => {
  resetBackdrop.hidden = true;
  invoke("factory_reset").catch((err) => {
    statusEl.textContent = `reset failed: ${err}`;
  });
  // After factory_reset, Rust emits a `settings` event; applySettings will
  // pick it up and refresh the sliders. Also drop any stale revert snapshot.
  snapshotSettings = null;
});
resetNoBtn.addEventListener("click", () => {
  resetBackdrop.hidden = true;
});

/* ---------- quit dialog state machine ---------- */
let phase = "closed";
let countdownTimer = null;
let focusIndex = 0;

function currentButtons() {
  if (phase === "confirm") return [btnYes, btnNo, btnCancel1];
  if (phase === "countdown") return [btnCancel];
  return [];
}
function applyFocus() {
  const buttons = currentButtons();
  buttons.forEach((b, i) => b.classList.toggle("focused", i === focusIndex));
  if (buttons[focusIndex]) buttons[focusIndex].focus();
}
function navigate(delta) {
  const buttons = currentButtons();
  if (buttons.length === 0) return;
  focusIndex = (focusIndex + delta + buttons.length) % buttons.length;
  applyFocus();
}
function activateFocused() {
  currentButtons()[focusIndex]?.click();
}
function openDialog() {
  phase = "confirm";
  focusIndex = 0;
  backdropEl.hidden = false;
  confirmEl.hidden = false;
  countdownEl.hidden = true;
  applyFocus();
}
function closeDialog() {
  phase = "closed";
  backdropEl.hidden = true;
  if (countdownTimer !== null) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
}
function dismiss() {
  closeDialog();
  invoke("resume_play").catch(() => {});
}
function confirmQuit() {
  phase = "countdown";
  focusIndex = 0;
  confirmEl.hidden = true;
  countdownEl.hidden = false;
  let n = 3;
  countdownNEl.textContent = n;
  applyFocus();
  countdownTimer = setInterval(() => {
    n -= 1;
    if (n <= 0) {
      clearInterval(countdownTimer);
      countdownTimer = null;
      invoke("quit_app").catch(() => {});
    } else {
      countdownNEl.textContent = n;
    }
  }, 1000);
}
function cancelQuit() { dismiss(); }

btnYes.addEventListener("click", confirmQuit);
btnNo.addEventListener("click", dismiss);
btnCancel1.addEventListener("click", dismiss);
btnCancel.addEventListener("click", cancelQuit);

function wireHoverFocus(buttons) {
  buttons.forEach((b) => {
    b.addEventListener("mouseenter", () => {
      const idx = currentButtons().indexOf(b);
      if (idx >= 0) {
        focusIndex = idx;
        applyFocus();
      }
    });
  });
}
wireHoverFocus([btnYes, btnNo, btnCancel1, btnCancel]);

/* ---------- event wiring ---------- */
listen("note", (event) => {
  const { action, midi, keycode, octave_shift } = event.payload;
  if (typeof octave_shift === "number") {
    octaveEl.textContent = octave_shift > 0 ? `+${octave_shift}` : `${octave_shift}`;
  }
  if (action === "on") {
    if (!held.has(midi)) held.set(midi, new Set());
    held.get(midi).add(keycode);
    setPianoKeyHeld(midi, true);
  } else if (action === "off") {
    const set = held.get(midi);
    if (set) {
      set.delete(keycode);
      if (set.size === 0) {
        held.delete(midi);
        setPianoKeyHeld(midi, false);
      }
    } else {
      setPianoKeyHeld(midi, false);
    }
  }
  renderHeld();
});

listen("chord", (event) => applyChordState(event.payload));

listen("pitch-key", (event) => {
  const dir = event.payload.direction;
  cancelPitchSpring();
  const delta = dir === "up" ? 1 : -1;
  pitchValue = Math.max(-8192, Math.min(8191, pitchValue + delta * PITCH_STEP));
  applyPitch();
  schedulePitchSpring();
});

listen("chord-section-key", (event) => {
  const { code, pressed } = event.payload;
  const btn = document.querySelector(`.chord-map-key[data-code="${code}"]`);
  if (btn) btn.classList.toggle("pressed", !!pressed);
});
listen("status", (event) => { statusEl.textContent = event.payload; });
listen("request-quit", () => openDialog());

listen("settings", (event) => {
  applySettings(event.payload);
});

listen("dialog-key", (event) => {
  const key = event.payload.key;
  if (key === "left" || key === "up") return navigate(-1);
  if (key === "right" || key === "down") return navigate(1);
  if (key === "enter") return activateFocused();

  if (phase === "confirm") {
    if (key === "y") confirmQuit();
    else if (key === "n" || key === "esc" || key === "c") dismiss();
  } else if (phase === "countdown") {
    if (key === "c") cancelQuit();
  }
});

/* ---------- boot ---------- */
buildOctaveKeyboard(pianoEl, PIANO_LOW, PIANO_HIGH);
buildChordMapGrid();
applyChordState({ chord: null, modifiers: { ctrl: false, alt: false, shift: false } });
statusEl.textContent = "ready — press a key";
renderHeld();

// Settle wheel handles after the keyboard panel lays out.
requestAnimationFrame(() => {
  placeHandle(modWheelEl, modHandleEl, 0);
  placeHandle(pitchWheelEl, pitchHandleEl, 0.5);
});
window.addEventListener("resize", () => {
  placeHandle(modWheelEl, modHandleEl, modValue / 127);
  placeHandle(pitchWheelEl, pitchHandleEl, (pitchValue + 8192) / 16383);
});
