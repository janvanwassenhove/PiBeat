use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use std::sync::Arc;

use super::effects::EffectChain;
use super::recorder::Recorder;
use super::synth::{Envelope, OscillatorType, SynthVoice};

/// Messages sent from the main thread to the audio thread
#[derive(Debug, Clone)]
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
    },
    PlaySample {
        samples: Vec<f32>,
        sample_rate: u32,
        amplitude: f32,
        rate: f32,
        pan: f32,
    },
    SetBpm(f32),
    SetMasterVolume(f32),
    Stop,
    SetEffect {
        reverb_mix: f32,
        delay_time: f32,
        delay_feedback: f32,
        distortion: f32,
        lpf_cutoff: f32,
        hpf_cutoff: f32,
    },
    /// Start an FX block — allocates an audio bus and creates the FX synth.
    /// All subsequent PlayNote/PlaySample commands route through this FX
    /// until the matching FxEnd.
    FxStart {
        fx_type: String,
        params: Vec<(String, f32)>,
    },
    /// End the current FX block — frees the FX synth, restores output bus.
    FxEnd,
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
}

struct SamplePlayback {
    data: Vec<f32>,
    position: f64,
    /// Effective playback rate combining user rate and sample-rate-conversion ratio
    rate: f64,
    amplitude: f32,
    pan: f32,
    done: bool,
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

        let (cmd_tx, cmd_rx): (Sender<AudioCommand>, Receiver<AudioCommand>) = bounded(4096);

        let state_clone = state.clone();
        let recorder_clone = recorder.clone();

        let mut voices: Vec<Voice> = Vec::new();
        let mut sample_playbacks: Vec<SamplePlayback> = Vec::new();
        let mut master_volume: f32 = 1.0;
        let mut effect_chain = EffectChain::new(sample_rate as f32);
        let mut waveform_write_pos: usize = 0;

        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
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
                                params: _, // Only used by SC engine
                            } => {
                                let voice = SynthVoice::new(
                                    synth_type,
                                    frequency,
                                    amplitude,
                                    sample_rate as f32,
                                    envelope,
                                );
                                voices.push(Voice {
                                    synth: voice,
                                    samples_elapsed: 0,
                                    duration_samples: (duration_secs * sample_rate as f32) as u64,
                                    pan,
                                });
                            }
                            AudioCommand::PlaySample {
                                samples,
                                sample_rate: file_sr,
                                amplitude,
                                rate,
                                pan,
                            } => {
                                // Combine user rate with sample-rate-conversion ratio
                                // so samples recorded at any SR play at correct pitch/speed
                                let sr_ratio = file_sr as f64 / sample_rate as f64;
                                let effective_rate = rate as f64 * sr_ratio;
                                sample_playbacks.push(SamplePlayback {
                                    data: samples,
                                    position: 0.0_f64,
                                    rate: effective_rate,
                                    amplitude,
                                    pan,
                                    done: false,
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
                                let mut s = state_clone.lock();
                                s.is_playing = false;
                            }
                            AudioCommand::SetEffect {
                                reverb_mix,
                                delay_time,
                                delay_feedback,
                                distortion,
                                lpf_cutoff,
                                hpf_cutoff,
                            } => {
                                effect_chain.set_reverb_mix(reverb_mix);
                                effect_chain.set_delay(delay_time, delay_feedback);
                                effect_chain.set_distortion(distortion);
                                effect_chain.set_lpf(lpf_cutoff);
                                effect_chain.set_hpf(hpf_cutoff);
                            }
                            // FxStart/FxEnd only used by SC engine; cpal ignores them
                            AudioCommand::FxStart { .. } | AudioCommand::FxEnd => {}
                        }
                    }

                    // Generate audio
                    let frames = data.len() / channels;
                    for frame in 0..frames {
                        let mut left = 0.0f32;
                        let mut right = 0.0f32;

                        // Mix synth voices
                        for voice in voices.iter_mut() {
                            if voice.samples_elapsed < voice.duration_samples {
                                let sample = voice.synth.next_sample();
                                let env = voice.synth.envelope_value(voice.samples_elapsed, voice.duration_samples);
                                let s = sample * env;
                                let l_gain = ((1.0 - voice.pan) * 0.5 + 0.5).min(1.0);
                                let r_gain = ((1.0 + voice.pan) * 0.5 + 0.5).min(1.0);
                                left += s * l_gain;
                                right += s * r_gain;
                                voice.samples_elapsed += 1;
                            }
                        }

                        // Mix sample playbacks (with cubic Hermite interpolation)
                        for sp in sample_playbacks.iter_mut() {
                            if !sp.done {
                                let idx = sp.position as usize;
                                let len = sp.data.len();
                                if idx + 1 < len {
                                    let frac = (sp.position - idx as f64) as f32;
                                    // Cubic Hermite interpolation for smooth playback
                                    let s = if idx >= 1 && idx + 2 < len {
                                        let y0 = sp.data[idx - 1];
                                        let y1 = sp.data[idx];
                                        let y2 = sp.data[idx + 1];
                                        let y3 = sp.data[idx + 2];
                                        let c0 = y1;
                                        let c1 = 0.5 * (y2 - y0);
                                        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                                        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                                        ((c3 * frac + c2) * frac + c1) * frac + c0
                                    } else {
                                        // Fall back to linear at boundaries
                                        sp.data[idx] * (1.0 - frac) + sp.data[idx + 1] * frac
                                    };
                                    let s = s * sp.amplitude;
                                    let l_gain = ((1.0 - sp.pan) * 0.5 + 0.5).min(1.0);
                                    let r_gain = ((1.0 + sp.pan) * 0.5 + 0.5).min(1.0);
                                    left += s * l_gain;
                                    right += s * r_gain;
                                    sp.position += sp.rate;
                                } else {
                                    sp.done = true;
                                }
                            }
                        }

                        // Apply effects
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

                        // Write to waveform buffer
                        {
                            let mut s = state_clone.lock();
                            let len = s.waveform_buffer.len();
                            s.waveform_buffer[waveform_write_pos % len] = mono_sample;
                            waveform_write_pos += 1;
                            s.is_playing = !voices.is_empty() || sample_playbacks.iter().any(|sp| !sp.done);
                        }
                    }

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
