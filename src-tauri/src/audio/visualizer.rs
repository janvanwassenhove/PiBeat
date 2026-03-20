//! # Audio-Reactive Visualization System
//!
//! This module provides a retro arcade / pixel-art band performance visualization
//! that reacts to musical output from the audio engine.
//!
//! ## Architecture
//!
//! ```text
//!  â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”    try_send()    â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”   Arc<Mutex<Snapshot>>   â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”
//!  â”‚  Scheduler    â”‚ â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â–º â”‚ Visual Engine â”‚ â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â–º â”‚ Frontendâ”‚
//!  â”‚  Thread       â”‚   (bounded,     â”‚ Thread        â”‚   (lock-free read)     â”‚ (poll)  â”‚
//!  â”‚               â”‚    drop-on-full)â”‚ (~30 FPS)     â”‚                        â”‚         â”‚
//!  â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜                 â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜                        â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜
//!        â”‚                                 â”‚
//!        â”‚ AudioCommand                    â”‚ PerformanceSnapshot
//!        â–¼                                 â”‚
//!  â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”                        â”‚
//!  â”‚ Audio Thread  â”‚  (NEVER touched       â”‚
//!  â”‚ (cpal cb)     â”‚   by visuals)         â”‚
//!  â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜                        â”‚
//! ```
//!
//! ## Safety guarantees
//!
//! 1. The audio callback thread NEVER performs any visualization work.
//! 2. Communication is one-directional: scheduler â†’ visual engine only.
//! 3. The bounded channel uses `try_send` â€” if the visual system is slow,
//!    events are silently dropped. No backpressure to the scheduler.
//! 4. The visual engine can be completely disabled with zero behavioral change
//!    in audio playback.
//! 5. The snapshot mutex uses `parking_lot::Mutex` which is fast and never
//!    poisons; the frontend read is effectively non-blocking.

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// â”€â”€â”€ Data Model â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Band member roles for the visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandMemberRole {
    Drummer,
    Bassist,
    Guitarist,
    Keyboard,
    Vocalist,
    Dj,
    Percussionist,
}

impl BandMemberRole {
    pub fn all() -> &'static [BandMemberRole] {
        &[
            BandMemberRole::Drummer,
            BandMemberRole::Bassist,
            BandMemberRole::Guitarist,
            BandMemberRole::Keyboard,
            BandMemberRole::Vocalist,
            BandMemberRole::Dj,
            BandMemberRole::Percussionist,
        ]
    }
}

/// Broad synth category hint for mapping notes to band members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthCategory {
    Bass,       // tb303, chip_bass, sub_pulse, gabber_kick
    Lead,       // saw, square, super_saw, chip_lead, tech_saws, hoover
    Pad,        // blade, prophet, hollow, dark_ambience, zawa
    Keys,       // fm, mod_fm, mod_sine, pretty_bell, dull_bell, piano
    Pluck,      // pluck, chip_lead
    Noise,      // noise, b_noise, p_noise, g_noise, c_noise, chip_noise
    Percussive, // gabber_kick
    Default,    // sine, triangle, beep, pulse
}

impl SynthCategory {
    /// Classify an OscillatorType name (lowercase) to a synth category.
    pub fn from_synth_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("tb303") || lower.contains("chipbass") || lower.contains("chip_bass")
            || lower.contains("subpulse") || lower.contains("sub_pulse")
        {
            SynthCategory::Bass
        } else if lower.contains("gabber") {
            SynthCategory::Percussive
        } else if lower.contains("supersaw") || lower.contains("super_saw")
            || lower.contains("techsaw") || lower.contains("tech_saw")
            || lower.contains("hoover")
            || lower == "saw" || lower == "dsaw" || lower == "d_saw"
            || lower == "square" || lower == "dpulse" || lower == "d_pulse"
            || lower.contains("chiplead") || lower.contains("chip_lead")
            || lower.contains("growl")
        {
            SynthCategory::Lead
        } else if lower.contains("blade") || lower.contains("prophet") || lower.contains("hollow")
            || lower.contains("dark") || lower.contains("zawa")
        {
            SynthCategory::Pad
        } else if lower.contains("fm") || lower.contains("mod")
            || lower.contains("bell") || lower.contains("piano")
        {
            SynthCategory::Keys
        } else if lower.contains("pluck") {
            SynthCategory::Pluck
        } else if lower.contains("noise") || lower.contains("bnoise") || lower.contains("pnoise")
            || lower.contains("gnoise") || lower.contains("cnoise")
        {
            SynthCategory::Noise
        } else {
            SynthCategory::Default
        }
    }

    /// Map synth category to the most appropriate band member.
    pub fn to_role(&self) -> BandMemberRole {
        match self {
            SynthCategory::Bass => BandMemberRole::Bassist,
            SynthCategory::Lead => BandMemberRole::Guitarist,
            SynthCategory::Pad => BandMemberRole::Keyboard,
            SynthCategory::Keys => BandMemberRole::Keyboard,
            SynthCategory::Pluck => BandMemberRole::Guitarist,
            SynthCategory::Noise => BandMemberRole::Dj,
            SynthCategory::Percussive => BandMemberRole::Drummer,
            SynthCategory::Default => BandMemberRole::Vocalist,
        }
    }
}

// â”€â”€â”€ Dance Styles â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Dance style changes how characters animate. User-selectable at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DanceStyle {
    Bounce,    // Default â€” bobbing up and down
    Headbang,  // Aggressive forward head motion
    Sway,      // Gentle side-to-side rocking
    Robot,     // Rigid angular staccato
    Funk,      // Loose syncopated groove
    Rave,      // Arms up, jumping, high energy
}

impl Default for DanceStyle {
    fn default() -> Self { DanceStyle::Bounce }
}

// â”€â”€â”€ Visual Effects â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Visual post-processing effects. Multiple can be active simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualEffect {
    Scanlines,    // Retro CRT scanlines
    PixelRain,    // Matrix-style falling digits
    StarField,    // Parallax background stars
    FireTrails,   // Flame particles from stage
    MirrorBall,   // Disco ball light reflections
    NeonGlow,     // Neon outline glow on characters
}

// â”€â”€â”€ Stage Decor â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Stage decor / backdrop. Only one active at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageDecor {
    RetroStage,   // Default pixel-art stage with floor grid
    Oscilloscope, // Live waveform backdrop
    SpaceScene,   // Stars and nebula
    CityNight,    // Neon city skyline
    Matrix,       // Digital rain code
    Underwater,   // Bubbles and deep blue
}

impl Default for StageDecor {
    fn default() -> Self { StageDecor::RetroStage }
}

// ─── Camera Mode ─────────────────────────────────────────────────────────────

/// Camera view mode for the visualizer window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraMode {
    FullStage,      // Default — full band view with crowd
    StageView,      // Tighter stage framing, no crowd, slightly zoomed
    CloseUp,        // Follow the most active member, zoomed in
    ZoomCharacter,  // Lock onto a specific member (camera_focus selects which)
    Auto,           // Cycle between views based on musical activity
}

impl Default for CameraMode {
    fn default() -> Self { CameraMode::FullStage }
}

/// Animation state for the drummer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrummerState {
    Idle,
    PlaySoft,
    PlayHard,
    Fill,
    CrashHit,
}

/// Animation state for non-drummer band members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberState {
    Idle,
    Groove,
    Accent,
    Intense,
    Solo,
}

/// Unified animation state for any band member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "state")]
pub enum BandAnimationState {
    Drummer(DrummerState),
    Member(MemberState),
}

impl BandAnimationState {
    /// Returns a 0.0â€“1.0 intensity value for this animation state.
    pub fn intensity(&self) -> f32 {
        match self {
            BandAnimationState::Drummer(s) => match s {
                DrummerState::Idle => 0.0,
                DrummerState::PlaySoft => 0.3,
                DrummerState::PlayHard => 0.7,
                DrummerState::Fill => 0.9,
                DrummerState::CrashHit => 1.0,
            },
            BandAnimationState::Member(s) => match s {
                MemberState::Idle => 0.0,
                MemberState::Groove => 0.3,
                MemberState::Accent => 0.6,
                MemberState::Intense => 0.8,
                MemberState::Solo => 1.0,
            },
        }
    }
}

/// State of a single band member for the current frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandMemberSnapshot {
    pub role: BandMemberRole,
    pub animation_state: BandAnimationState,
    /// 0.0â€“1.0 progress through the current animation state (for interpolation).
    pub animation_progress: f32,
    /// 0.0â€“1.0 per-member energy level.
    pub energy: f32,
}

/// Stage lighting state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageLighting {
    /// Overall stage brightness 0.0â€“1.0.
    pub brightness: f32,
    /// Strobe active flag.
    pub strobe_active: bool,
    /// Spotlight color as [R, G, B] 0â€“255.
    pub spotlight_color: [u8; 3],
    /// Beat flash intensity 0.0â€“1.0 (decays after each beat).
    pub beat_flash: f32,
}

impl Default for StageLighting {
    fn default() -> Self {
        Self {
            brightness: 0.3,
            strobe_active: false,
            spotlight_color: [100, 60, 200],
            beat_flash: 0.0,
        }
    }
}

/// Crowd reaction state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrowdState {
    /// 0.0â€“1.0 overall excitement level.
    pub excitement: f32,
    /// Number of crowd members "jumping" (0â€“20).
    pub jumping_count: u8,
    /// Whether the crowd is doing a wave.
    pub wave_active: bool,
}

impl Default for CrowdState {
    fn default() -> Self {
        Self {
            excitement: 0.0,
            jumping_count: 0,
            wave_active: false,
        }
    }
}

/// Complete visual state snapshot polled by the frontend.
/// This is the ONLY data structure the frontend reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Current state of all band members.
    pub band: Vec<BandMemberSnapshot>,
    /// Stage lighting state.
    pub lighting: StageLighting,
    /// Crowd reaction.
    pub crowd: CrowdState,
    /// Overall energy level 0.0â€“1.0 (smoothed).
    pub energy: f32,
    /// Current BPM (for beat-synced animations).
    pub bpm: f32,
    /// Beat counter (wraps at 4 for 4/4 time).
    pub beat_position: f32,
    /// Whether audio is currently playing.
    pub is_playing: bool,
    /// Monotonic frame counter for the visual engine.
    pub frame: u64,
    /// Current dance style (from config, for frontend rendering).
    pub dance_style: DanceStyle,
    /// Active visual effects (from config, for frontend rendering).
    pub active_effects: Vec<VisualEffect>,
    /// Stage decor / backdrop (from config, for frontend rendering).
    pub decor: StageDecor,
    /// Camera view mode.
    pub camera_mode: CameraMode,
    /// Which member to focus on (role name) when camera is ZoomCharacter.
    #[serde(default)]
    pub camera_focus: Option<String>,
    /// Which band members are visible (role name → bool). Empty = all visible.
    #[serde(default)]
    pub visible_members: std::collections::HashMap<String, bool>,
}

impl Default for PerformanceSnapshot {
    fn default() -> Self {
        Self {
            band: BandMemberRole::all()
                .iter()
                .map(|&role| BandMemberSnapshot {
                    role,
                    animation_state: match role {
                        BandMemberRole::Drummer | BandMemberRole::Percussionist => {
                            BandAnimationState::Drummer(DrummerState::Idle)
                        }
                        _ => BandAnimationState::Member(MemberState::Idle),
                    },
                    animation_progress: 0.0,
                    energy: 0.0,
                })
                .collect(),
            lighting: StageLighting::default(),
            crowd: CrowdState::default(),
            energy: 0.0,
            bpm: 120.0,
            beat_position: 0.0,
            is_playing: false,
            frame: 0,
            dance_style: DanceStyle::default(),
            active_effects: Vec::new(),
            decor: StageDecor::default(),
            camera_mode: CameraMode::default(),
            camera_focus: None,
            visible_members: std::collections::HashMap::new(),
        }
    }
}

/// Events published by the scheduler thread to describe musical activity.
/// These are lightweight, Copy-able, and allocation-free.
#[derive(Debug, Clone, Copy)]
pub enum PerformanceEvent {
    /// A note was played. Carries frequency (Hz), amplitude (0.0â€“1.0+),
    /// and a synth category hint for instrumentâ†’role mapping.
    NoteOn {
        frequency: f32,
        amplitude: f32,
        synth_hint: SynthCategory,
    },
    /// A sample was triggered.
    SampleHit {
        category: SampleCategory,
        amplitude: f32,
    },
    /// BPM changed.
    BpmChange {
        bpm: f32,
    },
    /// Playback started.
    PlaybackStarted,
    /// Playback stopped.
    PlaybackStopped,
    /// An effect block started (type hint).
    FxActive {
        fx_type: FxCategory,
    },
}

/// Broad category for triggered samples (derived from sample name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleCategory {
    Kick,
    Snare,
    HiHat,
    Clap,
    Cymbal,
    Tom,
    Percussion,
    Bass,
    Loop,
    Ambient,
    Vocal,
    Other,
}

impl SampleCategory {
    /// Classify a sample name into a broad category.
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("kick") || lower.contains("bd") {
            SampleCategory::Kick
        } else if lower.contains("snare") || lower.contains("sd") {
            SampleCategory::Snare
        } else if lower.contains("hihat") || lower.contains("hat") || lower.contains("hh") {
            SampleCategory::HiHat
        } else if lower.contains("clap") || lower.contains("cp") {
            SampleCategory::Clap
        } else if lower.contains("cymbal") || lower.contains("crash") || lower.contains("ride") {
            SampleCategory::Cymbal
        } else if lower.contains("tom") {
            SampleCategory::Tom
        } else if lower.contains("perc") || lower.contains("drum") || lower.contains("conga")
            || lower.contains("bongo") || lower.contains("shaker") || lower.contains("tambourine")
        {
            SampleCategory::Percussion
        } else if lower.contains("bass") {
            SampleCategory::Bass
        } else if lower.contains("loop") || lower.contains("break") {
            SampleCategory::Loop
        } else if lower.contains("ambi") || lower.contains("pad") || lower.contains("atmos") {
            SampleCategory::Ambient
        } else if lower.contains("vocal") || lower.contains("choir") || lower.contains("voice") {
            SampleCategory::Vocal
        } else {
            SampleCategory::Other
        }
    }
}

/// Broad FX category for visual hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxCategory {
    Reverb,
    Delay,
    Distortion,
    Filter,
    Modulation,
    Other,
}

impl FxCategory {
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("reverb") {
            FxCategory::Reverb
        } else if lower.contains("echo") || lower.contains("delay") {
            FxCategory::Delay
        } else if lower.contains("distortion") || lower.contains("bitcrusher")
            || lower.contains("krush")
        {
            FxCategory::Distortion
        } else if lower.contains("lpf") || lower.contains("hpf") || lower.contains("wobble") {
            FxCategory::Filter
        } else if lower.contains("flanger") || lower.contains("chorus")
            || lower.contains("ring_mod") || lower.contains("octaver")
        {
            FxCategory::Modulation
        } else {
            FxCategory::Other
        }
    }
}

// â”€â”€â”€ Visual Engine Configuration â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Configuration for the visual engine. Can be adjusted at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualEngineConfig {
    /// Target visual update rate in Hz (default: 30).
    pub target_fps: u32,
    /// How quickly energy decays when no events arrive (0.0â€“1.0 per frame).
    pub energy_decay: f32,
    /// How quickly animation states return to idle (seconds).
    pub idle_timeout: f32,
    /// Whether the crowd simulation is enabled.
    pub crowd_enabled: bool,
    /// Whether stage lighting effects are enabled.
    pub lighting_enabled: bool,
    /// Current dance style for character animations.
    #[serde(default)]
    pub dance_style: DanceStyle,
    /// Active visual effects (post-processing layers).
    #[serde(default)]
    pub visual_effects: Vec<VisualEffect>,
    /// Stage decor / backdrop.
    #[serde(default)]
    pub decor: StageDecor,
    /// Camera view mode.
    #[serde(default)]
    pub camera_mode: CameraMode,
    /// Which member role to focus on when camera_mode == ZoomCharacter.
    #[serde(default)]
    pub camera_focus: Option<String>,
    /// Which band members are visible (role name → bool). Empty = all visible.
    #[serde(default)]
    pub visible_members: std::collections::HashMap<String, bool>,
}

impl Default for VisualEngineConfig {
    fn default() -> Self {
        Self {
            target_fps: 30,
            energy_decay: 0.05,
            idle_timeout: 0.5,
            crowd_enabled: true,
            lighting_enabled: true,
            dance_style: DanceStyle::default(),
            visual_effects: Vec::new(),
            decor: StageDecor::default(),
            camera_mode: CameraMode::default(),
            camera_focus: None,
            visible_members: std::collections::HashMap::new(),
        }
    }
}

// â”€â”€â”€ Event Bridge â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Maximum number of events buffered before dropping.
/// 256 is enough for ~8 seconds of dense musical activity at 30fps consumption.
const EVENT_BRIDGE_CAPACITY: usize = 256;

/// Non-blocking, bounded event bridge from the scheduler to the visual engine.
///
/// # Safety contract
/// - `publish()` uses `try_send()` and NEVER blocks.
/// - If the channel is full, events are silently dropped.
/// - The audio thread / scheduler thread is never affected by visual system load.
pub struct EventBridge {
    tx: Sender<PerformanceEvent>,
    rx: Receiver<PerformanceEvent>,
}

impl EventBridge {
    /// Create a new event bridge with a bounded channel.
    pub fn new() -> Self {
        let (tx, rx) = bounded(EVENT_BRIDGE_CAPACITY);
        Self { tx, rx }
    }

    /// Get a cloneable publisher handle for the scheduler thread.
    /// This is the ONLY way events enter the visual system.
    pub fn publisher(&self) -> EventPublisher {
        EventPublisher {
            tx: self.tx.clone(),
        }
    }

    /// Get the receiver (consumed by the visual engine thread).
    pub fn receiver(&self) -> Receiver<PerformanceEvent> {
        self.rx.clone()
    }
}

/// A lightweight, cloneable, non-blocking event publisher.
///
/// This is given to the scheduler thread. It MUST NOT be used from the audio
/// callback thread â€” the scheduler thread is the correct place for this.
#[derive(Clone)]
pub struct EventPublisher {
    tx: Sender<PerformanceEvent>,
}

impl EventPublisher {
    /// Publish an event to the visual system.
    ///
    /// # Non-blocking guarantee
    /// Uses `try_send()`. If the channel is full, the event is silently dropped.
    /// Returns `true` if the event was sent, `false` if it was dropped.
    pub fn publish(&self, event: PerformanceEvent) -> bool {
        match self.tx.try_send(event) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                // Visual system is behind â€” drop the event. This is expected
                // and is NOT an error condition.
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                // Visual engine shut down â€” also fine, we just stop publishing.
                false
            }
        }
    }
}

// â”€â”€â”€ Band Director â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// The Band Director maps musical events to band member animation states.
///
/// It maintains internal state (energy levels, hit counters, timing) and
/// produces animation state decisions. It runs ONLY on the visual engine thread.
struct BandDirector {
    /// Per-member energy levels (0.0â€“1.0).
    member_energy: HashMap<BandMemberRole, f32>,
    /// Per-member last-hit time for idle timeout.
    member_last_hit: HashMap<BandMemberRole, Instant>,
    /// Per-member current state.
    member_state: HashMap<BandMemberRole, BandAnimationState>,
    /// Global energy accumulator.
    global_energy: f32,
    /// Recent hit counter (decays over time) for fill/intense detection.
    recent_hits: f32,
    /// BPM for beat-synced decisions.
    bpm: f32,
    /// Config reference.
    config: VisualEngineConfig,
}

impl BandDirector {
    fn new(config: VisualEngineConfig) -> Self {
        let mut member_energy = HashMap::new();
        let mut member_last_hit = HashMap::new();
        let mut member_state = HashMap::new();
        let now = Instant::now();

        for &role in BandMemberRole::all() {
            member_energy.insert(role, 0.0);
            member_last_hit.insert(role, now);
            member_state.insert(
                role,
                match role {
                    BandMemberRole::Drummer | BandMemberRole::Percussionist => {
                        BandAnimationState::Drummer(DrummerState::Idle)
                    }
                    _ => BandAnimationState::Member(MemberState::Idle),
                },
            );
        }

        Self {
            member_energy,
            member_last_hit,
            member_state,
            global_energy: 0.0,
            recent_hits: 0.0,
            bpm: 120.0,
            config,
        }
    }

    /// Process a batch of events and update internal state.
    fn process_events(&mut self, events: &[PerformanceEvent]) {
        let now = Instant::now();

        for event in events {
            match event {
                PerformanceEvent::NoteOn {
                    frequency,
                    amplitude,
                    synth_hint,
                } => {
                    self.recent_hits += 1.0;

                    // Primary mapping: use synth category hint.
                    // Fallback: frequency-based mapping for Default category.
                    let role = match synth_hint {
                        SynthCategory::Default => {
                            // No synth info â€” use frequency ranges:
                            // Bass: < 200 Hz â†’ Bassist
                            // Mid-low: 200â€“500 Hz â†’ Guitarist
                            // Mid: 500â€“2000 Hz â†’ Keyboard
                            // High: > 2000 Hz â†’ Vocalist
                            if *frequency < 200.0 {
                                BandMemberRole::Bassist
                            } else if *frequency < 500.0 {
                                BandMemberRole::Guitarist
                            } else if *frequency < 2000.0 {
                                BandMemberRole::Keyboard
                            } else {
                                BandMemberRole::Vocalist
                            }
                        }
                        _ => synth_hint.to_role(),
                    };

                    let energy = self.member_energy.entry(role).or_insert(0.0);
                    *energy = (*energy + amplitude * 0.5).min(1.0);
                    self.member_last_hit.insert(role, now);
                    self.global_energy = (self.global_energy + amplitude * 0.3).min(1.0);

                    // Determine animation state based on amplitude
                    let state = if *amplitude > 0.8 {
                        BandAnimationState::Member(MemberState::Intense)
                    } else if *amplitude > 0.5 {
                        BandAnimationState::Member(MemberState::Accent)
                    } else {
                        BandAnimationState::Member(MemberState::Groove)
                    };
                    self.member_state.insert(role, state);
                }

                PerformanceEvent::SampleHit {
                    category,
                    amplitude,
                } => {
                    self.recent_hits += 1.0;

                    match category {
                        SampleCategory::Kick => {
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Drummer)
                                .or_insert(0.0);
                            *energy = (*energy + amplitude * 0.7).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Drummer, now);
                            self.global_energy =
                                (self.global_energy + amplitude * 0.4).min(1.0);

                            let state = if *amplitude > 0.7 {
                                BandAnimationState::Drummer(DrummerState::PlayHard)
                            } else {
                                BandAnimationState::Drummer(DrummerState::PlaySoft)
                            };
                            self.member_state.insert(BandMemberRole::Drummer, state);
                        }
                        SampleCategory::Snare | SampleCategory::Clap => {
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Drummer)
                                .or_insert(0.0);
                            *energy = (*energy + amplitude * 0.6).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Drummer, now);

                            let state = if self.recent_hits > 6.0 {
                                BandAnimationState::Drummer(DrummerState::Fill)
                            } else if *amplitude > 0.6 {
                                BandAnimationState::Drummer(DrummerState::PlayHard)
                            } else {
                                BandAnimationState::Drummer(DrummerState::PlaySoft)
                            };
                            self.member_state.insert(BandMemberRole::Drummer, state);
                        }
                        SampleCategory::HiHat => {
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Drummer)
                                .or_insert(0.0);
                            *energy = (*energy + amplitude * 0.3).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Drummer, now);

                            // Hi-hats keep current state or soft play
                            if let Some(BandAnimationState::Drummer(DrummerState::Idle)) =
                                self.member_state.get(&BandMemberRole::Drummer)
                            {
                                self.member_state.insert(
                                    BandMemberRole::Drummer,
                                    BandAnimationState::Drummer(DrummerState::PlaySoft),
                                );
                            }
                        }
                        SampleCategory::Cymbal => {
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Drummer)
                                .or_insert(0.0);
                            *energy = (*energy + amplitude * 0.8).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Drummer, now);
                            self.member_state.insert(
                                BandMemberRole::Drummer,
                                BandAnimationState::Drummer(DrummerState::CrashHit),
                            );
                        }
                        SampleCategory::Bass => {
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Bassist)
                                .or_insert(0.0);
                            *energy = (*energy + amplitude * 0.6).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Bassist, now);
                            self.global_energy =
                                (self.global_energy + amplitude * 0.3).min(1.0);

                            let state = if *amplitude > 0.7 {
                                BandAnimationState::Member(MemberState::Accent)
                            } else {
                                BandAnimationState::Member(MemberState::Groove)
                            };
                            self.member_state.insert(BandMemberRole::Bassist, state);
                        }
                        SampleCategory::Loop => {
                            // Loops energize everyone slightly, DJ gets the most
                            let dj_energy = self.member_energy.entry(BandMemberRole::Dj).or_insert(0.0);
                            *dj_energy = (*dj_energy + amplitude * 0.6).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Dj, now);
                            self.member_state.insert(
                                BandMemberRole::Dj,
                                BandAnimationState::Member(MemberState::Accent),
                            );

                            for &role in BandMemberRole::all() {
                                if role == BandMemberRole::Dj { continue; }
                                let energy =
                                    self.member_energy.entry(role).or_insert(0.0);
                                *energy = (*energy + amplitude * 0.2).min(1.0);
                                self.member_last_hit.insert(role, now);
                            }
                            self.global_energy =
                                (self.global_energy + amplitude * 0.5).min(1.0);

                            // Put non-DJ members in groove
                            for &role in BandMemberRole::all() {
                                if role == BandMemberRole::Dj { continue; }
                                if role == BandMemberRole::Drummer || role == BandMemberRole::Percussionist {
                                    self.member_state.insert(
                                        role,
                                        BandAnimationState::Drummer(DrummerState::PlaySoft),
                                    );
                                } else {
                                    self.member_state.insert(
                                        role,
                                        BandAnimationState::Member(MemberState::Groove),
                                    );
                                }
                            }
                        }
                        SampleCategory::Ambient | SampleCategory::Vocal => {
                            let role = if *category == SampleCategory::Vocal {
                                BandMemberRole::Vocalist
                            } else {
                                BandMemberRole::Keyboard
                            };
                            let energy =
                                self.member_energy.entry(role).or_insert(0.0);
                            *energy = (*energy + amplitude * 0.4).min(1.0);
                            self.member_last_hit.insert(role, now);

                            self.member_state.insert(
                                role,
                                BandAnimationState::Member(MemberState::Groove),
                            );
                        }
                        _ => {
                            // General percussion / other â†’ percussionist
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Percussionist)
                                .or_insert(0.0);
                            *energy = (*energy + amplitude * 0.4).min(1.0);
                            self.member_last_hit.insert(BandMemberRole::Percussionist, now);

                            let state = if *amplitude > 0.6 {
                                BandAnimationState::Drummer(DrummerState::PlayHard)
                            } else {
                                BandAnimationState::Drummer(DrummerState::PlaySoft)
                            };
                            self.member_state.insert(BandMemberRole::Percussionist, state);
                        }
                    }
                }

                PerformanceEvent::BpmChange { bpm } => {
                    self.bpm = *bpm;
                }

                PerformanceEvent::PlaybackStarted => {
                    // Reset all states
                    for &role in BandMemberRole::all() {
                        self.member_energy.insert(role, 0.0);
                        self.member_last_hit.insert(role, now);
                    }
                    self.global_energy = 0.1;
                    self.recent_hits = 0.0;
                }

                PerformanceEvent::PlaybackStopped => {
                    // Immediately idle everyone
                    for &role in BandMemberRole::all() {
                        self.member_energy.insert(role, 0.0);
                        self.member_state.insert(
                            role,
                            match role {
                                BandMemberRole::Drummer | BandMemberRole::Percussionist => {
                                    BandAnimationState::Drummer(DrummerState::Idle)
                                }
                                _ => BandAnimationState::Member(MemberState::Idle),
                            },
                        );
                    }
                    self.global_energy = 0.0;
                    self.recent_hits = 0.0;
                }

                PerformanceEvent::FxActive { fx_type } => {
                    // FX makes relevant members more animated
                    match fx_type {
                        FxCategory::Reverb | FxCategory::Delay => {
                            // Atmospheric â†’ keyboard gets a boost
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Keyboard)
                                .or_insert(0.0);
                            *energy = (*energy + 0.1).min(1.0);
                        }
                        FxCategory::Distortion => {
                            // Heavy â†’ guitarist gets a boost
                            let energy = self
                                .member_energy
                                .entry(BandMemberRole::Guitarist)
                                .or_insert(0.0);
                            *energy = (*energy + 0.2).min(1.0);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Decay energy and transition idle members. Called once per visual frame.
    fn tick(&mut self, dt: f32) {
        let now = Instant::now();
        let idle_timeout = Duration::from_secs_f32(self.config.idle_timeout);
        let decay = self.config.energy_decay;

        // Decay global energy
        self.global_energy = (self.global_energy - decay * dt * 2.0).max(0.0);

        // Decay recent hits counter
        self.recent_hits = (self.recent_hits - dt * 4.0).max(0.0);

        // Per-member decay and idle transitions
        for &role in BandMemberRole::all() {
            let energy = self.member_energy.entry(role).or_insert(0.0);
            *energy = (*energy - decay * dt * 3.0).max(0.0);

            let last_hit = self
                .member_last_hit
                .get(&role)
                .copied()
                .unwrap_or(now);

            if now.duration_since(last_hit) > idle_timeout {
                // Transition to idle
                self.member_state.insert(
                    role,
                    match role {
                        BandMemberRole::Drummer | BandMemberRole::Percussionist => {
                            BandAnimationState::Drummer(DrummerState::Idle)
                        }
                        _ => BandAnimationState::Member(MemberState::Idle),
                    },
                );
            }

            // High global energy can push members into intense
            if self.global_energy > 0.7 && *energy > 0.3 {
                let current = self.member_state.get(&role).copied();
                match current {
                    Some(BandAnimationState::Member(MemberState::Groove)) => {
                        self.member_state.insert(
                            role,
                            BandAnimationState::Member(MemberState::Intense),
                        );
                    }
                    Some(BandAnimationState::Drummer(DrummerState::PlaySoft)) => {
                        self.member_state.insert(
                            role,
                            BandAnimationState::Drummer(DrummerState::PlayHard),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// Build the current band snapshot.
    fn snapshot(&self) -> Vec<BandMemberSnapshot> {
        BandMemberRole::all()
            .iter()
            .map(|&role| {
                let energy = self.member_energy.get(&role).copied().unwrap_or(0.0);
                let state = self.member_state.get(&role).copied().unwrap_or(match role {
                    BandMemberRole::Drummer | BandMemberRole::Percussionist => {
                        BandAnimationState::Drummer(DrummerState::Idle)
                    }
                    _ => BandAnimationState::Member(MemberState::Idle),
                });

                BandMemberSnapshot {
                    role,
                    animation_state: state,
                    animation_progress: energy, // use energy as animation progress
                    energy,
                }
            })
            .collect()
    }
}

// â”€â”€â”€ Stage Lighting Director â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

struct LightingDirector {
    beat_flash: f32,
    brightness: f32,
    strobe_timer: f32,
}

impl LightingDirector {
    fn new() -> Self {
        Self {
            beat_flash: 0.0,
            brightness: 0.3,
            strobe_timer: 0.0,
        }
    }

    fn on_beat(&mut self, energy: f32) {
        self.beat_flash = energy.min(1.0);
    }

    fn tick(&mut self, dt: f32, global_energy: f32) {
        // Decay beat flash
        self.beat_flash = (self.beat_flash - dt * 4.0).max(0.0);

        // Brightness follows energy with smoothing
        let target = 0.3 + global_energy * 0.7;
        self.brightness += (target - self.brightness) * dt * 3.0;

        // Strobe at high energy
        if global_energy > 0.85 {
            self.strobe_timer += dt;
        } else {
            self.strobe_timer = 0.0;
        }
    }

    fn snapshot(&self, global_energy: f32) -> StageLighting {
        let spotlight = if global_energy > 0.7 {
            [255, 100, 50] // warm orange-red for high energy
        } else if global_energy > 0.4 {
            [100, 60, 200] // purple for medium
        } else {
            [40, 60, 120] // cool blue for low
        };

        StageLighting {
            brightness: self.brightness,
            strobe_active: self.strobe_timer > 0.5 && ((self.strobe_timer * 10.0) as u32 % 2 == 0),
            spotlight_color: spotlight,
            beat_flash: self.beat_flash,
        }
    }
}

// â”€â”€â”€ Crowd Director â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

struct CrowdDirector {
    excitement: f32,
    wave_timer: f32,
}

impl CrowdDirector {
    fn new() -> Self {
        Self {
            excitement: 0.0,
            wave_timer: 0.0,
        }
    }

    fn tick(&mut self, dt: f32, global_energy: f32) {
        // Excitement follows energy with lag
        let target = global_energy;
        self.excitement += (target - self.excitement) * dt * 1.5;
        self.excitement = self.excitement.clamp(0.0, 1.0);

        // Wave timer at sustained high energy
        if self.excitement > 0.6 {
            self.wave_timer += dt;
        } else {
            self.wave_timer = (self.wave_timer - dt * 0.5).max(0.0);
        }
    }

    fn snapshot(&self) -> CrowdState {
        CrowdState {
            excitement: self.excitement,
            jumping_count: (self.excitement * 20.0).round() as u8,
            wave_active: self.wave_timer > 3.0,
        }
    }
}

// â”€â”€â”€ Visual Engine â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// The visual engine runs on its own thread, consuming events and producing
/// snapshots. It is completely independent from the audio thread.
pub struct VisualEngine {
    /// Shared snapshot that the frontend polls.
    snapshot: Arc<Mutex<PerformanceSnapshot>>,
    /// Whether the engine is enabled.
    enabled: Arc<AtomicBool>,
    /// Shared runtime configuration â€” can be updated from Tauri commands.
    config: Arc<Mutex<VisualEngineConfig>>,
    /// Handle to the engine thread (for clean shutdown).
    _thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl VisualEngine {
    /// Start the visual engine. Returns the engine handle and an event publisher.
    ///
    /// The publisher should be given to the scheduler thread(s).
    /// The snapshot can be read by the frontend via Tauri commands.
    pub fn start(config: VisualEngineConfig) -> (Self, EventBridge) {
        let bridge = EventBridge::new();
        let rx = bridge.receiver();
        let snapshot = Arc::new(Mutex::new(PerformanceSnapshot::default()));
        let enabled = Arc::new(AtomicBool::new(true));
        let shared_config = Arc::new(Mutex::new(config));

        let snapshot_clone = snapshot.clone();
        let enabled_clone = enabled.clone();
        let config_clone = shared_config.clone();

        let handle = std::thread::Builder::new()
            .name("visual-engine".to_string())
            .spawn(move || {
                Self::run_loop(rx, snapshot_clone, enabled_clone, config_clone);
            })
            .expect("Failed to spawn visual engine thread");

        (
            Self {
                snapshot,
                enabled,
                config: shared_config,
                _thread_handle: Some(handle),
            },
            bridge,
        )
    }

    /// The main visual engine loop. Runs at ~target_fps.
    fn run_loop(
        rx: Receiver<PerformanceEvent>,
        snapshot: Arc<Mutex<PerformanceSnapshot>>,
        enabled: Arc<AtomicBool>,
        shared_config: Arc<Mutex<VisualEngineConfig>>,
    ) {
        // Read initial config
        let initial_config = shared_config.lock().clone();
        let mut target_fps = initial_config.target_fps;
        let mut frame_duration = Duration::from_secs_f64(1.0 / target_fps as f64);
        let mut band_director = BandDirector::new(initial_config.clone());
        let mut lighting = LightingDirector::new();
        let mut crowd = CrowdDirector::new();
        let mut frame_counter: u64 = 0;
        let mut beat_accumulator: f64 = 0.0;
        let mut last_frame = Instant::now();
        let mut is_playing = false;

        // Reusable event buffer â€” avoids per-frame allocation
        let mut event_buf: Vec<PerformanceEvent> = Vec::with_capacity(EVENT_BRIDGE_CAPACITY);

        loop {
            let frame_start = Instant::now();
            let dt = frame_start.duration_since(last_frame).as_secs_f32();
            last_frame = frame_start;

            // Check if engine is disabled â€” if so, sleep longer and skip work
            if !enabled.load(Ordering::Relaxed) {
                // Drain events to prevent channel saturation, but don't process
                while rx.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            // Re-read config each frame (lock held ~nanoseconds)
            let config = shared_config.lock().clone();
            if config.target_fps != target_fps {
                target_fps = config.target_fps;
                frame_duration = Duration::from_secs_f64(1.0 / target_fps.max(1) as f64);
            }

            // â”€â”€ 1. Drain all available events (non-blocking) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
            event_buf.clear();
            while let Ok(event) = rx.try_recv() {
                event_buf.push(event);
                // If we have too many events this frame, keep the latest ones
                if event_buf.len() > 64 {
                    // Drop oldest events â€” keep the last 32
                    let drain_count = event_buf.len() - 32;
                    event_buf.drain(0..drain_count);
                }
            }

            // Track playback state from events
            for event in &event_buf {
                match event {
                    PerformanceEvent::PlaybackStarted => is_playing = true,
                    PerformanceEvent::PlaybackStopped => is_playing = false,
                    _ => {}
                }
            }

            // â”€â”€ 2. Process events through the band director â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
            band_director.process_events(&event_buf);

            // â”€â”€ 3. Tick decay and transitions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
            band_director.tick(dt);

            let global_energy = band_director.global_energy;

            // Beat accumulator
            let bpm = band_director.bpm;
            if is_playing && bpm > 0.0 {
                beat_accumulator += (bpm as f64 / 60.0) * dt as f64;
                // Flash on each beat
                let beat_frac = beat_accumulator % 1.0;
                if beat_frac < dt as f64 * (bpm as f64 / 60.0) {
                    lighting.on_beat(global_energy);
                }
            }

            // Tick sub-directors
            lighting.tick(dt, global_energy);
            if config.crowd_enabled {
                crowd.tick(dt, global_energy);
            }

            // ── 4. Build and publish snapshot ───────────────────────────
            let snap = PerformanceSnapshot {
                band: band_director.snapshot(),
                lighting: if config.lighting_enabled {
                    lighting.snapshot(global_energy)
                } else {
                    StageLighting::default()
                },
                crowd: if config.crowd_enabled {
                    crowd.snapshot()
                } else {
                    CrowdState::default()
                },
                energy: global_energy,
                bpm,
                beat_position: (beat_accumulator % 4.0) as f32,
                is_playing,
                frame: frame_counter,
                dance_style: config.dance_style.clone(),
                active_effects: config.visual_effects.clone(),
                decor: config.decor.clone(),
                camera_mode: config.camera_mode,
                camera_focus: config.camera_focus.clone(),
                visible_members: config.visible_members.clone(),
            };

            // Write snapshot â€” this lock is held for ~nanoseconds with parking_lot
            *snapshot.lock() = snap;
            frame_counter += 1;

            // â”€â”€ 5. Sleep until next frame â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
            // If we took longer than frame_duration, we simply skip â€” no catchup
        }
    }

    /// Get a clone of the current snapshot. Non-blocking read.
    pub fn get_snapshot(&self) -> PerformanceSnapshot {
        self.snapshot.lock().clone()
    }

    /// Enable or disable the visual engine.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Check if the visual engine is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Get the current engine configuration.
    pub fn get_config(&self) -> VisualEngineConfig {
        self.config.lock().clone()
    }

    /// Update the engine configuration at runtime.
    /// Changes take effect on the next frame (no audio impact).
    pub fn set_config(&self, config: VisualEngineConfig) {
        *self.config.lock() = config;
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Verify that publish() never blocks even when the channel is full.
    #[test]
    fn test_publish_never_blocks() {
        let bridge = EventBridge::new();
        let publisher = bridge.publisher();
        // Don't consume — let the channel fill up
        for _ in 0..EVENT_BRIDGE_CAPACITY {
            publisher.publish(PerformanceEvent::NoteOn {
                frequency: 440.0,
                amplitude: 0.5,
                synth_hint: SynthCategory::Default,
            });
        }

        // This MUST NOT block — it should drop the event
        let start = Instant::now();
        let sent = publisher.publish(PerformanceEvent::NoteOn {
            frequency: 440.0,
            amplitude: 0.5,
            synth_hint: SynthCategory::Default,
        });
        let elapsed = start.elapsed();

        assert!(!sent, "Event should have been dropped when channel is full");
        assert!(
            elapsed < Duration::from_millis(1),
            "publish() took {:?} — must be non-blocking",
            elapsed
        );
    }

    /// Verify that dropping events doesn't affect the publisher.
    #[test]
    fn test_events_drop_safely() {
        let bridge = EventBridge::new();
        let publisher = bridge.publisher();

        // Drop the receiver (simulating visual engine shutdown)
        drop(bridge);

        // Publishing should silently fail
        let sent = publisher.publish(PerformanceEvent::PlaybackStarted);
        assert!(!sent);
    }

    /// Verify that SampleCategory classifies correctly.
    #[test]
    fn test_sample_category_classification() {
        assert_eq!(SampleCategory::from_name("kick"), SampleCategory::Kick);
        assert_eq!(SampleCategory::from_name("snare"), SampleCategory::Snare);
        assert_eq!(SampleCategory::from_name("hihat"), SampleCategory::HiHat);
        assert_eq!(SampleCategory::from_name("loop_amen"), SampleCategory::Loop);
        assert_eq!(SampleCategory::from_name("ambi_choir"), SampleCategory::Ambient);
        assert_eq!(SampleCategory::from_name("bass_hit_c"), SampleCategory::Bass);
        assert_eq!(SampleCategory::from_name("something_random"), SampleCategory::Other);
    }

    /// Verify the visual engine thread starts and produces snapshots.
    #[test]
    fn test_visual_engine_produces_snapshots() {
        let config = VisualEngineConfig {
            target_fps: 60,
            ..Default::default()
        };
        let (engine, bridge) = VisualEngine::start(config);
        let publisher = bridge.publisher();

        // Send a note event
        publisher.publish(PerformanceEvent::NoteOn {
            frequency: 440.0,
            amplitude: 0.6,
            synth_hint: SynthCategory::Default,
        });

        // Wait for a few frames
        std::thread::sleep(Duration::from_millis(100));

        let snap = engine.get_snapshot();
        assert!(snap.frame > 0, "Visual engine should have processed frames");
        assert_eq!(snap.band.len(), 7, "Should have 7 band members");
    }

    /// Verify that disabling the engine stops processing.
    #[test]
    fn test_disable_engine() {
        let config = VisualEngineConfig::default();
        let (engine, _bridge) = VisualEngine::start(config);

        std::thread::sleep(Duration::from_millis(50));
        let frame_before = engine.get_snapshot().frame;

        engine.set_enabled(false);
        std::thread::sleep(Duration::from_millis(100));
        let frame_after = engine.get_snapshot().frame;

        assert!(
            frame_after - frame_before <= 2,
            "Engine should stop processing when disabled (before={}, after={})",
            frame_before,
            frame_after
        );
    }

    /// Verify that heavy event load doesn't block the publisher.
    #[test]
    fn test_heavy_load_no_blocking() {
        let bridge = EventBridge::new();
        let publisher = bridge.publisher();

        let start = Instant::now();
        for _ in 0..10_000 {
            publisher.publish(PerformanceEvent::NoteOn {
                frequency: 440.0,
                amplitude: 0.5,
                synth_hint: SynthCategory::Default,
            });
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(100),
            "10k publishes took {:?} — too slow, possible blocking",
            elapsed
        );
    }

    /// Verify BandDirector correctly maps events to animation states.
    #[test]
    fn test_band_director_mapping() {
        let config = VisualEngineConfig::default();
        let mut director = BandDirector::new(config);

        // Kick should activate drummer
        director.process_events(&[PerformanceEvent::SampleHit {
            category: SampleCategory::Kick,
            amplitude: 0.9,
        }]);

        let snap = director.snapshot();
        let drummer = snap.iter().find(|m| m.role == BandMemberRole::Drummer).unwrap();
        assert!(
            matches!(
                drummer.animation_state,
                BandAnimationState::Drummer(DrummerState::PlayHard)
            ),
            "Kick with amp 0.9 should trigger PlayHard, got {:?}",
            drummer.animation_state
        );

        // Low bass note should activate bassist
        director.process_events(&[PerformanceEvent::NoteOn {
            frequency: 80.0,
            amplitude: 0.6,
            synth_hint: SynthCategory::Default,
        }]);

        let snap = director.snapshot();
        let bassist = snap.iter().find(|m| m.role == BandMemberRole::Bassist).unwrap();
        assert!(
            matches!(
                bassist.animation_state,
                BandAnimationState::Member(MemberState::Accent)
            ),
            "Bass note with amp 0.6 should trigger Accent, got {:?}",
            bassist.animation_state
        );
    }

    /// Verify SynthCategory classifies synth names correctly.
    #[test]
    fn test_synth_category_classification() {
        assert_eq!(SynthCategory::from_synth_name("TB303"), SynthCategory::Bass);
        assert_eq!(SynthCategory::from_synth_name("ChipBass"), SynthCategory::Bass);
        assert_eq!(SynthCategory::from_synth_name("SubPulse"), SynthCategory::Bass);
        assert_eq!(SynthCategory::from_synth_name("SuperSaw"), SynthCategory::Lead);
        assert_eq!(SynthCategory::from_synth_name("Saw"), SynthCategory::Lead);
        assert_eq!(SynthCategory::from_synth_name("Blade"), SynthCategory::Pad);
        assert_eq!(SynthCategory::from_synth_name("Prophet"), SynthCategory::Pad);
        assert_eq!(SynthCategory::from_synth_name("FM"), SynthCategory::Keys);
        assert_eq!(SynthCategory::from_synth_name("Piano"), SynthCategory::Keys);
        assert_eq!(SynthCategory::from_synth_name("Pluck"), SynthCategory::Pluck);
        assert_eq!(SynthCategory::from_synth_name("GabberKick"), SynthCategory::Percussive);
        assert_eq!(SynthCategory::from_synth_name("Sine"), SynthCategory::Default);
    }

    /// Verify synth hint routes to correct band member.
    #[test]
    fn test_synth_hint_role_mapping() {
        let config = VisualEngineConfig::default();
        let mut director = BandDirector::new(config);

        director.process_events(&[PerformanceEvent::NoteOn {
            frequency: 1000.0,
            amplitude: 0.7,
            synth_hint: SynthCategory::Bass,
        }]);

        let snap = director.snapshot();
        let bassist = snap.iter().find(|m| m.role == BandMemberRole::Bassist).unwrap();
        assert!(
            bassist.energy > 0.0,
            "TB303 hint should route to Bassist even at high frequency"
        );

        director.process_events(&[PerformanceEvent::NoteOn {
            frequency: 100.0,
            amplitude: 0.6,
            synth_hint: SynthCategory::Lead,
        }]);

        let snap = director.snapshot();
        let guitarist = snap.iter().find(|m| m.role == BandMemberRole::Guitarist).unwrap();
        assert!(
            guitarist.energy > 0.0,
            "Lead hint should route to Guitarist even at low frequency"
        );
    }
}
