//! Locate and link the system FluidSynth library (libfluidsynth-dev).

fn main() {
    pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("fluidsynth")
        .expect(
            "FluidSynth >= 2.0 not found via pkg-config.\n\
             Install it with:  sudo apt install libfluidsynth-dev",
        );
}
