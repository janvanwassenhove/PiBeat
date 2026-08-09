//! Write the canonical SuperCollider SynthDef compilation script to disk.
//!
//! The SynthDef sources live in `audio::sc_synthdefs` as the single source of
//! truth. `compile_synthdefs.ps1` used to run a hand-maintained
//! `sc-bundle/synthdefs/compile_all.scd`, which meant SynthDef changes made in
//! Rust never reached the pre-compiled bundle that ships to users without
//! sclang. This binary regenerates that script so the two cannot drift.
//!
//! ```text
//! cargo run --example gen_synthdefs -- src-tauri/sc-bundle/synthdefs
//! ```
//!
//! Deliberately an example and not `src/bin/`. Anything under `src/bin/` is a
//! shipped binary, and Tauri's bundler picks a binary from that set — v0.3.0
//! went out with this 2 MB helper installed in place of PiBeat itself. A
//! dev-only code generator has no business in the release bundle.
//!
//! The argument is the directory the compiled `.scsyndef` files should be
//! written to; `compile_all.scd` is written into the same directory.

use std::path::PathBuf;

fn main() {
    let out_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("sc-bundle/synthdefs"));

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("Cannot create {}: {e}", out_dir.display());
        std::process::exit(1);
    }

    let script = sonic_daw_lib::audio::sc_synthdefs::generate_synthdef_script(&out_dir);
    let script_path = out_dir.join("compile_all.scd");
    if let Err(e) = std::fs::write(&script_path, script) {
        eprintln!("Cannot write {}: {e}", script_path.display());
        std::process::exit(1);
    }

    println!(
        "Wrote {} (SynthDef version {})",
        script_path.display(),
        sonic_daw_lib::audio::sc_synthdefs::SYNTHDEF_VERSION
    );
}
