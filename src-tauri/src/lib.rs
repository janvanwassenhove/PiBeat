mod audio;

use audio::engine::{AudioCommand, AudioEngine};
use audio::parser::{commands_to_audio, parse_code, ParsedCommand};
use audio::recorder::Recorder;
use audio::sample::{self, SampleInfo};
use audio::synth::{Envelope, OscillatorType};
use audio::sc_engine::{ScEngine, find_sc_bundle_dir};


use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Windows high-resolution timer (1ms precision for scheduler thread)
#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(uPeriod: u32) -> u32;
    fn timeEndPeriod(uPeriod: u32) -> u32;
}

struct AppState {
    engine: AudioEngine,
    sc_engine: Mutex<Option<ScEngine>>,
    use_sc: AtomicBool,
    sc_bundle_dir: Mutex<Option<PathBuf>>,
    recorder: Recorder,
    samples_dir: PathBuf,
    loaded_samples: Mutex<HashMap<String, (Vec<f32>, u32)>>,
    session_id: Mutex<u64>,
    log_messages: Mutex<Vec<LogEntry>>,
    user_samples_dir: Mutex<Option<PathBuf>>,
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
    pub file_type: String,       // "wav", "mp3"
    pub duration_secs: f32,
    pub sample_rate: u32,
    pub bpm_estimate: Option<f32>,
    pub audio_type: String,      // "drums", "vocal", "instrumental", "bass", "pad", "fx", "loop", "one-shot", "unknown"
    pub feeling: String,         // "energetic", "calm", "dark", "bright", "aggressive", "mellow", "neutral"
    pub tags: Vec<String>,
    pub folder: String,          // subfolder relative to user samples root
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

    // Parse the code
    let parsed = match parse_code(&code) {
        Ok(p) => {
            eprintln!("[run_code] Parsed {} top-level commands in {:.1}ms",
                p.len(), start.elapsed().as_secs_f64() * 1000.0);
            logs.push(LogEntry {
                timestamp: start.elapsed().as_secs_f64(),
                level: "info".to_string(),
                message: format!("Parsed {} top-level commands", p.len()),
            });
            p
        }
        Err(e) => {
            eprintln!("[run_code] Parse error: {}", e);
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

    // Log parsed structure summary
    let mut loop_count = 0;
    let mut sample_count = 0;
    let mut note_count = 0;
    for cmd in &parsed {
        match cmd {
            ParsedCommand::Loop { name, commands, .. } => {
                loop_count += 1;
                let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                eprintln!("[run_code]   live_loop :{} ({} inner cmds, stop={})",
                    name, commands.len(), has_stop);
            }
            ParsedCommand::PlaySample { name, .. } => {
                sample_count += 1;
                eprintln!("[run_code]   sample: {}", name);
            }
            ParsedCommand::PlayNote { .. } => { note_count += 1; }
            _ => {}
        }
    }
    if loop_count > 0 || sample_count > 0 || note_count > 0 {
        logs.push(LogEntry {
            timestamp: start.elapsed().as_secs_f64(),
            level: "info".to_string(),
            message: format!("{} loops, {} samples, {} notes found", loop_count, sample_count, note_count),
        });
    }
    
    // Get current BPM
    let (_, _, engine_bpm) = state.engine.get_state_snapshot();
    
    // Pre-scan parsed commands for use_bpm to get the code's intended BPM
    let effective_bpm = parsed.iter()
        .find_map(|cmd| if let ParsedCommand::SetBpm(b) = cmd { Some(*b) } else { None })
        .unwrap_or(engine_bpm);
    eprintln!("[run_code] Converting to audio commands at {} BPM (engine: {})...", effective_bpm, engine_bpm);
    
    // Convert to audio commands using the effective BPM
    let convert_start = Instant::now();
    let timed_commands = commands_to_audio(&parsed, effective_bpm);
    let convert_elapsed = convert_start.elapsed();
    eprintln!("[run_code] Generated {} timed commands in {:.1}ms",
        timed_commands.len(), convert_elapsed.as_secs_f64() * 1000.0);

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
    let max_time = timed_commands.iter()
        .map(|(t, _)| *t)
        .filter(|t| *t <= 600.0)
        .fold(0.0f32, f32::max);

    // Start a new playback session by incrementing the session ID
    // This invalidates all old scheduled threads from previous buffers
    let current_session = {
        let mut session = state.session_id.lock();
        *session = session.wrapping_add(1);
        *session
    };

    // Check if we should use SuperCollider engine
    let using_sc = state.use_sc.load(Ordering::Relaxed);

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
        let sc = sc_guard.as_ref().ok_or("SuperCollider engine not initialized")?;

        // Preload samples into SC buffers
        eprintln!("[run_code] Preloading samples into SuperCollider buffers...");
        let preload_start = Instant::now();
        match preload_samples_sc(&parsed, sc, &state.samples_dir) {
            Ok(()) => {
                eprintln!("[run_code] SC samples preloaded in {:.1}ms",
                    preload_start.elapsed().as_secs_f64() * 1000.0);
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
        eprintln!("[run_code] Scheduling {} commands via SuperCollider...", timed_commands.len());
        let max_schedule_time = 600.0f32;
        let mut scheduled_count = 0u32;

        // Build sample name → buffer ID map for this run
        let sample_names = collect_sample_names(&parsed);
        let mut sample_idx = 0usize;

        // Pre-process ALL events into a sorted schedule
        // All events go through the single scheduler thread for consistent timing
        enum ScEvent {
            PlaySample { buf_id: i32, amp: f32, rate: f32, pan: f32 },
            PlayNote { synth_type: OscillatorType, freq: f32, amp: f32, dur: f32, env: Envelope, pan: f32, params: Vec<(String, f32)> },
            SetEffect { rm: f32, dt: f32, df: f32, dist: f32, lpf: f32, hpf: f32 },
            FxStart { fx_type: String, params: Vec<(String, f32)> },
            FxEnd,
            SetBpm(f32),
            SetVolume(f32),
            Stop,
        }

        let mut all_events: Vec<(f32, ScEvent)> = Vec::new();

        for (time_offset, cmd) in &timed_commands {
            if *time_offset > max_schedule_time {
                if let AudioCommand::PlaySample { .. } = cmd {
                    sample_idx += 1;
                }
                continue;
            }

            match cmd {
                AudioCommand::PlaySample { amplitude, rate, pan, .. } => {
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
                            all_events.push((*time_offset, ScEvent::PlaySample {
                                buf_id,
                                amp: *amplitude,
                                rate: *rate,
                                pan: *pan,
                            }));
                            scheduled_count += 1;
                        } else {
                            eprintln!("[SC schedule] No buffer for sample '{}'", name);
                        }
                    }
                }
                AudioCommand::PlayNote { synth_type, frequency, amplitude, duration_secs, envelope, pan, ref params } => {
                    all_events.push((*time_offset, ScEvent::PlayNote {
                        synth_type: *synth_type,
                        freq: *frequency,
                        amp: *amplitude,
                        dur: *duration_secs,
                        env: *envelope,
                        pan: *pan,
                        params: params.clone(),
                    }));
                    scheduled_count += 1;
                }
                AudioCommand::SetEffect { reverb_mix, delay_time, delay_feedback, distortion, lpf_cutoff, hpf_cutoff } => {
                    all_events.push((*time_offset, ScEvent::SetEffect {
                        rm: *reverb_mix,
                        dt: *delay_time,
                        df: *delay_feedback,
                        dist: *distortion,
                        lpf: *lpf_cutoff,
                        hpf: *hpf_cutoff,
                    }));
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
                AudioCommand::FxStart { ref fx_type, ref params } => {
                    all_events.push((*time_offset, ScEvent::FxStart {
                        fx_type: fx_type.clone(),
                        params: params.clone(),
                    }));
                    scheduled_count += 1;
                }
                AudioCommand::FxEnd => {
                    all_events.push((*time_offset, ScEvent::FxEnd));
                    scheduled_count += 1;
                }
                AudioCommand::Stop => {
                    all_events.push((*time_offset, ScEvent::Stop));
                    scheduled_count += 1;
                }
            }
        }

        // Drop the SC lock before spawning the scheduler thread
        drop(sc_guard);

        // Sort all events by time offset for sequential processing
        all_events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let event_count = all_events.len();
        eprintln!("[run_code] Scheduling {} SC events in single scheduler thread", event_count);

        // Spawn a SINGLE scheduler thread for ALL events (including t=0)
        // This ensures consistent timing — all events use the same time reference
        if !all_events.is_empty() {
            let state_clone = Arc::clone(&*state);
            // Capture the reference time BEFORE spawning — pass it to the thread
            // so both the thread and the setup_time_ms use the same reference point
            let schedule_ref = Instant::now();
            scheduler_started = schedule_ref;
            std::thread::spawn(move || {
                // Set Windows timer resolution to 1ms for precise scheduling
                #[cfg(target_os = "windows")]
                unsafe {
                    timeBeginPeriod(1);
                }

                let start_time = schedule_ref;

                for (target_time, evt) in all_events {
                    // Check if session is still valid
                    if *state_clone.session_id.lock() != current_session {
                        eprintln!("[SC scheduler] Session cancelled, stopping scheduler");
                        #[cfg(target_os = "windows")]
                        unsafe { timeEndPeriod(1); }
                        return;
                    }

                    // Wait until the target time using high-precision timing
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let target = target_time as f64;
                    let wait = target - elapsed;
                    if wait > 0.0005 {
                        // Windows thread::sleep has ~15.6ms granularity by default.
                        // Use coarse sleep + spin-wait for precision.
                        if wait > 0.020 {
                            // Sleep for most of the time, leaving 18ms margin for spin-wait
                            let coarse = Duration::from_secs_f64((wait - 0.018).max(0.0));
                            std::thread::sleep(coarse);
                        }
                        // Spin-wait for the remaining time (up to ~18ms on Windows)
                        while (start_time.elapsed().as_secs_f64()) < target {
                            std::hint::spin_loop();
                        }
                    }

                    // Re-check session after sleeping
                    if *state_clone.session_id.lock() != current_session {
                        #[cfg(target_os = "windows")]
                        unsafe { timeEndPeriod(1); }
                        return;
                    }

                    // Execute the event
                    let sc_lock = state_clone.sc_engine.lock();
                    if let Some(ref sc) = *sc_lock {
                        match evt {
                            ScEvent::PlaySample { buf_id, amp, rate, pan } => {
                                if let Err(e) = sc.play_sample_buffer(buf_id, amp, rate, pan) {
                                    eprintln!("[SC scheduler] sample play failed: {}", e);
                                }
                            }
                            ScEvent::PlayNote { synth_type, freq, amp, dur, env, pan, ref params } => {
                                if let Err(e) = sc.play_note(synth_type, freq, amp, dur, &env, pan, params) {
                                    eprintln!("[SC scheduler] note play failed: {}", e);
                                }
                            }
                            ScEvent::SetEffect { rm, dt, df, dist, lpf, hpf } => {
                                let _ = sc.set_global_effects(rm, dt, df, dist, lpf, hpf);
                            }
                            ScEvent::SetBpm(bpm_val) => {
                                sc.state.lock().bpm = bpm_val;
                            }
                            ScEvent::SetVolume(vol) => {
                                sc.state.lock().master_volume = vol;
                            }
                            ScEvent::FxStart { ref fx_type, ref params } => {
                                if let Err(e) = sc.push_fx_bus(fx_type, params) {
                                    eprintln!("[SC scheduler] FxStart failed: {}", e);
                                }
                            }
                            ScEvent::FxEnd => {
                                if let Err(e) = sc.pop_fx_bus() {
                                    eprintln!("[SC scheduler] FxEnd failed: {}", e);
                                }
                            }
                            ScEvent::Stop => {
                                let _ = sc.stop_all();
                            }
                        }
                    }
                    drop(sc_lock);
                }
                eprintln!("[SC scheduler] All {} events played", event_count);

                // Restore default Windows timer resolution
                #[cfg(target_os = "windows")]
                unsafe { timeEndPeriod(1); }
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
                eprintln!("[run_code] Samples preloaded in {:.1}ms", preload_start.elapsed().as_secs_f64() * 1000.0);
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

        // Now schedule all commands with proper timing
        eprintln!("[run_code] Scheduling {} commands...", timed_commands.len());
        let mut scheduled_count = 0u32;
        let max_schedule_time = 600.0f32; // Cap at 10 minutes
        let engine = &state.engine;
        for (time_offset, cmd) in &timed_commands {
            // Skip commands scheduled beyond the max time
            if *time_offset > max_schedule_time {
                continue;
            }
            let cmd_to_send = match cmd {
                AudioCommand::PlaySample { .. } => {
                    continue;
                }
                other => other.clone(),
            };

            if *time_offset < 0.001 {
                engine.send_command(cmd_to_send)?;
            } else {
                // Schedule for later
                let cmd_clone = cmd_to_send.clone();
                let delay = Duration::from_secs_f32(*time_offset);
                let tx = state.engine.command_tx_clone();
                let state_clone = Arc::clone(&*state);
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    // Only send if this session is still active
                    if *state_clone.session_id.lock() == current_session {
                        if let Err(e) = tx.try_send(cmd_clone) {
                            eprintln!("[schedule] NOTE command send failed: {}", e);
                        }
                    }
                });
            }
            scheduled_count += 1;
        }
        eprintln!("[run_code] Scheduled {} non-sample commands", scheduled_count);

        // Schedule all sample playbacks with proper timing
        eprintln!("[run_code] Scheduling sample playbacks...");
        schedule_samples_with_timing(&parsed, &timed_commands, &state, current_session)?;
    }

    let total_elapsed = start.elapsed();
    eprintln!("[run_code] Total setup completed in {:.1}ms", total_elapsed.as_secs_f64() * 1000.0);

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
        setup_time_ms: scheduler_started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Preload all samples referenced in the parsed commands without playing them
fn preload_samples(parsed: &[ParsedCommand], state: &Arc<AppState>) -> Result<(), String> {
    for cmd in parsed {
        match cmd {
            ParsedCommand::PlaySample { name, .. } => {
                let mut loaded = state.loaded_samples.lock();
                let path = resolve_sample_path(name, &state.samples_dir);
                let path_str = path.to_string_lossy().to_string();
                eprintln!("[preload] sample '{}' -> resolved path '{}'", name, path_str);
                
                if !loaded.contains_key(&path_str) {
                    if path.exists() {
                        match sample::load_wav(&path_str) {
                            Ok((samples, sr)) => {
                                eprintln!("[preload] Loaded '{}': {} samples @ {}Hz", path_str, samples.len(), sr);
                                loaded.insert(path_str.clone(), (samples, sr));
                            }
                            Err(e) => {
                                eprintln!("[preload] ERROR loading '{}': {}", path_str, e);
                                return Err(format!("Failed to load sample '{}': {}", name, e));
                            }
                        }
                    } else {
                        eprintln!("[preload] WARNING: file not found '{}', using placeholder", path_str);
                        // Generate a simple placeholder beep for missing samples
                        let sr = 44100u32;
                        let dur = 0.2;
                        let n = (sr as f32 * dur) as usize;
                        let samples: Vec<f32> = (0..n)
                            .map(|i| {
                                let t = i as f32 / sr as f32;
                                (t * 440.0 * 2.0 * std::f32::consts::PI).sin()
                                    * (-t * 20.0).exp()
                            })
                            .collect();
                        loaded.insert(path_str.clone(), (samples, sr));
                    }
                }
            }
            ParsedCommand::Loop { commands, .. }
            | ParsedCommand::WithFx { commands, .. }
            | ParsedCommand::TimesLoop { commands, .. } => {
                preload_samples(commands, state)?;
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
    eprintln!("[schedule_samples] Collected {} sample names", sample_names.len());
    
    let max_schedule_time = 600.0f32; // Cap at 10 minutes
    let mut scheduled = 0u32;
    
    // Match them with PlaySample commands in timed_commands
    let mut sample_idx = 0;
    for (time_offset, cmd) in timed_commands {
        if let AudioCommand::PlaySample { amplitude, rate, pan, .. } = cmd {
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
                    eprintln!("[schedule_samples] #{} t={:.2}s '{}' -> scheduling ({} samples)", sample_idx - 1, time_offset, name, samples.len());
                    let cmd_to_send = AudioCommand::PlaySample {
                        samples: samples.clone(),
                        sample_rate: *sr,
                        amplitude: *amplitude,
                        rate: *rate,
                        pan: *pan,
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
                            if *state_clone.session_id.lock() == current_session {
                                if let Err(e) = tx.try_send(cmd_to_send) {
                                    eprintln!("[schedule_samples] SAMPLE command send failed: {}", e);
                                }
                            }
                        });
                    }
                    scheduled += 1;
                } else {
                    eprintln!("[schedule_samples] #{} MISS: '{}' not in loaded cache (resolved path: '{}')", sample_idx - 1, name, path_str);
                }
            }
        }
    }
    eprintln!("[schedule_samples] Scheduled {} sample playbacks", scheduled);
    Ok(())
}

/// Collect all sample names from parsed commands in execution order
fn collect_sample_names(parsed: &[ParsedCommand]) -> Vec<String> {
    let mut names = Vec::new();
    collect_sample_names_recursive(parsed, &mut names, 1);
    names
}

fn collect_sample_names_recursive(parsed: &[ParsedCommand], names: &mut Vec<String>, _loop_count: usize) {
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
                            (t * 440.0 * 2.0 * std::f32::consts::PI).sin()
                                * (-t * 20.0).exp()
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
            | ParsedCommand::TimesLoop { commands, .. } => {
                collect_logs(commands, logs);
            }
            _ => {}
        }
    }
}

/// Resolve a sample name to a file path.
/// Handles: full file paths, Sonic Pi built-in names, and searching the samples directory.
fn resolve_sample_path(name: &str, samples_dir: &std::path::Path) -> PathBuf {
    let trimmed = name.trim();
    eprintln!("[resolve_sample_path] input: '{}'", trimmed);

    // If it looks like an absolute file path (contains / or \\ and an extension)
    let as_path = PathBuf::from(trimmed);
    if as_path.is_absolute() {
        eprintln!("[resolve_sample_path] absolute path -> '{}' (exists={})", as_path.display(), as_path.exists());
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
    let mut session = state.session_id.lock();
    *session = session.wrapping_add(1);
    Ok("Stopped".to_string())
}

#[tauri::command]
fn get_waveform(state: tauri::State<Arc<AppState>>) -> Vec<f32> {
    if state.use_sc.load(Ordering::Relaxed) {
        if let Some(ref sc) = *state.sc_engine.lock() {
            sc.process_incoming();
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
            let (is_playing, master_volume, bpm) = sc.get_state_snapshot();
            return EngineStatus {
                is_playing,
                master_volume,
                bpm,
                is_recording: state.recorder.is_recording(),
            };
        }
    }
    let (is_playing, master_volume, bpm) = state.engine.get_state_snapshot();
    EngineStatus {
        is_playing,
        master_volume,
        bpm,
        is_recording: state.recorder.is_recording(),
    }
}

#[tauri::command]
fn set_volume(volume: f32, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    state
        .engine
        .send_command(AudioCommand::SetMasterVolume(volume))
}

#[tauri::command]
fn set_bpm(bpm: f32, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    state.engine.send_command(AudioCommand::SetBpm(bpm))
}

#[tauri::command]
fn start_recording(state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    state.recorder.start();
    Ok("Recording started".to_string())
}

#[tauri::command]
fn stop_recording(path: Option<String>, state: tauri::State<Arc<AppState>>) -> Result<String, String> {
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
    state.engine.send_command(AudioCommand::SetEffect {
        reverb_mix,
        delay_time,
        delay_feedback,
        distortion,
        lpf_cutoff,
        hpf_cutoff,
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
    })?;
    Ok("Playing sample".to_string())
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
    // Play middle C (C4 = 261.63 Hz) for 0.6 seconds
    state.engine.send_command(AudioCommand::PlayNote {
        synth_type: osc,
        frequency: 261.63,
        amplitude: 0.5,
        duration_secs: 0.6,
        envelope,
        pan: 0.0,
        params: vec![],
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
    state.user_samples_dir.lock().as_ref().map(|p| p.to_string_lossy().to_string())
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
    
    for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if ext_lower == "wav" || ext_lower == "mp3" {
                match analyze_audio_file(path, &root) {
                    Ok(info) => results.push(info),
                    Err(e) => {
                        eprintln!("[scan_user_samples] Failed to analyze {}: {}", path.display(), e);
                    }
                }
            }
        }
    }
    
    eprintln!("[scan_user_samples] Found {} audio files in {}", results.len(), dir.display());
    Ok(results)
}

/// Analyze a single audio file and produce metadata
fn analyze_audio_file(path: &std::path::Path, root: &std::path::Path) -> Result<UserSampleInfo, String> {
    let path_str = path.to_string_lossy().to_string();
    let ext = path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    
    let name = path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    
    let folder = path.parent()
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
    let tags = generate_tags(&name, &folder, &audio_type, &feeling, duration_secs, bpm_estimate);
    
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
        let energy: f32 = samples[i..i + frame_size].iter().map(|s| s * s).sum::<f32>() / frame_size as f32;
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
fn classify_audio_type(name: &str, folder: &str, samples: &[f32], sample_rate: u32, duration: f32) -> String {
    let name_lower = name.to_lowercase();
    let folder_lower = folder.to_lowercase();
    let context = format!("{} {}", name_lower, folder_lower);
    
    // Filename-based classification (most reliable)
    if context.contains("kick") || context.contains("bd_") || context.contains("bassdrum") || context.contains("bass_drum") {
        return "drums".to_string();
    }
    if context.contains("snare") || context.contains("sd_") || context.contains("clap") {
        return "drums".to_string();
    }
    if context.contains("hihat") || context.contains("hh_") || context.contains("hat_") || context.contains("cymbal") {
        return "drums".to_string();
    }
    if context.contains("drum") || context.contains("perc") || context.contains("tom_") || context.contains("rim") {
        return "drums".to_string();
    }
    if context.contains("vocal") || context.contains("voice") || context.contains("vox") || context.contains("sing") || context.contains("choir") {
        return "vocal".to_string();
    }
    if context.contains("bass") || context.contains("sub_") || context.contains("808") {
        return "bass".to_string();
    }
    if context.contains("pad") || context.contains("ambient") || context.contains("atmo") || context.contains("drone") {
        return "pad".to_string();
    }
    if context.contains("fx") || context.contains("sfx") || context.contains("riser") || context.contains("impact") || context.contains("sweep") || context.contains("whoosh") {
        return "fx".to_string();
    }
    if context.contains("loop") || context.contains("break") {
        return "loop".to_string();
    }
    if context.contains("lead") || context.contains("melody") || context.contains("synth") || context.contains("pluck") || context.contains("key") || context.contains("piano") || context.contains("guitar") {
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
    if context.contains("dark") || context.contains("horror") || context.contains("evil") || context.contains("sinister") {
        return "dark".to_string();
    }
    if context.contains("bright") || context.contains("happy") || context.contains("joy") || context.contains("upbeat") || context.contains("uplifting") {
        return "bright".to_string();
    }
    if context.contains("calm") || context.contains("chill") || context.contains("soft") || context.contains("gentle") || context.contains("relax") {
        return "calm".to_string();
    }
    if context.contains("aggro") || context.contains("aggressive") || context.contains("hard") || context.contains("heavy") || context.contains("distort") {
        return "aggressive".to_string();
    }
    if context.contains("energy") || context.contains("power") || context.contains("pump") || context.contains("drive") || context.contains("hype") {
        return "energetic".to_string();
    }
    if context.contains("mellow") || context.contains("smooth") || context.contains("warm") || context.contains("lo-fi") || context.contains("lofi") {
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
fn generate_tags(name: &str, folder: &str, audio_type: &str, feeling: &str, duration: f32, bpm: Option<f32>) -> Vec<String> {
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
        ("vintage", "vintage"), ("retro", "retro"), ("analog", "analog"),
        ("digital", "digital"), ("electronic", "electronic"), ("acoustic", "acoustic"),
        ("wet", "wet"), ("dry", "dry"), ("reverb", "reverb"),
        ("delay", "delay"), ("distort", "distorted"), ("clean", "clean"),
        ("mono", "mono"), ("stereo", "stereo"),
        ("minor", "minor"), ("major", "major"),
        ("trap", "trap"), ("house", "house"), ("techno", "techno"),
        ("dnb", "dnb"), ("dubstep", "dubstep"), ("hip_hop", "hip-hop"),
        ("jazz", "jazz"), ("rock", "rock"), ("pop", "pop"),
        ("cinematic", "cinematic"), ("orchestral", "orchestral"),
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
    let crossings = samples.windows(2)
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
    
    for &s in samples.iter().take(sample_rate as usize * 2) { // Analyze first 2 seconds
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
        return Err("SuperCollider not available or not booted. Call init_supercollider first.".to_string());
    } else {
        state.use_sc.store(false, Ordering::Relaxed);
        Ok(ScStatus {
            available: state.sc_engine.lock().is_some(),
            booted: state.sc_engine.lock().as_ref().map_or(false, |sc| sc.is_booted()),
            enabled: false,
            message: "SuperCollider engine disabled, using built-in engine".to_string(),
        })
    }
}

/// Preload samples into SuperCollider buffers
fn preload_samples_sc(
    parsed: &[ParsedCommand],
    sc: &ScEngine,
    samples_dir: &std::path::Path,
) -> Result<(), String> {
    for cmd in parsed {
        match cmd {
            ParsedCommand::PlaySample { name, .. } => {
                let path = resolve_sample_path(name, samples_dir);
                let path_str = path.to_string_lossy().to_string();
                if path.exists() {
                    // Load into SC buffer (cached internally by ScEngine)
                    sc.load_sample_buffer(&path_str)?;
                } else {
                    eprintln!("[SC preload] WARNING: sample not found: {}", path_str);
                }
            }
            ParsedCommand::Loop { commands, .. }
            | ParsedCommand::WithFx { commands, .. }
            | ParsedCommand::TimesLoop { commands, .. } => {
                preload_samples_sc(commands, sc, samples_dir)?;
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
                    eprintln!("[init] SuperCollider boot failed: {} — using built-in engine", e);
                    (None, false)
                }
            }
        }
        Err(e) => {
            eprintln!("[init] SuperCollider not found: {} — using built-in engine", e);
            (None, false)
        }
    };

    let app_state = Arc::new(AppState {
        engine,
        sc_engine: Mutex::new(sc_engine),
        use_sc: AtomicBool::new(use_sc),
        sc_bundle_dir: Mutex::new(sc_bundle_dir),
        recorder,
        samples_dir,
        loaded_samples: Mutex::new(HashMap::new()),
        session_id: Mutex::new(0),
        log_messages: Mutex::new(Vec::new()),
        user_samples_dir: Mutex::new(None),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state.clone())
        .setup(move |app| {
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
                        eprintln!("[init] Found SC bundle in Tauri resources: {}", sc_dir.display());
                        *app_state.sc_bundle_dir.lock() = Some(sc_dir.clone());

                        // If SC wasn't initialized yet, try now with the resource path
                        if app_state.sc_engine.lock().is_none() {
                            eprintln!("[init] Attempting SC init from Tauri resource bundle...");
                            match ScEngine::new(Some(sc_dir)) {
                                Ok(sc) => {
                                    match sc.boot() {
                                        Ok(()) => {
                                            eprintln!("[init] SC engine booted from resource bundle!");
                                            *app_state.sc_engine.lock() = Some(sc);
                                            app_state.use_sc.store(true, Ordering::Relaxed);
                                        }
                                        Err(e) => eprintln!("[init] SC boot from resource failed: {}", e),
                                    }
                                }
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
            get_waveform,
            get_status,
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
            init_supercollider,
            sc_status,
            toggle_sc_engine,
            set_user_samples_dir,
            get_user_samples_dir,
            scan_user_samples,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
