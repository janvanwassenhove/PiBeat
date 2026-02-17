use std::f32::consts::PI;

/// All Sonic Pi synth types
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
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
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct Envelope {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.3,
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
    pub fn new(
        osc_type: OscillatorType,
        frequency: f32,
        amplitude: f32,
        sample_rate: f32,
        envelope: Envelope,
    ) -> Self {
        let detune_amounts = [-0.11, -0.06, -0.02, 0.0, 0.02, 0.06, 0.11];

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

        // Determine FM parameters based on synth type
        let (mod_index, mod_ratio) = match osc_type {
            OscillatorType::FM => (5.0, 1.0),
            OscillatorType::ModFM => (8.0, 2.0),
            _ => (1.0, 1.0),
        };

        let lfo_rate = match osc_type {
            OscillatorType::ModSine | OscillatorType::ModSaw | OscillatorType::ModTri
            | OscillatorType::ModPulse | OscillatorType::ModDSaw => 6.0,
            OscillatorType::Zawa => 1.0,
            OscillatorType::Growl => 8.0,
            _ => 5.0,
        };

        // Filter cutoff varies by synth type
        let (filter_cutoff, filter_resonance) = match osc_type {
            OscillatorType::TB303 => (800.0, 0.8),
            OscillatorType::Blade => (2000.0, 0.3),
            OscillatorType::Hollow => (600.0, 0.9),
            OscillatorType::DarkAmbience => (300.0, 0.5),
            _ => (5000.0, 0.0),
        };

        let pulse_width = match osc_type {
            OscillatorType::SubPulse => 0.5,
            OscillatorType::Prophet => 0.3,
            _ => 0.5,
        };

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
        };
        sample * self.amplitude
    }

    pub fn envelope_value(&self, samples_elapsed: u64, total_samples: u64) -> f32 {
        let t = samples_elapsed as f32 / self.sample_rate;
        let total_t = total_samples as f32 / self.sample_rate;
        let release_start = total_t - self.envelope.release;

        if t < self.envelope.attack {
            t / self.envelope.attack
        } else if t < self.envelope.attack + self.envelope.decay {
            let decay_t = (t - self.envelope.attack) / self.envelope.decay;
            1.0 - (1.0 - self.envelope.sustain) * decay_t
        } else if t < release_start {
            self.envelope.sustain
        } else {
            let release_t = (t - release_start) / self.envelope.release;
            self.envelope.sustain * (1.0 - release_t).max(0.0)
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

    /// State-variable filter (LP/BP/HP) - updates internal state
    fn svf_tick(&mut self, input: f32) {
        let f = 2.0 * (PI * self.filter_cutoff / self.sample_rate).sin();
        let q = 1.0 - self.filter_resonance.min(0.99);
        self.filter_lp += f * self.filter_bp;
        self.filter_hp = input - self.filter_lp - q * self.filter_bp;
        self.filter_bp += f * self.filter_hp;
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
        s
    }

    fn square(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut s = if self.phase < 0.5 { 1.0 } else { -1.0 };
        s += Self::poly_blep(self.phase, dt);
        s -= Self::poly_blep((self.phase + 0.5) % 1.0, dt);
        self.advance_phase();
        s
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
        let mut s = if self.phase < self.pulse_width { 1.0 } else { -1.0 };
        s += Self::poly_blep(self.phase, dt);
        s -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt);
        self.advance_phase();
        s
    }

    fn super_saw(&mut self) -> f32 {
        let mut sum = 0.0f32;
        for i in 0..7 {
            let freq = self.frequency * (1.0 + self.detune_amounts[i] * 0.01);
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
        sum / 7.0
    }

    // ──────────────── Detuned Oscillators ────────────────

    /// :dsaw - two detuned saw oscillators (band-limited)
    fn detuned_saw(&mut self) -> f32 {
        let dt1 = self.frequency / self.sample_rate;
        let dt2 = self.frequency * 1.005 / self.sample_rate;
        let mut s1 = 2.0 * self.phase - 1.0;
        s1 -= Self::poly_blep(self.phase, dt1);
        let mut s2 = 2.0 * self.phase2 - 1.0;
        s2 -= Self::poly_blep(self.phase2, dt2);
        self.advance_phase();
        self.phase2 += dt2;
        if self.phase2 >= 1.0 { self.phase2 -= 1.0; }
        (s1 + s2) * 0.5
    }

    /// :dpulse - two detuned pulse oscillators (band-limited)
    fn detuned_pulse(&mut self) -> f32 {
        let dt1 = self.frequency / self.sample_rate;
        let dt2 = self.frequency * 1.005 / self.sample_rate;
        let mut s1 = if self.phase < self.pulse_width { 1.0 } else { -1.0 };
        s1 += Self::poly_blep(self.phase, dt1);
        s1 -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt1);
        let mut s2 = if self.phase2 < self.pulse_width { 1.0 } else { -1.0 };
        s2 += Self::poly_blep(self.phase2, dt2);
        s2 -= Self::poly_blep((self.phase2 + (1.0 - self.pulse_width)) % 1.0, dt2);
        self.advance_phase();
        self.phase2 += dt2;
        if self.phase2 >= 1.0 { self.phase2 -= 1.0; }
        (s1 + s2) * 0.5
    }

    /// :dtri - two detuned triangle oscillators
    fn detuned_tri(&mut self) -> f32 {
        let tri = |p: f32| if p < 0.5 { 4.0 * p - 1.0 } else { 3.0 - 4.0 * p };
        let s1 = tri(self.phase);
        let s2 = tri(self.phase2);
        self.advance_phase();
        self.phase2 += self.frequency * 1.005 / self.sample_rate;
        if self.phase2 >= 1.0 { self.phase2 -= 1.0; }
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
        if self.mod_phase >= 1.0 { self.mod_phase -= 1.0; }
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
        let tri = if self.phase < 0.5 { 4.0 * self.phase - 1.0 } else { 3.0 - 4.0 * self.phase };
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
        let mut pulse_val = if self.phase < self.pulse_width { 1.0 } else { -1.0 };
        pulse_val += Self::poly_blep(self.phase, dt1);
        pulse_val -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt1);
        self.advance_phase();
        self.phase2 += dt2;
        if self.phase2 >= 1.0 { self.phase2 -= 1.0; }
        saw1 * 0.4 + saw2 * 0.3 + pulse_val * 0.3
    }

    /// :zawa - slowly evolving phase-modulated synth
    fn zawa(&mut self) -> f32 {
        let lfo = self.advance_lfo();
        let mod_depth = 2.0 + 2.0 * lfo;
        let modulator = (self.mod_phase * 2.0 * PI).sin();
        let s = ((self.phase + mod_depth * modulator) * 2.0 * PI).sin();
        self.advance_phase();
        self.mod_phase += self.frequency * 0.5 / self.sample_rate;
        if self.mod_phase >= 1.0 { self.mod_phase -= 1.0; }
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
            if self.phases[i] >= 1.0 { self.phases[i] -= 1.0; }
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
            if self.phases[i] >= 1.0 { self.phases[i] -= 1.0; }
            let mut s = 2.0 * self.phases[i] - 1.0;
            s -= Self::poly_blep(self.phases[i], dt);
            sum += s;
        }
        self.advance_phase();
        sum / 7.0
    }

    /// :hoover - classic hoover: band-limited detuned saws with sub oscillator
    fn hoover(&mut self) -> f32 {
        let mut sum = 0.0f32;
        let detunes = [-0.09, -0.04, 0.0, 0.04, 0.09];
        for i in 0..5 {
            let freq = self.frequency * (1.0 + detunes[i] * 0.05);
            let dt = freq / self.sample_rate;
            self.phases[i] += dt;
            if self.phases[i] >= 1.0 { self.phases[i] -= 1.0; }
            let mut s = 2.0 * self.phases[i] - 1.0;
            s -= Self::poly_blep(self.phases[i], dt);
            sum += s;
        }
        // Sub oscillator one octave down
        let sub = (self.phase2 * 2.0 * PI).sin();
        self.phase2 += (self.frequency * 0.5) / self.sample_rate;
        if self.phase2 >= 1.0 { self.phase2 -= 1.0; }
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
        if self.mod_phase >= 1.0 { self.mod_phase -= 1.0; }
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
        if white > 0.0 { 1.0 } else { -1.0 }
    }

    // ──────────────── Sub ────────────────

    /// :subpulse - band-limited pulse wave with sub-octave added
    fn sub_pulse(&mut self) -> f32 {
        let dt = self.frequency / self.sample_rate;
        let mut main_pulse = if self.phase < self.pulse_width { 1.0 } else { -1.0 };
        main_pulse += Self::poly_blep(self.phase, dt);
        main_pulse -= Self::poly_blep((self.phase + (1.0 - self.pulse_width)) % 1.0, dt);
        let sub = (self.phase2 * 2.0 * PI).sin();
        self.advance_phase();
        self.phase2 += (self.frequency * 0.5) / self.sample_rate;
        if self.phase2 >= 1.0 { self.phase2 -= 1.0; }
        main_pulse * 0.6 + sub * 0.4
    }
}

/// Convert MIDI note number to frequency
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// Convert note name to MIDI number  
pub fn note_name_to_midi(name: &str) -> Option<u8> {
    let name = name.trim().to_uppercase();
    let (note_part, octave_str) = if name.len() >= 2 {
        if name.chars().nth(1) == Some('S') || name.chars().nth(1) == Some('#') {
            (&name[..2], &name[2..])
        } else if name.chars().nth(1) == Some('B') && name.len() > 2 {
            (&name[..2], &name[2..])
        } else {
            (&name[..1], &name[1..])
        }
    } else {
        return None;
    };

    let base = match note_part {
        "C" => 0,
        "CS" | "C#" | "DB" => 1,
        "D" => 2,
        "DS" | "D#" | "EB" => 3,
        "E" => 4,
        "F" => 5,
        "FS" | "F#" | "GB" => 6,
        "G" => 7,
        "GS" | "G#" | "AB" => 8,
        "A" => 9,
        "AS" | "A#" | "BB" => 10,
        "B" => 11,
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
