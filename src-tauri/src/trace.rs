//! Hot-path tracing gate.
//!
//! PiBeat used to `eprintln!` once per scheduled audio event. On a dense
//! piece that is tens of thousands of formatted writes through a locked
//! stderr handle, happening on the very thread that is supposed to be
//! dispatching notes on time — it shows up directly as timing jitter and
//! as a busy CPU core during playback.
//!
//! Per-event diagnostics now go through [`trace!`], which compiles to an
//! atomic load and is off unless the user asks for it:
//!
//! ```text
//! PIBEAT_TRACE=1   # or "true" / "on" / "yes"
//! ```
//!
//! Messages that happen once per run (parse summary, engine selection,
//! errors) keep using `eprintln!` — they are not on a hot path and are
//! useful in bug reports.

use std::sync::atomic::{AtomicBool, Ordering};

static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE_INITIALISED: AtomicBool = AtomicBool::new(false);

/// Read `PIBEAT_TRACE` from the environment. Called once during setup.
pub fn init_from_env() {
    let on = std::env::var("PIBEAT_TRACE")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "on" | "yes")
        })
        .unwrap_or(false);
    TRACE_ENABLED.store(on, Ordering::Relaxed);
    TRACE_INITIALISED.store(true, Ordering::Relaxed);
    if on {
        eprintln!("[trace] Verbose per-event tracing enabled (PIBEAT_TRACE)");
    }
}

/// Whether verbose per-event tracing is on.
#[inline(always)]
pub fn enabled() -> bool {
    TRACE_ENABLED.load(Ordering::Relaxed)
}

/// Turn tracing on or off at runtime (used by tests and the log panel toggle).
pub fn set_enabled(on: bool) {
    TRACE_ENABLED.store(on, Ordering::Relaxed);
    TRACE_INITIALISED.store(true, Ordering::Relaxed);
}

/// `eprintln!` that is skipped entirely unless tracing is enabled.
///
/// The arguments are not evaluated when tracing is off, so formatting a
/// per-note message costs one relaxed atomic load.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::trace::enabled() {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_is_off_by_default_and_togglable() {
        set_enabled(false);
        assert!(!enabled());
        set_enabled(true);
        assert!(enabled());
        set_enabled(false);
    }
}
