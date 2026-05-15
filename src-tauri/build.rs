//! Locate and link the system FluidSynth library (libfluidsynth-dev), then hand
//! off to tauri-build so Tauri can wire up its resources.

fn main() {
    pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("fluidsynth")
        .expect(
            "FluidSynth >= 2.0 not found via pkg-config.\n\
             Install it with:  sudo apt install libfluidsynth-dev",
        );
    tauri_build::build();
}
