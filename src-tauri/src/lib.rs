pub mod audio;
#[macro_use]
pub mod trace;

use audio::engine::{AudioCommand, AudioEngine};
use audio::parser::{commands_to_audio, validate_and_parse, ParsedCommand};
use audio::recorder::Recorder;
use audio::sample::{self, SampleInfo};
use audio::sc_engine::{self, find_sc_bundle_dir, ScEngine};
use audio::synth::{Envelope, OscillatorType};
use audio::visualizer::{
    EventPublisher, FxCategory, PerformanceEvent, PerformanceSnapshot, SampleCategory,
    SynthCategory, VisualEngine, VisualEngineConfig,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

// Windows high-resolution timer (1ms precision for scheduler thread)
#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(uPeriod: u32) -> u32;
    fn timeEndPeriod(uPeriod: u32) -> u32;
}

/// Represents an active line interval: (start_time_secs, end_time_secs, line_number)
#[derive(Debug, Clone, Serialize)]
pub struct LineInterval {
    pub start: f32,
    pub end: f32,
    pub line: usize,
}

struct AppState {
    engine: AudioEngine,
    sc_engine: Mutex<Option<ScEngine>>,
    use_sc: AtomicBool,
    sc_bundle_dir: Mutex<Option<PathBuf>>,
    recorder: Recorder,
    samples_dir: PathBuf,
    loaded_samples: Mutex<HashMap<String, (Vec<f32>, u32)>>,
    /// Sample durations in seconds for beat_stretch calculation
    sample_durations: Mutex<HashMap<String, f32>>,
    /// Monotonically increasing playback session counter.
    ///
    /// Scheduler threads compare it against the session they were started for
    /// and bail out when it changes. That check runs twice per scheduled
    /// event, so it is an atomic rather than a mutex — taking a lock tens of
    /// thousands of times on the thread that has to dispatch notes on time is
    /// contention nobody needs.
    session_id: AtomicU64,
    log_messages: Mutex<Vec<LogEntry>>,
    user_samples_dir: Mutex<Option<PathBuf>>,
    /// Line intervals for highlighting: each entry is (start_time, end_time, line_number)
    active_line_intervals: Mutex<Vec<LineInterval>>,
    /// Instant when playback started (for computing elapsed time)
    playback_start: Mutex<Option<Instant>>,
    /// Whether playback is currently paused (scheduler threads check this)
    is_paused: AtomicBool,
    /// Visual engine for audio-reactive band visualization (optional consumer).
    visual_engine: VisualEngine,
    /// Event publisher for sending performance events to the visual engine.
    /// Cloned into each scheduler thread. Uses try_send — NEVER blocks audio.
    visual_publisher: EventPublisher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    timestamp: f64,
    level: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct EngineStatus {
    is_playing: bool,
    master_volume: f32,
    bpm: f32,
    is_recording: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RunResult {
    success: bool,
    message: String,
    logs: Vec<LogEntry>,
    duration_estimate: f32,
    effective_bpm: f32,
    setup_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSampleInfo {
    pub name: String,
    pub path: String,
    pub file_type: String, // "wav", "mp3"
    pub duration_secs: f32,
    pub sample_rate: u32,
    pub bpm_estimate: Option<f32>,
    pub audio_type: String, // "drums", "vocal", "instrumental", "bass", "pad", "fx", "loop", "one-shot", "unknown"
    pub feeling: String, // "energetic", "calm", "dark", "bright", "aggressive", "mellow", "neutral"
    pub tags: Vec<String>,
    pub folder: String, // subfolder relative to user samples root
}

/// Lightweight file info returned by the fast discover phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSample {
    pub name: String,
    pub path: String,
    pub file_type: String,
    pub folder: String,
    pub file_size: u64,
    pub modified_ms: u64,
}

/// SuperCollider event types used for scheduling and visualization.
enum ScEvent {
    PlaySample {
        buf_id: i32,
        amp: f32,
        rate: f32,
        pan: f32,
        fx_context: u64,
    },
    PlayNote {
        synth_type: OscillatorType,
        freq: f32,
        amp: f32,
        dur: f32,
        env: Envelope,
        pan: f32,
        params: Vec<(String, f32)>,
        fx_context: u64,
    },
    SetEffect {
        rm: f32,
        room: f32,
        dt: f32,
        df: f32,
        dist: f32,
        lpf: f32,
        hpf: f32,
    },
    FxStart {
        fx_type: String,
        params: Vec<(String, f32)>,
        fx_id: u64,
        parent_fx_id: u64,
    },
    FxEnd {
        fx_id: u64,
    },
    SetBpm(f32),
    SetVolume(f32),
    Stop,
    SetRuntimeVar { key: String, value: f64 },
}

#[tauri::command]
fn run_code(code: String, state: tauri::State<Arc<AppState>>) -> Result<RunResult, String> {
    let start = Instant::now();
    let mut logs = Vec::new();

    // Log the code size
    let line_count = code.lines().count();
    eprintln!("[run_code] Parsing {} lines of code...", line_count);
    logs.push(LogEntry {
        timestamp: 0.0,
        level: "info".to_string(),
        message: format!("Parsing {} lines...", line_count),
    });

    // Validate and parse the code
    let parse_result = match validate_and_parse(&code) {
        Ok(r) => {
            eprintln!(
                "[run_code] Parsed {} top-level commands in {:.1}ms ({} warnings)",
                r.commands.len(),
                start.elapsed().as_secs_f64() * 1000.0,
                r.warnings.len()
            );
            logs.push(LogEntry {
                timestamp: start.elapsed().as_secs_f64(),
                level: "info".to_string(),
                message: format!("Parsed {} top-level commands", r.commands.len()),
            });
            r
        }
        Err(e) => {
            eprintln!("[run_code] \x1b[31mValidation/parse error: {}\x1b[0m", e);
            logs.push(LogEntry {
                timestamp: start.elapsed().as_secs_f64(),
                level: "error".to_string(),
                message: format!("Parse error: {}", e),
            });
            // Store logs even on error
            let mut log_store = state.log_messages.lock();
            log_store.extend(logs.clone());
            return Err(format!("Parse error: {}", e));
        }
    };

    // Surface validation warnings in the log panel
    for w in &parse_result.warnings {
        let msg = if w.line > 0 {
            format!("Line {}: {} — '{}'", w.line, w.message, w.source_text)
        } else {
            format!("{} — '{}'", w.message, w.source_text)
        };
        eprintln!("[run_code] \x1b[33mWARN: {}\x1b[0m", msg);
        logs.push(LogEntry {
            timestamp: start.elapsed().as_secs_f64(),
            level: "warning".to_string(),
            message: msg,
        });
    }

    let parsed = parse_result.commands;

    // Log parsed structure summary
    let mut loop_count = 0;
    let mut sample_count = 0;
    let mut note_count = 0;
    for cmd in &parsed {
        match cmd {
            ParsedCommand::Loop { name, commands, .. } => {
                loop_count += 1;
                let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                trace!(
                    "[run_code]   live_loop :{} ({} inner cmds, stop={})",
                    name,
                    commands.len(),
                    has_stop
                );
            }
            ParsedCommand::PlaySample { name, .. } => {
                sample_count += 1;
                trace!("[run_code]   sample: {}", name);
            }
            ParsedCommand::PlayNote { .. } => {
                note_count += 1;
            }
            _ => {}
        }
    }
    if loop_count > 0 || sample_count > 0 || note_count > 0 {
        logs.push(LogEntry {
            timestamp: start.elapsed().as_secs_f64(),
            level: "info".to_string(),
            message: format!(
                "{} loops, {} samples, {} notes found",
                loop_count, sample_count, note_count
            ),
        });
    }

    // Get current BPM
    let (_, _, engine_bpm) = state.engine.get_state_snapshot();

    // Pre-scan parsed commands for use_bpm to get the code's intended BPM
    let effective_bpm = parsed
        .iter()
        .find_map(|cmd| {
            if let ParsedCommand::SetBpm(b) = cmd {
                Some(*b)
            } else {
                None
            }
        })
        .unwrap_or(engine_bpm);
    eprintln!(
        "[run_code] Converting to audio commands at {} BPM (engine: {})...",
        effective_bpm, engine_bpm
    );

    // Convert to audio commands using the effective BPM
    let convert_start = Instant::now();
    let timed_commands = commands_to_audio(&parsed, effective_bpm);
    let convert_elapsed = convert_start.elapsed();
    eprintln!(
        "[run_code] Generated {} timed commands in {:.1}ms",
        timed_commands.len(),
        convert_elapsed.as_secs_f64() * 1000.0
    );

    if timed_commands.len() > 50_000 {
        let warn = format!("WARNING: {} commands generated - this may be slow. Consider adding 'stop' to live_loops.", timed_commands.len());
        eprintln!("[run_code] {}", warn);
        logs.push(LogEntry {
            timestamp: start.elapsed().as_secs_f64(),
            level: "warning".to_string(),
            message: warn,
        });
    }

    // Collect log messages from parsed commands
    collect_logs(&parsed, &mut logs);

    // Calculate total duration estimate (cap at 10 minutes)
    let max_time = timed_commands
        .iter()
        .map(|(t, _)| *t)
        .filter(|t| *t <= 600.0)
        .fold(0.0f32, f32::max);

    // Start a new playback session by incrementing the session ID
    // This invalidates all old scheduled threads from previous buffers
    let current_session = state.session_id.fetch_add(1, Ordering::SeqCst).wrapping_add(1);

    // Build line intervals for highlighting
    let line_intervals = build_line_intervals(&code, effective_bpm);
    // Compute max highlight end time so scheduler threads know how long to keep
    // playback_start alive (intervals must remain queryable until they expire).
    let max_highlight_end = line_intervals
        .iter()
        .map(|iv| iv.end)
        .fold(0.0f32, f32::max);
    eprintln!(
        "[run_code] Built {} line intervals for highlighting (max_end={:.2}s)",
        line_intervals.len(),
        max_highlight_end
    );
    *state.active_line_intervals.lock() = line_intervals;

    // Check if we should use SuperCollider engine
    let mut using_sc = state.use_sc.load(Ordering::Relaxed);

    // Early SC health check: verify scsynth is alive before committing to the SC path.
    // If SC is enabled but the server is unresponsive (crashed, killed, etc.),
    // fall back to the built-in cpal engine so the user still hears audio.
    // This fixes the case where preview_synth (always cpal) works but run_code
    // (SC when enabled) produces no sound.
    if using_sc {
        let sc_guard = state.sc_engine.lock();
        let sc_ok = sc_guard
            .as_ref()
            .map(|sc| sc.check_status().is_ok())
            .unwrap_or(false);
        drop(sc_guard);
        if !sc_ok {
            eprintln!("[run_code] SuperCollider not responding — falling back to built-in engine");
            logs.push(LogEntry {
                timestamp: start.elapsed().as_secs_f64(),
                level: "warning".to_string(),
                message: "SuperCollider not responding — using built-in engine".to_string(),
            });
            using_sc = false;
        }
    }

    // Track when the scheduler starts — used for playhead sync
    // Updated right before spawning the scheduler thread so the frontend
    // can offset the playhead by the time elapsed since scheduling began
    let mut scheduler_started = start; // default to function start

    if using_sc {
        // ============================================================
        // SUPERCOLLIDER ENGINE PATH
        // ============================================================
        eprintln!("[run_code] Using SuperCollider engine");
        logs.push(LogEntry {
            timestamp: start.elapsed().as_secs_f64(),
            level: "info".to_string(),
            message: "Using SuperCollider engine".to_string(),
        });

        // Stop any previous playback before starting new code
        // This ensures clean state when switching buffers
        {
            let sc_stop = state.sc_engine.lock();
            if let Some(ref sc) = *sc_stop {
                let _ = sc.stop_all();
            }
        }

        let sc_guard = state.sc_engine.lock();
        let sc = sc_guard
            .as_ref()
            .ok_or("SuperCollider engine not initialized")?;

        // Reload SynthDefs before each run to ensure they're available.
        // During boot, /d_loadDir confirmation can be missed due to stale
        // /done messages from earlier commands, so we reload here as a
        // safety net (cheap operation — scsynth caches unchanged defs).
        eprintln!("[run_code] Reloading SynthDefs...");
        match sc.reload_synthdefs() {
            Ok(path) => {
                logs.push(LogEntry {
                    timestamp: start.elapsed().as_secs_f64(),
                    level: "debug".to_string(),
                    message: format!("SynthDefs loaded from: {}", path),
                });
            }
            Err(e) => {
                eprintln!("[run_code] SynthDef reload warning: {}", e);
                logs.push(LogEntry {
                    timestamp: start.elapsed().as_secs_f64(),
                    level: "warning".to_string(),
                    message: format!("SynthDef reload issue: {}", e),
                });
            }
        }

        // Preload samples into SC buffers
        eprintln!("[run_code] Preloading samples into SuperCollider buffers...");
        let preload_start = Instant::now();
        match preload_samples_sc(&parsed, sc, &state.samples_dir, &state.sample_durations) {
            Ok(()) => {
                eprintln!(
                    "[run_code] SC samples preloaded in {:.1}ms",
                    preload_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            Err(e) => {
                eprintln!("[run_code] SC sample preload error: {}", e);
                logs.push(LogEntry {
                    timestamp: start.elapsed().as_secs_f64(),
                    level: "error".to_string(),
                    message: format!("SC sample load error: {}", e),
                });
                let mut log_store = state.log_messages.lock();
                log_store.extend(logs.clone());
                return Err(format!("SC sample load error: {}", e));
            }
        }

        // Schedule commands via SuperCollider OSC
        eprintln!(
            "[run_code] Scheduling {} commands via SuperCollider...",
            timed_commands.len()
        );
        let max_schedule_time = 600.0f32;
        let mut scheduled_count = 0u32;

        // Build sample name → buffer ID map for this run
        let sample_names = collect_sample_names(&parsed);
        let mut sample_idx = 0usize;

        // Pre-process ALL events into a sorted schedule
        // All events go through the single scheduler thread for consistent timing

        let mut all_events: Vec<(f32, ScEvent)> = Vec::new();

        for (time_offset, cmd) in &timed_commands {
            if *time_offset > max_schedule_time {
                if let AudioCommand::PlaySample { .. } = cmd {
                    sample_idx += 1;
                }
                continue;
            }

            match cmd {
                AudioCommand::PlaySample {
                    amplitude,
                    rate,
                    pan,
                    beat_stretch,
                    start,
                    finish,
                    fx_context,
                    ..
                } => {
                    if sample_idx < sample_names.len() {
                        let name = &sample_names[sample_idx];
                        sample_idx += 1;
                        let path = resolve_sample_path(name, &state.samples_dir);
                        let path_str = path.to_string_lossy().to_string();

                        let buf_id = {
                            let loaded = sc.loaded_buffers.lock();
                            loaded.get(&path_str).copied()
                        };

                        if let Some(buf_id) = buf_id {
                            // Calculate adjusted rate for beat_stretch
                            let mut final_rate = *rate;
                            if let Some(bs) = beat_stretch {
                                if *bs > 0.0 {
                                    // Get sample duration from cache
                                    let durations = state.sample_durations.lock();
                                    if let Some(&duration_secs) = durations.get(&path_str) {
                                        // Apply start/finish to calculate effective duration
                                        let start_frac = start.unwrap_or(0.0).clamp(0.0, 1.0);
                                        let finish_frac = finish.unwrap_or(1.0).clamp(0.0, 1.0);
                                        let effective_duration = duration_secs * (finish_frac - start_frac);
                                        // Desired duration in seconds (beat_stretch beats at current BPM)
                                        let beat_duration = 60.0 / effective_bpm;
                                        let desired_duration_secs = bs * beat_duration;
                                        // Rate adjustment: rate = sample_duration / desired_duration
                                        final_rate = *rate * (effective_duration / desired_duration_secs);
                                        eprintln!(
                                            "[SC] beat_stretch: {} beats -> sample {:.2}s at BPM {} = target {:.2}s, rate {:.3}",
                                            bs, effective_duration, effective_bpm, desired_duration_secs, final_rate
                                        );
                                    }
                                }
                            }
                            // Note: SC sample playback with start/finish would need additional
                            // handling via SC buffer start frame. For now, we adjust rate only.
                            all_events.push((
                                *time_offset,
                                ScEvent::PlaySample {
                                    buf_id,
                                    amp: *amplitude,
                                    rate: final_rate,
                                    pan: *pan,
                                    fx_context: *fx_context,
                                },
                            ));
                            scheduled_count += 1;
                        } else {
                            eprintln!("[SC schedule] No buffer for sample '{}'", name);
                        }
                    }
                }
                AudioCommand::PlayNote {
                    synth_type,
                    frequency,
                    amplitude,
                    duration_secs,
                    envelope,
                    pan,
                    ref params,
                    fx_context,
                } => {
                    all_events.push((
                        *time_offset,
                        ScEvent::PlayNote {
                            synth_type: *synth_type,
                            freq: *frequency,
                            amp: *amplitude,
                            dur: *duration_secs,
                            env: *envelope,
                            pan: *pan,
                            params: params.clone(),
                            fx_context: *fx_context,
                        },
                    ));
                    scheduled_count += 1;
                }
                AudioCommand::SetEffect {
                    reverb_mix,
                    reverb_room,
                    delay_time,
                    delay_feedback,
                    distortion,
                    lpf_cutoff,
                    hpf_cutoff,
                    ..
                } => {
                    all_events.push((
                        *time_offset,
                        ScEvent::SetEffect {
                            rm: *reverb_mix,
                            room: *reverb_room,
                            dt: *delay_time,
                            df: *delay_feedback,
                            dist: *distortion,
                            lpf: *lpf_cutoff,
                            hpf: *hpf_cutoff,
                        },
                    ));
                    scheduled_count += 1;
                }
                AudioCommand::SetBpm(bpm_val) => {
                    all_events.push((*time_offset, ScEvent::SetBpm(*bpm_val)));
                    scheduled_count += 1;
                }
                AudioCommand::SetMasterVolume(vol) => {
                    all_events.push((*time_offset, ScEvent::SetVolume(*vol)));
                    scheduled_count += 1;
                }
                AudioCommand::FxStart {
                    ref fx_type,
                    ref params,
                    fx_id,
                    parent_fx_id,
                } => {
                    all_events.push((
                        *time_offset,
                        ScEvent::FxStart {
                            fx_type: fx_type.clone(),
                            params: params.clone(),
                            fx_id: *fx_id,
                            parent_fx_id: *parent_fx_id,
                        },
                    ));
                    scheduled_count += 1;
                }
                AudioCommand::FxEnd { fx_id } => {
                    all_events.push((*time_offset, ScEvent::FxEnd { fx_id: *fx_id }));
                    scheduled_count += 1;
                }
                AudioCommand::Stop => {
                    all_events.push((*time_offset, ScEvent::Stop));
                    scheduled_count += 1;
                }
                AudioCommand::SetRuntimeVar { ref key, value } => {
                    all_events.push((*time_offset, ScEvent::SetRuntimeVar {
                        key: key.clone(),
                        value: *value,
                    }));
                    scheduled_count += 1;
                }
            }
        }

        // Log SC event details to the Log Panel for diagnostics
        for (time_offset, evt) in &all_events {
            match evt {
                ScEvent::PlayNote {
                    synth_type,
                    freq,
                    amp,
                    dur,
                    env,
                    ..
                } => {
                    let def_name = audio::sc_synthdefs::synthdef_name(synth_type);
                    logs.push(LogEntry {
                        timestamp: start.elapsed().as_secs_f64(),
                        level: "debug".to_string(),
                        message: format!(
                            "SC note @{:.3}s: {} freq={:.1}Hz amp={:.2} dur={:.2}s env(a={:.2} d={:.2} s_lvl={:.2} r={:.2})",
                            time_offset, def_name, freq, amp, dur,
                            env.attack, env.decay, env.sustain, env.release,
                        ),
                    });
                }
                ScEvent::PlaySample {
                    buf_id, amp, rate, ..
                } => {
                    logs.push(LogEntry {
                        timestamp: start.elapsed().as_secs_f64(),
                        level: "debug".to_string(),
                        message: format!(
                            "SC sample @{:.3}s: buf={} amp={:.2} rate={:.2}",
                            time_offset, buf_id, amp, rate
                        ),
                    });
                }
                _ => {}
            }
        }

        // Drop the SC lock before spawning the scheduler thread
        drop(sc_guard);

        // Sort all events by time offset for sequential processing.
        // Secondary sort by event type priority ensures correct ordering
        // when multiple events share the same timestamp:
        //   FxStart (0) → PlayNote/PlaySample/Other (1) → FxEnd (2)
        // This guarantees FX buses exist before notes are played on them,
        // and FX synths aren't freed until after all enclosed notes fire.
        all_events.sort_by(|a, b| {
            let time_cmp = a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal);
            if time_cmp != std::cmp::Ordering::Equal {
                return time_cmp;
            }
            // Same timestamp: sort by event type priority
            fn event_priority(evt: &ScEvent) -> u8 {
                match evt {
                    ScEvent::FxStart { .. } => 0,
                    ScEvent::FxEnd { .. } => 2,
                    _ => 1,
                }
            }
            event_priority(&a.1).cmp(&event_priority(&b.1))
        });

        let event_count = all_events.len();
        eprintln!(
            "[run_code] Scheduling {} SC events in single scheduler thread",
            event_count
        );
        logs.push(LogEntry {
            timestamp: start.elapsed().as_secs_f64(),
            level: "info".to_string(),
            message: format!("Scheduling {} SC events", event_count),
        });

        // Spawn a SINGLE scheduler thread for ALL events (including t=0)
        // This ensures consistent timing — all events use the same time reference
        if !all_events.is_empty() {
            let state_clone = Arc::clone(&*state);
            // Capture the reference time BEFORE spawning — pass it to the thread
            // so both the thread and the setup_time_ms use the same reference point
            let schedule_ref = Instant::now();
            scheduler_started = schedule_ref;
            // Events are timestamped SCHED_AHEAD_SECS into the future, so the
            // first note sounds then — not now. Anchor line highlighting to
            // that audible epoch or every highlight runs half a second early.
            let sched_ahead = Duration::from_secs_f64(sc_engine::SCHED_AHEAD_SECS);
            *state.playback_start.lock() = Some(schedule_ref + sched_ahead);
            let vis_pub_sc = state.visual_publisher.clone();
            vis_pub_sc.publish(PerformanceEvent::PlaybackStarted);
            vis_pub_sc.publish(PerformanceEvent::BpmChange { bpm: effective_bpm });
            // Wall-clock anchor for OSC timetags. Paired with `schedule_ref`
            // (a monotonic Instant) so event N's audible time is
            // `sched_epoch + SCHED_AHEAD_SECS + target_time`, computed from a
            // single reference rather than from "now" at dispatch.
            let sched_epoch = SystemTime::now();
            std::thread::spawn(move || {
                let state_for_cleanup = Arc::clone(&state_clone);
                let vis_for_cleanup = vis_pub_sc.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Set Windows timer resolution to 1ms for precise scheduling
                #[cfg(target_os = "windows")]
                unsafe {
                    timeBeginPeriod(1);
                }

                let start_time = schedule_ref;

                // Runtime state for set/get variables (master_amp, stop_all, pause_all, etc.)
                let mut runtime_vars: HashMap<String, f64> = HashMap::new();
                let mut dispatched_count: usize = 0;
                let mut failed_count: usize = 0;
                let progress_interval = (event_count / 10).max(100);

                for (target_time, evt) in all_events {
                    // Check runtime stop_all flag
                    if let Some(&stop_val) = runtime_vars.get("stop_all") {
                        if stop_val != 0.0 {
                            eprintln!("[SC scheduler] stop_all triggered, stopping playback");
                            let sc_lock = state_clone.sc_engine.lock();
                            if let Some(ref sc) = *sc_lock {
                                let _ = sc.stop_all();
                            }
                            break;
                        }
                    }

                    // Check if session is still valid
                    if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                        eprintln!("[SC scheduler] Session cancelled, stopping scheduler");
                        vis_pub_sc.publish(PerformanceEvent::PlaybackStopped);
                        #[cfg(target_os = "windows")]
                        unsafe {
                            timeEndPeriod(1);
                        }
                        return;
                    }

                    // The event is audible at `SCHED_AHEAD + target_time` and
                    // must reach scsynth DISPATCH_LOOKAHEAD before that.
                    // Precision no longer comes from waking at exactly the
                    // right moment — every message carries an OSC timetag, so
                    // scsynth places it on the exact sample. That lets this
                    // loop sleep instead of burning a core in a spin-wait for
                    // up to 18ms before every single note.
                    let target = target_time as f64;
                    let dispatch_at = sc_engine::SCHED_AHEAD_SECS + target
                        - sc_engine::DISPATCH_LOOKAHEAD_SECS;
                    let wait = dispatch_at - start_time.elapsed().as_secs_f64();
                    if wait > 0.0 {
                        std::thread::sleep(Duration::from_secs_f64(wait));
                    }
                    // Wall-clock instant this event should sound.
                    let play_at = sched_epoch
                        + Duration::from_secs_f64(sc_engine::SCHED_AHEAD_SECS + target);

                    // Re-check session after sleeping
                    if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                        vis_pub_sc.publish(PerformanceEvent::PlaybackStopped);
                        #[cfg(target_os = "windows")]
                        unsafe {
                            timeEndPeriod(1);
                        }
                        return;
                    }

                    // Publish visual events for SC commands (non-blocking, fire-and-forget)
                    publish_sc_visual_event(&vis_pub_sc, &evt);

                    // Handle runtime variable commands (processed by scheduler, not SC)
                    if let ScEvent::SetRuntimeVar { ref key, value } = evt {
                        trace!("[SC scheduler] runtime set :{} = {:.4}", key, value);
                        runtime_vars.insert(key.clone(), value);
                        continue;
                    }

                    // Check external pause (from pause_audio command)
                    if state_clone.is_paused.load(Ordering::Relaxed) {
                        // Spin-wait while paused, checking session validity
                        while state_clone.is_paused.load(Ordering::Relaxed) {
                            if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                                vis_pub_sc.publish(PerformanceEvent::PlaybackStopped);
                                #[cfg(target_os = "windows")]
                                unsafe { timeEndPeriod(1); }
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        // After resuming, adjust start_time so events stay aligned
                    }

                    // Check runtime pause_all — skip audio commands but keep processing
                    if let Some(&pause_val) = runtime_vars.get("pause_all") {
                        if pause_val != 0.0 {
                            continue;
                        }
                    }

                    // Apply runtime master_amp scaling
                    let evt = if let Some(&master_amp) = runtime_vars.get("master_amp") {
                        if master_amp < 1.0 - f64::EPSILON || master_amp > 1.0 + f64::EPSILON {
                            match evt {
                                ScEvent::PlaySample { buf_id, amp, rate, pan, fx_context } => {
                                    ScEvent::PlaySample {
                                        buf_id,
                                        amp: (amp as f64 * master_amp) as f32,
                                        rate,
                                        pan,
                                        fx_context,
                                    }
                                }
                                ScEvent::PlayNote { synth_type, freq, amp, dur, env, pan, params, fx_context } => {
                                    ScEvent::PlayNote {
                                        synth_type,
                                        freq,
                                        amp: (amp as f64 * master_amp) as f32,
                                        dur,
                                        env,
                                        pan,
                                        params,
                                        fx_context,
                                    }
                                }
                                other => other,
                            }
                        } else {
                            evt
                        }
                    } else {
                        evt
                    };

                    // Execute the event
                    let mut event_ok = true;
                    let sc_lock = state_clone.sc_engine.lock();
                    if let Some(ref sc) = *sc_lock {
                        match evt {
                            ScEvent::PlaySample {
                                buf_id,
                                amp,
                                rate,
                                pan,
                                fx_context,
                            } => {
                                if let Err(e) = sc.play_sample_buffer(buf_id, amp, rate, pan, fx_context, Some(play_at)) {
                                    eprintln!("[SC scheduler] sample play failed: {}", e);
                                    event_ok = false;
                                    let mut log_store = state_clone.log_messages.lock();
                                    log_store.push(LogEntry {
                                        timestamp: start_time.elapsed().as_secs_f64(),
                                        level: "error".to_string(),
                                        message: format!("SC sample play failed: {}", e),
                                    });
                                }
                            }
                            ScEvent::PlayNote {
                                synth_type,
                                freq,
                                amp,
                                dur,
                                env,
                                pan,
                                ref params,
                                fx_context,
                            } => {
                                let def_name = audio::sc_synthdefs::synthdef_name(&synth_type);
                                trace!("[SC scheduler] Playing {} freq={:.1} amp={:.2} dur={:.2} env(a={:.2},d={:.2},s={:.2},r={:.2})",
                                    def_name, freq, amp, dur, env.attack, env.decay, env.sustain, env.release);
                                if let Err(e) =
                                    sc.play_note(synth_type, freq, amp, dur, &env, pan, params, fx_context, Some(play_at))
                                {
                                    eprintln!("[SC scheduler] note play failed: {}", e);
                                    event_ok = false;
                                    let mut log_store = state_clone.log_messages.lock();
                                    log_store.push(LogEntry {
                                        timestamp: start_time.elapsed().as_secs_f64(),
                                        level: "error".to_string(),
                                        message: format!(
                                            "SC note play failed ({}): {}",
                                            def_name, e
                                        ),
                                    });
                                }
                            }
                            ScEvent::SetEffect {
                                rm,
                                room: _,
                                dt,
                                df,
                                dist,
                                lpf,
                                hpf,
                            } => {
                                let _ = sc.set_global_effects(rm, dt, df, dist, lpf, hpf);
                            }
                            ScEvent::SetBpm(bpm_val) => {
                                sc.state.lock().bpm = bpm_val;
                            }
                            ScEvent::SetVolume(vol) => {
                                sc.state.lock().master_volume = vol;
                            }
                            ScEvent::FxStart {
                                ref fx_type,
                                ref params,
                                fx_id,
                                parent_fx_id,
                            } => {
                                if let Err(e) = sc.push_fx_bus(fx_id, parent_fx_id, fx_type, params, Some(play_at)) {
                                    eprintln!("[SC scheduler] FxStart failed: {}", e);
                                }
                            }
                            ScEvent::FxEnd { fx_id } => {
                                if let Err(e) = sc.pop_fx_bus(fx_id) {
                                    eprintln!("[SC scheduler] FxEnd failed: {}", e);
                                }
                            }
                            ScEvent::Stop => {
                                let _ = sc.stop_all();
                            }
                            ScEvent::SetRuntimeVar { .. } => {
                                // Already handled above via continue; should not reach here
                            }
                        }
                    }
                    drop(sc_lock);
                    dispatched_count += 1;
                    if !event_ok { failed_count += 1; }
                    // Log periodic progress to the Log Panel
                if dispatched_count % progress_interval == 0 {
                        let pct = (dispatched_count * 100) / event_count;
                        let elapsed_s = start_time.elapsed().as_secs_f64();
                        trace!("[SC scheduler] Progress: {}/{} events ({}%) at {:.1}s, {} failures",
                            dispatched_count, event_count, pct, elapsed_s, failed_count);
                        let mut log_store = state_clone.log_messages.lock();
                        log_store.push(LogEntry {
                            timestamp: elapsed_s,
                            level: "debug".to_string(),
                            message: format!("SC progress: {}/{} events ({}%), {} failed",
                                dispatched_count, event_count, pct, failed_count),
                        });
                    }
                }
                eprintln!("[SC scheduler] All {} events dispatched ({} failed)", event_count, failed_count);
                // Wait until all line-highlight intervals have expired before
                // clearing playback_start, so sample / late lines stay lit.
                // Highlight times are relative to the audible epoch, which is
                // SCHED_AHEAD_SECS after this thread started.
                let elapsed =
                    (start_time.elapsed().as_secs_f32() - sc_engine::SCHED_AHEAD_SECS as f32).max(0.0);
                let remaining = max_highlight_end - elapsed;
                if remaining > 0.0 {
                    // Check session validity while waiting
                    let wait_until = std::time::Instant::now() + Duration::from_secs_f32(remaining);
                    while std::time::Instant::now() < wait_until {
                        if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
                *state_clone.playback_start.lock() = None;
                state_clone.active_line_intervals.lock().clear();
                vis_pub_sc.publish(PerformanceEvent::PlaybackStopped);
                {
                    let mut log_store = state_clone.log_messages.lock();
                    log_store.push(LogEntry {
                        timestamp: start_time.elapsed().as_secs_f64(),
                        level: "info".to_string(),
                        message: format!("SC scheduler: {}/{} events played ({} failed)",
                            dispatched_count, event_count, failed_count),
                    });
                }

                // Restore default Windows timer resolution
                #[cfg(target_os = "windows")]
                unsafe {
                    timeEndPeriod(1);
                }
                })); // end catch_unwind

                // If the scheduler thread panicked, ensure state is cleaned up
                if let Err(panic_info) = result {
                    let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        format!("SC scheduler PANICKED: {}", s)
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        format!("SC scheduler PANICKED: {}", s)
                    } else {
                        "SC scheduler PANICKED (unknown error)".to_string()
                    };
                    eprintln!("[FATAL] {}", msg);
                    // Clean up state so highlighting stops
                    *state_for_cleanup.playback_start.lock() = None;
                    state_for_cleanup.active_line_intervals.lock().clear();
                    vis_for_cleanup.publish(PerformanceEvent::PlaybackStopped);
                    let mut log_store = state_for_cleanup.log_messages.lock();
                    log_store.push(LogEntry {
                        timestamp: 0.0,
                        level: "error".to_string(),
                        message: msg,
                    });
                    #[cfg(target_os = "windows")]
                    unsafe {
                        timeEndPeriod(1);
                    }
                }
            });
        }
    } else {
        // ============================================================
        // CPAL ENGINE PATH (original)
        // ============================================================
        // First, load all samples from the parsed commands
        eprintln!("[run_code] Preloading samples...");
        let preload_start = Instant::now();
        match preload_samples(&parsed, &state) {
            Ok(()) => {
                eprintln!(
                    "[run_code] Samples preloaded in {:.1}ms",
                    preload_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            Err(e) => {
                eprintln!("[run_code] Sample preload error: {}", e);
                logs.push(LogEntry {
                    timestamp: start.elapsed().as_secs_f64(),
                    level: "error".to_string(),
                    message: format!("Sample load error: {}", e),
                });
                let mut log_store = state.log_messages.lock();
                log_store.extend(logs.clone());
                return Err(format!("Sample load error: {}", e));
            }
        }

        // Build a merged, time-sorted event list for the single scheduler thread.
        // Sample commands need their actual audio data resolved first.
        let max_schedule_time = 600.0f32; // Cap at 10 minutes
        let sample_names = collect_sample_names(&parsed);
        let mut all_events: Vec<(f32, AudioCommand)> = Vec::with_capacity(timed_commands.len());
        // Parallel array of optional sample categories for visual event publishing.
        // Same length as all_events — None for non-sample commands.
        let mut visual_sample_hints: Vec<Option<SampleCategory>> = Vec::with_capacity(timed_commands.len());
        let mut sample_idx = 0usize;

        for (time_offset, cmd) in &timed_commands {
            if *time_offset > max_schedule_time {
                // Keep sample_idx in sync even for skipped events
                if let AudioCommand::PlaySample { .. } = cmd {
                    sample_idx += 1;
                }
                continue;
            }
            match cmd {
                AudioCommand::PlaySample {
                    amplitude,
                    rate,
                    pan,
                    sustain_secs,
                    beat_stretch,
                    start,
                    finish,
                    envelope,
                    ..
                } => {
                    // Resolve sample data from preloaded cache
                    if sample_idx < sample_names.len() {
                        let name = &sample_names[sample_idx];
                        let sample_cat = SampleCategory::from_name(name);
                        sample_idx += 1;
                        let loaded = state.loaded_samples.lock();
                        let path = resolve_sample_path(name, &state.samples_dir);
                        let path_str = path.to_string_lossy().to_string();
                        if let Some((samples, sr)) = loaded.get(&path_str) {
                            // Apply start/finish to slice the sample
                            let start_frac = start.unwrap_or(0.0).clamp(0.0, 1.0);
                            let finish_frac = finish.unwrap_or(1.0).clamp(0.0, 1.0);
                            let start_idx = (start_frac * samples.len() as f32) as usize;
                            let end_idx = (finish_frac * samples.len() as f32) as usize;
                            let sliced_samples = if start_idx < end_idx && end_idx <= samples.len() {
                                samples[start_idx..end_idx].to_vec()
                            } else {
                                samples.clone()
                            };
                            
                            // Calculate adjusted rate for beat_stretch
                            let mut final_rate = *rate;
                            if let Some(bs) = beat_stretch {
                                if *bs > 0.0 {
                                    // Sample duration in seconds
                                    let sample_duration_secs = sliced_samples.len() as f32 / *sr as f32;
                                    // Desired duration in seconds (beat_stretch beats at current BPM)
                                    let beat_duration = 60.0 / effective_bpm;
                                    let desired_duration_secs = bs * beat_duration;
                                    // Rate adjustment: faster rate = shorter playback
                                    // We want rate such that sample_duration / rate = desired_duration
                                    // => rate = sample_duration / desired_duration
                                    final_rate = *rate * (sample_duration_secs / desired_duration_secs);
                                    trace!(
                                        "[cpal] beat_stretch: {} beats -> sample {:.2}s at BPM {} = target {:.2}s, rate {:.3}",
                                        bs, sample_duration_secs, effective_bpm, desired_duration_secs, final_rate
                                    );
                                }
                            }
                            
                            all_events.push((
                                *time_offset,
                                AudioCommand::PlaySample {
                                    samples: sliced_samples,
                                    sample_rate: *sr,
                                    amplitude: *amplitude,
                                    rate: final_rate,
                                    pan: *pan,
                                    sustain_secs: *sustain_secs,
                                    beat_stretch: *beat_stretch,
                                    start: *start,
                                    finish: *finish,
                                    envelope: envelope.clone(),
                                    fx_context: 0,
                                },
                            ));
                            visual_sample_hints.push(Some(sample_cat));
                        } else {
                            trace!("[cpal scheduler] sample '{}' not in cache, skipping", name);
                        }
                    }
                }
                other => {
                    all_events.push((*time_offset, other.clone()));
                    visual_sample_hints.push(None);
                }
            }
        }

        // Sort by time, with event type priority as tiebreaker:
        //   FxStart (0) → PlayNote/PlaySample/Other (1) → FxEnd (2)
        // Sort indices to keep visual_sample_hints in sync
        let mut indices: Vec<usize> = (0..all_events.len()).collect();
        indices.sort_by(|&a, &b| {
            let time_cmp = all_events[a].0.partial_cmp(&all_events[b].0).unwrap_or(std::cmp::Ordering::Equal);
            if time_cmp != std::cmp::Ordering::Equal {
                return time_cmp;
            }
            fn cmd_priority(cmd: &AudioCommand) -> u8 {
                match cmd {
                    AudioCommand::FxStart { .. } => 0,
                    AudioCommand::FxEnd { .. } => 2,
                    _ => 1,
                }
            }
            cmd_priority(&all_events[a].1).cmp(&cmd_priority(&all_events[b].1))
        });
        let sorted_events: Vec<(f32, AudioCommand)> = indices.iter().map(|&i| all_events[i].clone()).collect();
        let sorted_hints: Vec<Option<SampleCategory>> = indices.iter().map(|&i| visual_sample_hints[i]).collect();
        let all_events = sorted_events;
        let visual_sample_hints = sorted_hints;
        let event_count = all_events.len();
        eprintln!(
            "[run_code] Scheduling {} merged cpal events via single scheduler thread",
            event_count
        );

        // Spawn a single scheduler thread with high-precision timing
        let schedule_ref = Instant::now();
        scheduler_started = schedule_ref;
        // Set playback start for line highlighting
        *state.playback_start.lock() = Some(schedule_ref);
        let tx = state.engine.command_tx_clone();
        let vis_pub = state.visual_publisher.clone();
        let state_clone = Arc::clone(&*state);
        // Publish playback start to visual engine (non-blocking)
        vis_pub.publish(PerformanceEvent::PlaybackStarted);
        vis_pub.publish(PerformanceEvent::BpmChange { bpm: effective_bpm });
        std::thread::spawn(move || {
            let state_for_cleanup = Arc::clone(&state_clone);
            let vis_for_cleanup = vis_pub.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(target_os = "windows")]
            unsafe {
                timeBeginPeriod(1);
            }

            let start_time = schedule_ref;

            // Runtime state for set/get variables (master_amp, stop_all, pause_all, etc.)
            let mut runtime_vars: HashMap<String, f64> = HashMap::new();
            let mut dispatched_count: usize = 0;
            let mut send_failures: usize = 0;
            let progress_interval = (event_count / 10).max(100);

            for (idx, (target_time, cmd)) in all_events.into_iter().enumerate() {
                // Check runtime stop_all flag
                if let Some(&stop_val) = runtime_vars.get("stop_all") {
                    if stop_val != 0.0 {
                        eprintln!("[cpal scheduler] stop_all triggered, stopping playback");
                        let _ = tx.try_send(AudioCommand::Stop);
                        break;
                    }
                }

                // Check if session is still valid
                if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                    eprintln!("[cpal scheduler] Session cancelled, stopping");
                    vis_pub.publish(PerformanceEvent::PlaybackStopped);
                    #[cfg(target_os = "windows")]
                    unsafe {
                        timeEndPeriod(1);
                    }
                    return;
                }

                // Wait until the target time using high-precision timing
                let elapsed = start_time.elapsed().as_secs_f64();
                let target = target_time as f64;
                let wait = target - elapsed;
                if wait > 0.0005 {
                    if wait > 0.020 {
                        // Coarse sleep leaving 18ms margin for spin-wait
                        let coarse = Duration::from_secs_f64((wait - 0.018).max(0.0));
                        std::thread::sleep(coarse);
                    }
                    // Spin-wait for remaining time
                    while start_time.elapsed().as_secs_f64() < target {
                        std::hint::spin_loop();
                    }
                }

                // Re-check session after sleeping
                if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                    vis_pub.publish(PerformanceEvent::PlaybackStopped);
                    #[cfg(target_os = "windows")]
                    unsafe {
                        timeEndPeriod(1);
                    }
                    return;
                }

                // Handle runtime variable commands (processed by scheduler, not audio thread)
                if let AudioCommand::SetRuntimeVar { ref key, value } = cmd {
                    trace!("[cpal scheduler] runtime set :{} = {:.4}", key, value);
                    runtime_vars.insert(key.clone(), value);
                    continue;
                }

                // Check external pause (from pause_audio command)
                if state_clone.is_paused.load(Ordering::Relaxed) {
                    // Spin-wait while paused, checking session validity
                    while state_clone.is_paused.load(Ordering::Relaxed) {
                        if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                            vis_pub.publish(PerformanceEvent::PlaybackStopped);
                            #[cfg(target_os = "windows")]
                            unsafe { timeEndPeriod(1); }
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }

                // Check runtime pause_all — skip audio commands but keep processing
                if let Some(&pause_val) = runtime_vars.get("pause_all") {
                    if pause_val != 0.0 {
                        continue;
                    }
                }

                // Apply runtime master_amp scaling to audio commands
                let cmd = if let Some(&master_amp) = runtime_vars.get("master_amp") {
                    if master_amp < 1.0 - f64::EPSILON || master_amp > 1.0 + f64::EPSILON {
                        match cmd {
                            AudioCommand::PlayNote { synth_type, frequency, amplitude, duration_secs, envelope, pan, params, fx_context } => {
                                AudioCommand::PlayNote {
                                    synth_type,
                                    frequency,
                                    amplitude: (amplitude as f64 * master_amp) as f32,
                                    duration_secs,
                                    envelope,
                                    pan,
                                    params,
                                    fx_context,
                                }
                            }
                            AudioCommand::PlaySample { samples, sample_rate, amplitude, rate, pan, sustain_secs, beat_stretch, start, finish, envelope, fx_context } => {
                                AudioCommand::PlaySample {
                                    samples,
                                    sample_rate,
                                    amplitude: (amplitude as f64 * master_amp) as f32,
                                    rate,
                                    pan,
                                    sustain_secs,
                                    beat_stretch,
                                    start,
                                    finish,
                                    envelope,
                                    fx_context,
                                }
                            }
                            other => other,
                        }
                    } else {
                        cmd
                    }
                } else {
                    cmd
                };

                // Publish visual events BEFORE sending audio command (non-blocking)
                // Use sample category hints for accurate visual mapping
                if let Some(cat) = visual_sample_hints.get(idx).copied().flatten() {
                    if let AudioCommand::PlaySample { amplitude, .. } = &cmd {
                        vis_pub.publish(PerformanceEvent::SampleHit {
                            category: cat,
                            amplitude: *amplitude,
                        });
                    }
                } else {
                    publish_visual_event(&vis_pub, &cmd);
                }

                // Send command to cpal engine (with retry on failure)
                match tx.try_send(cmd) {
                    Ok(()) => {}
                    Err(crossbeam_channel::TrySendError::Full(cmd)) => {
                        // Channel full — retry with brief backoff
                        send_failures += 1;
                        let mut sent = false;
                        for _ in 0..10 {
                            std::thread::sleep(Duration::from_millis(1));
                            match tx.try_send(cmd.clone()) {
                                Ok(()) => { sent = true; break; }
                                Err(_) => {}
                            }
                        }
                        if !sent {
                            eprintln!("[cpal scheduler] command DROPPED after retries (channel full)");
                        }
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        eprintln!("[cpal scheduler] channel disconnected — audio engine stopped");
                        break;
                    }
                }
                dispatched_count += 1;
                // Log periodic progress to the Log Panel
                if dispatched_count % progress_interval == 0 {
                    let pct = (dispatched_count * 100) / event_count;
                    let elapsed_s = start_time.elapsed().as_secs_f64();
                    trace!("[cpal scheduler] Progress: {}/{} events ({}%) at {:.1}s, {} send failures",
                        dispatched_count, event_count, pct, elapsed_s, send_failures);
                    let mut log_store = state_clone.log_messages.lock();
                    log_store.push(LogEntry {
                        timestamp: elapsed_s,
                        level: "debug".to_string(),
                        message: format!("cpal progress: {}/{} events ({}%), {} send failures",
                            dispatched_count, event_count, pct, send_failures),
                    });
                }
            }

            eprintln!("[cpal scheduler] All {} events dispatched ({} send failures)", event_count, send_failures);
            // Wait until all line-highlight intervals have expired before
            // clearing playback_start, so sample / late lines stay lit.
            let elapsed = start_time.elapsed().as_secs_f32();
            let remaining = max_highlight_end - elapsed;
            if remaining > 0.0 {
                let wait_until = std::time::Instant::now() + Duration::from_secs_f32(remaining);
                while std::time::Instant::now() < wait_until {
                    if state_clone.session_id.load(Ordering::Relaxed) != current_session {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            *state_clone.playback_start.lock() = None;
            state_clone.active_line_intervals.lock().clear();
            vis_pub.publish(PerformanceEvent::PlaybackStopped);

            #[cfg(target_os = "windows")]
            unsafe {
                timeEndPeriod(1);
            }
            })); // end catch_unwind

            // If the scheduler thread panicked, ensure state is cleaned up
            if let Err(panic_info) = result {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("cpal scheduler PANICKED: {}", s)
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("cpal scheduler PANICKED: {}", s)
                } else {
                    "cpal scheduler PANICKED (unknown error)".to_string()
                };
                eprintln!("[FATAL] {}", msg);
                *state_for_cleanup.playback_start.lock() = None;
                state_for_cleanup.active_line_intervals.lock().clear();
                vis_for_cleanup.publish(PerformanceEvent::PlaybackStopped);
                let mut log_store = state_for_cleanup.log_messages.lock();
                log_store.push(LogEntry {
                    timestamp: 0.0,
                    level: "error".to_string(),
                    message: msg,
                });
                #[cfg(target_os = "windows")]
                unsafe {
                    timeEndPeriod(1);
                }
            }
        });
    }

    let total_elapsed = start.elapsed();
    eprintln!(
        "[run_code] Total setup completed in {:.1}ms",
        total_elapsed.as_secs_f64() * 1000.0
    );

    // Store logs
    {
        let mut log_store = state.log_messages.lock();
        log_store.extend(logs.clone());
        // Keep only last 1000 entries
        if log_store.len() > 1000 {
            let drain = log_store.len() - 1000;
            log_store.drain(0..drain);
        }
    }

    Ok(RunResult {
        success: true,
        message: format!(
            "Code executed in {:.1}ms{}",
            start.elapsed().as_secs_f64() * 1000.0,
            if using_sc { " (SuperCollider)" } else { "" }
        ),
        logs,
        duration_estimate: max_time + 1.0,
        effective_bpm,
        // How far into the piece playback already is when the frontend gets
        // this response, so the timeline playhead can start in the right
        // place. On the SC path the first note is timestamped
        // SCHED_AHEAD_SECS out, so this is normally negative — the playhead
        // starts slightly in the future and counts down to zero.
        setup_time_ms: scheduler_started.elapsed().as_secs_f64() * 1000.0
            - if using_sc {
                sc_engine::SCHED_AHEAD_SECS * 1000.0
            } else {
                0.0
            },
    })
}

/// Convert an AudioCommand into a PerformanceEvent and publish it to the visual engine.
/// This runs on the scheduler thread — NEVER on the audio callback.
/// Uses try_send internally so it can NEVER block.
fn publish_visual_event(publisher: &EventPublisher, cmd: &AudioCommand) {
    match cmd {
        AudioCommand::PlayNote {
            frequency,
            amplitude,
            synth_type,
            ..
        } => {
            let synth_name = format!("{:?}", synth_type);
            publisher.publish(PerformanceEvent::NoteOn {
                frequency: *frequency,
                amplitude: *amplitude,
                synth_hint: SynthCategory::from_synth_name(&synth_name),
            });
        }
        AudioCommand::PlaySample { amplitude, .. } => {
            // For cpal path we don't have the sample name easily.
            // Use amplitude-based generic percussion event.
            publisher.publish(PerformanceEvent::SampleHit {
                category: SampleCategory::Other,
                amplitude: *amplitude,
            });
        }
        AudioCommand::SetBpm(bpm) => {
            publisher.publish(PerformanceEvent::BpmChange { bpm: *bpm });
        }
        AudioCommand::Stop => {
            publisher.publish(PerformanceEvent::PlaybackStopped);
        }
        AudioCommand::FxStart { fx_type, .. } => {
            publisher.publish(PerformanceEvent::FxActive {
                fx_type: FxCategory::from_name(fx_type),
            });
        }
        _ => {}
    }
}

/// Publish visual events for SuperCollider scheduler commands.
/// Uses try_send internally so it can NEVER block.
fn publish_sc_visual_event(publisher: &EventPublisher, evt: &ScEvent) {
    match evt {
        ScEvent::PlayNote {
            freq, amp, synth_type, ..
        } => {
            let synth_name = format!("{:?}", synth_type);
            publisher.publish(PerformanceEvent::NoteOn {
                frequency: *freq,
                amplitude: *amp,
                synth_hint: SynthCategory::from_synth_name(&synth_name),
            });
        }
        ScEvent::PlaySample { amp, .. } => {
            publisher.publish(PerformanceEvent::SampleHit {
                category: SampleCategory::Other,
                amplitude: *amp,
            });
        }
        ScEvent::SetBpm(bpm) => {
            publisher.publish(PerformanceEvent::BpmChange { bpm: *bpm });
        }
        ScEvent::FxStart { fx_type, .. } => {
            publisher.publish(PerformanceEvent::FxActive {
                fx_type: FxCategory::from_name(fx_type),
            });
        }
        ScEvent::Stop => {
            publisher.publish(PerformanceEvent::PlaybackStopped);
        }
        _ => {}
    }
}

/// Preload all samples referenced in the parsed commands without playing them
fn preload_samples(parsed: &[ParsedCommand], state: &Arc<AppState>) -> Result<(), String> {
    for cmd in parsed {
        match cmd {
            ParsedCommand::PlaySample { name, .. } => {
                let mut loaded = state.loaded_samples.lock();
                let path = resolve_sample_path(name, &state.samples_dir);
                let path_str = path.to_string_lossy().to_string();
                trace!(
                    "[preload] sample '{}' -> resolved path '{}'",
                    name, path_str
                );

                if !loaded.contains_key(&path_str) {
                    if path.exists() {
                        match sample::load_wav(&path_str) {
                            Ok((samples, sr)) => {
                                trace!(
                                    "[preload] Loaded '{}': {} samples @ {}Hz",
                                    path_str,
                                    samples.len(),
                                    sr
                                );
                                // Store duration for beat_stretch calculation
                                let duration_secs = samples.len() as f32 / sr as f32;
                                state.sample_durations.lock().insert(path_str.clone(), duration_secs);
                                loaded.insert(path_str.clone(), (samples, sr));
                            }
                            Err(e) => {
                                eprintln!("[preload] ERROR loading '{}': {}", path_str, e);
                                return Err(format!("Failed to load sample '{}': {}", name, e));
                            }
                        }
                    } else {
                        eprintln!(
                            "[preload] WARNING: file not found '{}', using placeholder",
                            path_str
                        );
                        // Generate a simple placeholder beep for missing samples
                        let sr = 44100u32;
                        let dur = 0.2;
                        let n = (sr as f32 * dur) as usize;
                        let samples: Vec<f32> = (0..n)
                            .map(|i| {
                                let t = i as f32 / sr as f32;
                                (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * (-t * 20.0).exp()
                            })
                            .collect();
                        // Store placeholder duration
                        state.sample_durations.lock().insert(path_str.clone(), dur);
                        loaded.insert(path_str.clone(), (samples, sr));
                    }
                }
            }
            ParsedCommand::Loop { commands, .. }
            | ParsedCommand::WithFx { commands, .. }
            | ParsedCommand::TimesLoop { commands, .. } => {
                preload_samples(commands, state)?;
            }
            ParsedCommand::ConditionalRandom { command, .. } => {
                preload_samples(&[(**command).clone()], state)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Schedule sample playbacks according to the timed commands
fn schedule_samples_with_timing(
    parsed: &[ParsedCommand],
    timed_commands: &[(f32, AudioCommand)],
    state: &Arc<AppState>,
    current_session: u64,
) -> Result<(), String> {
    // Build a list of sample names from parsed commands in order
    let sample_names = collect_sample_names(parsed);
    eprintln!(
        "[schedule_samples] Collected {} sample names",
        sample_names.len()
    );

    let max_schedule_time = 600.0f32; // Cap at 10 minutes
    let mut scheduled = 0u32;

    // Match them with PlaySample commands in timed_commands
    let mut sample_idx = 0;
    for (time_offset, cmd) in timed_commands {
        if let AudioCommand::PlaySample {
            amplitude,
            rate,
            pan,
            ..
        } = cmd
        {
            if sample_idx < sample_names.len() {
                let name = &sample_names[sample_idx];
                sample_idx += 1;

                // Skip commands beyond max time
                if *time_offset > max_schedule_time {
                    continue;
                }

                // Load the sample data
                let loaded = state.loaded_samples.lock();
                let path = resolve_sample_path(name, &state.samples_dir);
                let path_str = path.to_string_lossy().to_string();

                if let Some((samples, sr)) = loaded.get(&path_str) {
                    trace!(
                        "[schedule_samples] #{} t={:.2}s '{}' -> scheduling ({} samples)",
                        sample_idx - 1,
                        time_offset,
                        name,
                        samples.len()
                    );
                    let cmd_to_send = AudioCommand::PlaySample {
                        samples: samples.clone(),
                        sample_rate: *sr,
                        amplitude: *amplitude,
                        rate: *rate,
                        pan: *pan,
                        sustain_secs: None,
                        beat_stretch: None,
                        start: None,
                        finish: None,
                        envelope: None,
                        fx_context: 0,
                    };

                    if *time_offset < 0.001 {
                        state.engine.send_command(cmd_to_send)?;
                    } else {
                        // Schedule for later
                        let delay = Duration::from_secs_f32(*time_offset);
                        let tx = state.engine.command_tx_clone();
                        let state_clone = Arc::clone(&*state);
                        std::thread::spawn(move || {
                            std::thread::sleep(delay);
                            // Only send if this session is still active
                            if state_clone.session_id.load(Ordering::Relaxed) == current_session {
                                if let Err(e) = tx.try_send(cmd_to_send) {
                                    eprintln!(
                                        "[schedule_samples] SAMPLE command send failed: {}",
                                        e
                                    );
                                }
                            }
                        });
                    }
                    scheduled += 1;
                } else {
                    trace!("[schedule_samples] #{} MISS: '{}' not in loaded cache (resolved path: '{}')", sample_idx - 1, name, path_str);
                }
            }
        }
    }
    eprintln!(
        "[schedule_samples] Scheduled {} sample playbacks",
        scheduled
    );
    Ok(())
}

/// Collect all sample names from parsed commands in execution order
fn collect_sample_names(parsed: &[ParsedCommand]) -> Vec<String> {
    let mut names = Vec::new();
    collect_sample_names_recursive(parsed, &mut names, 1);
    names
}

fn collect_sample_names_recursive(
    parsed: &[ParsedCommand],
    names: &mut Vec<String>,
    _loop_count: usize,
) {
    for cmd in parsed {
        match cmd {
            ParsedCommand::PlaySample { name, .. } => {
                names.push(name.clone());
            }
            ParsedCommand::Loop { commands, .. } => {
                // Check if body contains stop — if so, only expand once
                let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                let iters = if has_stop { 1 } else { 500 };
                for _ in 0..iters {
                    collect_sample_names_recursive(commands, names, 1);
                    // Safety cap
                    if names.len() > 100_000 {
                        eprintln!("[run_code] WARNING: sample name collection capped at 100k");
                        return;
                    }
                }
            }
            ParsedCommand::TimesLoop { count, commands } => {
                for _ in 0..*count {
                    collect_sample_names_recursive(commands, names, 1);
                }
            }
            ParsedCommand::WithFx { commands, .. } => {
                collect_sample_names_recursive(commands, names, 1);
            }
            ParsedCommand::Stop => {
                // Stop means we don't continue collecting from subsequent commands
                return;
            }
            ParsedCommand::ConditionalRandom { command, .. } => {
                // Always collect sample names from conditional commands
                // (audio engine emits with amp=0 when condition fails)
                collect_sample_names_recursive(&[(**command).clone()], names, 1);
            }
            ParsedCommand::AtBlock { commands, .. }
            | ParsedCommand::SwingBlock { commands, .. } => {
                // Collect sample names from at/time_warp/with_swing blocks.
                // Missing a nested block here would shift every later sample's
                // index and make the whole run play the wrong sounds.
                collect_sample_names_recursive(commands, names, 1);
            }
            ParsedCommand::SleepUntil(_) => {
                // SleepUntil doesn't contain samples
            }
            _ => {}
        }
    }
}

fn process_sample_command(cmd: &ParsedCommand, state: &Arc<AppState>) -> Result<(), String> {
    match cmd {
        ParsedCommand::PlaySample {
            name,
            rate,
            amplitude,
            pan,
            ..
        } => {
            let mut loaded = state.loaded_samples.lock();

            // Determine the file path to load
            let path = resolve_sample_path(name, &state.samples_dir);
            let path_str = path.to_string_lossy().to_string();

            if !loaded.contains_key(&path_str) {
                if path.exists() {
                    match sample::load_wav(&path_str) {
                        Ok((samples, sr)) => {
                            loaded.insert(path_str.clone(), (samples, sr));
                        }
                        Err(e) => {
                            return Err(format!("Failed to load sample '{}': {}", name, e));
                        }
                    }
                } else {
                    // Generate a simple placeholder beep for missing samples
                    let sr = 44100u32;
                    let dur = 0.2;
                    let n = (sr as f32 * dur) as usize;
                    let samples: Vec<f32> = (0..n)
                        .map(|i| {
                            let t = i as f32 / sr as f32;
                            (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * (-t * 20.0).exp()
                        })
                        .collect();
                    loaded.insert(path_str.clone(), (samples, sr));
                }
            }

            if let Some((samples, sr)) = loaded.get(&path_str) {
                state.engine.send_command(AudioCommand::PlaySample {
                    samples: samples.clone(),
                    sample_rate: *sr,
                    amplitude: *amplitude,
                    rate: *rate,
                    pan: *pan,
                    sustain_secs: None,
                    beat_stretch: None,
                    start: None,
                    finish: None,
                    envelope: None,
                    fx_context: 0,
                })?;
            }
        }
        ParsedCommand::Loop { commands, .. }
        | ParsedCommand::WithFx { commands, .. }
        | ParsedCommand::TimesLoop { commands, .. } => {
            for sub in commands {
                process_sample_command(sub, state)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_logs(parsed: &[ParsedCommand], logs: &mut Vec<LogEntry>) {
    for cmd in parsed {
        match cmd {
            ParsedCommand::Log(msg) => {
                logs.push(LogEntry {
                    timestamp: 0.0,
                    level: "info".to_string(),
                    message: msg.clone(),
                });
            }
            ParsedCommand::Comment(msg) => {
                logs.push(LogEntry {
                    timestamp: 0.0,
                    level: "comment".to_string(),
                    message: msg.clone(),
                });
            }
            ParsedCommand::Loop { commands, .. }
            | ParsedCommand::WithFx { commands, .. }
            | ParsedCommand::TimesLoop { commands, .. }
            | ParsedCommand::AtBlock { commands, .. }
            | ParsedCommand::SwingBlock { commands, .. } => {
                collect_logs(commands, logs);
            }
            _ => {}
        }
    }
}

/// Build line intervals from source code for highlighting.
/// Each interval represents when a line of code is "active" during playback.
/// Properly handles live_loop (parallel, repeating), loop, N.times, in_thread,
/// with_fx, and nested block structures.
fn build_line_intervals(code: &str, bpm: f32) -> Vec<LineInterval> {
    let beat_duration = 60.0 / bpm;
    let lines: Vec<&str> = code.lines().collect();
    let mut intervals = Vec::new();
    let mut current_time = 0.0f32;

    // Max playback horizon for looping constructs (seconds)
    let max_horizon: f32 = 120.0;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let line_num = i + 1;

        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // ----- live_loop :name do  /  loop do  /  in_thread do -----
        let is_live_loop = line.starts_with("live_loop ");
        let is_loop = line == "loop do" || line == "loop {";
        let is_in_thread = line.starts_with("in_thread");
        if is_live_loop || is_loop || is_in_thread {
            if let Some(end_idx) = find_block_end(&lines, i) {
                let body_start = i + 1;
                let body_end = end_idx; // exclusive (the `end` line)
                // Highlight the header briefly
                intervals.push(LineInterval {
                    start: current_time,
                    end: current_time + 0.1,
                    line: line_num,
                });

                // Compute one iteration's intervals + body duration
                let (body_intervals, body_exec_time) =
                    build_body_intervals(&lines, body_start, body_end, bpm);
                // Use the actual execution time (from sleep statements) as the
                // loop period.  Fall back to beat_duration if no sleeps found.
                let body_dur = if body_exec_time > 0.0 {
                    body_exec_time
                } else {
                    beat_duration
                };

                // live_loop / loop: repeat, starting from current_time, in parallel
                // in_thread: also starts at current_time (parallel)
                let loop_start = current_time;
                if is_live_loop || is_loop {
                    let mut t = loop_start;
                    while t < max_horizon {
                        for iv in &body_intervals {
                            let s = t + iv.start;
                            let e = t + iv.end;
                            if s < max_horizon {
                                intervals.push(LineInterval {
                                    start: s,
                                    end: e.min(max_horizon),
                                    line: iv.line,
                                });
                            }
                        }
                        // Highlight the `end` line at end of each iteration
                        intervals.push(LineInterval {
                            start: (t + body_dur).min(max_horizon),
                            end: (t + body_dur + 0.05).min(max_horizon),
                            line: end_idx + 1,
                        });
                        t += body_dur;
                    }
                } else {
                    // in_thread: single pass from current_time
                    for iv in &body_intervals {
                        intervals.push(LineInterval {
                            start: loop_start + iv.start,
                            end: loop_start + iv.end,
                            line: iv.line,
                        });
                    }
                }
                // live_loop / in_thread don't advance external current_time
                // (they run in parallel). loop blocks also run in parallel.
                i = end_idx + 1;
                continue;
            }
        }

        // ----- N.times do -----
        if line.contains(".times do") || line.contains(".times {") {
            let count = line
                .split('.')
                .next()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(1);
            if let Some(end_idx) = find_block_end(&lines, i) {
                intervals.push(LineInterval {
                    start: current_time,
                    end: current_time + 0.05,
                    line: line_num,
                });
                let (body_intervals, body_exec_time) =
                    build_body_intervals(&lines, i + 1, end_idx, bpm);
                let body_dur = body_exec_time.max(0.01);

                for rep in 0..count {
                    let t = current_time + rep as f32 * body_dur;
                    for iv in &body_intervals {
                        intervals.push(LineInterval {
                            start: t + iv.start,
                            end: t + iv.end,
                            line: iv.line,
                        });
                    }
                }
                current_time += count as f32 * body_dur;
                i = end_idx + 1;
                continue;
            }
        }

        // ----- with_fx :name do -----
        if line.starts_with("with_fx ") || line.starts_with("with_synth ")
            || line.starts_with("with_bpm ")
        {
            if let Some(end_idx) = find_block_end(&lines, i) {
                intervals.push(LineInterval {
                    start: current_time,
                    end: current_time + 0.1,
                    line: line_num,
                });
                let (body_intervals, body_exec_time) =
                    build_body_intervals(&lines, i + 1, end_idx, bpm);
                for iv in &body_intervals {
                    intervals.push(LineInterval {
                        start: current_time + iv.start,
                        end: current_time + iv.end,
                        line: iv.line,
                    });
                }
                current_time += body_exec_time;
                i = end_idx + 1;
                continue;
            }
        }

        // ----- sleep N -----
        if line.starts_with("sleep ") {
            if let Some(beats_str) = line.strip_prefix("sleep ") {
                if let Ok(beats) = beats_str.trim().parse::<f32>() {
                    intervals.push(LineInterval {
                        start: current_time,
                        end: current_time + 0.05,
                        line: line_num,
                    });
                    current_time += beats * beat_duration;
                }
            }
            i += 1;
            continue;
        }

        // ----- play / play_pattern_timed -----
        if line.starts_with("play ") || line.starts_with("play_pattern_timed ") {
            let duration = extract_play_duration(line, 0.5);
            intervals.push(LineInterval {
                start: current_time,
                end: current_time + duration * beat_duration,
                line: line_num,
            });
            i += 1;
            continue;
        }

        // ----- sample -----
        if line.starts_with("sample ") {
            let duration = 0.5; // ~0.5s for typical drum samples
            intervals.push(LineInterval {
                start: current_time,
                end: current_time + duration,
                line: line_num,
            });
            i += 1;
            continue;
        }

        // ----- use_bpm, use_synth, use_synth_defaults, etc. -----
        if line.starts_with("use_") || line.starts_with("set_") {
            intervals.push(LineInterval {
                start: current_time,
                end: current_time + 0.05,
                line: line_num,
            });
            i += 1;
            continue;
        }

        // ----- Default: brief highlight -----
        intervals.push(LineInterval {
            start: current_time,
            end: current_time + 0.1,
            line: line_num,
        });
        i += 1;
    }

    intervals.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    intervals
}

/// Find the matching `end` for a block starting at line index `start`.
/// Returns the index of the `end` line, accounting for nested blocks.
fn find_block_end(lines: &[&str], start: usize) -> Option<usize> {
    let mut depth = 1usize;
    for j in (start + 1)..lines.len() {
        let trimmed = lines[j].trim();
        // Detect block openers
        if trimmed.ends_with(" do")
            || trimmed.ends_with(" do |")
            || trimmed.contains(" do |")
            || trimmed.ends_with(" {")
            || (trimmed.starts_with("if ") && !trimmed.contains("if one_in")
                && (trimmed.ends_with("do") || trimmed.ends_with("then")
                    || (!trimmed.contains("sleep") && !trimmed.contains("sample")
                        && !trimmed.contains("play"))))
        {
            depth += 1;
        }
        if trimmed == "end" || trimmed.starts_with("end ") || trimmed.starts_with("end#") {
            depth -= 1;
            if depth == 0 {
                return Some(j);
            }
        }
    }
    None
}

/// Build intervals for a block body (lines[body_start..body_end]).
/// Returns (intervals, body_execution_time) where intervals have time offsets
/// relative to 0, and body_execution_time is the accumulated time from sleep
/// statements (the actual loop period).
fn build_body_intervals(
    lines: &[&str],
    body_start: usize,
    body_end: usize,
    bpm: f32,
) -> (Vec<LineInterval>, f32) {
    let beat_duration = 60.0 / bpm;
    let mut intervals = Vec::new();
    let mut t = 0.0f32;
    let mut j = body_start;

    while j < body_end {
        let line = lines[j].trim();
        let line_num = j + 1;

        if line.is_empty() || line.starts_with('#') {
            j += 1;
            continue;
        }

        // Nested N.times do
        if line.contains(".times do") || line.contains(".times {") {
            let count = line
                .split('.')
                .next()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(1);
            if let Some(end_idx) = find_block_end(lines, j) {
                intervals.push(LineInterval {
                    start: t,
                    end: t + 0.05,
                    line: line_num,
                });
                let (inner, inner_exec_time) = build_body_intervals(lines, j + 1, end_idx, bpm);
                let inner_dur = inner_exec_time.max(0.01);
                for rep in 0..count {
                    let off = t + rep as f32 * inner_dur;
                    for iv in &inner {
                        intervals.push(LineInterval {
                            start: off + iv.start,
                            end: off + iv.end,
                            line: iv.line,
                        });
                    }
                }
                t += count as f32 * inner_dur;
                j = end_idx + 1;
                continue;
            }
        }

        // Nested with_fx / with_synth
        if line.starts_with("with_fx ") || line.starts_with("with_synth ")
            || line.starts_with("with_bpm ")
        {
            if let Some(end_idx) = find_block_end(lines, j) {
                intervals.push(LineInterval {
                    start: t,
                    end: t + 0.1,
                    line: line_num,
                });
                let (inner, inner_exec_time) = build_body_intervals(lines, j + 1, end_idx, bpm);
                for iv in &inner {
                    intervals.push(LineInterval {
                        start: t + iv.start,
                        end: t + iv.end,
                        line: iv.line,
                    });
                }
                t += inner_exec_time;
                j = end_idx + 1;
                continue;
            }
        }

        // Nested if / unless blocks
        if (line.starts_with("if ") || line.starts_with("unless "))
            && !line.contains("sample ") && !line.contains("play ")
        {
            if let Some(end_idx) = find_block_end(lines, j) {
                intervals.push(LineInterval {
                    start: t,
                    end: t + 0.05,
                    line: line_num,
                });
                let (inner, inner_exec_time) = build_body_intervals(lines, j + 1, end_idx, bpm);
                for iv in &inner {
                    intervals.push(LineInterval {
                        start: t + iv.start,
                        end: t + iv.end,
                        line: iv.line,
                    });
                }
                t += inner_exec_time;
                j = end_idx + 1;
                continue;
            }
        }

        // sleep
        if line.starts_with("sleep ") {
            if let Some(beats_str) = line.strip_prefix("sleep ") {
                if let Ok(beats) = beats_str.trim().parse::<f32>() {
                    intervals.push(LineInterval {
                        start: t,
                        end: t + 0.05,
                        line: line_num,
                    });
                    t += beats * beat_duration;
                }
            }
            j += 1;
            continue;
        }

        // play
        if line.starts_with("play ") || line.starts_with("play_pattern_timed ") {
            let duration = extract_play_duration(line, 0.5);
            intervals.push(LineInterval {
                start: t,
                end: t + duration * beat_duration,
                line: line_num,
            });
            j += 1;
            continue;
        }

        // sample
        if line.starts_with("sample ") {
            intervals.push(LineInterval {
                start: t,
                end: t + 0.5,
                line: line_num,
            });
            j += 1;
            continue;
        }

        // use_*, set_*
        if line.starts_with("use_") || line.starts_with("set_") {
            intervals.push(LineInterval {
                start: t,
                end: t + 0.05,
                line: line_num,
            });
            j += 1;
            continue;
        }

        // Default
        intervals.push(LineInterval {
            start: t,
            end: t + 0.1,
            line: line_num,
        });
        j += 1;
    }

    (intervals, t)
}

/// Extract estimated duration in beats from a play line.
fn extract_play_duration(line: &str, default: f32) -> f32 {
    if line.contains("sustain:") {
        line.split("sustain:")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(default)
    } else if line.contains("release:") {
        line.split("release:")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(default)
    } else {
        default
    }
}

/// Resolve a sample name to a file path.
/// Handles: full file paths, Sonic Pi built-in names, and searching the samples directory.
fn resolve_sample_path(name: &str, samples_dir: &std::path::Path) -> PathBuf {
    let trimmed = name.trim();
    trace!("[resolve_sample_path] input: '{}'", trimmed);

    // If it looks like an absolute file path (contains / or \\ and an extension)
    let as_path = PathBuf::from(trimmed);
    if as_path.is_absolute() {
        eprintln!(
            "[resolve_sample_path] absolute path -> '{}' (exists={})",
            as_path.display(),
            as_path.exists()
        );
        return as_path;
    }

    // If it contains a file extension, treat as relative path
    if trimmed.contains('.') && (trimmed.contains('/') || trimmed.contains('\\')) {
        return PathBuf::from(trimmed);
    }

    // Built-in sample: try drums subdirectory first
    let sample_path = samples_dir.join("drums").join(format!("{}.wav", trimmed));
    if sample_path.exists() {
        return sample_path;
    }

    // Try samples root
    let alt_path = samples_dir.join(format!("{}.wav", trimmed));
    if alt_path.exists() {
        return alt_path;
    }

    // Search all subdirectories for a matching file
    for entry in walkdir::WalkDir::new(samples_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let fname = entry.file_name().to_string_lossy();
        if fname.contains(trimmed) {
            return entry.path().to_path_buf();
        }
    }

    // Fallback
    sample_path
}

#[tauri::command]
fn stop_audio(state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    // Stop both engines
    state.engine.send_command(AudioCommand::Stop)?;
    if let Some(ref sc) = *state.sc_engine.lock() {
        let _ = sc.stop_all();
    }
    // Increment session ID to invalidate all scheduled threads
    state.session_id.fetch_add(1, Ordering::SeqCst);
    // Clear playback state for line highlighting
    *state.playback_start.lock() = None;
    state.active_line_intervals.lock().clear();
    // Reset pause state
    state.is_paused.store(false, Ordering::Relaxed);
    // Notify visual engine that playback stopped (non-blocking)
    state.visual_publisher.publish(PerformanceEvent::PlaybackStopped);
    Ok("Stopped".to_string())
}

#[tauri::command]
fn pause_audio(state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    state.is_paused.store(true, Ordering::Relaxed);
    Ok("Paused".to_string())
}

#[tauri::command]
fn resume_audio(state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    state.is_paused.store(false, Ordering::Relaxed);
    Ok("Resumed".to_string())
}

#[tauri::command]
fn get_waveform(state: tauri::State<Arc<AppState>>) -> Vec<f32> {
    if state.use_sc.load(Ordering::Relaxed) {
        if let Some(ref sc) = *state.sc_engine.lock() {
            sc.process_incoming(); // picks up /b_setn from previous request
            sc.request_scope_buffer(); // fire off next /b_getn request
            return sc.get_waveform();
        }
    }
    state.engine.get_waveform()
}

#[tauri::command]
fn get_status(state: tauri::State<Arc<AppState>>) -> EngineStatus {
    if state.use_sc.load(Ordering::Relaxed) {
        if let Some(ref sc) = *state.sc_engine.lock() {
            sc.process_incoming();
            // Surface any SC errors into the Log Panel
            let errors = sc.drain_errors();
            if !errors.is_empty() {
                let mut log_store = state.log_messages.lock();
                for err in errors {
                    log_store.push(LogEntry {
                        timestamp: 0.0,
                        level: "error".to_string(),
                        message: err,
                    });
                }
            }
            let (sc_playing, master_volume, bpm) = sc.get_state_snapshot();
            let scheduler_active = state.playback_start.lock().is_some();
            return EngineStatus {
                is_playing: sc_playing || scheduler_active,
                master_volume,
                bpm,
                is_recording: state.recorder.is_recording(),
            };
        }
    }
    let (engine_playing, master_volume, bpm) = state.engine.get_state_snapshot();
    // The scheduler thread sets playback_start to Some(...) while running.
    // Use this as the authoritative "is playing" flag so that brief gaps
    // between scheduled events (where no voices/samples are active) don't
    // cause the frontend to think playback has stopped.
    let scheduler_active = state.playback_start.lock().is_some();
    EngineStatus {
        is_playing: engine_playing || scheduler_active,
        master_volume,
        bpm,
        is_recording: state.recorder.is_recording(),
    }
}

/// Get the currently active line numbers based on elapsed playback time.
/// Returns a list of line numbers (1-indexed) that should be highlighted.
#[tauri::command]
fn get_active_lines(state: tauri::State<Arc<AppState>>) -> Vec<usize> {
    let playback_start = state.playback_start.lock();
    let Some(start_instant) = *playback_start else {
        return vec![];
    };

    let elapsed = start_instant.elapsed().as_secs_f32();
    let intervals = state.active_line_intervals.lock();

    // Find all lines that are active at the current time
    // An interval is active if elapsed is between start and end
    let mut active = Vec::new();
    for interval in intervals.iter() {
        if elapsed >= interval.start && elapsed <= interval.end {
            if !active.contains(&interval.line) {
                active.push(interval.line);
            }
        }
        // Early exit if we've passed all possible intervals
        if interval.start > elapsed + 1.0 {
            break;
        }
    }

    active
}

#[tauri::command]
fn set_volume(volume: f32, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    eprintln!("[set_volume] Setting master volume to {:.3}", volume);
    // Immediately update the shared state so the audio callback picks it up
    // on its next invocation (it reads master_volume from shared state each cycle).
    state.engine.state.lock().master_volume = volume;
    // Also send through the command channel for completeness
    let _ = state.engine.send_command(AudioCommand::SetMasterVolume(volume));
    // Also update SC engine state when active
    if state.use_sc.load(Ordering::Relaxed) {
        if let Some(ref sc) = *state.sc_engine.lock() {
            sc.state.lock().master_volume = volume;
        }
    }
    Ok(())
}

#[tauri::command]
fn set_bpm(bpm: f32, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    state.engine.state.lock().bpm = bpm;
    state.engine.send_command(AudioCommand::SetBpm(bpm))?;
    if state.use_sc.load(Ordering::Relaxed) {
        if let Some(ref sc) = *state.sc_engine.lock() {
            sc.state.lock().bpm = bpm;
        }
    }
    Ok(())
}

#[tauri::command]
fn start_recording(state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    state.recorder.start();
    Ok("Recording started".to_string())
}

#[tauri::command]
fn stop_recording(
    path: Option<String>,
    state: tauri::State<Arc<AppState>>,
) -> Result<String, String> {
    state.recorder.stop();
    let save_path = path.unwrap_or_else(|| {
        let home = dirs_next().unwrap_or_else(|| PathBuf::from("."));
        home.join("sonic_daw_recording.wav")
            .to_string_lossy()
            .to_string()
    });
    state.recorder.save_to_file(&save_path)
}

fn dirs_next() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(|s| PathBuf::from(s).join("Music"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .map(|s| PathBuf::from(s).join("Music"))
    }
}

#[tauri::command]
fn list_samples(state: tauri::State<Arc<AppState>>) -> Vec<SampleInfo> {
    sample::list_samples(&state.samples_dir.to_string_lossy())
}

#[tauri::command]
fn get_logs(state: tauri::State<Arc<AppState>>) -> Vec<LogEntry> {
    state.log_messages.lock().clone()
}

#[tauri::command]
fn clear_logs(state: tauri::State<Arc<AppState>>) {
    state.log_messages.lock().clear();
}

#[tauri::command]
fn set_effects(
    reverb_mix: f32,
    delay_time: f32,
    delay_feedback: f32,
    distortion: f32,
    lpf_cutoff: f32,
    hpf_cutoff: f32,
    state: tauri::State<Arc<AppState>>,
) -> Result<(), String> {
    // Also route effects to SuperCollider engine when active
    if state.use_sc.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(ref sc) = *state.sc_engine.lock() {
            let _ = sc.set_global_effects(
                reverb_mix, delay_time, delay_feedback, distortion, lpf_cutoff, hpf_cutoff,
            );
        }
    }

    // Auto-compute delay_mix from delay_time (if delay is active, use 50% wet)
    let delay_mix = if delay_time > 0.001 { 0.5 } else { 0.0 };

    state.engine.send_command(AudioCommand::SetEffect {
        reverb_mix,
        reverb_room: 0.6,
        delay_time,
        delay_feedback,
        distortion,
        lpf_cutoff,
        hpf_cutoff,
        slicer_phase: 0.25,
        slicer_mix: 0.0,
        slicer_wave: 0,
        bitcrusher_bits: 16.0,
        bitcrusher_sample_rate: 44100.0,
        bitcrusher_mix: 0.0,
        compressor_threshold: 1.0,
        compressor_clamp_time: 0.01,
        compressor_relax_time: 0.1,
        compressor_mix: 0.0,
        normaliser_level: 2.0,
        // New effects (disabled by default)
        flanger_rate: 0.0,
        flanger_depth: 0.0,
        flanger_feedback: 0.0,
        flanger_mix: 0.0,
        chorus_rate: 0.0,
        chorus_depth: 0.0,
        chorus_mix: 0.0,
        ring_mod_freq: 0.0,
        ring_mod_mix: 0.0,
        pan_position: 0.0,
        wobble_rate: 0.0,
        wobble_depth: 0.0,
        wobble_mix: 0.0,
        octaver_mix: 0.0,
        octaver_sub_amp: 0.0,
        octaver_super_amp: 0.0,
        reverb_damp: 0.0,
        delay_mix,
        lpf_res: 0.0,
        hpf_res: 0.0,
        cutoff_is_hz: true,
    })
}

#[tauri::command]
fn play_sample_file(path: String, state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    let (samples, sr) = sample::load_wav(&path)?;
    state.engine.send_command(AudioCommand::PlaySample {
        samples,
        sample_rate: sr,
        amplitude: 1.0,
        rate: 1.0,
        pan: 0.0,
        sustain_secs: None,
        beat_stretch: None,
        start: None,
        finish: None,
        envelope: None,
        fx_context: 0,
    })?;
    Ok("Playing sample".to_string())
}

#[tauri::command]
fn get_sample_peaks(path: String, num_peaks: usize) -> Result<Vec<f32>, String> {
    let (samples, _sr) = sample::load_wav(&path)?;
    if samples.is_empty() {
        return Ok(vec![0.0; num_peaks]);
    }
    let chunk_size = (samples.len() as f64 / num_peaks as f64).ceil() as usize;
    let peaks: Vec<f32> = samples
        .chunks(chunk_size.max(1))
        .map(|chunk| chunk.iter().fold(0.0f32, |acc, &s| acc.max(s.abs())))
        .collect();
    // Pad or trim to exact num_peaks
    let mut result = peaks;
    result.resize(num_peaks, 0.0);
    Ok(result)
}

/// Return the duration in seconds for a list of sample identifiers.
/// Each identifier can be a built-in name (e.g. "bd_haus") or an absolute file path.
/// Results are cached in `loaded_samples` for subsequent calls.
#[tauri::command]
fn get_sample_durations(
    names: Vec<String>,
    state: tauri::State<Arc<AppState>>,
) -> Result<HashMap<String, f32>, String> {
    let mut result = HashMap::new();

    for name in &names {
        let path = resolve_sample_path(name, &state.samples_dir);
        let path_str = path.to_string_lossy().to_string();

        // Check loaded cache first
        let loaded = state.loaded_samples.lock();
        if let Some((samples, sr)) = loaded.get(&path_str) {
            let dur = samples.len() as f32 / *sr as f32;
            result.insert(name.clone(), dur);
            continue;
        }
        drop(loaded);

        // Not cached — try to load and measure
        if path.exists() {
            match sample::load_wav(&path_str) {
                Ok((samples, sr)) => {
                    let dur = samples.len() as f32 / sr as f32;
                    result.insert(name.clone(), dur);
                    // Cache for future use
                    let mut loaded = state.loaded_samples.lock();
                    loaded.insert(path_str, (samples, sr));
                }
                Err(e) => {
                    eprintln!("[get_sample_durations] Failed to load '{}': {}", name, e);
                    result.insert(name.clone(), 0.0);
                }
            }
        } else {
            eprintln!(
                "[get_sample_durations] File not found for '{}' at '{}'",
                name, path_str
            );
            result.insert(name.clone(), 0.0);
        }
    }

    Ok(result)
}

#[tauri::command]
fn preview_synth(synth_name: String, state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    let osc = parse_synth_name_for_preview(&synth_name);
    let envelope = Envelope {
        attack: 0.01,
        decay: 0.1,
        sustain: 0.6,
        release: 0.2,
    };

    // Always use the built-in cpal engine for preview.
    // This avoids latency from SC health checks and ensures instant feedback
    // regardless of SuperCollider state.
    state.engine.send_command(AudioCommand::PlayNote {
        synth_type: osc,
        frequency: 261.63,
        amplitude: 0.5,
        duration_secs: 0.6,
        envelope,
        pan: 0.0,
        params: vec![],
        fx_context: 0,
    })?;
    Ok(format!("Previewing synth: {}", synth_name))
}

/// Map a synth name string to an OscillatorType for preview
fn parse_synth_name_for_preview(name: &str) -> OscillatorType {
    match name {
        "sine" | "beep" => OscillatorType::Sine,
        "saw" => OscillatorType::Saw,
        "square" => OscillatorType::Square,
        "tri" | "triangle" => OscillatorType::Triangle,
        "noise" => OscillatorType::Noise,
        "pulse" => OscillatorType::Pulse,
        "supersaw" | "super_saw" => OscillatorType::SuperSaw,
        "dsaw" => OscillatorType::DSaw,
        "dpulse" => OscillatorType::DPulse,
        "dtri" => OscillatorType::DTri,
        "fm" => OscillatorType::FM,
        "mod_fm" => OscillatorType::ModFM,
        "mod_sine" => OscillatorType::ModSine,
        "mod_saw" => OscillatorType::ModSaw,
        "mod_dsaw" => OscillatorType::ModDSaw,
        "mod_tri" => OscillatorType::ModTri,
        "mod_pulse" => OscillatorType::ModPulse,
        "tb303" => OscillatorType::TB303,
        "prophet" => OscillatorType::Prophet,
        "zawa" => OscillatorType::Zawa,
        "blade" => OscillatorType::Blade,
        "tech_saws" => OscillatorType::TechSaws,
        "hoover" => OscillatorType::Hoover,
        "pluck" => OscillatorType::Pluck,
        "piano" => OscillatorType::Piano,
        "pretty_bell" => OscillatorType::PrettyBell,
        "dull_bell" => OscillatorType::DullBell,
        "hollow" => OscillatorType::Hollow,
        "dark_ambience" => OscillatorType::DarkAmbience,
        "growl" => OscillatorType::Growl,
        "chiplead" | "chip_lead" => OscillatorType::ChipLead,
        "chipbass" | "chip_bass" => OscillatorType::ChipBass,
        "chipnoise" | "chip_noise" => OscillatorType::ChipNoise,
        "bnoise" | "brown_noise" => OscillatorType::BNoise,
        "pnoise" | "pink_noise" => OscillatorType::PNoise,
        "gnoise" | "grey_noise" => OscillatorType::GNoise,
        "cnoise" | "clip_noise" => OscillatorType::CNoise,
        "subpulse" | "sub_pulse" => OscillatorType::SubPulse,
        _ => OscillatorType::Sine,
    }
}

#[tauri::command]
fn save_recording(path: String, state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    state.recorder.save_to_file(&path)
}

#[tauri::command]
fn get_env_var(key: String) -> Option<String> {
    std::env::var(key).ok()
}

/// Save code content to a file (used for .sonicpi files)
#[tauri::command]
fn save_code_to_file(path: String, content: String) -> Result<String, String> {
    std::fs::write(&path, &content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(format!("Saved to {}", path))
}

/// Read code content from a file (used for .sonicpi files)
#[tauri::command]
fn read_code_from_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))
}

// ============================================================
// USER SAMPLE SCANNING & ANALYSIS
// ============================================================

/// Set the user samples directory path
#[tauri::command]
fn set_user_samples_dir(dir: String, state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    let path = PathBuf::from(&dir);
    if !path.exists() {
        return Err(format!("Directory does not exist: {}", dir));
    }
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", dir));
    }
    *state.user_samples_dir.lock() = Some(path);
    Ok(format!("User samples directory set to: {}", dir))
}

/// Get the current user samples directory
#[tauri::command]
fn get_user_samples_dir(state: tauri::State<Arc<AppState>>) -> Option<String> {
    state
        .user_samples_dir
        .lock()
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
}

/// Scan user samples directory and analyze each audio file
#[tauri::command]
fn scan_user_samples(state: tauri::State<Arc<AppState>>) -> Result<Vec<UserSampleInfo>, String> {
    let dir = state.user_samples_dir.lock().clone();
    let dir = dir.ok_or_else(|| "No user samples directory set".to_string())?;

    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }

    let mut results = Vec::new();
    let root = dir.clone();

    for entry in walkdir::WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if ext_lower == "wav" || ext_lower == "mp3" {
                match analyze_audio_file(path, &root) {
                    Ok(info) => results.push(info),
                    Err(e) => {
                        eprintln!(
                            "[scan_user_samples] Failed to analyze {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    eprintln!(
        "[scan_user_samples] Found {} audio files in {}",
        results.len(),
        dir.display()
    );
    Ok(results)
}

/// Fast discovery: walk directory and return basic file info without audio analysis
#[tauri::command]
fn discover_user_samples(
    state: tauri::State<Arc<AppState>>,
) -> Result<Vec<DiscoveredSample>, String> {
    let dir = state.user_samples_dir.lock().clone();
    let dir = dir.ok_or_else(|| "No user samples directory set".to_string())?;

    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }

    let root = dir.clone();
    let mut results = Vec::new();

    for entry in walkdir::WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if ext_lower == "wav" || ext_lower == "mp3" {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let folder = path
                    .parent()
                    .map(|p| {
                        p.strip_prefix(&root)
                            .unwrap_or(p)
                            .to_string_lossy()
                            .to_string()
                    })
                    .unwrap_or_default();
                // Get file metadata for change detection
                let (file_size, modified_ms) = match std::fs::metadata(path) {
                    Ok(meta) => {
                        let size = meta.len();
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        (size, modified)
                    }
                    Err(_) => (0, 0),
                };
                results.push(DiscoveredSample {
                    name,
                    path: path.to_string_lossy().to_string(),
                    file_type: ext_lower,
                    folder,
                    file_size,
                    modified_ms,
                });
            }
        }
    }

    eprintln!(
        "[discover_user_samples] Found {} audio files in {}",
        results.len(),
        dir.display()
    );
    Ok(results)
}

/// Analyze a single audio file by path and return full metadata
#[tauri::command]
fn analyze_user_sample(
    path: String,
    state: tauri::State<Arc<AppState>>,
) -> Result<UserSampleInfo, String> {
    let dir = state.user_samples_dir.lock().clone();
    let root = dir.ok_or_else(|| "No user samples directory set".to_string())?;
    let file_path = std::path::Path::new(&path);
    analyze_audio_file(file_path, &root)
}

/// Analyze a single audio file and produce metadata
fn analyze_audio_file(
    path: &std::path::Path,
    root: &std::path::Path,
) -> Result<UserSampleInfo, String> {
    let path_str = path.to_string_lossy().to_string();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let folder = path
        .parent()
        .map(|p| {
            p.strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();

    // Load audio data for analysis
    let (samples, sample_rate) = sample::load_wav(&path_str)?;

    let duration_secs = if sample_rate > 0 {
        samples.len() as f32 / sample_rate as f32
    } else {
        0.0
    };

    // Estimate BPM using onset detection
    let bpm_estimate = estimate_bpm(&samples, sample_rate);

    // Classify the audio type based on spectral content and filename hints
    let audio_type = classify_audio_type(&name, &folder, &samples, sample_rate, duration_secs);

    // Detect the feeling/mood
    let feeling = detect_feeling(&name, &folder, &samples, sample_rate);

    // Generate tags from all analysis
    let tags = generate_tags(
        &name,
        &folder,
        &audio_type,
        &feeling,
        duration_secs,
        bpm_estimate,
    );

    Ok(UserSampleInfo {
        name,
        path: path_str,
        file_type: ext,
        duration_secs,
        sample_rate,
        bpm_estimate,
        audio_type,
        feeling,
        tags,
        folder,
    })
}

/// Estimate BPM from audio using onset detection (energy-based)
fn estimate_bpm(samples: &[f32], sample_rate: u32) -> Option<f32> {
    if samples.len() < (sample_rate as usize) {
        return None; // Too short for meaningful BPM detection
    }

    let hop_size = sample_rate as usize / 20; // 50ms hops
    let frame_size = hop_size * 2;

    if samples.len() < frame_size {
        return None;
    }

    // Compute energy in each frame
    let mut energies: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + frame_size <= samples.len() {
        let energy: f32 = samples[i..i + frame_size]
            .iter()
            .map(|s| s * s)
            .sum::<f32>()
            / frame_size as f32;
        energies.push(energy);
        i += hop_size;
    }

    if energies.len() < 4 {
        return None;
    }

    // Compute spectral flux (onset strength)
    let mut onset_strength: Vec<f32> = Vec::new();
    onset_strength.push(0.0);
    for j in 1..energies.len() {
        let diff = (energies[j] - energies[j - 1]).max(0.0);
        onset_strength.push(diff);
    }

    // Normalize onset strength
    let max_onset = onset_strength.iter().cloned().fold(0.0f32, f32::max);
    if max_onset < 1e-6 {
        return None;
    }
    for v in onset_strength.iter_mut() {
        *v /= max_onset;
    }

    // Find peaks in onset strength (threshold: 0.3)
    let threshold = 0.3;
    let mut peak_positions: Vec<usize> = Vec::new();
    for j in 1..onset_strength.len() - 1 {
        if onset_strength[j] > threshold
            && onset_strength[j] >= onset_strength[j - 1]
            && onset_strength[j] >= onset_strength[j + 1]
        {
            peak_positions.push(j);
        }
    }

    if peak_positions.len() < 2 {
        return None;
    }

    // Calculate intervals between peaks
    let mut intervals: Vec<f32> = Vec::new();
    for j in 1..peak_positions.len() {
        let interval_samples = (peak_positions[j] - peak_positions[j - 1]) as f32 * hop_size as f32;
        let interval_secs = interval_samples / sample_rate as f32;
        if interval_secs > 0.2 && interval_secs < 2.0 {
            // Reasonable range: 30-300 BPM
            intervals.push(interval_secs);
        }
    }

    if intervals.is_empty() {
        return None;
    }

    // Median interval
    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_interval = intervals[intervals.len() / 2];

    let raw_bpm = 60.0 / median_interval;

    // Normalize to standard range (60-200 BPM)
    let bpm = if raw_bpm < 60.0 {
        raw_bpm * 2.0
    } else if raw_bpm > 200.0 {
        raw_bpm / 2.0
    } else {
        raw_bpm
    };

    // Round to nearest integer
    Some((bpm * 10.0).round() / 10.0)
}

/// Classify audio type based on filename, spectral content, and duration
fn classify_audio_type(
    name: &str,
    folder: &str,
    samples: &[f32],
    sample_rate: u32,
    duration: f32,
) -> String {
    let name_lower = name.to_lowercase();
    let folder_lower = folder.to_lowercase();
    let context = format!("{} {}", name_lower, folder_lower);

    // Filename-based classification (most reliable)
    if context.contains("kick")
        || context.contains("bd_")
        || context.contains("bassdrum")
        || context.contains("bass_drum")
    {
        return "drums".to_string();
    }
    if context.contains("snare") || context.contains("sd_") || context.contains("clap") {
        return "drums".to_string();
    }
    if context.contains("hihat")
        || context.contains("hh_")
        || context.contains("hat_")
        || context.contains("cymbal")
    {
        return "drums".to_string();
    }
    if context.contains("drum")
        || context.contains("perc")
        || context.contains("tom_")
        || context.contains("rim")
    {
        return "drums".to_string();
    }
    if context.contains("vocal")
        || context.contains("voice")
        || context.contains("vox")
        || context.contains("sing")
        || context.contains("choir")
    {
        return "vocal".to_string();
    }
    if context.contains("bass") || context.contains("sub_") || context.contains("808") {
        return "bass".to_string();
    }
    if context.contains("pad")
        || context.contains("ambient")
        || context.contains("atmo")
        || context.contains("drone")
    {
        return "pad".to_string();
    }
    if context.contains("fx")
        || context.contains("sfx")
        || context.contains("riser")
        || context.contains("impact")
        || context.contains("sweep")
        || context.contains("whoosh")
    {
        return "fx".to_string();
    }
    if context.contains("loop") || context.contains("break") {
        return "loop".to_string();
    }
    if context.contains("lead")
        || context.contains("melody")
        || context.contains("synth")
        || context.contains("pluck")
        || context.contains("key")
        || context.contains("piano")
        || context.contains("guitar")
    {
        return "instrumental".to_string();
    }

    // Duration-based heuristics
    if duration < 0.5 {
        return "one-shot".to_string();
    }

    // Spectral analysis for unknown samples
    if !samples.is_empty() && sample_rate > 0 {
        // Check zero-crossing rate (high = percussive/noise, low = tonal)
        let zcr = zero_crossing_rate(samples);

        // Check RMS energy distribution
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

        // High ZCR + short duration = likely drums/percussion
        if zcr > 0.15 && duration < 1.5 {
            return "drums".to_string();
        }

        // Very low frequency content = likely bass
        let low_energy_ratio = spectral_low_ratio(samples, sample_rate);
        if low_energy_ratio > 0.7 {
            return "bass".to_string();
        }

        // Long duration with low RMS variation = likely pad
        if duration > 3.0 && rms < 0.3 {
            return "pad".to_string();
        }
    }

    if duration > 2.0 {
        "loop".to_string()
    } else {
        "one-shot".to_string()
    }
}

/// Detect the feeling/mood of an audio sample
fn detect_feeling(name: &str, folder: &str, samples: &[f32], _sample_rate: u32) -> String {
    let context = format!("{} {}", name.to_lowercase(), folder.to_lowercase());

    // Filename-based mood detection
    if context.contains("dark")
        || context.contains("horror")
        || context.contains("evil")
        || context.contains("sinister")
    {
        return "dark".to_string();
    }
    if context.contains("bright")
        || context.contains("happy")
        || context.contains("joy")
        || context.contains("upbeat")
        || context.contains("uplifting")
    {
        return "bright".to_string();
    }
    if context.contains("calm")
        || context.contains("chill")
        || context.contains("soft")
        || context.contains("gentle")
        || context.contains("relax")
    {
        return "calm".to_string();
    }
    if context.contains("aggro")
        || context.contains("aggressive")
        || context.contains("hard")
        || context.contains("heavy")
        || context.contains("distort")
    {
        return "aggressive".to_string();
    }
    if context.contains("energy")
        || context.contains("power")
        || context.contains("pump")
        || context.contains("drive")
        || context.contains("hype")
    {
        return "energetic".to_string();
    }
    if context.contains("mellow")
        || context.contains("smooth")
        || context.contains("warm")
        || context.contains("lo-fi")
        || context.contains("lofi")
    {
        return "mellow".to_string();
    }

    // Spectral analysis for mood
    if !samples.is_empty() {
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let crest_factor = if rms > 0.0 { peak / rms } else { 1.0 };

        if rms > 0.4 && crest_factor < 3.0 {
            return "aggressive".to_string();
        }
        if rms > 0.25 {
            return "energetic".to_string();
        }
        if rms < 0.08 {
            return "calm".to_string();
        }
    }

    "neutral".to_string()
}

/// Generate tags for a sample based on all analysis data
fn generate_tags(
    name: &str,
    folder: &str,
    audio_type: &str,
    feeling: &str,
    duration: f32,
    bpm: Option<f32>,
) -> Vec<String> {
    let mut tags = Vec::new();

    // Add the audio type as a tag
    tags.push(audio_type.to_string());

    // Add the feeling as a tag
    if feeling != "neutral" {
        tags.push(feeling.to_string());
    }

    // Duration categories
    if duration < 0.3 {
        tags.push("short".to_string());
    } else if duration < 2.0 {
        tags.push("medium".to_string());
    } else if duration < 10.0 {
        tags.push("long".to_string());
    } else {
        tags.push("extra-long".to_string());
    }

    // BPM tags
    if let Some(b) = bpm {
        if b < 90.0 {
            tags.push("slow".to_string());
        } else if b < 130.0 {
            tags.push("mid-tempo".to_string());
        } else if b < 160.0 {
            tags.push("fast".to_string());
        } else {
            tags.push("very-fast".to_string());
        }
    }

    // Filename-based extra tags
    let name_lower = name.to_lowercase();
    let folder_lower = folder.to_lowercase();
    let ctx = format!("{} {}", name_lower, folder_lower);

    let keyword_tags = [
        ("vintage", "vintage"),
        ("retro", "retro"),
        ("analog", "analog"),
        ("digital", "digital"),
        ("electronic", "electronic"),
        ("acoustic", "acoustic"),
        ("wet", "wet"),
        ("dry", "dry"),
        ("reverb", "reverb"),
        ("delay", "delay"),
        ("distort", "distorted"),
        ("clean", "clean"),
        ("mono", "mono"),
        ("stereo", "stereo"),
        ("minor", "minor"),
        ("major", "major"),
        ("trap", "trap"),
        ("house", "house"),
        ("techno", "techno"),
        ("dnb", "dnb"),
        ("dubstep", "dubstep"),
        ("hip_hop", "hip-hop"),
        ("jazz", "jazz"),
        ("rock", "rock"),
        ("pop", "pop"),
        ("cinematic", "cinematic"),
        ("orchestral", "orchestral"),
    ];

    for (keyword, tag) in keyword_tags {
        if ctx.contains(keyword) && !tags.contains(&tag.to_string()) {
            tags.push(tag.to_string());
        }
    }

    tags
}

/// Calculate zero-crossing rate of audio samples
fn zero_crossing_rate(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let crossings = samples
        .windows(2)
        .filter(|w| (w[0] >= 0.0 && w[1] < 0.0) || (w[0] < 0.0 && w[1] >= 0.0))
        .count();
    crossings as f32 / (samples.len() - 1) as f32
}

/// Calculate ratio of energy in low frequencies (< 300 Hz) using simple band analysis
fn spectral_low_ratio(samples: &[f32], sample_rate: u32) -> f32 {
    if samples.is_empty() || sample_rate == 0 {
        return 0.5;
    }

    // Simple approach: low-pass filter and compare energy
    let cutoff = 300.0;
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
    let dt = 1.0 / sample_rate as f32;
    let alpha = dt / (rc + dt);

    let mut lp = 0.0f32;
    let mut low_energy = 0.0f32;
    let mut total_energy = 0.0f32;

    for &s in samples.iter().take(sample_rate as usize * 2) {
        // Analyze first 2 seconds
        lp = lp + alpha * (s - lp);
        low_energy += lp * lp;
        total_energy += s * s;
    }

    if total_energy < 1e-10 {
        return 0.5;
    }

    low_energy / total_energy
}

// ============================================================
// SUPERCOLLIDER COMMANDS
// ============================================================

#[derive(Debug, Clone, Serialize)]
struct ScStatus {
    available: bool,
    booted: bool,
    enabled: bool,
    message: String,
}

#[tauri::command]
fn init_supercollider(state: tauri::State<Arc<AppState>>) -> Result<ScStatus, String> {
    eprintln!("[SC] Initializing SuperCollider...");

    // Get the bundle directory (may have been resolved from Tauri resource dir)
    let bundle_dir = state.sc_bundle_dir.lock().clone();

    // Try to create the SC engine (tries bundle dir first, then system install)
    match ScEngine::new(bundle_dir) {
        Ok(sc) => {
            // Try to boot scsynth
            match sc.boot() {
                Ok(()) => {
                    let status = ScStatus {
                        available: true,
                        booted: true,
                        enabled: true,
                        message: "SuperCollider engine initialized and ready".to_string(),
                    };
                    *state.sc_engine.lock() = Some(sc);
                    state.use_sc.store(true, Ordering::Relaxed);
                    eprintln!("[SC] Engine ready and enabled");
                    Ok(status)
                }
                Err(e) => {
                    let status = ScStatus {
                        available: true,
                        booted: false,
                        enabled: false,
                        message: format!("SuperCollider found but failed to boot: {}", e),
                    };
                    eprintln!("[SC] Boot failed: {}", e);
                    Ok(status)
                }
            }
        }
        Err(e) => {
            let status = ScStatus {
                available: false,
                booted: false,
                enabled: false,
                message: format!("SuperCollider not available: {}", e),
            };
            eprintln!("[SC] Not available: {}", e);
            Ok(status)
        }
    }
}

#[tauri::command]
fn sc_status(state: tauri::State<Arc<AppState>>) -> ScStatus {
    let sc = state.sc_engine.lock();
    match sc.as_ref() {
        Some(sc) => ScStatus {
            available: true,
            booted: sc.is_booted(),
            enabled: state.use_sc.load(Ordering::Relaxed),
            message: if sc.is_booted() {
                "SuperCollider engine running".to_string()
            } else {
                "SuperCollider engine not booted".to_string()
            },
        },
        None => ScStatus {
            available: false,
            booted: false,
            enabled: false,
            message: "SuperCollider not initialized".to_string(),
        },
    }
}

#[tauri::command]
fn toggle_sc_engine(enabled: bool, state: tauri::State<Arc<AppState>>) -> Result<ScStatus, String> {
    if enabled {
        // Check if SC is available and booted
        let sc = state.sc_engine.lock();
        if let Some(ref sc_eng) = *sc {
            if sc_eng.is_booted() {
                drop(sc);
                state.use_sc.store(true, Ordering::Relaxed);
                return Ok(ScStatus {
                    available: true,
                    booted: true,
                    enabled: true,
                    message: "SuperCollider engine enabled".to_string(),
                });
            }
        }
        return Err(
            "SuperCollider not available or not booted. Call init_supercollider first.".to_string(),
        );
    } else {
        state.use_sc.store(false, Ordering::Relaxed);
        Ok(ScStatus {
            available: state.sc_engine.lock().is_some(),
            booted: state
                .sc_engine
                .lock()
                .as_ref()
                .map_or(false, |sc| sc.is_booted()),
            enabled: false,
            message: "SuperCollider engine disabled, using built-in engine".to_string(),
        })
    }
}

// ─── Visualization Tauri Commands ───────────────────────────────────────────

/// Get the current visual performance snapshot.
/// Polled by the frontend at ~30fps. Non-blocking read of the shared snapshot.
#[tauri::command]
fn get_visual_snapshot(state: tauri::State<Arc<AppState>>) -> PerformanceSnapshot {
    state.visual_engine.get_snapshot()
}

/// Enable or disable the visual engine.
/// When disabled, the engine thread sleeps and consumes zero CPU.
/// Audio playback is completely unaffected either way.
#[tauri::command]
fn set_visual_enabled(enabled: bool, state: tauri::State<Arc<AppState>>) -> bool {
    state.visual_engine.set_enabled(enabled);
    state.visual_engine.is_enabled()
}

/// Check whether the visual engine is currently enabled.
#[tauri::command]
fn get_visual_enabled(state: tauri::State<Arc<AppState>>) -> bool {
    state.visual_engine.is_enabled()
}

/// Get the current visual engine configuration.
#[tauri::command]
fn get_visual_config(state: tauri::State<Arc<AppState>>) -> VisualEngineConfig {
    state.visual_engine.get_config()
}

/// Update the visual engine configuration at runtime.
/// Changes take effect on the next frame. Audio is never affected.
#[tauri::command]
fn set_visual_config(config: VisualEngineConfig, state: tauri::State<Arc<AppState>>) -> VisualEngineConfig {
    state.visual_engine.set_config(config);
    state.visual_engine.get_config()
}

// ──────────────────────────────────────────────
// Parity Analysis — Deep validation of Sonic Pi compatibility
// ──────────────────────────────────────────────

/// Result of a comprehensive parity analysis
#[derive(Debug, Clone, Serialize)]
struct ParityReport {
    /// Overall parity score (0.0 – 1.0)
    score: f32,
    /// Total features used in the code
    features_used: usize,
    /// Features fully supported
    features_supported: usize,
    /// Features with partial support
    features_partial: usize,
    /// Features unsupported
    features_unsupported: usize,
    /// Detailed findings per category
    categories: Vec<ParityCategory>,
    /// Specific fix suggestions
    suggestions: Vec<ParitySuggestion>,
    /// Parse warnings from the code
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ParityCategory {
    name: String,
    status: String, // "full", "partial", "unsupported", "unused"
    items: Vec<ParityItem>,
}

#[derive(Debug, Clone, Serialize)]
struct ParityItem {
    feature: String,
    status: String, // "supported", "partial", "unsupported"
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct ParitySuggestion {
    severity: String, // "error", "warning", "info"
    feature: String,
    message: String,
    /// Optional replacement code
    fix: Option<String>,
}

/// Recursively collect parsed command usage stats
fn collect_usage(
    cmds: &[ParsedCommand],
    synths: &mut Vec<String>,
    samples: &mut Vec<String>,
    effects: &mut Vec<String>,
    constructs: &mut Vec<String>,
    sample_params: &mut Vec<String>,
) {
    for cmd in cmds {
        match cmd {
            ParsedCommand::PlayNote { synth_type, params, .. } => {
                synths.push(format!("{:?}", synth_type).to_lowercase());
                for (k, _) in params {
                    if !constructs.contains(&format!("param:{}", k)) {
                        constructs.push(format!("param:{}", k));
                    }
                }
            }
            ParsedCommand::PlaySample { name, beat_stretch, start, finish, lpf, hpf, envelope, sustain_beats, .. } => {
                samples.push(name.clone());
                if beat_stretch.is_some() { sample_params.push("beat_stretch".into()); }
                if start.is_some() { sample_params.push("start".into()); }
                if finish.is_some() { sample_params.push("finish".into()); }
                if lpf.is_some() { sample_params.push("lpf".into()); }
                if hpf.is_some() { sample_params.push("hpf".into()); }
                if envelope.is_some() { sample_params.push("envelope".into()); }
                if sustain_beats.is_some() { sample_params.push("sustain".into()); }
            }
            ParsedCommand::SetSynth(osc) => {
                synths.push(format!("{:?}", osc).to_lowercase());
            }
            ParsedCommand::WithFx { fx_type, commands, params, .. } => {
                effects.push(fx_type.clone());
                for (k, _) in params {
                    if !constructs.contains(&format!("fx_param:{}:{}", fx_type, k)) {
                        constructs.push(format!("fx_param:{}:{}", fx_type, k));
                    }
                }
                collect_usage(commands, synths, samples, effects, constructs, sample_params);
            }
            ParsedCommand::Loop { commands, sync_with, .. } => {
                constructs.push("live_loop".into());
                if sync_with.is_some() {
                    constructs.push("sync_param".into());
                }
                collect_usage(commands, synths, samples, effects, constructs, sample_params);
            }
            ParsedCommand::TimesLoop { commands, .. } => {
                constructs.push("times_loop".into());
                collect_usage(commands, synths, samples, effects, constructs, sample_params);
            }
            ParsedCommand::ConditionalRandom { command, .. } => {
                constructs.push("one_in".into());
                collect_usage(&[(**command).clone()], synths, samples, effects, constructs, sample_params);
            }
            ParsedCommand::AtBlock { commands, .. } => {
                constructs.push("at_block".into());
                collect_usage(commands, synths, samples, effects, constructs, sample_params);
            }
            ParsedCommand::SwingBlock { commands, .. } => {
                constructs.push("with_swing".into());
                collect_usage(commands, synths, samples, effects, constructs, sample_params);
            }
            ParsedCommand::Cue(_) => { constructs.push("cue".into()); }
            ParsedCommand::Sync(_) => { constructs.push("sync".into()); }
            ParsedCommand::Stop => { constructs.push("stop".into()); }
            ParsedCommand::SetVariable { .. } => { constructs.push("variable".into()); }
            ParsedCommand::Sleep(_) => { constructs.push("sleep".into()); }
            ParsedCommand::SetBpm(_) => { constructs.push("use_bpm".into()); }
            ParsedCommand::SetVolume(_) => { constructs.push("set_volume".into()); }
            ParsedCommand::Log(_) | ParsedCommand::Comment(_) => {}
            ParsedCommand::SleepUntil(_) => { constructs.push("sleep_until".into()); }
        }
    }
}

#[tauri::command]
fn validate_parity(code: String, state: tauri::State<Arc<AppState>>) -> Result<ParityReport, String> {
    let parse_result = validate_and_parse(&code)
        .map_err(|e| format!("Parse error: {}", e))?;

    let warnings: Vec<String> = parse_result.warnings.iter()
        .map(|w| if w.line > 0 {
            format!("Line {}: {} — '{}'", w.line, w.message, w.source_text)
        } else {
            format!("{} — '{}'", w.message, w.source_text)
        })
        .collect();

    // Collect usage statistics
    let mut synths_used = Vec::new();
    let mut samples_used = Vec::new();
    let mut effects_used = Vec::new();
    let mut constructs_used = Vec::new();
    let mut sample_params_used = Vec::new();

    collect_usage(
        &parse_result.commands,
        &mut synths_used,
        &mut samples_used,
        &mut effects_used,
        &mut constructs_used,
        &mut sample_params_used,
    );

    // Deduplicate
    synths_used.sort(); synths_used.dedup();
    samples_used.sort(); samples_used.dedup();
    effects_used.sort(); effects_used.dedup();
    constructs_used.sort(); constructs_used.dedup();
    sample_params_used.sort(); sample_params_used.dedup();

    // Try converting to audio commands
    let effective_bpm = {
        let s = state.engine.state.lock();
        s.bpm
    };
    let audio_events = commands_to_audio(&parse_result.commands, effective_bpm);
    let note_count = audio_events.iter().filter(|(_, cmd)| matches!(cmd, AudioCommand::PlayNote { .. })).count();
    let sample_count = audio_events.iter().filter(|(_, cmd)| matches!(cmd, AudioCommand::PlaySample { .. })).count();

    // Build parity categories
    let mut categories = Vec::new();
    let mut suggestions = Vec::new();
    let mut total_features = 0usize;
    let mut supported = 0usize;
    let mut partial = 0usize;
    let mut unsupported = 0usize;

    // --- Synth category ---
    let fully_supported_synths = [
        "sine", "saw", "square", "triangle", "noise", "pulse", "supersaw", "tb303",
        "prophet", "blade", "pluck", "fm", "beep", "darkambience", "hollow", "growl",
        "prettybell", "dullbell", "chiplead", "chipbass", "chipnoise", "techsaws",
        "hoover", "zawa", "modfm", "modsine", "modsaw", "modtri", "modpulse",
        "dsaw", "dpulse", "dtri", "subpulse", "gabberkick", "piano",
        "brownnoise", "pinknoise", "greynoise", "clipnoise",
    ];
    let mut synth_items = Vec::new();
    for s in &synths_used {
        let clean = s.replace("_", "").to_lowercase();
        total_features += 1;
        if fully_supported_synths.iter().any(|fs| clean.contains(fs)) {
            supported += 1;
            synth_items.push(ParityItem {
                feature: s.clone(),
                status: "supported".into(),
                detail: "Full oscillator parity with Sonic Pi".into(),
            });
        } else {
            partial += 1;
            synth_items.push(ParityItem {
                feature: s.clone(),
                status: "partial".into(),
                detail: "Approximated — may differ from Sonic Pi".into(),
            });
            suggestions.push(ParitySuggestion {
                severity: "warning".into(),
                feature: format!("synth:{}", s),
                message: format!("Synth '{}' has approximate parity — timbre may differ", s),
                fix: None,
            });
        }
    }
    categories.push(ParityCategory {
        name: "Synths".into(),
        status: if synth_items.is_empty() { "unused".into() }
            else if synth_items.iter().all(|i| i.status == "supported") { "full".into() }
            else { "partial".into() },
        items: synth_items,
    });

    // --- Effects category ---
    let fully_supported_fx = [
        "reverb", "gverb", "echo", "delay", "distortion", "lpf", "rlpf",
        "hpf", "rhpf", "flanger", "chorus", "ring_mod", "wobble", "ixi_techno",
        "octaver", "pan", "slicer", "bitcrusher", "krush", "compressor", "normaliser",
        "normalizer", "bpf", "rbpf", "nbpf", "nrbpf", "nrlpf", "nrhpf",
        "tremolo", "ping_pong", "level", "mono", "band_eq", "tanh",
        "whammy", "pitch_shift",
    ];
    let unsupported_fx: [&str; 0] = [];
    let mut fx_items = Vec::new();
    for f in &effects_used {
        total_features += 1;
        let fl = f.to_lowercase();
        if fully_supported_fx.contains(&fl.as_str()) {
            supported += 1;
            fx_items.push(ParityItem {
                feature: f.clone(),
                status: "supported".into(),
                detail: "Full effect parity with Sonic Pi".into(),
            });
        } else if unsupported_fx.contains(&fl.as_str()) {
            unsupported += 1;
            fx_items.push(ParityItem {
                feature: f.clone(),
                status: "unsupported".into(),
                detail: "Not implemented in PiBeat".into(),
            });
            suggestions.push(ParitySuggestion {
                severity: "error".into(),
                feature: format!("fx:{}", f),
                message: format!("Effect '{}' is not available in PiBeat", f),
                fix: Some(suggest_fx_replacement(&fl)),
            });
        } else {
            partial += 1;
            fx_items.push(ParityItem {
                feature: f.clone(),
                status: "partial".into(),
                detail: "May have limited parameter support".into(),
            });
        }
    }
    categories.push(ParityCategory {
        name: "Effects".into(),
        status: if fx_items.is_empty() { "unused".into() }
            else if fx_items.iter().all(|i| i.status == "supported") { "full".into() }
            else if fx_items.iter().any(|i| i.status == "unsupported") { "partial".into() }
            else { "partial".into() },
        items: fx_items,
    });

    // --- Sample features category ---
    let mut sample_items = Vec::new();
    let supported_sample_params = ["beat_stretch", "start", "finish", "sustain", "envelope", "lpf", "hpf"];
    for p in &sample_params_used {
        total_features += 1;
        if supported_sample_params.contains(&p.as_str()) {
            supported += 1;
            sample_items.push(ParityItem {
                feature: p.clone(),
                status: "supported".into(),
                detail: "Fully implemented".into(),
            });
        } else {
            partial += 1;
            sample_items.push(ParityItem {
                feature: p.clone(),
                status: "partial".into(),
                detail: "Approximate implementation".into(),
            });
        }
    }
    categories.push(ParityCategory {
        name: "Sample Features".into(),
        status: if sample_items.is_empty() { "unused".into() }
            else if sample_items.iter().all(|i| i.status == "supported") { "full".into() }
            else { "partial".into() },
        items: sample_items,
    });

    // --- Language constructs category ---
    let full_constructs = [
        "live_loop", "times_loop", "sleep", "use_bpm", "set_volume",
        "variable", "one_in", "at_block", "stop",
    ];
    let partial_constructs = [
        ("sync", "Parsed and logged but does not block — threads start immediately"),
        ("cue", "Parsed and logged but does not trigger any waiting threads"),
        ("sync_param", "sync: parameter on live_loop is parsed but not enforced"),
    ];
    let mut construct_items = Vec::new();
    for c in &constructs_used {
        total_features += 1;
        if full_constructs.contains(&c.as_str()) {
            supported += 1;
            construct_items.push(ParityItem {
                feature: c.clone(),
                status: "supported".into(),
                detail: "Full parity".into(),
            });
        } else if let Some((_, detail)) = partial_constructs.iter().find(|(name, _)| name == c) {
            partial += 1;
            construct_items.push(ParityItem {
                feature: c.clone(),
                status: "partial".into(),
                detail: detail.to_string(),
            });
            suggestions.push(ParitySuggestion {
                severity: "warning".into(),
                feature: c.clone(),
                message: detail.to_string(),
                fix: if c == "sync" || c == "cue" || c == "sync_param" {
                    Some("Use separate live_loop blocks — they run concurrently without sync".into())
                } else { None },
            });
        } else {
            supported += 1; // Default to supported for unknown constructs that parsed correctly
            construct_items.push(ParityItem {
                feature: c.clone(),
                status: "supported".into(),
                detail: "Parsed successfully".into(),
            });
        }
    }
    categories.push(ParityCategory {
        name: "Language Constructs".into(),
        status: if construct_items.is_empty() { "unused".into() }
            else if construct_items.iter().all(|i| i.status == "supported") { "full".into() }
            else { "partial".into() },
        items: construct_items,
    });

    // --- Audio output summary ---
    categories.push(ParityCategory {
        name: "Audio Output".into(),
        status: "full".into(),
        items: vec![
            ParityItem {
                feature: "Notes generated".into(),
                status: "supported".into(),
                detail: format!("{} note events scheduled", note_count),
            },
            ParityItem {
                feature: "Samples generated".into(),
                status: "supported".into(),
                detail: format!("{} sample events scheduled", sample_count),
            },
            ParityItem {
                feature: "Total events".into(),
                status: "supported".into(),
                detail: format!("{} total audio events at {} BPM", audio_events.len(), effective_bpm),
            },
        ],
    });

    // Detect code patterns that indicate parity issues
    let code_lower = code.to_lowercase();
    if code_lower.contains("control ") {
        suggestions.push(ParitySuggestion {
            severity: "warning".into(),
            feature: "control".into(),
            message: "`control` is parsed but no-op — use explicit notes with timing instead".into(),
            fix: Some("# Instead of: control s, note: :e4\n# Use:\nplay :c4, sustain: 1\nsleep 1\nplay :e4, sustain: 9".into()),
        });
    }
    if code_lower.contains("should_stop?") || code_lower.contains("time.now") {
        suggestions.push(ParitySuggestion {
            severity: "error".into(),
            feature: "unsupported_ruby".into(),
            message: "Ruby runtime features (should_stop?, Time.now) are not supported in PiBeat".into(),
            fix: None,
        });
    }
    if code_lower.contains("lambda") || code_lower.contains("proc") || code_lower.contains("-> {") {
        suggestions.push(ParitySuggestion {
            severity: "error".into(),
            feature: "lambda/proc".into(),
            message: "Ruby lambdas and procs are not supported — use `define :name do ... end` instead".into(),
            fix: Some("define :my_func do\n  # your code here\nend".into()),
        });
    }

    // Calculate score
    let total = total_features.max(1) as f32;
    let score = (supported as f32 + partial as f32 * 0.5) / total;

    Ok(ParityReport {
        score: score.min(1.0),
        features_used: total_features,
        features_supported: supported,
        features_partial: partial,
        features_unsupported: unsupported,
        categories,
        suggestions,
        warnings,
    })
}

fn suggest_fx_replacement(fx: &str) -> String {
    match fx {
        "pitch_shift" => "Use `rate:` parameter on samples or pitch via synth frequency".into(),
        "whammy" => "Use `with_fx :wobble` for similar LFO modulation".into(),
        "band_eq" => "Use combination of `with_fx :lpf` and `with_fx :hpf` for band filtering".into(),
        "tanh" => "Use `with_fx :distortion, distort: 0.3` for soft clipping".into(),
        "vowel" => "Use `with_fx :lpf` + `with_fx :hpf` for formant-like filtering".into(),
        _ => format!("Effect '{}' is not available — check PiBeat docs for alternatives", fx),
    }
}

/// Preload samples into SuperCollider buffers and cache duration info
fn preload_samples_sc(
    parsed: &[ParsedCommand],
    sc: &ScEngine,
    samples_dir: &std::path::Path,
    sample_durations: &Mutex<HashMap<String, f32>>,
) -> Result<(), String> {
    for cmd in parsed {
        match cmd {
            ParsedCommand::PlaySample { name, .. } => {
                let path = resolve_sample_path(name, samples_dir);
                let path_str = path.to_string_lossy().to_string();
                if path.exists() {
                    // Load into SC buffer (cached internally by ScEngine)
                    sc.load_sample_buffer(&path_str)?;
                    // Also load duration info for beat_stretch calculation
                    if !sample_durations.lock().contains_key(&path_str) {
                        if let Ok((samples, sr)) = sample::load_wav(&path_str) {
                            let duration_secs = samples.len() as f32 / sr as f32;
                            sample_durations.lock().insert(path_str.clone(), duration_secs);
                        }
                    }
                } else {
                    eprintln!("[SC preload] WARNING: sample not found: {}", path_str);
                }
            }
            ParsedCommand::Loop { commands, .. }
            | ParsedCommand::WithFx { commands, .. }
            | ParsedCommand::TimesLoop { commands, .. } => {
                preload_samples_sc(commands, sc, samples_dir, sample_durations)?;
            }
            ParsedCommand::ConditionalRandom { command, .. } => {
                preload_samples_sc(&[(**command).clone()], sc, samples_dir, sample_durations)?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create recorder first (we'll get sample rate from it)
    let recorder = Recorder::new(44100); // Default, will be updated

    // Create engine with recorder
    let engine = AudioEngine::new(recorder.clone()).expect("Failed to initialize audio engine");

    let sample_rate = {
        let s = engine.state.lock();
        s.sample_rate
    };

    // Update recorder with correct sample rate if needed
    let recorder = if sample_rate != 44100 {
        Recorder::new(sample_rate)
    } else {
        recorder
    };

    // Recreate engine with correct sample rate recorder if needed
    let engine = if sample_rate != 44100 {
        AudioEngine::new(recorder.clone()).expect("Failed to initialize audio engine")
    } else {
        engine
    };

    // Set up samples directory
    let samples_dir = sample::get_samples_dir();
    let _ = sample::ensure_default_samples(&samples_dir);

    // Discover bundled SC files (checks exe dir, dev paths, env var)
    let sc_bundle_dir = find_sc_bundle_dir();
    if let Some(ref dir) = sc_bundle_dir {
        eprintln!("[init] Found SC bundle at: {}", dir.display());
    } else {
        eprintln!("[init] No SC bundle found, will try system install or on-demand init");
    }

    // Try to initialize SuperCollider engine (non-blocking, fails gracefully)
    let (sc_engine, use_sc) = match ScEngine::new(sc_bundle_dir.clone()) {
        Ok(sc) => {
            eprintln!("[init] SuperCollider found, attempting boot...");
            match sc.boot() {
                Ok(()) => {
                    eprintln!("[init] SuperCollider engine booted successfully!");
                    (Some(sc), true)
                }
                Err(e) => {
                    eprintln!(
                        "[init] SuperCollider boot failed: {} — using built-in engine",
                        e
                    );
                    (None, false)
                }
            }
        }
        Err(e) => {
            eprintln!(
                "[init] SuperCollider not found: {} — using built-in engine",
                e
            );
            (None, false)
        }
    };

    // Start the visual engine (runs on its own thread, independent of audio)
    let (visual_engine, visual_bridge) = VisualEngine::start(VisualEngineConfig::default());
    let visual_publisher = visual_bridge.publisher();

    let app_state = Arc::new(AppState {
        engine,
        sc_engine: Mutex::new(sc_engine),
        use_sc: AtomicBool::new(use_sc),
        sc_bundle_dir: Mutex::new(sc_bundle_dir),
        recorder,
        samples_dir,
        loaded_samples: Mutex::new(HashMap::new()),
        sample_durations: Mutex::new(HashMap::new()),
        session_id: AtomicU64::new(0),
        log_messages: Mutex::new(Vec::new()),
        user_samples_dir: Mutex::new(None),
        active_line_intervals: Mutex::new(Vec::new()),
        playback_start: Mutex::new(None),
        is_paused: AtomicBool::new(false),
        visual_engine,
        visual_publisher,
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state.clone())
        .setup(move |app| {
            // Register global shortcuts individually so one failure doesn't block others
            {
                use tauri::Manager;
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
                let gs = app.global_shortcut();
                // Unregister all shortcuts first to avoid "already registered" errors
                // (e.g. from a previous crash that didn't clean up)
                let _ = gs.unregister_all();
                for shortcut_str in &["Alt+R", "Alt+S", "Alt+Shift+R"] {
                    match gs.on_shortcut(*shortcut_str, |app, shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            use tauri::Emitter;
                            let _ = app.emit("global-shortcut", shortcut.into_string());
                        }
                    }) {
                        Ok(_) => eprintln!("[shortcuts] Registered {}", shortcut_str),
                        Err(e) => eprintln!("[shortcuts] Failed to register {}: {}", shortcut_str, e),
                    }
                }
            }

            // Try to resolve SC bundle from Tauri's resource directory
            // This handles production builds where resources are bundled with the app
            use tauri::Manager;
            if app_state.sc_bundle_dir.lock().is_none() {
                if let Ok(resource_dir) = app.path().resource_dir() {
                    let sc_dir = resource_dir.join("sc-bundle");
                    #[cfg(target_os = "windows")]
                    let has_scsynth = sc_dir.join("scsynth.exe").exists();
                    #[cfg(not(target_os = "windows"))]
                    let has_scsynth = sc_dir.join("scsynth").exists();

                    if has_scsynth {
                        eprintln!(
                            "[init] Found SC bundle in Tauri resources: {}",
                            sc_dir.display()
                        );
                        *app_state.sc_bundle_dir.lock() = Some(sc_dir.clone());

                        // If SC wasn't initialized yet, try now with the resource path
                        if app_state.sc_engine.lock().is_none() {
                            eprintln!("[init] Attempting SC init from Tauri resource bundle...");
                            match ScEngine::new(Some(sc_dir)) {
                                Ok(sc) => match sc.boot() {
                                    Ok(()) => {
                                        eprintln!("[init] SC engine booted from resource bundle!");
                                        *app_state.sc_engine.lock() = Some(sc);
                                        app_state.use_sc.store(true, Ordering::Relaxed);
                                    }
                                    Err(e) => {
                                        eprintln!("[init] SC boot from resource failed: {}", e)
                                    }
                                },
                                Err(e) => eprintln!("[init] SC init from resource failed: {}", e),
                            }
                        }
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            run_code,
            stop_audio,
            pause_audio,
            resume_audio,
            get_waveform,
            get_status,
            get_active_lines,
            set_volume,
            set_bpm,
            start_recording,
            stop_recording,
            list_samples,
            get_logs,
            clear_logs,
            set_effects,
            play_sample_file,
            preview_synth,
            save_recording,
            get_env_var,
            save_code_to_file,
            read_code_from_file,
            init_supercollider,
            sc_status,
            toggle_sc_engine,
            set_user_samples_dir,
            get_user_samples_dir,
            scan_user_samples,
            discover_user_samples,
            analyze_user_sample,
            get_sample_peaks,
            get_sample_durations,
            get_visual_snapshot,
            set_visual_enabled,
            get_visual_enabled,
            get_visual_config,
            set_visual_config,
            validate_parity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
