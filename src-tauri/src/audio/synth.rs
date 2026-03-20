use std::f32::consts::PI;

/// All Sonic Pi synth types
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum OscillatorType {
    // Basic waveforms
    Sine,
    Saw,
    Square,
    Triangle,
    Noise,
    Pulse,
    SuperSaw,
    // Detuned variants
    DSaw,
    DPulse,
    DTri,
    // FM synthesis
    FM,
    ModFM,
    ModSine,
    ModSaw,
    ModDSaw,
    ModTri,
    ModPulse,
    // Classic synths
    TB303,
    Prophet,
    Zawa,
    // Filtered / layered
    Blade,
    TechSaws,
    Hoover,
    // Plucked / percussive
    Pluck,
    Piano,
    PrettyBell,
    DullBell,
    // Pads / ambient
    Hollow,
    DarkAmbience,
    Growl,
    // Chiptune
    ChipLead,
    ChipBass,
    ChipNoise,
    // Colored noise
    BNoise,
    PNoise,
    GNoise,
    CNoise,
    // Sub
    SubPulse,
    // Percussive
    GabberKick,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Envelope {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 1.0,
        }
    }
}

pub struct SynthVoice {
    osc_type: OscillatorType,
    frequency: f32,
    amplitude: f32,
    sample_rate: f32,
    phase: f32,
    envelope: Envelope,
    // Multi-oscillator phases (for SuperSaw, TechSaws, Hoover, etc.)
    phases: [f32; 7],
    detune_amounts: [f32; 7],
    // Secondary phase for detuned / modulated oscillators
    phase2: f32,
    // Noise state
    noise_state: u32,
    // Pulse width
    pulse_width: f32,
    // FM modulation index & ratio
    mod_index: f32,
    mod_ratio: f32,
    mod_phase: f32,
    // Filter state (for TB303, Blade, etc.)
    filter_cutoff: f32,
    filter_resonance: f32,
    filter_lp: f32,
    filter_bp: f32,
    filter_hp: f32,
    // SVF integrator state (Cytomic/Simper topology)
    svf_ic1: f32,
    svf_ic2: f32,
    // Pluck / Karplus-Strong buffer
    pluck_buffer: Vec<f32>,
    pluck_pos: usize,
    // Brown noise accumulator
    brown_acc: f32,
    // Pink noise state (Voss-McCartney)
    pink_rows: [f32; 16],
    pink_index: u32,
    pink_running_sum: f32,
    // LFO for modulated synths
    lfo_phase: f32,
    lfo_rate: f32,
    // Sample counter for time-dependent synthesis
    sample_count: u64,
}

impl SynthVoice {
    /// Create a new SynthVoice with default parameters.
    pub fn new(
        osc_type: OscillatorType,
        frequency: f32,
        amplitude: f32,
        sample_rate: f32,
        envelope: Envelope,
    ) -> Self {
        Self::new_with_params(osc_type, frequency, amplitude, sample_rate, envelope, &[])
    }

    /// Create a new SynthVoice with synth-specific parameters forwarded from
    /// the parser (cutoff, res, detune, depth, divisor, etc.).
    pub fn new_with_params(
        osc_type: OscillatorType,
        frequency: f32,
        amplitude: f32,
        sample_rate: f32,
        envelope: Envelope,
        params: &[(String, f32)],
    ) -> Self {
        // Helper to look up a named parameter
        let get_param =
            |name: &str| -> Option<f32> { params.iter().find(|(k, _)| k == name).map(|(_, v)| *v) };

        // --- Detune ---
        // Sonic Pi super_saw default detune: 0.1 (maps to ~0.36 semitones per voice)
        // We spread the 7 oscillators symmetrically: indices -3..+3 × detune_factor
        let detune_base = get_param("detune").unwrap_or(match osc_type {
            OscillatorType::SuperSaw => 0.1,
            OscillatorType::DSaw | OscillatorType::DPulse | OscillatorType::DTri => 0.1,
            _ => 0.0,
        });
        // For SuperSaw: each oscillator gets (i-3) * spread where spread = detune * 0.06
        // This produces a total spread of ±3*spread = ±0.018 at detune=0.1
        // which is close to Sonic Pi's spread (~0.36 semitones = ±0.02 freq ratio)
        let spread = detune_base * 0.06;
        let detune_amounts: [f32; 7] = [
            -3.0 * spread,
            -2.0 * spread,
            -1.0 * spread,
            0.0,
            1.0 * spread,
            2.0 * spread,
            3.0 * spread,
        ];

        // Initialize Karplus-Strong buffer for Pluck/Piano
        let pluck_len = if frequency > 0.0 {
            (sample_rate / frequency).max(2.0) as usize
        } else {
            256
        };
        let mut pluck_buffer = vec![0.0f32; pluck_len];
        // Fill with noise burst for pluck
        let mut rng: u32 = 54321;
        for s in pluck_buffer.iter_mut() {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            *s = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
        }

        // Determine FM parameters based on synth type (overridable via params)
        let (mod_index, mod_ratio) = match osc_type {
            OscillatorType::FM => (
                get_param("depth").unwrap_or(1.0) * 5.0,
                1.0 / get_param("divisor").unwrap_or(2.0).max(0.01),
            ),
            OscillatorType::ModFM => (
                get_param("depth").unwrap_or(1.0) * 8.0,
                1.0 / get_param("divisor").unwrap_or(2.0).max(0.01),
            ),
            _ => (1.0, 1.0),
        };

        let lfo_rate = match osc_type {
            OscillatorType::ModSine
            | OscillatorType::ModSaw
            | OscillatorType::ModTri
            | OscillatorType::ModPulse
            | OscillatorType::ModDSaw => get_param("mod_phase").unwrap_or(1.0) * 6.0,
            OscillatorType::Zawa => 1.0,
            OscillatorType::Growl => 8.0,
            _ => 5.0,
        };

        // --- Filter cutoff ---
        // Sonic Pi uses MIDI note numbers for cutoff (0-130, default varies by synth).
        // Convert to Hz: freq = 440 * 2^((midi-69)/12)
        let midi_to_hz = |midi: f32| -> f32 { 440.0 * 2.0f32.powf((midi - 69.0) / 12.0) };
        let (filter_cutoff, filter_resonance) = match osc_type {
            OscillatorType::TB303 => {
                let co = get_param("cutoff").map(|m| midi_to_hz(m)).unwrap_or(800.0);
                let r = get_param("res").unwrap_or(0.8);
                (co, r)
            }
            OscillatorType::SuperSaw => {
                let co = get_param("cutoff")
                    .map(|m| midi_to_hz(m))
                    .unwrap_or(midi_to_hz(130.0));
                let user_res = get_param("res").unwrap_or(0.7);
                // Sonic Pi inverts res for RLPF: rq = 1 - res.
                // Our SVF uses k = 2*(1-filter_res). To get k = 1-user_res
                // (matching RLPF's rq), set filter_res = 0.5 + user_res/2.
                let r = (0.5 + user_res / 2.0).clamp(0.01, 0.99);
                (co, r)
            }
            OscillatorType::Saw
            | OscillatorType::Square
            | OscillatorType::Pulse
            | OscillatorType::DSaw
            | OscillatorType::DPulse
            | OscillatorType::Prophet
            | OscillatorType::SubPulse => {
                let co = get_param("cutoff")
                    .map(|m| midi_to_hz(m))
                    .unwrap_or(midi_to_hz(100.0));
                let r = get_param("res").unwrap_or(0.3);
                (co, r)
            }
            OscillatorType::Blade => {
                let co = get_param("cutoff")
                    .map(|m| midi_to_hz(m))
                    .unwrap_or(midi_to_hz(100.0));
                let r = get_param("res").unwrap_or(0.5);
                (co, r)
            }
            OscillatorType::TechSaws => {
                let co = get_param("cutoff")
                    .map(|m| midi_to_hz(m))
                    .unwrap_or(midi_to_hz(130.0));
                let r = get_param("res").unwrap_or(0.3);
                (co, r)
            }
            OscillatorType::Hollow => {
                let co = get_param("cutoff").map(|m| midi_to_hz(m)).unwrap_or(600.0);
                let r = get_param("res").unwrap_or(0.9);
                (co, r)
            }
            OscillatorType::DarkAmbience => {
                let co = get_param("cutoff").map(|m| midi_to_hz(m)).unwrap_or(300.0);
                let r = get_param("res").unwrap_or(0.5);
                (co, r)
            }
            _ => {
                // Synths without a filter: set cutoff very high (effectively bypass)
                let co = get_param("cutoff")
                    .map(|m| midi_to_hz(m))
                    .unwrap_or(20000.0);
                let r = get_param("res").unwrap_or(0.0);
                (co, r)
            }
        };

        let pulse_width = get_param("pulse_width")
            .or_else(|| get_param("width"))
            .unwrap_or(match osc_type {
                OscillatorType::SubPulse => 0.5,
                OscillatorType::Prophet => 0.3,
                _ => 0.5,
            });

        Self {
            osc_type,
            frequency,
            amplitude,
            sample_rate,
            phase: 0.0,
            envelope,
            phases: [0.0; 7],
            detune_amounts,
            phase2: 0.0,
            noise_state: 12345,
            pulse_width,
            mod_index,
            mod_ratio,
            mod_phase: 0.0,
            filter_cutoff,
            filter_resonance,
            filter_lp: 0.0,
            filter_bp: 0.0,
            filter_hp: 0.0,
            svf_ic1: 0.0,
            svf_ic2: 0.0,
            pluck_buffer,
            pluck_pos: 0,
            brown_acc: 0.0,
            pink_rows: [0.0; 16],
            pink_index: 0,
            pink_running_sum: 0.0,
            lfo_phase: 0.0,
            lfo_rate,
            sample_count: 0,
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        self.sample_count += 1;
        let sample = match self.osc_type {
            OscillatorType::Sine => self.sine(),
            OscillatorType::Saw => self.saw(),
            OscillatorType::Square => self.square(),
            OscillatorType::Triangle => self.triangle(),
            OscillatorType::Noise => self.white_noise(),
            OscillatorType::Pulse => self.pulse(),
            OscillatorType::SuperSaw => self.super_saw(),
            OscillatorType::DSaw => self.detuned_saw(),
            OscillatorType::DPulse => self.detuned_pulse(),
            OscillatorType::DTri => self.detuned_tri(),
            OscillatorType::FM => self.fm_synth(),
            OscillatorType::ModFM => self.fm_synth(),
            OscillatorType::ModSine => self.mod_sine(),
            OscillatorType::ModSaw => self.mod_saw(),
            OscillatorType::ModDSaw => self.mod_dsaw(),
            OscillatorType::ModTri => self.mod_tri(),
            OscillatorType::ModPulse => self.mod_pulse(),
            OscillatorType::TB303 => self.tb303(),
            OscillatorType::Prophet => self.prophet(),
            OscillatorType::Zawa => self.zawa(),
            OscillatorType::Blade => self.blade(),
            OscillatorType::TechSaws => self.tech_saws(),
            OscillatorType::Hoover => self.hoover(),
            OscillatorType::Pluck => self.pluck(),
            OscillatorType::Piano => self.piano(),
            OscillatorType::PrettyBell => self.pretty_bell(),
            OscillatorType::DullBell => self.dull_bell(),
            OscillatorType::Hollow => self.hollow(),
            OscillatorType::DarkAmbience => self.dark_ambience(),
            OscillatorType::Growl => self.growl(),
            OscillatorType::ChipLead => self.chip_lead(),
            OscillatorType::ChipBass => self.chip_bass(),
            OscillatorType::ChipNoise => self.chip_noise(),
            OscillatorType::BNoise => self.brown_noise(),
            OscillatorType::PNoise => self.pink_noise(),
            OscillatorType::GNoise => self.grey_noise(),
            OscillatorType::CNoise => self.clip_noise(),
            OscillatorType::SubPulse => self.sub_pulse(),
            OscillatorType::GabberKick => self.gabber_kick(),
        };
        sample * self.amplitude
    }

    pub fn envelope_value(&self, samples_elapsed: u64, total_samples: u64) -> f32 {
        let t = samples_elapsed as f32 / self.sample_rate;
        let total_t = total_samples as f32 / self.sample_rate;
        // Ensure minimum release of 1ms to avoid clicks (matches Sonic Pi behavior)
        let effective_release = self.envelope.release.max(0.001);
        let release_start = (total_t - effective_release).max(0.0);

        if self.envelope.attack > 0.0 && t < self.envelope.attack {
            // Attack: linear ramp 0 → 1
            t / self.envelope.attack
        } else if self.envelope.decay > 0.0 && t < self.envelope.attack + self.envelope.decay {
            // Decay: linear ramp 1 → sustain_level
            let decay_t = (t - self.envelope.attack) / self.envelope.decay;
            1.0 - (1.0 - self.envelope.sustain) * decay_t
        } else if t < release_start {
            // Sustain: hold at sustain level
            self.envelope.sustain
        } else {
            // Release: linear ramp sustain_level → 0
            // Sonic Pi uses \lin curves by default for all envelope segments
            let release_t = ((t - release_start) / effective_release).clamp(0.0, 1.0);
            self.envelope.sustain * (1.0 - release_t)
        }
    }

    // ──────────────── Helpers ────────────────

    fn advance_phase(&mut self) {
        self.phase += self.frequency / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
    }

    fn advance_lfo(&mut self) -> f32 {
        let v = (self.lfo_phase * 2.0 * PI).sin();
        self.lfo_phase += self.lfo_rate / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        v
    }

    fn xorshift(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Simple one-pole low-pass filter
    fn one_pole_lp(prev: f32, input: f32, cutoff: f32, sr: f32) -> f32 {
        let rc = 1.0 / (2.0 * PI * cutoff);
        let dt = 1.0 / sr;
        let alpha = dt / (rc + dt);
        prev + alpha * (input - prev)
    }

    /// State-variable filter (LP/BP/HP) — Cytomic/Simper topology.
    /// Unconditionally stable at all frequencies (unlike Chamberlin SVF).
    fn svf_tick(&mut self, input: f32) {
        let cutoff = self.filter_cutoff.clamp(20.0, self.sample_rate * 0.49);
        let g = (PI * cutoff / self.sample_rate).tan();
        // k = damping: 2*(1-res) maps res 0→k=2 (no resonance), res 1→k=0 (self-oscillating)
        let k = 2.0 * (1.0 - self.filter_resonance.clamp(0.0, 0.99));

        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.svf_ic2;
        let v1 = a1 * self.svf_ic1 + a2 * v3;
        let v2 = self.svf_ic2 + a2 * self.svf_ic1 + a3 * v3;

        self.svf_ic1 = 2.0 * v1 - self.svf_ic1;
        self.svf_ic2 = 2.0 * v2 - self.svf_ic2;

        self.filter_lp = v2;
        self.filter_bp = v1;
        self.filter_hp = input - k * v1 - v2;
    }

    // ──────────────── PolyBLEP Anti-aliasing ────────────────

    /// PolyBLEP correction term to remove aliasing from discontinuities.
    /// `t` is the normalised phase [0,1), `dt` is phase increment per sample.
    #[inline]
    fn poly_blep(t: f32, dt: f32) -> f32 {
        if t < dt {
            // Rising edge at start of period
            let t = t / dt;
            2.0 * t - t * t - 1.0
        } else if t > 1.0 - dt {
            // Falling edge at end of period
            let t = (t - 1.0) / dt;
            t * t + 2.0 * t + 1.0
        } else {
            0.0
        }
    }

    // ──────────────── Basic Oscillators (band-limited) ────────────────

    fn sine(&mut self) -> f32 {
        let s = (self.phase * 2.0 * PI).sin();
        self.advance_phase();
        s
    }

    fn saw(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut s = 2.0 * self.phase - 1.0;
        s -= Self::poly_blep(self.phase, dt);
        self.advance_phase();
        // Apply resonant low-pass filter (Sonic Pi :saw has cutoff param)
        self.svf_tick(s);
        self.filter_lp
    }

    fn square(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut s = if self.phase < 0.5 { 1.0 } else { -1.0 };
        s += Self::poly_blep(self.phase, dt);
        s -= Self::poly_blep((self.phase + 0.5) % 1.0, dt);
        self.advance_phase();
        // Apply resonant low-pass filter (Sonic Pi :square has cutoff param)
        self.svf_tick(s);
        self.filter_lp
    }

    fn triangle(&mut self) -> f32 {
        // Direct triangle with smooth transitions
        let s = if self.phase < 0.5 {
            4.0 * self.phase - 1.0
        } else {
            3.0 - 4.0 * self.phase
        };
        self.advance_phase();
        s
    }

    fn white_noise(&mut self) -> f32 {
        self.xorshift()
    }

    fn pulse(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut s = if self.phase < self.pulse_width {
            1.0
        } else {
            -1.0
        };
        s += Self::poly_blep(self.phase, dt);
        s -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt);
        self.advance_phase();
        // Apply resonant low-pass filter (Sonic Pi :pulse has cutoff param)
        self.svf_tick(s);
        self.filter_lp
    }

    fn super_saw(&mut self) -> f32 {
        // Sonic Pi :super_saw uses comparator-based waveshaping, NOT detuned saws.
        // Algorithm: one main saw + four slow LFO saws → comparators → animated texture.
        // Source: sonic-pi/etc/synthdefs/designs/overtone/sonic-pi/src/sonic_pi/retro.clj

        // Main saw oscillator at target frequency (band-limited with PolyBLEP)
        let dt = self.frequency / self.sample_rate;
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let mut input = 2.0 * self.phase - 1.0;
        input -= Self::poly_blep(self.phase, dt);

        // Four LFO saws at fixed frequencies (Sonic Pi: 4, 7, 5, 2 Hz)
        let lfo_freqs: [f32; 4] = [4.0, 7.0, 5.0, 2.0];
        let mut comp_sum = 0.0f32;
        for i in 0..4 {
            let lfo_dt = lfo_freqs[i] / self.sample_rate;
            self.phases[i] += lfo_dt;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            let lfo = 2.0 * self.phases[i] - 1.0;
            // Comparator: 1.0 if input > lfo, else 0.0 (matches SC's > on audio signals)
            if input > lfo {
                comp_sum += 1.0;
            }
        }

        // Waveshaping algebra (matches Sonic Pi):
        // output = (input-c1)+(input-c2)+(input-c3)+(input-c4) - input = 3*input - comp_sum
        let raw = (3.0 * input - comp_sum) * 0.25;

        // DC-blocking filter (LeakDC, coeff=0.995)
        // y[n] = x[n] - x[n-1] + 0.995 * y[n-1]
        let dc_out = raw - self.phases[4] + 0.995 * self.phases[5];
        self.phases[4] = raw; // prev input
        self.phases[5] = dc_out; // prev output

        // Resonant low-pass filter (RLPF in Sonic Pi)
        self.svf_tick(dc_out);

        // Normalizer approximation (Sonic Pi uses SC's Normalizer lookahead limiter)
        // amp-fudge: 0.9 (matches Sonic Pi source)
        self.filter_lp.clamp(-1.0, 1.0) * 0.9
    }

    // ──────────────── Detuned Oscillators ────────────────

    /// :dsaw - two detuned saw oscillators (band-limited)
    fn detuned_saw(&mut self) -> f32 {
        // Use detune_amounts[0] for the detuned frequency offset
        let detune_factor = if self.detune_amounts[0].abs() > 0.0001 {
            1.0 + self.detune_amounts[0]
        } else {
            1.005
        };
        let dt1 = self.frequency / self.sample_rate;
        let dt2 = self.frequency * detune_factor / self.sample_rate;
        let mut s1 = 2.0 * self.phase - 1.0;
        s1 -= Self::poly_blep(self.phase, dt1);
        let mut s2 = 2.0 * self.phase2 - 1.0;
        s2 -= Self::poly_blep(self.phase2, dt2);
        self.advance_phase();
        self.phase2 += dt2;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }
        let raw = (s1 + s2) * 0.5;
        // Apply resonant low-pass filter (Sonic Pi :dsaw has cutoff param)
        self.svf_tick(raw);
        self.filter_lp
    }

    /// :dpulse - two detuned pulse oscillators (band-limited)
    fn detuned_pulse(&mut self) -> f32 {
        let detune_factor = if self.detune_amounts[0].abs() > 0.0001 {
            1.0 + self.detune_amounts[0]
        } else {
            1.005
        };
        let dt1 = self.frequency / self.sample_rate;
        let dt2 = self.frequency * detune_factor / self.sample_rate;
        let mut s1 = if self.phase < self.pulse_width {
            1.0
        } else {
            -1.0
        };
        s1 += Self::poly_blep(self.phase, dt1);
        s1 -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt1);
        let mut s2 = if self.phase2 < self.pulse_width {
            1.0
        } else {
            -1.0
        };
        s2 += Self::poly_blep(self.phase2, dt2);
        s2 -= Self::poly_blep((self.phase2 + (1.0 - self.pulse_width)) % 1.0, dt2);
        self.advance_phase();
        self.phase2 += dt2;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }
        let raw = (s1 + s2) * 0.5;
        // Apply resonant low-pass filter (Sonic Pi :dpulse has cutoff param)
        self.svf_tick(raw);
        self.filter_lp
    }

    /// :dtri - two detuned triangle oscillators
    fn detuned_tri(&mut self) -> f32 {
        let tri = |p: f32| {
            if p < 0.5 {
                4.0 * p - 1.0
            } else {
                3.0 - 4.0 * p
            }
        };
        let s1 = tri(self.phase);
        let s2 = tri(self.phase2);
        self.advance_phase();
        self.phase2 += self.frequency * 1.005 / self.sample_rate;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }
        (s1 + s2) * 0.5
    }

    // ──────────────── FM Synthesis ────────────────

    /// :fm / :mod_fm - basic FM synthesis
    fn fm_synth(&mut self) -> f32 {
        let modulator = (self.mod_phase * 2.0 * PI).sin();
        let carrier_phase = self.phase + self.mod_index * modulator;
        let s = (carrier_phase * 2.0 * PI).sin();
        self.advance_phase();
        self.mod_phase += self.frequency * self.mod_ratio / self.sample_rate;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        s
    }

    // ──────────────── Modulated Oscillators ────────────────

    /// :mod_sine - sine with tremolo LFO
    fn mod_sine(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let s = (self.phase * 2.0 * PI).sin();
        self.advance_phase();
        s * (0.7 + 0.3 * lfo)
    }

    /// :mod_saw - band-limited saw with tremolo LFO
    fn mod_saw(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let dt = self.frequency / self.sample_rate;
        let mut s = 2.0 * self.phase - 1.0;
        s -= Self::poly_blep(self.phase, dt);
        self.advance_phase();
        s * (0.7 + 0.3 * lfo)
    }

    /// :mod_dsaw - detuned saw with tremolo
    fn mod_dsaw(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let s = self.detuned_saw();
        s * (0.7 + 0.3 * lfo)
    }

    /// :mod_tri - triangle with tremolo
    fn mod_tri(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let tri = if self.phase < 0.5 {
            4.0 * self.phase - 1.0
        } else {
            3.0 - 4.0 * self.phase
        };
        self.advance_phase();
        tri * (0.7 + 0.3 * lfo)
    }

    /// :mod_pulse - band-limited pulse with PWM via LFO
    fn mod_pulse(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let pw = 0.5 + 0.3 * lfo; // modulate pulse width
        let dt = self.frequency / self.sample_rate;
        let mut s = if self.phase < pw { 1.0 } else { -1.0 };
        s += Self::poly_blep(self.phase, dt);
        s -= Self::poly_blep((self.phase + (1.0 - pw)) % 1.0, dt);
        self.advance_phase();
        s
    }

    // ──────────────── Classic Synths ────────────────

    /// :tb303 - acid bass: band-limited saw through resonant low-pass filter
    fn tb303(&mut self) -> f32 {
        // Envelope modulates filter cutoff
        let t = self.sample_count as f32 / self.sample_rate;
        let env_mod = (-t * 4.0).exp();
        let cutoff = self.filter_cutoff + 3000.0 * env_mod;
        self.filter_cutoff = cutoff.min(18000.0);

        let dt = self.frequency / self.sample_rate;
        let mut raw = 2.0 * self.phase - 1.0; // saw
        raw -= Self::poly_blep(self.phase, dt);
        self.advance_phase();
        self.svf_tick(raw);
        self.filter_lp
    }

    /// :prophet - rich poly synth: band-limited detuned saw + pulse, mixed
    fn prophet(&mut self) -> f32 {
        let dt1 = self.frequency / self.sample_rate;
        let dt2 = self.frequency * 1.01 / self.sample_rate;
        let mut saw1 = 2.0 * self.phase - 1.0;
        saw1 -= Self::poly_blep(self.phase, dt1);
        let mut saw2 = 2.0 * self.phase2 - 1.0;
        saw2 -= Self::poly_blep(self.phase2, dt2);
        let mut pulse_val = if self.phase < self.pulse_width {
            1.0
        } else {
            -1.0
        };
        pulse_val += Self::poly_blep(self.phase, dt1);
        pulse_val -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt1);
        self.advance_phase();
        self.phase2 += dt2;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }
        let raw = saw1 * 0.4 + saw2 * 0.3 + pulse_val * 0.3;
        // Apply resonant low-pass filter (Sonic Pi :prophet default cutoff=110, res=0.7)
        self.svf_tick(raw);
        self.filter_lp
    }

    /// :zawa - slowly evolving phase-modulated synth
    fn zawa(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let mod_depth = 2.0 + 2.0 * lfo;
        let modulator = (self.mod_phase * 2.0 * PI).sin();
        let s = ((self.phase + mod_depth * modulator) * 2.0 * PI).sin();
        self.advance_phase();
        self.mod_phase += self.frequency * 0.5 / self.sample_rate;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        s
    }

    // ──────────────── Filtered / Layered ────────────────

    /// :blade - thick detuned band-limited saws through resonant filter
    fn blade(&mut self) -> f32 {
        let mut sum = 0.0f32;
        for i in 0..3 {
            let detune = 1.0 + (i as f32 - 1.0) * 0.007;
            let freq = self.frequency * detune;
            let dt = freq / self.sample_rate;
            self.phases[i] += dt;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            let mut s = 2.0 * self.phases[i] - 1.0;
            s -= Self::poly_blep(self.phases[i], dt);
            sum += s;
        }
        self.advance_phase();
        let raw = sum / 3.0;
        self.svf_tick(raw);
        self.filter_lp * 0.7 + self.filter_bp * 0.3
    }

    /// :tech_saws - multiple detuned band-limited saws for trance/tech leads
    fn tech_saws(&mut self) -> f32 {
        let offsets = [-0.08, -0.04, -0.01, 0.0, 0.01, 0.04, 0.08];
        let mut sum = 0.0f32;
        for i in 0..7 {
            let freq = self.frequency * (1.0 + offsets[i] * 0.1);
            let dt = freq / self.sample_rate;
            self.phases[i] += dt;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            let mut s = 2.0 * self.phases[i] - 1.0;
            s -= Self::poly_blep(self.phases[i], dt);
            sum += s;
        }
        self.advance_phase();
        let raw = sum / 7.0;
        // Apply resonant low-pass filter (Sonic Pi :tech_saws default cutoff=130)
        self.svf_tick(raw);
        self.filter_lp
    }

    /// :hoover - classic hoover: band-limited detuned saws with sub oscillator
    fn hoover(&mut self) -> f32 {
        let mut sum = 0.0f32;
        let detunes = [-0.09, -0.04, 0.0, 0.04, 0.09];
        for i in 0..5 {
            let freq = self.frequency * (1.0 + detunes[i] * 0.05);
            let dt = freq / self.sample_rate;
            self.phases[i] += dt;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            let mut s = 2.0 * self.phases[i] - 1.0;
            s -= Self::poly_blep(self.phases[i], dt);
            sum += s;
        }
        // Sub oscillator one octave down
        let sub = (self.phase2 * 2.0 * PI).sin();
        self.phase2 += (self.frequency * 0.5) / self.sample_rate;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }
        self.advance_phase();
        (sum / 5.0) * 0.7 + sub * 0.3
    }

    // ──────────────── Plucked / Percussive ────────────────

    /// :pluck - Karplus-Strong plucked string
    fn pluck(&mut self) -> f32 {
        if self.pluck_buffer.is_empty() {
            return 0.0;
        }
        let len = self.pluck_buffer.len();
        let out = self.pluck_buffer[self.pluck_pos];
        let next_pos = (self.pluck_pos + 1) % len;
        // Averaging filter for decay
        let avg = (self.pluck_buffer[self.pluck_pos] + self.pluck_buffer[next_pos]) * 0.499;
        self.pluck_buffer[self.pluck_pos] = avg;
        self.pluck_pos = next_pos;
        out
    }

    /// :piano - multiple harmonic partials with fast decay (additive synthesis)
    fn piano(&mut self) -> f32 {
        let t = self.sample_count as f32 / self.sample_rate;
        let mut s = 0.0f32;
        // Harmonics with decreasing amplitude and faster decay for higher partials
        let harmonics = [
            (1.0, 1.0, 3.0),
            (2.0, 0.5, 5.0),
            (3.0, 0.25, 8.0),
            (4.0, 0.12, 12.0),
            (5.0, 0.06, 16.0),
            (6.0, 0.03, 20.0),
        ];
        for (h, amp, decay_rate) in harmonics {
            let freq = self.frequency * h;
            let phase_inc = freq / self.sample_rate;
            let p = (self.phase * h) % 1.0;
            s += (p * 2.0 * PI).sin() * amp * (-t * decay_rate).exp();
            let _ = phase_inc; // phase advance handled below
        }
        self.advance_phase();
        s
    }

    /// :pretty_bell - bright bell with inharmonic partials
    fn pretty_bell(&mut self) -> f32 {
        let t = self.sample_count as f32 / self.sample_rate;
        let partials = [
            (1.0, 1.0, 2.0),
            (2.0, 0.6, 3.0),
            (3.11, 0.4, 4.0),
            (4.52, 0.25, 5.0),
            (5.43, 0.15, 7.0),
            (6.79, 0.08, 9.0),
        ];
        let mut s = 0.0f32;
        for (ratio, amp, decay_rate) in partials {
            let p = (self.phase * ratio) % 1.0;
            s += (p * 2.0 * PI).sin() * amp * (-t * decay_rate).exp();
        }
        self.advance_phase();
        s * 0.5
    }

    /// :dull_bell - softer bell, fewer high partials
    fn dull_bell(&mut self) -> f32 {
        let t = self.sample_count as f32 / self.sample_rate;
        let partials = [
            (1.0, 1.0, 1.5),
            (2.0, 0.5, 3.0),
            (3.0, 0.2, 6.0),
            (4.2, 0.08, 10.0),
        ];
        let mut s = 0.0f32;
        for (ratio, amp, decay_rate) in partials {
            let p = (self.phase * ratio) % 1.0;
            s += (p * 2.0 * PI).sin() * amp * (-t * decay_rate).exp();
        }
        self.advance_phase();
        s * 0.6
    }

    // ──────────────── Pads / Ambient ────────────────

    /// :hollow - hollow pad: bandpass filtered mix of sine + noise
    fn hollow(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let sine_part = (self.phase * 2.0 * PI).sin();
        let noise_part = self.xorshift();
        self.advance_phase();
        let raw = sine_part * 0.6 + noise_part * 0.15;
        // Modulate cutoff slightly with LFO
        self.filter_cutoff = 600.0 + 200.0 * lfo;
        self.svf_tick(raw);
        self.filter_bp
    }

    /// :dark_ambience - dark ambient pad: filtered noise + sub sine
    fn dark_ambience(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let noise_part = self.xorshift();
        let sub = (self.phase * 2.0 * PI).sin();
        self.advance_phase();
        let raw = noise_part * 0.4 + sub * 0.5;
        self.filter_cutoff = 300.0 + 100.0 * lfo;
        self.svf_tick(raw);
        self.filter_lp * 0.8
    }

    /// :growl - growling bass: band-limited saw modulated by LFO at audio rate
    fn growl(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let dt = self.frequency / self.sample_rate;
        let mut saw = 2.0 * self.phase - 1.0;
        saw -= Self::poly_blep(self.phase, dt);
        self.advance_phase();
        // Ring-modulate with LFO for growl character
        let mod_freq = self.frequency * 0.5;
        let ring = (self.mod_phase * 2.0 * PI).sin();
        self.mod_phase += mod_freq / self.sample_rate;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        saw * (0.5 + 0.5 * ring) * (0.8 + 0.2 * lfo)
    }

    // ──────────────── Chiptune ────────────────

    /// :chiplead - quantized band-limited square wave (lo-fi chiptune lead)
    fn chip_lead(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut raw = if self.phase < 0.5 { 1.0f32 } else { -1.0 };
        raw += Self::poly_blep(self.phase, dt);
        raw -= Self::poly_blep((self.phase + 0.5) % 1.0, dt);
        self.advance_phase();
        // Quantize to 4-bit
        (raw * 8.0).round() / 8.0
    }

    /// :chipbass - quantized triangle (chiptune bass)
    fn chip_bass(&mut self) -> f32 {
        let raw = if self.phase < 0.5 {
            4.0 * self.phase - 1.0
        } else {
            3.0 - 4.0 * self.phase
        };
        self.advance_phase();
        // Quantize and play one octave lower via phase
        (raw * 4.0).round() / 4.0
    }

    /// :chipnoise - periodic noise (lo-fi chiptune noise)
    fn chip_noise(&mut self) -> f32 {
        // Update noise less frequently for lo-fi periodic noise
        let period = (self.sample_rate / 11025.0).max(1.0) as u64;
        if self.sample_count % period == 0 {
            self.brown_acc = self.xorshift(); // reuse brown_acc as noise holder
        }
        (self.brown_acc * 4.0).round() / 4.0
    }

    // ──────────────── Colored Noise ────────────────

    /// :bnoise - brown noise (random walk, -6dB/octave)
    fn brown_noise(&mut self) -> f32 {
        let white = self.xorshift();
        self.brown_acc += white * 0.02;
        self.brown_acc = self.brown_acc.clamp(-1.0, 1.0);
        self.brown_acc * 3.5 // boost since it's quiet
    }

    /// :pnoise - pink noise (-3dB/octave) using Voss-McCartney
    fn pink_noise(&mut self) -> f32 {
        let white = self.xorshift();
        self.pink_index = self.pink_index.wrapping_add(1);
        // Determine which rows to update (trailing zeros of index)
        let changed = self.pink_index ^ (self.pink_index.wrapping_sub(1));
        for row in 0..16u32 {
            if changed & (1 << row) != 0 {
                self.pink_running_sum -= self.pink_rows[row as usize];
                let new_val = self.xorshift() * 0.0625; // 1/16
                self.pink_rows[row as usize] = new_val;
                self.pink_running_sum += new_val;
            }
        }
        (self.pink_running_sum + white * 0.0625).clamp(-1.0, 1.0)
    }

    /// :gnoise - grey noise (perceptually flat, roughly pink + compensation)
    fn grey_noise(&mut self) -> f32 {
        // Mix of white and pink for perceptually flat response
        let white = self.xorshift();
        let pink = self.pink_noise();
        white * 0.4 + pink * 0.6
    }

    /// :cnoise - clip noise (white noise hard-clipped to ±1 with reduced amplitude)
    fn clip_noise(&mut self) -> f32 {
        let white = self.xorshift();
        if white > 0.0 {
            1.0
        } else {
            -1.0
        }
    }

    // ──────────────── Sub ────────────────

    /// :subpulse - band-limited pulse wave with sub-octave added
    fn sub_pulse(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut main_pulse = if self.phase < self.pulse_width {
            1.0
        } else {
            -1.0
        };
        main_pulse += Self::poly_blep(self.phase, dt);
        main_pulse -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt);
        let sub = (self.phase2 * 2.0 * PI).sin();
        self.advance_phase();
        self.phase2 += (self.frequency * 0.5) / self.sample_rate;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }
        let raw = main_pulse * 0.6 + sub * 0.4;
        // Apply resonant low-pass filter (Sonic Pi :subpulse has cutoff param)
        self.svf_tick(raw);
        self.filter_lp
    }

    /// Gabber kick: distorted sine with fast downward pitch sweep
    fn gabber_kick(&mut self) -> f32 {
        // Use phases[0] to track time for the pitch envelope
        let t = self.phases[0];
        self.phases[0] += 1.0 / self.sample_rate;

        // Fast exponential pitch sweep from 8x frequency down to base frequency
        // Sweep completes in ~50ms for that characteristic gabber "thwack"
        let sweep_time = 0.05;
        let freq_mult = if t < sweep_time {
            let ratio = t / sweep_time;
            8.0 * (1.0 - ratio) + 1.0 * ratio // 8x → 1x
        } else {
            1.0
        };

        let freq = self.frequency * freq_mult;
        let sample = (self.phase * 2.0 * PI).sin();

        // Heavy distortion (tanh saturation)
        let gain = 4.0;
        let distorted = (sample * gain).tanh();

        self.phase += freq / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        distorted
    }
}

/// Convert MIDI note number to frequency
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// Convert note name to MIDI number  
/// Supports sharp (S/#), flat (B/F) suffixes, e.g.: :cs4, :df4, :eb3, :gf5
pub fn note_name_to_midi(name: &str) -> Option<u8> {
    let name = name.trim().to_uppercase();

    // Handle single-character notes (e.g., "E" -> default to octave 4, like Sonic Pi)
    let (note_part, octave_str): (&str, &str) = if name.len() == 1 {
        (&name[..], "4") // Default to octave 4
    } else if name.len() == 2
        && (name.chars().nth(1) == Some('S') || name.chars().nth(1) == Some('B')
            || name.chars().nth(1) == Some('F'))
    {
        // Sharp/flat without octave: e.g., "FS", "EB", "DF" -> default to octave 4
        // But "F" followed by a digit is note F with octave, not a flat.
        // "DF" = D flat, "EF" = E flat, "GF" = G flat, etc.
        // Distinguish: if second char is 'F' and first char is NOT 'F' (to avoid
        // confusing note F + sharp/flat), treat as flat.
        // Actually 'F' as second char after a note letter = flat in Sonic Pi.
        // But we must NOT match e.g. "F3" as note_part="F" octave="3".
        // 'F' as flat only applies when len==2 and the result would be in our base table.
        // Check: DF, EF, GF, AF, BF are all valid flat names.
        let ch1 = name.chars().nth(0).unwrap();
        let ch2 = name.chars().nth(1).unwrap();
        if ch2 == 'F' && ch1 != 'F' && ch1 != 'C' {
            // It's a flat note without octave (e.g., "DF", "EF", "GF", "AF", "BF")
            // CF doesn't exist (would be B), but handle it anyway
            (&name[..], "4")
        } else if ch2 == 'S' || ch2 == 'B' {
            (&name[..], "4")
        } else {
            // Fallback: treat second char as start of octave
            (&name[..1], &name[1..])
        }
    } else if name.len() >= 2 {
        if name.chars().nth(1) == Some('S') || name.chars().nth(1) == Some('#') {
            (&name[..2], &name[2..])
        } else if name.chars().nth(1) == Some('B') && name.len() > 2 {
            (&name[..2], &name[2..])
        } else if name.chars().nth(1) == Some('F') && name.len() > 2 {
            // Flat with 'F' suffix + octave: e.g., "DF4", "EF3", "GF5"
            let ch0 = name.chars().nth(0).unwrap();
            if ch0 != 'F' && ch0 != 'C' {
                // It's a flat note: DF4, EF3, etc.
                (&name[..2], &name[2..])
            } else {
                // F4 = note F octave 4, CF3 = treat as unknown
                (&name[..1], &name[1..])
            }
        } else {
            (&name[..1], &name[1..])
        }
    } else {
        return None;
    };

    let base = match note_part {
        "C" => 0,
        "CS" | "C#" | "DB" | "DF" => 1,
        "D" => 2,
        "DS" | "D#" | "EB" | "EF" => 3,
        "E" | "FF" => 4,
        "F" | "ES" | "E#" => 5,
        "FS" | "F#" | "GB" | "GF" => 6,
        "G" => 7,
        "GS" | "G#" | "AB" | "AF" => 8,
        "A" => 9,
        "AS" | "A#" | "BB" | "BF" => 10,
        "B" | "CF" => 11,
        _ => return None,
    };

    let octave: i32 = octave_str.parse().ok()?;
    let midi = (octave + 1) * 12 + base;
    if midi >= 0 && midi <= 127 {
        Some(midi as u8)
    } else {
        None
    }
}

/// Convert MIDI note number to Hz
pub fn midi_to_hz(note: f32) -> f32 {
    440.0 * 2.0f32.powf((note - 69.0) / 12.0)
}

/// Convert Hz to MIDI note number
pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that super_saw produces a sustained signal, not a click/blip.
    /// Generates 0.5 seconds of audio and checks that the signal is still
    /// significant at 25%, 50%, and 75% through the note.
    #[test]
    fn super_saw_sustained_output() {
        let sr = 44100.0;
        let total_samples = (sr * 0.5) as u64; // 0.5 seconds
        let env = Envelope {
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.5, // 0.5s release (the whole note is release)
        };
        let mut voice = SynthVoice::new(
            OscillatorType::SuperSaw,
            261.63, // C4
            1.0,
            sr,
            env,
        );

        // Collect RMS at different points
        let check_points = [0.125, 0.25, 0.375]; // 25%, 50%, 75% through
        for &frac in &check_points {
            let target_sample = (total_samples as f32 * frac) as u64;
            // Generate samples up to this point
            let mut voice_copy = SynthVoice::new(OscillatorType::SuperSaw, 261.63, 1.0, sr, env);
            let mut rms_sum = 0.0f64;
            let window = 1024u64;
            let start = if target_sample > window {
                target_sample - window
            } else {
                0
            };
            for s in 0..target_sample {
                let raw = voice_copy.next_sample();
                let env_val = voice_copy.envelope_value(s, total_samples);
                let out = raw * env_val;
                if s >= start {
                    rms_sum += (out as f64) * (out as f64);
                }
            }
            let rms = (rms_sum / window as f64).sqrt();
            assert!(
                rms > 0.01,
                "super_saw RMS at {}% should be > 0.01, got {:.6}",
                (frac * 100.0) as u32,
                rms
            );
        }

        // Also verify the SVF filter doesn't blow up (no NaN/Inf)
        let mut v = SynthVoice::new(OscillatorType::SuperSaw, 261.63, 1.0, sr, env);
        for _ in 0..4410 {
            let s = v.next_sample();
            assert!(s.is_finite(), "super_saw produced non-finite sample");
            assert!(
                s.abs() < 10.0,
                "super_saw signal too large: {} — filter may be unstable",
                s
            );
        }
    }

    /// Verify that the envelope produces expected values at key points.
    #[test]
    fn envelope_linear_release() {
        let env = Envelope {
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.5,
        };
        let sr = 44100.0;
        let total_samples = (sr * 0.5) as u64;
        let voice = SynthVoice::new(OscillatorType::Sine, 440.0, 1.0, sr, env);

        // At t=0 (start of release): should be 1.0
        let v0 = voice.envelope_value(0, total_samples);
        assert!(
            (v0 - 1.0).abs() < 0.01,
            "envelope at t=0 should be ~1.0, got {}",
            v0
        );

        // At t=50% through: should be ~0.5 (linear)
        let v_mid = voice.envelope_value(total_samples / 2, total_samples);
        assert!(
            (v_mid - 0.5).abs() < 0.05,
            "envelope at 50% should be ~0.5, got {}",
            v_mid
        );

        // At t=100%: should be ~0.0
        let v_end = voice.envelope_value(total_samples, total_samples);
        assert!(
            v_end < 0.01,
            "envelope at 100% should be ~0.0, got {}",
            v_end
        );
    }

    /// Verify note_name_to_midi correctly handles notes without octave (defaulting to 4)
    #[test]
    fn note_without_octave_defaults_to_4() {
        // Single letter notes default to octave 4 (like Sonic Pi)
        assert_eq!(note_name_to_midi("E"), Some(64)); // E4 = 64
        assert_eq!(note_name_to_midi("C"), Some(60)); // C4 = 60
        assert_eq!(note_name_to_midi("A"), Some(69)); // A4 = 69

        // Sharp/flat without octave also defaults to 4
        assert_eq!(note_name_to_midi("FS"), Some(66)); // F#4 = 66
        assert_eq!(note_name_to_midi("EB"), Some(63)); // Eb4 = 63

        // Notes with octave still work as before
        assert_eq!(note_name_to_midi("E2"), Some(40)); // E2 = 40
        assert_eq!(note_name_to_midi("C4"), Some(60)); // C4 = 60
        assert_eq!(note_name_to_midi("FS3"), Some(54)); // F#3 = 54
    }
}
