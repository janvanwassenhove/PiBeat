use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use std::sync::Arc;

use super::effects::{EffectChain, VoiceFx};
use super::recorder::Recorder;
use super::synth::{Envelope, OscillatorType, SynthVoice};

/// Messages sent from the main thread to the audio thread
#[derive(Debug, Clone, serde::Serialize)]
pub enum AudioCommand {
    PlayNote {
        synth_type: OscillatorType,
        frequency: f32,
        amplitude: f32,
        duration_secs: f32,
        envelope: Envelope,
        pan: f32,
        /// Synth-specific parameters (cutoff, res, detune, depth, etc.)
        /// forwarded to SuperCollider as named OSC args.
        params: Vec<(String, f32)>,
        /// FX context ID: if inside a with_fx block, this is the innermost
        /// FX block's ID so the SC engine routes to the correct bus.
        /// 0 = no FX context (route to hardware output).
        fx_context: u64,
    },
    PlaySample {
        samples: Vec<f32>,
        sample_rate: u32,
        amplitude: f32,
        rate: f32,
        pan: f32,
        /// Optional sustain in seconds; truncates playback with a short fade
        sustain_secs: Option<f32>,
        /// beat_stretch: desired beats for this sample (rate may be adjusted by caller)
        beat_stretch: Option<f32>,
        /// start: normalized position (0.0-1.0) in sample to begin playback
        start: Option<f32>,
        /// finish: normalized position (0.0-1.0) in sample to end playback
        finish: Option<f32>,
        /// Optional ADSR envelope (attack/decay/sustain_level/release in seconds)
        envelope: Option<Envelope>,
        /// FX context ID (same as PlayNote.fx_context).
        fx_context: u64,
    },
    SetBpm(f32),
    SetMasterVolume(f32),
    Stop,
    SetEffect {
        reverb_mix: f32,
        reverb_room: f32,
        delay_time: f32,
        delay_feedback: f32,
        distortion: f32,
        lpf_cutoff: f32,
        hpf_cutoff: f32,
        slicer_phase: f32,
        slicer_mix: f32,
        slicer_wave: i32,
        bitcrusher_bits: f32,
        bitcrusher_sample_rate: f32,
        bitcrusher_mix: f32,
        compressor_threshold: f32,
        compressor_clamp_time: f32,
        compressor_relax_time: f32,
        compressor_mix: f32,
        normaliser_level: f32,
        // New effects
        flanger_rate: f32,
        flanger_depth: f32,
        flanger_feedback: f32,
        flanger_mix: f32,
        chorus_rate: f32,
        chorus_depth: f32,
        chorus_mix: f32,
        ring_mod_freq: f32,
        ring_mod_mix: f32,
        pan_position: f32,
        wobble_rate: f32,
        wobble_depth: f32,
        wobble_mix: f32,
        octaver_mix: f32,
        octaver_sub_amp: f32,
        octaver_super_amp: f32,
        // Parity: additional params for Sonic Pi equivalence
        reverb_damp: f32,
        delay_mix: f32,
        lpf_res: f32,
        hpf_res: f32,
        /// When true, lpf_cutoff and hpf_cutoff are Hz values (from UI panel);
        /// when false, values ≤130 are treated as MIDI notes (from parser).
        cutoff_is_hz: bool,
    },
    /// Start an FX block — allocates an audio bus and creates the FX synth.
    /// All subsequent PlayNote/PlaySample commands route through this FX
    /// until the matching FxEnd.
    FxStart {
        fx_type: String,
        params: Vec<(String, f32)>,
        /// Unique ID for this FX block (used by SC engine to map bus routing
        /// correctly when multiple concurrent live_loops use with_fx).
        fx_id: u64,
        /// Parent FX block ID (for nested with_fx), or 0 = hardware output.
        parent_fx_id: u64,
    },
    /// End the current FX block — frees the FX synth, restores output bus.
    FxEnd {
        fx_id: u64,
    },
    /// Set a runtime variable in the scheduler's state.
    /// Processed by the scheduler thread, NOT the audio callback.
    SetRuntimeVar {
        key: String,
        value: f64,
    },
}

/// Shared audio state for waveform visualization
pub struct AudioState {
    pub waveform_buffer: Vec<f32>,
    pub is_playing: bool,
    pub master_volume: f32,
    pub bpm: f32,
    pub sample_rate: u32,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            waveform_buffer: vec![0.0; 2048],
            is_playing: false,
            master_volume: 1.0,
            bpm: 120.0,
            sample_rate: 44100,
        }
    }
}

pub struct AudioEngine {
    pub state: Arc<Mutex<AudioState>>,
    command_tx: Sender<AudioCommand>,
    _stream: Mutex<Option<cpal::Stream>>,
}

// Safety: We only access the Stream through Mutex, and only the audio callback
// thread uses it internally. The Stream is kept alive but never moved between threads.
unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}

struct Voice {
    synth: SynthVoice,
    samples_elapsed: u64,
    duration_samples: u64,
    pan: f32,
    /// Per-voice FX chain from scoped `with_fx` blocks (None = dry)
    voice_fx: Option<VoiceFx>,
}

struct SamplePlayback {
    data: Vec<f32>,
    position: f64,
    /// Effective playback rate combining user rate and sample-rate-conversion ratio
    rate: f64,
    amplitude: f32,
    pan: f32,
    done: bool,
    /// Optional duration limit in samples (from sustain: parameter)
    max_samples: Option<u64>,
    /// Counts samples played for sustain truncation
    samples_elapsed: u64,
    /// Position in sample data where playback should stop (from finish: parameter)
    finish_sample: Option<usize>,
    /// Per-voice FX chain from scoped `with_fx` blocks (None = dry)
    voice_fx: Option<VoiceFx>,
    /// Optional ADSR envelope in sample counts
    envelope: Option<SampleEnvelope>,
}

/// ADSR envelope state for sample playback (durations in sample counts)
struct SampleEnvelope {
    attack_samples: u64,
    decay_samples: u64,
    sustain_level: f32,
    release_samples: u64,
    /// Total playback samples before release starts (auto-calculated from sample duration)
    sustain_end: u64,
}

impl AudioEngine {
    pub fn new(recorder: Recorder) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device found")?;

        let supported = device
            .default_output_config()
            .map_err(|e| format!("No default config: {}", e))?;

        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;

        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate: supported.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let state = Arc::new(Mutex::new(AudioState {
            sample_rate,
            ..Default::default()
        }));

        let (cmd_tx, cmd_rx): (Sender<AudioCommand>, Receiver<AudioCommand>) = bounded(16384);

        let state_clone = state.clone();
        let recorder_clone = recorder.clone();

        let mut voices: Vec<Voice> = Vec::new();
        let mut sample_playbacks: Vec<SamplePlayback> = Vec::new();
        let mut master_volume: f32 = 1.0;
        let mut effect_chain = EffectChain::new(sample_rate as f32);
        let mut waveform_write_pos: usize = 0;
        // Per-voice FX: track active FX blocks via FxStart/FxEnd stack
        let mut fx_stack: Vec<(String, Vec<(String, f32)>)> = Vec::new();
        // Shared reverb bus for per-voice reverb sends (separate from global/UI reverb)
        let mut fx_reverb_bus = super::effects::EffectChain::new(sample_rate as f32);
        // Shared delay bus for per-voice delay sends
        let mut fx_delay_buf_l: Vec<f32> = vec![0.0; (sample_rate as usize) * 2];
        let mut fx_delay_buf_r: Vec<f32> = vec![0.0; (sample_rate as usize) * 2];
        let mut fx_delay_write_pos: usize = 0;
        let mut fx_delay_read_samples: usize = 0;
        let mut fx_delay_feedback: f32 = 0.5;

        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Sync master_volume from shared state so that set_volume
                    // changes take effect immediately, even if the
                    // SetMasterVolume command hasn't been delivered through the
                    // channel yet (e.g. when the channel is congested).
                    master_volume = state_clone.lock().master_volume;

                    // Process commands
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        match cmd {
                            AudioCommand::PlayNote {
                                synth_type,
                                frequency,
                                amplitude,
                                duration_secs,
                                envelope,
                                pan,
                                params,
                                fx_context: _,
                            } => {
                                let voice = SynthVoice::new_with_params(
                                    synth_type,
                                    frequency,
                                    amplitude,
                                    sample_rate as f32,
                                    envelope,
                                    &params,
                                );
                                // Create per-voice FX chain from active FX stack
                                let voice_fx = VoiceFx::from_fx_stack(&fx_stack, sample_rate as f32);
                                voices.push(Voice {
                                    synth: voice,
                                    samples_elapsed: 0,
                                    duration_samples: (duration_secs * sample_rate as f32) as u64,
                                    pan,
                                    voice_fx,
                                });
                            }
                            AudioCommand::PlaySample {
                                samples,
                                sample_rate: file_sr,
                                amplitude,
                                rate,
                                pan,
                                sustain_secs,
                                beat_stretch: _,
                                start,
                                finish,
                                envelope,
                                fx_context: _,
                            } => {
                                // Combine user rate with sample-rate-conversion ratio
                                // so samples recorded at any SR play at correct pitch/speed
                                let sr_ratio = file_sr as f64 / sample_rate as f64;
                                let effective_rate = rate as f64 * sr_ratio;
                                let max_samp =
                                    sustain_secs.map(|s| (s * sample_rate as f32) as u64);
                                
                                let len = samples.len();
                                // Calculate start position from start: parameter (0.0-1.0)
                                let start_pos = match start {
                                    Some(s) => ((s.clamp(0.0, 1.0) as f64) * len as f64) as usize,
                                    None => 0,
                                };
                                // Calculate finish position from finish: parameter (0.0-1.0)
                                let finish_pos = match finish {
                                    Some(f) => Some(((f.clamp(0.0, 1.0) as f64) * len as f64) as usize),
                                    None => None,
                                };

                                // Build sample envelope if present
                                let sample_env = envelope.map(|env| {
                                    let attack_s = (env.attack * sample_rate as f32) as u64;
                                    let decay_s = (env.decay * sample_rate as f32) as u64;
                                    let release_s = (env.release * sample_rate as f32) as u64;
                                    // Calculate total playback length in samples
                                    let play_len = if let Some(max) = max_samp {
                                        max
                                    } else {
                                        let end = finish_pos.unwrap_or(len);
                                        let start = start_pos;
                                        let data_samples = if end > start { end - start } else { 0 };
                                        (data_samples as f64 / effective_rate.abs()) as u64
                                    };
                                    let sustain_end = play_len.saturating_sub(release_s);
                                    SampleEnvelope {
                                        attack_samples: attack_s,
                                        decay_samples: decay_s,
                                        sustain_level: env.sustain,
                                        release_samples: release_s,
                                        sustain_end,
                                    }
                                });
                                
                                sample_playbacks.push(SamplePlayback {
                                    data: samples,
                                    position: start_pos as f64,
                                    rate: effective_rate,
                                    amplitude,
                                    pan,
                                    done: false,
                                    max_samples: max_samp,
                                    samples_elapsed: 0,
                                    finish_sample: finish_pos,
                                    voice_fx: VoiceFx::from_fx_stack(&fx_stack, sample_rate as f32),
                                    envelope: sample_env,
                                });
                            }
                            AudioCommand::SetBpm(bpm) => {
                                let mut s = state_clone.lock();
                                s.bpm = bpm;
                            }
                            AudioCommand::SetMasterVolume(vol) => {
                                master_volume = vol;
                                let mut s = state_clone.lock();
                                s.master_volume = vol;
                            }
                            AudioCommand::Stop => {
                                voices.clear();
                                sample_playbacks.clear();
                                fx_stack.clear();
                                let mut s = state_clone.lock();
                                s.is_playing = false;
                            }
                            AudioCommand::SetEffect {
                                reverb_mix,
                                reverb_room,
                                delay_time,
                                delay_feedback,
                                distortion,
                                lpf_cutoff,
                                hpf_cutoff,
                                slicer_phase,
                                slicer_mix,
                                slicer_wave,
                                bitcrusher_bits,
                                bitcrusher_sample_rate,
                                bitcrusher_mix,
                                compressor_threshold,
                                compressor_clamp_time,
                                compressor_relax_time,
                                compressor_mix,
                                normaliser_level,
                                // New effects
                                flanger_rate,
                                flanger_depth,
                                flanger_feedback,
                                flanger_mix,
                                chorus_rate,
                                chorus_depth,
                                chorus_mix,
                                ring_mod_freq,
                                ring_mod_mix,
                                pan_position,
                                wobble_rate,
                                wobble_depth,
                                wobble_mix,
                                octaver_mix,
                                octaver_sub_amp,
                                octaver_super_amp,
                                reverb_damp,
                                delay_mix,
                                lpf_res,
                                hpf_res,
                                cutoff_is_hz,
                            } => {
                                effect_chain.set_reverb_mix(reverb_mix);
                                effect_chain.set_reverb_room(reverb_room);
                                effect_chain.set_reverb_damp(reverb_damp);
                                effect_chain.set_delay(delay_time, delay_feedback);
                                effect_chain.set_delay_mix(delay_mix);
                                effect_chain.set_distortion(distortion);
                                if cutoff_is_hz {
                                    effect_chain.set_lpf_hz(lpf_cutoff);
                                    effect_chain.set_hpf_hz(hpf_cutoff);
                                } else {
                                    effect_chain.set_lpf(lpf_cutoff);
                                    effect_chain.set_hpf(hpf_cutoff);
                                }
                                effect_chain.set_lpf_res(lpf_res);
                                effect_chain.set_hpf_res(hpf_res);
                                effect_chain.set_slicer(slicer_phase, slicer_mix, slicer_wave);
                                effect_chain.set_bitcrusher(
                                    bitcrusher_bits,
                                    bitcrusher_sample_rate,
                                    bitcrusher_mix,
                                );
                                effect_chain.set_compressor(
                                    compressor_threshold,
                                    compressor_clamp_time,
                                    compressor_relax_time,
                                    compressor_mix,
                                );
                                effect_chain.set_normaliser(normaliser_level);
                                // New effects
                                effect_chain.set_flanger(
                                    flanger_rate,
                                    flanger_depth,
                                    flanger_feedback,
                                    flanger_mix,
                                );
                                effect_chain.set_chorus(chorus_rate, chorus_depth, chorus_mix);
                                effect_chain.set_ring_mod(ring_mod_freq, ring_mod_mix);
                                effect_chain.set_pan(pan_position);
                                effect_chain.set_wobble(wobble_rate, wobble_depth, wobble_mix);
                                effect_chain.set_octaver(
                                    octaver_mix,
                                    octaver_sub_amp,
                                    octaver_super_amp,
                                );
                            }
                            // FxStart/FxEnd: manage per-voice FX stack for scoped with_fx blocks
                            AudioCommand::FxStart { fx_type, mut params, fx_id: _, parent_fx_id: _ } => {
                                // Convert echo/delay/ping_pong phase from beats to seconds
                                if fx_type == "echo" || fx_type == "delay" || fx_type == "ping_pong" {
                                    let bpm = state_clone.lock().bpm;
                                    let beat_dur = 60.0 / bpm;
                                    for (k, v) in params.iter_mut() {
                                        if k == "phase" || k == "time" {
                                            *v *= beat_dur;
                                        }
                                    }
                                }
                                fx_stack.push((fx_type, params));
                            }
                            AudioCommand::FxEnd { fx_id: _ } => {
                                fx_stack.pop();
                            }
                            // SetRuntimeVar is handled by the scheduler thread, not the audio callback
                            AudioCommand::SetRuntimeVar { .. } => {}
                        }
                    }

                    // Generate audio
                    let frames = data.len() / channels;
                    let mut waveform_local_buf: Vec<f32> = Vec::with_capacity(frames);
                    let mut local_is_playing = false;
                    for frame in 0..frames {
                        let mut left = 0.0f32;
                        let mut right = 0.0f32;
                        // Per-voice reverb/delay send accumulators (reset each frame)
                        let mut reverb_send_l = 0.0f32;
                        let mut reverb_send_r = 0.0f32;
                        let mut delay_send_l = 0.0f32;
                        let mut delay_send_r = 0.0f32;
                        let mut has_voice_reverb = false;
                        let mut has_voice_delay = false;
                        let mut voice_reverb_room = 0.6f32;
                        let mut voice_reverb_damp = 0.5f32;
                        let mut voice_delay_time = 0.25f32;
                        let mut voice_delay_feedback = 0.5f32;

                        // Mix synth voices
                        for voice in voices.iter_mut() {
                            if voice.samples_elapsed < voice.duration_samples {
                                let sample = voice.synth.next_sample();
                                let env = voice
                                    .synth
                                    .envelope_value(voice.samples_elapsed, voice.duration_samples);
                                let mut s = sample * env;

                                // Apply per-voice FX chain (from scoped with_fx blocks)
                                if let Some(ref mut vfx) = voice.voice_fx {
                                    s = vfx.process(s);
                                }

                                // Determine pan (per-voice FX may override)
                                let effective_pan = voice.voice_fx.as_ref()
                                    .and_then(|vfx| vfx.pan_override())
                                    .unwrap_or(voice.pan);

                                // Equal-power panning (constant-power pan law matching Sonic Pi's Pan2)
                                let pan_rad = (effective_pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                                let l_gain = pan_rad.cos();
                                let r_gain = pan_rad.sin();
                                left += s * l_gain;
                                right += s * r_gain;

                                // Route to shared reverb/delay buses if voice has send effects
                                if let Some(ref vfx) = voice.voice_fx {
                                    if vfx.reverb_send > 0.001 {
                                        reverb_send_l += s * l_gain * vfx.reverb_send;
                                        reverb_send_r += s * r_gain * vfx.reverb_send;
                                        has_voice_reverb = true;
                                        voice_reverb_room = vfx.reverb_room;
                                        voice_reverb_damp = vfx.reverb_damp;
                                        // Reduce dry signal by reverb mix to avoid doubling
                                        left -= s * l_gain * vfx.reverb_send;
                                        right -= s * r_gain * vfx.reverb_send;
                                    }
                                    if vfx.delay_send > 0.001 {
                                        delay_send_l += s * l_gain * vfx.delay_send;
                                        delay_send_r += s * r_gain * vfx.delay_send;
                                        has_voice_delay = true;
                                        voice_delay_time = vfx.delay_time;
                                        voice_delay_feedback = vfx.delay_feedback;
                                        // Reduce dry signal
                                        left -= s * l_gain * vfx.delay_send;
                                        right -= s * r_gain * vfx.delay_send;
                                    }
                                }

                                voice.samples_elapsed += 1;
                            }
                        }

                        // Mix sample playbacks (with cubic Hermite interpolation)
                        for sp in sample_playbacks.iter_mut() {
                            if !sp.done {
                                // Sustain truncation: stop playback after max_samples
                                if let Some(max_s) = sp.max_samples {
                                    if sp.samples_elapsed >= max_s {
                                        sp.done = true;
                                        continue;
                                    }
                                }
                                let idx = sp.position as usize;
                                let len = sp.data.len();
                                // Use finish_sample if set, otherwise end of sample
                                let end_pos = sp.finish_sample.unwrap_or(len);
                                // Support both forward and reverse playback
                                let in_bounds = if sp.rate >= 0.0 {
                                    idx + 1 < len && idx + 1 < end_pos
                                } else {
                                    // Reverse playback: position decreases, check >= 0
                                    sp.position >= 0.0 && idx < len
                                };
                                if in_bounds {
                                    let frac = (sp.position - idx as f64).abs() as f32;
                                    // Cubic Hermite interpolation for smooth playback
                                    let mut s = if idx >= 1 && idx + 2 < len {
                                        let y0 = sp.data[idx - 1];
                                        let y1 = sp.data[idx];
                                        let y2 = sp.data[idx + 1];
                                        let y3 = sp.data[idx + 2];
                                        let c0 = y1;
                                        let c1 = 0.5 * (y2 - y0);
                                        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                                        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                                        ((c3 * frac + c2) * frac + c1) * frac + c0
                                    } else if idx + 1 < len {
                                        // Fall back to linear at boundaries
                                        sp.data[idx] * (1.0 - frac) + sp.data[idx + 1] * frac
                                    } else {
                                        sp.data[idx]
                                    };
                                    // Apply fade-out near the sustain end or finish position to avoid clicks
                                    let mut amp = sp.amplitude;

                                    // Apply ADSR envelope if present
                                    if let Some(ref env) = sp.envelope {
                                        let t = sp.samples_elapsed;
                                        let env_gain = if t < env.attack_samples {
                                            // Attack phase: ramp from 0 to 1
                                            if env.attack_samples > 0 { t as f32 / env.attack_samples as f32 } else { 1.0 }
                                        } else if t < env.attack_samples + env.decay_samples {
                                            // Decay phase: ramp from 1 to sustain_level
                                            let decay_pos = (t - env.attack_samples) as f32;
                                            let decay_len = env.decay_samples as f32;
                                            if decay_len > 0.0 {
                                                1.0 - (1.0 - env.sustain_level) * (decay_pos / decay_len)
                                            } else {
                                                env.sustain_level
                                            }
                                        } else if t < env.sustain_end {
                                            // Sustain phase: hold at sustain_level
                                            env.sustain_level
                                        } else {
                                            // Release phase: ramp from sustain_level to 0
                                            let release_pos = (t - env.sustain_end) as f32;
                                            let release_len = env.release_samples as f32;
                                            if release_len > 0.0 {
                                                let progress = (release_pos / release_len).min(1.0);
                                                env.sustain_level * (1.0 - progress)
                                            } else {
                                                0.0
                                            }
                                        };
                                        amp *= env_gain;
                                    }

                                    if let Some(max_s) = sp.max_samples {
                                        let fade_samples = (sample_rate as f32 * 0.005) as u64; // 5ms fade
                                        let remaining = max_s.saturating_sub(sp.samples_elapsed);
                                        if remaining < fade_samples && fade_samples > 0 {
                                            amp *= remaining as f32 / fade_samples as f32;
                                        }
                                    }
                                    // Also fade near finish position
                                    if let Some(finish) = sp.finish_sample {
                                        let fade_samples = (sample_rate as f32 * 0.005) as usize; // 5ms fade
                                        let remaining = finish.saturating_sub(idx);
                                        if remaining < fade_samples && fade_samples > 0 {
                                            amp *= remaining as f32 / fade_samples as f32;
                                        }
                                    }
                                    s *= amp;

                                    // Apply per-voice FX chain (from scoped with_fx blocks)
                                    if let Some(ref mut vfx) = sp.voice_fx {
                                        s = vfx.process(s);
                                    }

                                    // Determine pan (per-voice FX may override)
                                    let effective_pan = sp.voice_fx.as_ref()
                                        .and_then(|vfx| vfx.pan_override())
                                        .unwrap_or(sp.pan);

                                    // Equal-power panning (matching Sonic Pi's Pan2)
                                    let pan_rad = (effective_pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                                    let l_gain = pan_rad.cos();
                                    let r_gain = pan_rad.sin();
                                    left += s * l_gain;
                                    right += s * r_gain;

                                    // Route to shared reverb/delay buses if sample has send effects
                                    if let Some(ref vfx) = sp.voice_fx {
                                        if vfx.reverb_send > 0.001 {
                                            reverb_send_l += s * l_gain * vfx.reverb_send;
                                            reverb_send_r += s * r_gain * vfx.reverb_send;
                                            has_voice_reverb = true;
                                            voice_reverb_room = vfx.reverb_room;
                                            voice_reverb_damp = vfx.reverb_damp;
                                            left -= s * l_gain * vfx.reverb_send;
                                            right -= s * r_gain * vfx.reverb_send;
                                        }
                                        if vfx.delay_send > 0.001 {
                                            delay_send_l += s * l_gain * vfx.delay_send;
                                            delay_send_r += s * r_gain * vfx.delay_send;
                                            has_voice_delay = true;
                                            voice_delay_time = vfx.delay_time;
                                            voice_delay_feedback = vfx.delay_feedback;
                                            left -= s * l_gain * vfx.delay_send;
                                            right -= s * r_gain * vfx.delay_send;
                                        }
                                    }

                                    sp.position += sp.rate;
                                    sp.samples_elapsed += 1;
                                } else {
                                    sp.done = true;
                                }
                            }
                        }

                        // Process per-voice reverb send bus
                        if has_voice_reverb {
                            fx_reverb_bus.set_reverb_mix(1.0); // Full wet since we already scaled by send level
                            fx_reverb_bus.set_reverb_room(voice_reverb_room);
                            fx_reverb_bus.set_reverb_damp(voice_reverb_damp);
                            let (rev_l, rev_r) = fx_reverb_bus.process(reverb_send_l, reverb_send_r);
                            left += rev_l;
                            right += rev_r;
                        }

                        // Process per-voice delay send bus
                        if has_voice_delay {
                            let delay_samples = (voice_delay_time * sample_rate as f32) as usize;
                            let buf_len = fx_delay_buf_l.len();
                            if delay_samples > 0 && delay_samples < buf_len {
                                fx_delay_read_samples = delay_samples;
                                fx_delay_feedback = voice_delay_feedback.clamp(0.0, 0.95);
                            }
                            let read_pos = if fx_delay_write_pos >= fx_delay_read_samples {
                                fx_delay_write_pos - fx_delay_read_samples
                            } else {
                                buf_len - (fx_delay_read_samples - fx_delay_write_pos)
                            };
                            let dl = fx_delay_buf_l[read_pos % buf_len];
                            let dr = fx_delay_buf_r[read_pos % buf_len];
                            fx_delay_buf_l[fx_delay_write_pos % buf_len] = delay_send_l + dl * fx_delay_feedback;
                            fx_delay_buf_r[fx_delay_write_pos % buf_len] = delay_send_r + dr * fx_delay_feedback;
                            fx_delay_write_pos = (fx_delay_write_pos + 1) % buf_len;
                            left += dl;
                            right += dr;
                        }

                        // Apply global effects (from UI panel)
                        let (proc_l, proc_r) = effect_chain.process(left, right);
                        left = proc_l * master_volume;
                        right = proc_r * master_volume;

                        // Clip
                        left = left.clamp(-1.0, 1.0);
                        right = right.clamp(-1.0, 1.0);

                        // Write to output
                        for ch in 0..channels {
                            data[frame * channels + ch] = if ch % 2 == 0 { left } else { right };
                        }

                        // Record the mixed audio (mono mix of left and right)
                        let mono_sample = (left + right) * 0.5;
                        recorder_clone.push_samples(&[mono_sample]);

                        // Accumulate waveform sample locally (written to shared buffer once per callback)
                        waveform_local_buf.push(mono_sample);
                        local_is_playing = !voices.is_empty() || sample_playbacks.iter().any(|sp| !sp.done);
                    }

                    // Batch-write waveform buffer and is_playing once per callback
                    // to minimize mutex contention with the UI thread
                    {
                        let mut s = state_clone.lock();
                        let len = s.waveform_buffer.len();
                        for &sample in &waveform_local_buf {
                            s.waveform_buffer[waveform_write_pos % len] = sample;
                            waveform_write_pos += 1;
                        }
                        s.is_playing = local_is_playing;
                    }
                    waveform_local_buf.clear();

                    // Remove finished voices and samples
                    voices.retain(|v| v.samples_elapsed < v.duration_samples);
                    sample_playbacks.retain(|sp| !sp.done);
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            ),
            _ => {
                return Err(format!(
                    "Unsupported sample format: {:?}",
                    supported.sample_format()
                ));
            }
        }
        .map_err(|e| format!("Failed to build stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to play stream: {}", e))?;

        Ok(Self {
            state,
            command_tx: cmd_tx,
            _stream: Mutex::new(Some(stream)),
        })
    }

    pub fn send_command(&self, cmd: AudioCommand) -> Result<(), String> {
        self.command_tx
            .try_send(cmd)
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    pub fn command_tx_clone(&self) -> Sender<AudioCommand> {
        self.command_tx.clone()
    }

    pub fn get_waveform(&self) -> Vec<f32> {
        let s = self.state.lock();
        s.waveform_buffer.clone()
    }

    pub fn get_state_snapshot(&self) -> (bool, f32, f32) {
        let s = self.state.lock();
        (s.is_playing, s.master_volume, s.bpm)
    }
}
