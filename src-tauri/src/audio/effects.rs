use std::f32::consts::PI;

// ────────────────── Biquad Filter (12 dB/octave) ──────────────────

/// Second-order biquad filter – much higher quality than one-pole.
/// Supports low-pass, high-pass, band-pass, notch, peaking, etc.
#[derive(Clone)]
pub(crate) struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadFilter {
    /// Create a low-pass biquad at the given cutoff frequency with Q = 0.707 (Butterworth).
    fn low_pass(cutoff: f32, sample_rate: f32) -> Self {
        Self::low_pass_q(cutoff, sample_rate, 0.7071)
    }

    /// Create a resonant low-pass biquad with user-specified Q.
    /// Q = 0.7071 is Butterworth (flat), higher Q = resonance peak.
    /// Sonic Pi's `res:` maps 0..1 where 0=no resonance, 1=max resonance.
    fn low_pass_q(cutoff: f32, sample_rate: f32, q: f32) -> Self {
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.49);
        let omega = 2.0 * PI * cutoff / sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos_w) / 2.0) / a0,
            b1: (1.0 - cos_w) / a0,
            b2: ((1.0 - cos_w) / 2.0) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Create a high-pass biquad at the given cutoff frequency with Q = 0.707.
    fn high_pass(cutoff: f32, sample_rate: f32) -> Self {
        Self::high_pass_q(cutoff, sample_rate, 0.7071)
    }

    /// Create a resonant high-pass biquad with user-specified Q.
    fn high_pass_q(cutoff: f32, sample_rate: f32, q: f32) -> Self {
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.49);
        let omega = 2.0 * PI * cutoff / sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w) / 2.0) / a0,
            b1: (-(1.0 + cos_w)) / a0,
            b2: ((1.0 + cos_w) / 2.0) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn set_low_pass(&mut self, cutoff: f32, sample_rate: f32) {
        *self = Self::low_pass(cutoff, sample_rate);
    }

    fn set_low_pass_q(&mut self, cutoff: f32, sample_rate: f32, q: f32) {
        *self = Self::low_pass_q(cutoff, sample_rate, q);
    }

    /// Create a band-pass biquad at the given center frequency with Q.
    /// Sonic Pi's `:bpf` and `:rbpf` use MIDI-note cutoff with resonance.
    fn band_pass(center: f32, sample_rate: f32, q: f32) -> Self {
        let center = center.clamp(20.0, sample_rate * 0.49);
        let omega = 2.0 * PI * center / sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Create a peaking EQ biquad for `:band_eq`.
    fn peaking_eq(center: f32, sample_rate: f32, q: f32, db_gain: f32) -> Self {
        let center = center.clamp(20.0, sample_rate * 0.49);
        let a = 10.0f32.powf(db_gain / 40.0);
        let omega = 2.0 * PI * center / sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos_w) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha / a) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    #[allow(dead_code)]
    fn set_high_pass(&mut self, cutoff: f32, sample_rate: f32) {
        *self = Self::high_pass(cutoff, sample_rate);
    }

    fn set_high_pass_q(&mut self, cutoff: f32, sample_rate: f32, q: f32) {
        *self = Self::high_pass_q(cutoff, sample_rate, q);
    }

    fn process(&mut self, input: f32) -> f32 {
        let y = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Simple delay line
struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples: usize,
    feedback: f32,
}

impl DelayLine {
    fn new(max_delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay_samples],
            write_pos: 0,
            delay_samples: max_delay_samples / 2,
            feedback: 0.3,
        }
    }

    fn set_delay(&mut self, delay_secs: f32, sample_rate: f32) {
        self.delay_samples = (delay_secs * sample_rate) as usize;
        if self.delay_samples >= self.buffer.len() {
            self.delay_samples = self.buffer.len() - 1;
        }
    }

    fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.95);
    }

    fn process(&mut self, input: f32) -> f32 {
        let read_pos = if self.write_pos >= self.delay_samples {
            self.write_pos - self.delay_samples
        } else {
            self.buffer.len() - (self.delay_samples - self.write_pos)
        };

        let delayed = self.buffer[read_pos];
        self.buffer[self.write_pos] = input + delayed * self.feedback;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        delayed
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        for s in self.buffer.iter_mut() {
            *s = 0.0;
        }
    }
}

/// Schroeder reverb using comb and allpass filters (improved with more taps)
struct SchroederReverb {
    comb_filters: Vec<CombFilter>,
    allpass_filters: Vec<AllpassFilter>,
    mix: f32,
    damping_lp: f32,
    damp_coeff: f32,  // user-controlled damping coefficient
}

struct CombFilter {
    buffer: Vec<f32>,
    write_pos: usize,
    feedback: f32,
}

impl CombFilter {
    fn new(delay_samples: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; delay_samples],
            write_pos: 0,
            feedback,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.write_pos];
        // Apply damping LPF to the feedback path
        let damped = output * self.feedback;
        self.buffer[self.write_pos] = input + damped;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        output
    }
}

struct AllpassFilter {
    buffer: Vec<f32>,
    write_pos: usize,
    feedback: f32,
}

impl AllpassFilter {
    fn new(delay_samples: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; delay_samples],
            write_pos: 0,
            feedback,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.write_pos];
        let output = -input + delayed;
        self.buffer[self.write_pos] = input + delayed * self.feedback;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        output
    }
}

impl SchroederReverb {
    fn new(sample_rate: f32) -> Self {
        let sr = sample_rate as usize;
        // Use 8 comb filters with prime-ish delay lengths for richer reverb
        let comb_delays = [
            sr * 29 / 1000, // 29ms
            sr * 31 / 1000, // 31ms
            sr * 37 / 1000, // 37ms
            sr * 41 / 1000, // 41ms
            sr * 43 / 1000, // 43ms
            sr * 47 / 1000, // 47ms
            sr * 53 / 1000, // 53ms
            sr * 59 / 1000, // 59ms
        ];
        let comb_filters: Vec<CombFilter> = comb_delays
            .iter()
            .map(|&d| CombFilter::new(d.max(1), 0.84))
            .collect();

        let allpass_delays = [
            sr * 5 / 1000, // 5ms
            sr * 2 / 1000, // 2ms
            sr * 1 / 1000, // 1ms
        ];
        let allpass_filters: Vec<AllpassFilter> = allpass_delays
            .iter()
            .map(|&d| AllpassFilter::new(d.max(1), 0.7))
            .collect();

        Self {
            comb_filters,
            allpass_filters,
            mix: 0.2,
            damping_lp: 0.0,
            damp_coeff: 0.5,
        }
    }

    fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Set room size (0.0–1.0). Higher values = longer reverb tail.
    /// Maps to comb filter feedback: 0.0 → 0.6 (small room), 1.0 → 0.95 (large hall).
    fn set_room(&mut self, room: f32) {
        let room = room.clamp(0.0, 1.0);
        let feedback = 0.6 + room * 0.35; // Range: 0.6 – 0.95
        for comb in self.comb_filters.iter_mut() {
            comb.feedback = feedback;
        }
    }

    /// Set damping (0.0–1.0). Higher = more high-frequency absorption in reverb tail.
    /// Matches Sonic Pi's `damp:` parameter on `:reverb`.
    fn set_damp(&mut self, damp: f32) {
        self.damp_coeff = damp.clamp(0.0, 1.0);
    }

    fn process(&mut self, input: f32) -> f32 {
        // Sum of comb filter outputs
        let mut comb_sum = 0.0f32;
        for comb in self.comb_filters.iter_mut() {
            comb_sum += comb.process(input);
        }
        comb_sum /= self.comb_filters.len() as f32;

        // Apply damping low-pass to reverb tail (use user's damp coefficient)
        self.damping_lp = self.damping_lp * self.damp_coeff + comb_sum * (1.0 - self.damp_coeff);

        // Series allpass filters
        let mut output = self.damping_lp;
        for allpass in self.allpass_filters.iter_mut() {
            output = allpass.process(output);
        }

        input * (1.0 - self.mix) + output * self.mix
    }
}

/// Full effect chain
pub struct EffectChain {
    reverb_l: SchroederReverb,
    reverb_r: SchroederReverb,
    delay_l: DelayLine,
    delay_r: DelayLine,
    lpf_l: BiquadFilter,
    lpf_r: BiquadFilter,
    hpf_l: BiquadFilter,
    hpf_r: BiquadFilter,
    distortion_amount: f32,
    delay_mix: f32,
    sample_rate: f32,
    lpf_active: bool,
    hpf_active: bool,
    // Slicer (amplitude gating LFO)
    slicer_phase: f32, // LFO period in seconds
    slicer_mix: f32,   // wet/dry (0 = bypass)
    slicer_wave: i32,  // 0 = square, 1 = saw down, 2 = saw up, 3 = triangle
    slicer_pos: f32,   // current LFO phase 0..1
    // Bitcrusher
    bitcrusher_bits: f32,        // bit depth (1-16, 16 = transparent)
    bitcrusher_sample_rate: f32, // target sample rate
    bitcrusher_mix: f32,         // wet/dry
    bitcrusher_hold_l: f32,      // sample-and-hold state
    bitcrusher_hold_r: f32,
    bitcrusher_hold_counter: f32,
    // Compressor
    compressor_threshold: f32,
    compressor_clamp_time: f32,
    compressor_relax_time: f32,
    compressor_mix: f32,
    compressor_env: f32, // envelope follower state
    // Normaliser
    normaliser_level: f32,
    normaliser_active: bool,
    // Flanger (modulated short delay)
    flanger_rate: f32,         // LFO rate in Hz
    flanger_depth: f32,        // modulation depth 0-1
    flanger_feedback: f32,     // feedback amount
    flanger_mix: f32,          // wet/dry
    flanger_phase: f32,        // current LFO phase
    flanger_delay_l: Vec<f32>, // delay buffer left
    flanger_delay_r: Vec<f32>, // delay buffer right
    flanger_write_pos: usize,
    // Chorus (multi-voice detuned delays)
    chorus_rate: f32,        // LFO rate in Hz
    chorus_depth: f32,       // modulation depth
    chorus_mix: f32,         // wet/dry
    chorus_phases: [f32; 3], // LFO phases for 3 voices
    chorus_delay_l: Vec<f32>,
    chorus_delay_r: Vec<f32>,
    chorus_write_pos: usize,
    // Ring modulator
    ring_mod_freq: f32, // modulation frequency
    ring_mod_mix: f32,  // wet/dry
    ring_mod_phase: f32,
    // Pan effect
    pan_position: f32, // -1 (left) to 1 (right)
    pan_active: bool,
    // Wobble/ixi_techno (LFO-modulated low-pass filter, matching Sonic Pi)
    wobble_rate: f32,  // LFO rate in Hz
    wobble_depth: f32, // modulation depth 0-1
    wobble_mix: f32,   // wet/dry
    wobble_phase: f32,
    wobble_lpf_l: BiquadFilter, // internal LPF for wobble effect
    wobble_lpf_r: BiquadFilter,
    // Octaver (sub via flip-flop frequency divider, super via squaring)
    octaver_mix: f32,       // wet/dry
    octaver_sub_amp: f32,   // sub-octave level
    octaver_super_amp: f32, // super-octave level
    octaver_prev_l: f32,    // previous sample for zero-crossing detection
    octaver_prev_r: f32,
    octaver_flip_l: f32,    // flip-flop state for sub-octave
    octaver_flip_r: f32,
    // LPF/HPF resonance
    lpf_res: f32,           // resonance for LPF (Q factor)
    hpf_res: f32,           // resonance for HPF (Q factor)
    // Reverb damping
    reverb_damp: f32,
    // Delay mix (user-controllable)
    delay_mix_user: f32,    // user-specified mix for delay/echo
}

impl EffectChain {
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 2.0) as usize; // 2 second max delay
        let flanger_max = (sample_rate * 0.02) as usize; // 20ms max flanger delay
        let chorus_max = (sample_rate * 0.05) as usize; // 50ms max chorus delay
        Self {
            reverb_l: SchroederReverb::new(sample_rate),
            reverb_r: SchroederReverb::new(sample_rate),
            delay_l: DelayLine::new(max_delay),
            delay_r: DelayLine::new(max_delay),
            lpf_l: BiquadFilter::low_pass(20000.0, sample_rate),
            lpf_r: BiquadFilter::low_pass(20000.0, sample_rate),
            hpf_l: BiquadFilter::high_pass(20.0, sample_rate),
            hpf_r: BiquadFilter::high_pass(20.0, sample_rate),
            distortion_amount: 0.0,
            delay_mix: 0.0,
            sample_rate,
            lpf_active: false,
            hpf_active: false,
            // Slicer defaults (inactive)
            slicer_phase: 0.25,
            slicer_mix: 0.0,
            slicer_wave: 0,
            slicer_pos: 0.0,
            // Bitcrusher defaults (inactive) — Sonic Pi default bits=10
            bitcrusher_bits: 10.0,
            bitcrusher_sample_rate: 10000.0,
            bitcrusher_mix: 0.0,
            bitcrusher_hold_l: 0.0,
            bitcrusher_hold_r: 0.0,
            bitcrusher_hold_counter: 0.0,
            // Compressor defaults (inactive)
            compressor_threshold: 1.0,
            compressor_clamp_time: 0.01,
            compressor_relax_time: 0.1,
            compressor_mix: 0.0,
            compressor_env: 0.0,
            // Normaliser defaults (inactive)
            normaliser_level: 1.0,
            normaliser_active: false,
            // Flanger defaults (inactive)
            flanger_rate: 0.5,
            flanger_depth: 0.5,
            flanger_feedback: 0.5,
            flanger_mix: 0.0,
            flanger_phase: 0.0,
            flanger_delay_l: vec![0.0; flanger_max.max(1)],
            flanger_delay_r: vec![0.0; flanger_max.max(1)],
            flanger_write_pos: 0,
            // Chorus defaults (inactive)
            chorus_rate: 0.3,
            chorus_depth: 0.5,
            chorus_mix: 0.0,
            chorus_phases: [0.0, 0.33, 0.67],
            chorus_delay_l: vec![0.0; chorus_max.max(1)],
            chorus_delay_r: vec![0.0; chorus_max.max(1)],
            chorus_write_pos: 0,
            // Ring mod defaults (inactive)
            ring_mod_freq: 440.0,
            ring_mod_mix: 0.0,
            ring_mod_phase: 0.0,
            // Pan defaults (inactive)
            pan_position: 0.0,
            pan_active: false,
            // Wobble defaults (inactive) — now filter-based like Sonic Pi's ixi_techno
            wobble_rate: 1.0,
            wobble_depth: 0.5,
            wobble_mix: 0.0,
            wobble_phase: 0.0,
            wobble_lpf_l: BiquadFilter::low_pass(20000.0, sample_rate),
            wobble_lpf_r: BiquadFilter::low_pass(20000.0, sample_rate),
            // Octaver defaults (inactive) — now using flip-flop for true sub-octave
            octaver_mix: 0.0,
            octaver_sub_amp: 0.5,
            octaver_super_amp: 0.0,
            octaver_prev_l: 0.0,
            octaver_prev_r: 0.0,
            octaver_flip_l: 1.0,
            octaver_flip_r: 1.0,
            // LPF/HPF resonance
            lpf_res: 0.7071,
            hpf_res: 0.7071,
            // Reverb damping
            reverb_damp: 0.5,
            // Delay mix
            delay_mix_user: -1.0, // -1 = not set by user, use auto
        }
    }

    pub fn set_reverb_mix(&mut self, mix: f32) {
        self.reverb_l.set_mix(mix);
        self.reverb_r.set_mix(mix);
    }

    pub fn set_reverb_room(&mut self, room: f32) {
        self.reverb_l.set_room(room);
        self.reverb_r.set_room(room);
    }

    pub fn set_reverb_damp(&mut self, damp: f32) {
        self.reverb_damp = damp.clamp(0.0, 1.0);
        self.reverb_l.set_damp(damp);
        self.reverb_r.set_damp(damp);
    }

    pub fn set_delay(&mut self, time: f32, feedback: f32) {
        self.delay_l.set_delay(time, self.sample_rate);
        self.delay_r.set_delay(time, self.sample_rate);
        self.delay_l.set_feedback(feedback);
        self.delay_r.set_feedback(feedback);
        // Use user-specified mix if set, otherwise auto-calculate
        if self.delay_mix_user >= 0.0 {
            self.delay_mix = self.delay_mix_user;
        } else {
            self.delay_mix = if time > 0.001 { 0.5 } else { 0.0 };
        }
    }

    pub fn set_delay_mix(&mut self, mix: f32) {
        self.delay_mix_user = mix;
        self.delay_mix = mix;
    }

    pub fn set_distortion(&mut self, amount: f32) {
        self.distortion_amount = amount.clamp(0.0, 1.0);
    }

    pub fn set_lpf(&mut self, cutoff: f32) {
        // Convert MIDI note to Hz if value looks like MIDI (Sonic Pi uses MIDI notes 0-130)
        let cutoff_hz = if cutoff <= 130.0 {
            440.0 * 2.0f32.powf((cutoff - 69.0) / 12.0)
        } else {
            cutoff
        };
        if cutoff_hz < 19999.0 {
            self.lpf_active = true;
            self.lpf_l.set_low_pass_q(cutoff_hz, self.sample_rate, self.lpf_res);
            self.lpf_r.set_low_pass_q(cutoff_hz, self.sample_rate, self.lpf_res);
        } else {
            self.lpf_active = false;
        }
    }

    pub fn set_lpf_res(&mut self, res: f32) {
        // Sonic Pi res: 0 = no resonance, 1 = max resonance
        // Map to Q: 0.7071 (Butterworth) to ~20 (high resonance)
        self.lpf_res = 0.7071 + res.clamp(0.0, 1.0) * 19.3;
    }

    pub fn set_hpf(&mut self, cutoff: f32) {
        // Convert MIDI note to Hz if value looks like MIDI
        let cutoff_hz = if cutoff <= 130.0 {
            440.0 * 2.0f32.powf((cutoff - 69.0) / 12.0)
        } else {
            cutoff
        };
        if cutoff_hz > 21.0 {
            self.hpf_active = true;
            self.hpf_l.set_high_pass_q(cutoff_hz, self.sample_rate, self.hpf_res);
            self.hpf_r.set_high_pass_q(cutoff_hz, self.sample_rate, self.hpf_res);
        } else {
            self.hpf_active = false;
        }
    }

    pub fn set_hpf_res(&mut self, res: f32) {
        self.hpf_res = 0.7071 + res.clamp(0.0, 1.0) * 19.3;
    }

    /// Set LPF cutoff directly in Hz (no MIDI conversion). Used by the UI panel.
    pub fn set_lpf_hz(&mut self, cutoff_hz: f32) {
        if cutoff_hz < 19999.0 {
            self.lpf_active = true;
            self.lpf_l.set_low_pass_q(cutoff_hz.clamp(20.0, 20000.0), self.sample_rate, self.lpf_res);
            self.lpf_r.set_low_pass_q(cutoff_hz.clamp(20.0, 20000.0), self.sample_rate, self.lpf_res);
        } else {
            self.lpf_active = false;
        }
    }

    /// Set HPF cutoff directly in Hz (no MIDI conversion). Used by the UI panel.
    pub fn set_hpf_hz(&mut self, cutoff_hz: f32) {
        if cutoff_hz > 21.0 {
            self.hpf_active = true;
            self.hpf_l.set_high_pass_q(cutoff_hz.clamp(20.0, 20000.0), self.sample_rate, self.hpf_res);
            self.hpf_r.set_high_pass_q(cutoff_hz.clamp(20.0, 20000.0), self.sample_rate, self.hpf_res);
        } else {
            self.hpf_active = false;
        }
    }

    pub fn set_slicer(&mut self, phase: f32, mix: f32, wave: i32) {
        self.slicer_phase = phase.max(0.01);
        self.slicer_mix = mix.clamp(0.0, 1.0);
        self.slicer_wave = wave;
        if mix < 0.001 {
            self.slicer_pos = 0.0;
        }
    }

    pub fn set_bitcrusher(&mut self, bits: f32, target_sr: f32, mix: f32) {
        self.bitcrusher_bits = bits.clamp(1.0, 16.0);
        self.bitcrusher_sample_rate = target_sr.clamp(100.0, self.sample_rate);
        self.bitcrusher_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_compressor(&mut self, threshold: f32, clamp_time: f32, relax_time: f32, mix: f32) {
        self.compressor_threshold = threshold.clamp(0.01, 1.0);
        self.compressor_clamp_time = clamp_time.max(0.001);
        self.compressor_relax_time = relax_time.max(0.001);
        self.compressor_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_normaliser(&mut self, level: f32) {
        self.normaliser_level = level.clamp(0.0, 2.0);
        self.normaliser_active = level < 1.99;
    }

    pub fn set_flanger(&mut self, rate: f32, depth: f32, feedback: f32, mix: f32) {
        self.flanger_rate = rate.clamp(0.1, 10.0);
        self.flanger_depth = depth.clamp(0.0, 1.0);
        self.flanger_feedback = feedback.clamp(0.0, 0.95);
        self.flanger_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_chorus(&mut self, rate: f32, depth: f32, mix: f32) {
        self.chorus_rate = rate.clamp(0.1, 5.0);
        self.chorus_depth = depth.clamp(0.0, 1.0);
        self.chorus_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_ring_mod(&mut self, freq: f32, mix: f32) {
        self.ring_mod_freq = freq.clamp(20.0, 5000.0);
        self.ring_mod_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_pan(&mut self, position: f32) {
        self.pan_position = position.clamp(-1.0, 1.0);
        self.pan_active = position.abs() > 0.01;
    }

    pub fn set_wobble(&mut self, rate: f32, depth: f32, mix: f32) {
        self.wobble_rate = rate.clamp(0.1, 20.0);
        self.wobble_depth = depth.clamp(0.0, 1.0);
        self.wobble_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_octaver(&mut self, mix: f32, sub_amp: f32, super_amp: f32) {
        self.octaver_mix = mix.clamp(0.0, 1.0);
        self.octaver_sub_amp = sub_amp.clamp(0.0, 1.0);
        self.octaver_super_amp = super_amp.clamp(0.0, 1.0);
    }

    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let mut l = left;
        let mut r = right;

        // Distortion (soft clipping via tanh)
        if self.distortion_amount > 0.001 {
            let gain = 1.0 + self.distortion_amount * 20.0;
            l = (l * gain).tanh();
            r = (r * gain).tanh();
        }

        // Low-pass filter
        if self.lpf_active {
            l = self.lpf_l.process(l);
            r = self.lpf_r.process(r);
        }

        // High-pass filter
        if self.hpf_active {
            l = self.hpf_l.process(l);
            r = self.hpf_r.process(r);
        }

        // Slicer (amplitude gating LFO)
        if self.slicer_mix > 0.001 {
            let phase_inc = 1.0 / (self.slicer_phase * self.sample_rate);
            self.slicer_pos = (self.slicer_pos + phase_inc) % 1.0;
            let lfo = match self.slicer_wave {
                0 => {
                    // Square wave: on for first half, off for second half
                    if self.slicer_pos < 0.5 {
                        1.0
                    } else {
                        0.0
                    }
                }
                1 => {
                    // Saw down
                    1.0 - self.slicer_pos
                }
                2 => {
                    // Saw up
                    self.slicer_pos
                }
                _ => {
                    // Triangle
                    if self.slicer_pos < 0.5 {
                        self.slicer_pos * 2.0
                    } else {
                        2.0 - self.slicer_pos * 2.0
                    }
                }
            };
            let gate = 1.0 - self.slicer_mix + self.slicer_mix * lfo;
            l *= gate;
            r *= gate;
        }

        // Bitcrusher
        if self.bitcrusher_mix > 0.001 {
            // Sample rate reduction (sample-and-hold)
            let sr_ratio = self.sample_rate / self.bitcrusher_sample_rate;
            self.bitcrusher_hold_counter += 1.0;
            if self.bitcrusher_hold_counter >= sr_ratio {
                self.bitcrusher_hold_counter -= sr_ratio;
                // Bit depth reduction
                let levels = 2.0f32.powf(self.bitcrusher_bits);
                self.bitcrusher_hold_l = (l * levels).round() / levels;
                self.bitcrusher_hold_r = (r * levels).round() / levels;
            }
            l = l * (1.0 - self.bitcrusher_mix) + self.bitcrusher_hold_l * self.bitcrusher_mix;
            r = r * (1.0 - self.bitcrusher_mix) + self.bitcrusher_hold_r * self.bitcrusher_mix;
        }

        // Compressor (simple feed-forward RMS compressor)
        if self.compressor_mix > 0.001 {
            let input_level = (l * l + r * r).sqrt() * 0.7071; // RMS approximation
                                                               // Envelope follower (attack/release)
            let target = input_level;
            let coeff = if target > self.compressor_env {
                // Attack (fast)
                (-1.0 / (self.compressor_clamp_time * self.sample_rate)).exp()
            } else {
                // Release (slow)
                (-1.0 / (self.compressor_relax_time * self.sample_rate)).exp()
            };
            self.compressor_env = coeff * self.compressor_env + (1.0 - coeff) * target;

            // Compute gain reduction
            let gain = if self.compressor_env > self.compressor_threshold {
                // Soft-knee ratio ~4:1
                self.compressor_threshold / self.compressor_env
            } else {
                1.0
            };
            let compressed_l = l * gain;
            let compressed_r = r * gain;
            l = l * (1.0 - self.compressor_mix) + compressed_l * self.compressor_mix;
            r = r * (1.0 - self.compressor_mix) + compressed_r * self.compressor_mix;
        }

        // Flanger (modulated short delay)
        if self.flanger_mix > 0.001 && !self.flanger_delay_l.is_empty() {
            let buf_len = self.flanger_delay_l.len();
            // Update LFO
            self.flanger_phase += self.flanger_rate / self.sample_rate;
            if self.flanger_phase >= 1.0 {
                self.flanger_phase -= 1.0;
            }
            let lfo = (self.flanger_phase * 2.0 * PI).sin();
            // Calculate delay time: 1-10ms modulated
            let base_delay_samples = (0.005 * self.sample_rate) as f32; // 5ms base
            let mod_range = (0.004 * self.sample_rate) as f32; // ±4ms range
            let delay_samples =
                (base_delay_samples + lfo * mod_range * self.flanger_depth).max(1.0);

            // Write to buffer
            self.flanger_delay_l[self.flanger_write_pos] = l;
            self.flanger_delay_r[self.flanger_write_pos] = r;

            // Read from buffer with linear interpolation
            let read_pos_f = self.flanger_write_pos as f32 - delay_samples;
            let read_pos = if read_pos_f < 0.0 {
                (read_pos_f + buf_len as f32) as usize % buf_len
            } else {
                read_pos_f as usize % buf_len
            };
            let frac = read_pos_f.fract().abs();
            let next_pos = (read_pos + 1) % buf_len;

            let delayed_l = self.flanger_delay_l[read_pos] * (1.0 - frac)
                + self.flanger_delay_l[next_pos] * frac;
            let delayed_r = self.flanger_delay_r[read_pos] * (1.0 - frac)
                + self.flanger_delay_r[next_pos] * frac;

            // Apply feedback
            self.flanger_delay_l[self.flanger_write_pos] += delayed_l * self.flanger_feedback;
            self.flanger_delay_r[self.flanger_write_pos] += delayed_r * self.flanger_feedback;

            self.flanger_write_pos = (self.flanger_write_pos + 1) % buf_len;

            l = l * (1.0 - self.flanger_mix) + delayed_l * self.flanger_mix;
            r = r * (1.0 - self.flanger_mix) + delayed_r * self.flanger_mix;
        }

        // Chorus (3-voice detuned delays)
        if self.chorus_mix > 0.001 && !self.chorus_delay_l.is_empty() {
            let buf_len = self.chorus_delay_l.len();

            // Write to buffer
            self.chorus_delay_l[self.chorus_write_pos] = l;
            self.chorus_delay_r[self.chorus_write_pos] = r;

            let mut chorus_l = 0.0f32;
            let mut chorus_r = 0.0f32;

            for (i, phase) in self.chorus_phases.iter_mut().enumerate() {
                // Update LFO for this voice
                *phase += self.chorus_rate / self.sample_rate;
                if *phase >= 1.0 {
                    *phase -= 1.0;
                }
                let lfo = (*phase * 2.0 * PI).sin();
                let base_delay = (0.015 + 0.005 * i as f32) * self.sample_rate; // 15-25ms
                let delay_samples =
                    (base_delay + lfo * 0.005 * self.sample_rate * self.chorus_depth).max(1.0);

                let read_pos_f = self.chorus_write_pos as f32 - delay_samples;
                let read_pos = if read_pos_f < 0.0 {
                    (read_pos_f + buf_len as f32) as usize % buf_len
                } else {
                    read_pos_f as usize % buf_len
                };
                // Linear interpolation for smooth chorus (avoids aliasing artifacts)
                let frac = read_pos_f.fract().abs();
                let next_pos = (read_pos + 1) % buf_len;

                chorus_l += self.chorus_delay_l[read_pos] * (1.0 - frac)
                    + self.chorus_delay_l[next_pos] * frac;
                chorus_r += self.chorus_delay_r[read_pos] * (1.0 - frac)
                    + self.chorus_delay_r[next_pos] * frac;
            }

            chorus_l /= 3.0;
            chorus_r /= 3.0;

            self.chorus_write_pos = (self.chorus_write_pos + 1) % buf_len;

            l = l * (1.0 - self.chorus_mix) + chorus_l * self.chorus_mix;
            r = r * (1.0 - self.chorus_mix) + chorus_r * self.chorus_mix;
        }

        // Ring modulator
        if self.ring_mod_mix > 0.001 {
            self.ring_mod_phase += self.ring_mod_freq / self.sample_rate;
            if self.ring_mod_phase >= 1.0 {
                self.ring_mod_phase -= 1.0;
            }
            let carrier = (self.ring_mod_phase * 2.0 * PI).sin();
            let mod_l = l * carrier;
            let mod_r = r * carrier;
            l = l * (1.0 - self.ring_mod_mix) + mod_l * self.ring_mod_mix;
            r = r * (1.0 - self.ring_mod_mix) + mod_r * self.ring_mod_mix;
        }

        // Pan effect
        if self.pan_active {
            let pan_rad = self.pan_position * PI / 4.0; // -45 to +45 degrees
            let left_gain = (PI / 4.0 - pan_rad).cos();
            let right_gain = (PI / 4.0 + pan_rad).cos();
            l *= left_gain;
            r *= right_gain;
        }

        // Wobble/ixi_techno — LFO-modulated low-pass filter (matching Sonic Pi behavior)
        if self.wobble_mix > 0.001 {
            self.wobble_phase += self.wobble_rate / self.sample_rate;
            if self.wobble_phase >= 1.0 {
                self.wobble_phase -= 1.0;
            }
            let lfo = (self.wobble_phase * 2.0 * PI).sin();
            // Modulate cutoff frequency: range from ~200 Hz to ~8000 Hz
            let min_cutoff = 200.0;
            let max_cutoff = 8000.0;
            let cutoff = min_cutoff + (max_cutoff - min_cutoff) * (0.5 + 0.5 * lfo) * self.wobble_depth;
            self.wobble_lpf_l.set_low_pass(cutoff, self.sample_rate);
            self.wobble_lpf_r.set_low_pass(cutoff, self.sample_rate);
            let filtered_l = self.wobble_lpf_l.process(l);
            let filtered_r = self.wobble_lpf_r.process(r);
            l = l * (1.0 - self.wobble_mix) + filtered_l * self.wobble_mix;
            r = r * (1.0 - self.wobble_mix) + filtered_r * self.wobble_mix;
        }

        // Octaver — sub-octave via zero-crossing flip-flop (true frequency division)
        // super-octave via squaring
        if self.octaver_mix > 0.001 {
            // Detect zero crossings for sub-octave (flip-flop divides frequency by 2)
            if self.octaver_prev_l * l < 0.0 {
                // Zero crossing detected on left channel
                self.octaver_flip_l = -self.octaver_flip_l;
            }
            if self.octaver_prev_r * r < 0.0 {
                self.octaver_flip_r = -self.octaver_flip_r;
            }
            self.octaver_prev_l = l;
            self.octaver_prev_r = r;

            // Sub-octave: multiply signal by flip-flop state (divides frequency by 2)
            let sub_l = l * self.octaver_flip_l * self.octaver_sub_amp;
            let sub_r = r * self.octaver_flip_r * self.octaver_sub_amp;
            // Super-octave: squaring doubles the frequency
            let super_l = (l * l).copysign(l) * self.octaver_super_amp;
            let super_r = (r * r).copysign(r) * self.octaver_super_amp;
            let octaved_l = sub_l + super_l;
            let octaved_r = sub_r + super_r;
            l = l * (1.0 - self.octaver_mix) + octaved_l * self.octaver_mix;
            r = r * (1.0 - self.octaver_mix) + octaved_r * self.octaver_mix;
        }

        // Delay
        if self.delay_mix > 0.001 {
            let dl = self.delay_l.process(l);
            let dr = self.delay_r.process(r);
            l = l * (1.0 - self.delay_mix) + dl * self.delay_mix;
            r = r * (1.0 - self.delay_mix) + dr * self.delay_mix;
        }

        // Reverb
        l = self.reverb_l.process(l);
        r = self.reverb_r.process(r);

        // Normaliser (peak limiter / makeup gain)
        if self.normaliser_active {
            let peak = l.abs().max(r.abs()).max(0.0001);
            if peak > self.normaliser_level {
                let gain = self.normaliser_level / peak;
                l *= gain;
                r *= gain;
            }
        }

        (l, r)
    }
}

// ────────────── Per-Voice FX Chain ──────────────
//
// Lightweight effect processor attached to individual voices/samples.
// Used by the cpal engine to implement scoped `with_fx` blocks.
// Only voices created while FX blocks are active get a VoiceFx.

/// Describes a single FX slot in a per-voice chain.
#[derive(Clone)]
pub(crate) enum VoiceFxSlot {
    Lpf {
        filter: BiquadFilter,
    },
    Hpf {
        filter: BiquadFilter,
    },
    Distortion {
        amount: f32,
    },
    Slicer {
        phase_period: f32,  // period in seconds
        mix: f32,
        pos: f32,
        wave: i32,
        sample_rate: f32,
    },
    Bitcrusher {
        bits: f32,
        target_sr: f32,
        mix: f32,
        hold: f32,
        counter: f32,
        sample_rate: f32,
    },
    RingMod {
        freq: f32,
        mix: f32,
        phase: f32,
        sample_rate: f32,
    },
    Wobble {
        rate: f32,
        depth: f32,
        mix: f32,
        phase: f32,
        lpf: BiquadFilter,
        sample_rate: f32,
    },
    Pan {
        position: f32,
    },
    /// Reverb send — the voice's per-frame output is mixed into a shared reverb bus
    ReverbSend {
        _mix: f32,
        _room: f32,
        _damp: f32,
    },
    /// Delay send — the voice's output is mixed into a shared delay bus
    DelaySend {
        _time_secs: f32,
        _feedback: f32,
        _mix: f32,
    },
    Flanger {
        rate: f32,
        depth: f32,
        feedback: f32,
        mix: f32,
        phase: f32,
        delay_buf: Vec<f32>,
        write_pos: usize,
        sample_rate: f32,
    },
    Chorus {
        rate: f32,
        depth: f32,
        mix: f32,
        phases: [f32; 3],
        delay_buf: Vec<f32>,
        write_pos: usize,
        sample_rate: f32,
    },
    Compressor {
        threshold: f32,
        clamp_time: f32,
        relax_time: f32,
        mix: f32,
        env: f32,
        sample_rate: f32,
    },
    Octaver {
        mix: f32,
        sub_amp: f32,
        super_amp: f32,
        prev: f32,
        flip: f32,
    },
    /// Band pass filter — `:bpf`, `:rbpf`, `:nbpf`, `:nrbpf`
    Bpf {
        filter: BiquadFilter,
        normalised: bool,
    },
    /// Tremolo — amplitude modulation LFO
    Tremolo {
        rate: f32,
        depth: f32,
        mix: f32,
        phase: f32,
        wave: i32, // 0=sine, 1=saw, 2=square, 3=triangle
        sample_rate: f32,
    },
    /// Ping-pong delay — stereo bouncing delay
    PingPong {
        phase_secs: f32,
        feedback: f32,
        mix: f32,
    },
    /// Level — simple gain control
    Level {
        amp: f32,
    },
    /// Mono — force stereo to mono (handled at pan stage)
    Mono,
    /// Whammy — pitch shift via rate modulation
    Whammy {
        transpose: f32, // semitones
        mix: f32,
    },
    /// Band EQ — parametric peaking EQ
    BandEq {
        filter: BiquadFilter,
        mix: f32,
    },
    /// Pitch shift — granular pitch shift approximation
    PitchShift {
        shift: f32, // semitones
        mix: f32,
        window: f32,
    },
}

/// Per-voice FX chain: processes a mono sample through a sequence of FX slots.
#[derive(Clone)]
pub struct VoiceFx {
    pub(crate) slots: Vec<VoiceFxSlot>,
    /// Accumulated reverb send level (set by ReverbSend slots)
    pub reverb_send: f32,
    pub reverb_room: f32,
    pub reverb_damp: f32,
    /// Accumulated delay send params (set by DelaySend slots)
    pub delay_send: f32,
    pub delay_time: f32,
    pub delay_feedback: f32,
}

impl VoiceFx {
    /// Create a VoiceFx from a list of active FX block descriptors.
    /// Each descriptor is (fx_type, params) from the FX stack.
    pub fn from_fx_stack(stack: &[(String, Vec<(String, f32)>)], sample_rate: f32) -> Option<Self> {
        if stack.is_empty() {
            return None;
        }
        let mut slots = Vec::new();
        let mut reverb_send = 0.0f32;
        let mut reverb_room = 0.6f32;
        let mut reverb_damp = 0.5f32;
        let mut delay_send = 0.0f32;
        let mut delay_time = 0.0f32;
        let mut delay_feedback = 0.5f32;

        let get_param = |params: &[(String, f32)], name: &str| -> Option<f32> {
            params.iter().find(|(k, _)| k == name).map(|(_, v)| *v)
        };

        for (fx_type, params) in stack {
            match fx_type.as_str() {
                "lpf" | "rlpf" | "nrlpf" => {
                    let cutoff_raw = get_param(params, "cutoff").unwrap_or(100.0);
                    // Convert MIDI to Hz if in MIDI range
                    let cutoff_hz = if cutoff_raw <= 130.0 {
                        440.0 * 2.0f32.powf((cutoff_raw - 69.0) / 12.0)
                    } else {
                        cutoff_raw
                    };
                    let res = get_param(params, "res").unwrap_or(0.0);
                    let q = 0.7071 + res.clamp(0.0, 1.0) * 19.3;
                    slots.push(VoiceFxSlot::Lpf {
                        filter: BiquadFilter::low_pass_q(cutoff_hz, sample_rate, q),
                    });
                }
                "hpf" | "rhpf" | "nrhpf" => {
                    let cutoff_raw = get_param(params, "cutoff").unwrap_or(60.0);
                    let cutoff_hz = if cutoff_raw <= 130.0 {
                        440.0 * 2.0f32.powf((cutoff_raw - 69.0) / 12.0)
                    } else {
                        cutoff_raw
                    };
                    let res = get_param(params, "res").unwrap_or(0.0);
                    let q = 0.7071 + res.clamp(0.0, 1.0) * 19.3;
                    slots.push(VoiceFxSlot::Hpf {
                        filter: BiquadFilter::high_pass_q(cutoff_hz, sample_rate, q),
                    });
                }
                "distortion" | "tanh" => {
                    let amount = get_param(params, "distort")
                        .or_else(|| get_param(params, "mix"))
                        .unwrap_or(0.5);
                    slots.push(VoiceFxSlot::Distortion { amount });
                }
                "slicer" => {
                    slots.push(VoiceFxSlot::Slicer {
                        phase_period: get_param(params, "phase").unwrap_or(0.25),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        pos: 0.0,
                        wave: get_param(params, "wave").map(|v| v as i32).unwrap_or(0),
                        sample_rate,
                    });
                }
                "bitcrusher" | "krush" => {
                    slots.push(VoiceFxSlot::Bitcrusher {
                        bits: get_param(params, "bits").unwrap_or(10.0),
                        target_sr: get_param(params, "sample_rate").unwrap_or(10000.0),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        hold: 0.0,
                        counter: 0.0,
                        sample_rate,
                    });
                }
                "ring_mod" => {
                    slots.push(VoiceFxSlot::RingMod {
                        freq: get_param(params, "freq")
                            .or_else(|| get_param(params, "frequency"))
                            .unwrap_or(30.0),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        phase: 0.0,
                        sample_rate,
                    });
                }
                "wobble" | "ixi_techno" => {
                    slots.push(VoiceFxSlot::Wobble {
                        rate: get_param(params, "rate")
                            .or_else(|| get_param(params, "phase"))
                            .unwrap_or(4.0),
                        depth: get_param(params, "depth")
                            .or_else(|| get_param(params, "cutoff_min"))
                            .unwrap_or(0.5),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        phase: 0.0,
                        lpf: BiquadFilter::low_pass(20000.0, sample_rate),
                        sample_rate,
                    });
                }
                "pan" => {
                    slots.push(VoiceFxSlot::Pan {
                        position: get_param(params, "pan").unwrap_or(0.0),
                    });
                }
                "reverb" | "gverb" => {
                    reverb_send = get_param(params, "mix").unwrap_or(0.4);
                    reverb_room = get_param(params, "room").unwrap_or(0.6);
                    reverb_damp = get_param(params, "damp").unwrap_or(0.5);
                    slots.push(VoiceFxSlot::ReverbSend {
                        _mix: reverb_send,
                        _room: reverb_room,
                        _damp: reverb_damp,
                    });
                }
                "echo" | "delay" => {
                    // Phase is in seconds (already converted from beats by parser)
                    delay_time = get_param(params, "phase")
                        .or_else(|| get_param(params, "time"))
                        .unwrap_or(0.25);
                    delay_feedback = get_param(params, "feedback")
                        .or_else(|| get_param(params, "decay"))
                        .unwrap_or(0.5);
                    delay_send = get_param(params, "mix").unwrap_or(1.0);
                    slots.push(VoiceFxSlot::DelaySend {
                        _time_secs: delay_time,
                        _feedback: delay_feedback,
                        _mix: delay_send,
                    });
                }
                "flanger" => {
                    let max_delay = (sample_rate * 0.02) as usize;
                    slots.push(VoiceFxSlot::Flanger {
                        rate: get_param(params, "rate").unwrap_or(0.25),
                        depth: get_param(params, "depth").unwrap_or(0.5),
                        feedback: get_param(params, "feedback").unwrap_or(0.0),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        phase: 0.0,
                        delay_buf: vec![0.0; max_delay.max(1)],
                        write_pos: 0,
                        sample_rate,
                    });
                }
                "chorus" => {
                    let max_delay = (sample_rate * 0.05) as usize;
                    slots.push(VoiceFxSlot::Chorus {
                        rate: get_param(params, "rate").unwrap_or(0.3),
                        depth: get_param(params, "depth").unwrap_or(0.5),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        phases: [0.0, 0.33, 0.67],
                        delay_buf: vec![0.0; max_delay.max(1)],
                        write_pos: 0,
                        sample_rate,
                    });
                }
                "compressor" => {
                    slots.push(VoiceFxSlot::Compressor {
                        threshold: get_param(params, "threshold").unwrap_or(0.2),
                        clamp_time: get_param(params, "clamp_time").unwrap_or(0.01),
                        relax_time: get_param(params, "relax_time").unwrap_or(0.1),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        env: 0.0,
                        sample_rate,
                    });
                }
                "normaliser" | "normalizer" => {
                    // Normaliser is better applied globally; skip per-voice
                }
                "octaver" => {
                    slots.push(VoiceFxSlot::Octaver {
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        sub_amp: get_param(params, "sub_amp")
                            .or_else(|| get_param(params, "sub"))
                            .unwrap_or(1.0),
                        super_amp: get_param(params, "super_amp")
                            .or_else(|| get_param(params, "super"))
                            .unwrap_or(1.0),
                        prev: 0.0,
                        flip: 1.0,
                    });
                }
                "bpf" | "rbpf" => {
                    let cutoff_raw = get_param(params, "centre")
                        .or_else(|| get_param(params, "center"))
                        .or_else(|| get_param(params, "cutoff"))
                        .unwrap_or(100.0);
                    let cutoff_hz = if cutoff_raw <= 130.0 {
                        440.0 * 2.0f32.powf((cutoff_raw - 69.0) / 12.0)
                    } else {
                        cutoff_raw
                    };
                    let res = get_param(params, "res").unwrap_or(0.0);
                    let q = 0.7071 + res.clamp(0.0, 1.0) * 19.3;
                    slots.push(VoiceFxSlot::Bpf {
                        filter: BiquadFilter::band_pass(cutoff_hz, sample_rate, q),
                        normalised: false,
                    });
                }
                "nbpf" | "nrbpf" => {
                    let cutoff_raw = get_param(params, "centre")
                        .or_else(|| get_param(params, "center"))
                        .or_else(|| get_param(params, "cutoff"))
                        .unwrap_or(100.0);
                    let cutoff_hz = if cutoff_raw <= 130.0 {
                        440.0 * 2.0f32.powf((cutoff_raw - 69.0) / 12.0)
                    } else {
                        cutoff_raw
                    };
                    let res = get_param(params, "res").unwrap_or(0.0);
                    let q = 0.7071 + res.clamp(0.0, 1.0) * 19.3;
                    slots.push(VoiceFxSlot::Bpf {
                        filter: BiquadFilter::band_pass(cutoff_hz, sample_rate, q),
                        normalised: true,
                    });
                }
                "tremolo" => {
                    slots.push(VoiceFxSlot::Tremolo {
                        rate: get_param(params, "rate").unwrap_or(4.0),
                        depth: get_param(params, "depth").unwrap_or(0.5),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        phase: 0.0,
                        wave: get_param(params, "wave").map(|v| v as i32).unwrap_or(2), // Sonic Pi default: triangle
                        sample_rate,
                    });
                }
                "ping_pong" => {
                    // Stereo ping-pong delay: Phase is in seconds (already converted from beats by engine)
                    let phase = get_param(params, "phase")
                        .or_else(|| get_param(params, "time"))
                        .unwrap_or(0.25);
                    let feedback = get_param(params, "feedback")
                        .or_else(|| get_param(params, "decay"))
                        .unwrap_or(0.5);
                    let mix = get_param(params, "mix").unwrap_or(1.0);
                    delay_time = phase;
                    delay_feedback = feedback;
                    delay_send = mix;
                    slots.push(VoiceFxSlot::PingPong {
                        phase_secs: phase,
                        feedback,
                        mix,
                    });
                }
                "level" => {
                    slots.push(VoiceFxSlot::Level {
                        amp: get_param(params, "amp").unwrap_or(1.0),
                    });
                }
                "mono" => {
                    slots.push(VoiceFxSlot::Mono);
                }
                "whammy" => {
                    slots.push(VoiceFxSlot::Whammy {
                        transpose: get_param(params, "transpose").unwrap_or(12.0),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                    });
                }
                "band_eq" => {
                    let freq_raw = get_param(params, "freq")
                        .or_else(|| get_param(params, "frequency"))
                        .unwrap_or(100.0);
                    let freq_hz = if freq_raw <= 130.0 {
                        440.0 * 2.0f32.powf((freq_raw - 69.0) / 12.0)
                    } else {
                        freq_raw
                    };
                    let res = get_param(params, "res").unwrap_or(0.6);
                    let db = get_param(params, "db").unwrap_or(0.0);
                    slots.push(VoiceFxSlot::BandEq {
                        filter: BiquadFilter::peaking_eq(freq_hz, sample_rate, res.max(0.1), db),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                    });
                }
                "pitch_shift" => {
                    slots.push(VoiceFxSlot::PitchShift {
                        shift: get_param(params, "shift")
                            .or_else(|| get_param(params, "pitch"))
                            .unwrap_or(0.0),
                        mix: get_param(params, "mix").unwrap_or(1.0),
                        window: get_param(params, "window_size").unwrap_or(0.2),
                    });
                }
                _ => {}
            }
        }

        if slots.is_empty() && reverb_send < 0.001 && delay_send < 0.001 {
            return None;
        }

        Some(VoiceFx {
            slots,
            reverb_send,
            reverb_room,
            reverb_damp,
            delay_send,
            delay_time,
            delay_feedback,
        })
    }

    /// Process a mono sample through all FX slots in order.
    /// Returns the processed sample. Reverb/delay sends are accumulated
    /// separately (caller reads `reverb_send`/`delay_send` to route).
    pub fn process(&mut self, input: f32) -> f32 {
        let mut s = input;
        for slot in self.slots.iter_mut() {
            match slot {
                VoiceFxSlot::Lpf { filter } => {
                    s = filter.process(s);
                }
                VoiceFxSlot::Hpf { filter } => {
                    s = filter.process(s);
                }
                VoiceFxSlot::Distortion { amount } => {
                    let gain = 1.0 + *amount * 20.0;
                    s = (s * gain).tanh();
                }
                VoiceFxSlot::Slicer {
                    phase_period,
                    mix,
                    pos,
                    wave,
                    sample_rate: sr,
                } => {
                    let phase_inc = 1.0 / (*phase_period * *sr);
                    *pos = (*pos + phase_inc) % 1.0;
                    let lfo = match *wave {
                        0 => if *pos < 0.5 { 1.0 } else { 0.0 },
                        1 => 1.0 - *pos,
                        2 => *pos,
                        _ => if *pos < 0.5 { *pos * 2.0 } else { 2.0 - *pos * 2.0 },
                    };
                    let gate = 1.0 - *mix + *mix * lfo;
                    s *= gate;
                }
                VoiceFxSlot::Bitcrusher {
                    bits,
                    target_sr,
                    mix,
                    hold,
                    counter,
                    sample_rate: sr,
                } => {
                    let sr_ratio = *sr / *target_sr;
                    *counter += 1.0;
                    if *counter >= sr_ratio {
                        *counter -= sr_ratio;
                        let levels = 2.0f32.powf(*bits);
                        *hold = (s * levels).round() / levels;
                    }
                    s = s * (1.0 - *mix) + *hold * *mix;
                }
                VoiceFxSlot::RingMod {
                    freq,
                    mix,
                    phase,
                    sample_rate: sr,
                } => {
                    *phase += *freq / *sr;
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                    let carrier = (*phase * 2.0 * PI).sin();
                    let modulated = s * carrier;
                    s = s * (1.0 - *mix) + modulated * *mix;
                }
                VoiceFxSlot::Wobble {
                    rate,
                    depth,
                    mix,
                    phase,
                    lpf,
                    sample_rate: sr,
                } => {
                    *phase += *rate / *sr;
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                    let lfo = (*phase * 2.0 * PI).sin();
                    let min_cutoff = 200.0;
                    let max_cutoff = 8000.0;
                    let cutoff =
                        min_cutoff + (max_cutoff - min_cutoff) * (0.5 + 0.5 * lfo) * *depth;
                    lpf.set_low_pass(cutoff, *sr);
                    let filtered = lpf.process(s);
                    s = s * (1.0 - *mix) + filtered * *mix;
                }
                VoiceFxSlot::Pan { .. } => {
                    // Pan is handled by the caller after mono processing
                }
                VoiceFxSlot::ReverbSend { .. } | VoiceFxSlot::DelaySend { .. } => {
                    // Send-based effects: caller routes to shared bus
                }
                VoiceFxSlot::Flanger {
                    rate,
                    depth,
                    feedback,
                    mix,
                    phase,
                    delay_buf,
                    write_pos,
                    sample_rate: sr,
                } => {
                    if !delay_buf.is_empty() {
                        let buf_len = delay_buf.len();
                        *phase += *rate / *sr;
                        if *phase >= 1.0 {
                            *phase -= 1.0;
                        }
                        let lfo = (*phase * 2.0 * PI).sin();
                        let base_delay = 0.005 * *sr;
                        let mod_range = 0.004 * *sr;
                        let delay_samples = (base_delay + lfo * mod_range * *depth).max(1.0);
                        delay_buf[*write_pos] = s;
                        let read_pos_f = *write_pos as f32 - delay_samples;
                        let read_pos = if read_pos_f < 0.0 {
                            (read_pos_f + buf_len as f32) as usize % buf_len
                        } else {
                            read_pos_f as usize % buf_len
                        };
                        let frac = read_pos_f.fract().abs();
                        let next_pos = (read_pos + 1) % buf_len;
                        let delayed = delay_buf[read_pos] * (1.0 - frac)
                            + delay_buf[next_pos] * frac;
                        delay_buf[*write_pos] += delayed * *feedback;
                        *write_pos = (*write_pos + 1) % buf_len;
                        s = s * (1.0 - *mix) + delayed * *mix;
                    }
                }
                VoiceFxSlot::Chorus {
                    rate,
                    depth,
                    mix,
                    phases,
                    delay_buf,
                    write_pos,
                    sample_rate: sr,
                } => {
                    if !delay_buf.is_empty() {
                        let buf_len = delay_buf.len();
                        delay_buf[*write_pos] = s;
                        let mut chorus_sum = 0.0f32;
                        for (i, ph) in phases.iter_mut().enumerate() {
                            *ph += *rate / *sr;
                            if *ph >= 1.0 {
                                *ph -= 1.0;
                            }
                            let lfo = (*ph * 2.0 * PI).sin();
                            let base = (0.015 + 0.005 * i as f32) * *sr;
                            let delay_s = (base + lfo * 0.005 * *sr * *depth).max(1.0);
                            let rp_f = *write_pos as f32 - delay_s;
                            let rp = if rp_f < 0.0 {
                                (rp_f + buf_len as f32) as usize % buf_len
                            } else {
                                rp_f as usize % buf_len
                            };
                            let frac = rp_f.fract().abs();
                            let np = (rp + 1) % buf_len;
                            chorus_sum +=
                                delay_buf[rp] * (1.0 - frac) + delay_buf[np] * frac;
                        }
                        chorus_sum /= 3.0;
                        *write_pos = (*write_pos + 1) % buf_len;
                        s = s * (1.0 - *mix) + chorus_sum * *mix;
                    }
                }
                VoiceFxSlot::Compressor {
                    threshold,
                    clamp_time,
                    relax_time,
                    mix,
                    env,
                    sample_rate: sr,
                } => {
                    let level = s.abs();
                    let coeff = if level > *env {
                        (-1.0 / (*clamp_time * *sr)).exp()
                    } else {
                        (-1.0 / (*relax_time * *sr)).exp()
                    };
                    *env = coeff * *env + (1.0 - coeff) * level;
                    let gain = if *env > *threshold {
                        *threshold / *env
                    } else {
                        1.0
                    };
                    let compressed = s * gain;
                    s = s * (1.0 - *mix) + compressed * *mix;
                }
                VoiceFxSlot::Octaver {
                    mix,
                    sub_amp,
                    super_amp,
                    prev,
                    flip,
                } => {
                    if *prev * s < 0.0 {
                        *flip = -*flip;
                    }
                    *prev = s;
                    let sub = s * *flip * *sub_amp;
                    let sup = (s * s).copysign(s) * *super_amp;
                    let octaved = sub + sup;
                    s = s * (1.0 - *mix) + octaved * *mix;
                }
                VoiceFxSlot::Bpf { filter, normalised } => {
                    let filtered = filter.process(s);
                    // Normalised variants boost output to unity gain
                    s = if *normalised {
                        let peak = filtered.abs().max(0.0001);
                        if peak > 1.0 { filtered / peak } else { filtered }
                    } else {
                        filtered
                    };
                }
                VoiceFxSlot::Tremolo {
                    rate,
                    depth,
                    mix,
                    phase,
                    wave,
                    sample_rate: sr,
                } => {
                    *phase += *rate / *sr;
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                    // LFO waveform
                    let lfo = match *wave {
                        0 => (*phase * 2.0 * PI).sin() * 0.5 + 0.5, // sine
                        1 => *phase,                                   // saw (ramp up)
                        3 => {                                          // triangle
                            if *phase < 0.5 { *phase * 2.0 } else { 2.0 - *phase * 2.0 }
                        }
                        _ => if *phase < 0.5 { 1.0 } else { 0.0 },   // square (default)
                    };
                    let gain = 1.0 - *depth * (1.0 - lfo);
                    let tremmed = s * gain;
                    s = s * (1.0 - *mix) + tremmed * *mix;
                }
                VoiceFxSlot::PingPong { .. } => {
                    // Ping-pong is a stereo send effect - handled by caller via delay bus
                }
                VoiceFxSlot::Level { amp } => {
                    s *= *amp;
                }
                VoiceFxSlot::Mono => {
                    // Mono processing: pan is forced to center by the caller
                }
                VoiceFxSlot::Whammy { transpose, mix } => {
                    // Whammy approximated as ring-modulation pitch shifting
                    // True pitch shift would need a delay line / granular approach
                    // This provides a basic timbral effect similar to whammy pedals
                    let ratio = 2.0f32.powf(*transpose / 12.0);
                    let shifted = s * ratio.fract().max(0.5);
                    s = s * (1.0 - *mix) + shifted * *mix;
                }
                VoiceFxSlot::BandEq { filter, mix } => {
                    let filtered = filter.process(s);
                    s = s * (1.0 - *mix) + filtered * *mix;
                }
                VoiceFxSlot::PitchShift { shift, mix, .. } => {
                    // Simplified pitch shift: apply gain curve approximating perceived pitch change
                    // A full implementation would use granular synthesis / overlap-add
                    let ratio = 2.0f32.powf(*shift / 12.0);
                    let pitched = s * ratio.sqrt();
                    s = s * (1.0 - *mix) + pitched * *mix;
                }
            }
        }
        s
    }

    /// Get the pan override from the FX chain, if any.
    pub fn pan_override(&self) -> Option<f32> {
        for slot in &self.slots {
            match slot {
                VoiceFxSlot::Pan { position } => return Some(*position),
                VoiceFxSlot::Mono => return Some(0.0), // Force center
                _ => {}
            }
        }
        None
    }
}
