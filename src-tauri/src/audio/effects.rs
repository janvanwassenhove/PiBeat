use std::f32::consts::PI;

// ────────────────── Biquad Filter (12 dB/octave) ──────────────────

/// Second-order biquad filter – much higher quality than one-pole.
/// Supports low-pass, high-pass, band-pass, notch, peaking, etc.
#[derive(Clone)]
struct BiquadFilter {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl BiquadFilter {
    /// Create a low-pass biquad at the given cutoff frequency with Q = 0.707 (Butterworth).
    fn low_pass(cutoff: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * PI * cutoff / sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * 0.7071);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos_w) / 2.0) / a0,
            b1: (1.0 - cos_w) / a0,
            b2: ((1.0 - cos_w) / 2.0) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// Create a high-pass biquad at the given cutoff frequency with Q = 0.707.
    fn high_pass(cutoff: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * PI * cutoff / sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * 0.7071);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w) / 2.0) / a0,
            b1: (-(1.0 + cos_w)) / a0,
            b2: ((1.0 + cos_w) / 2.0) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    fn set_low_pass(&mut self, cutoff: f32, sample_rate: f32) {
        *self = Self::low_pass(cutoff, sample_rate);
    }

    fn set_high_pass(&mut self, cutoff: f32, sample_rate: f32) {
        *self = Self::high_pass(cutoff, sample_rate);
    }

    fn process(&mut self, input: f32) -> f32 {
        let y = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
              - self.a1 * self.y1 - self.a2 * self.y2;
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
            sr * 29 / 1000,  // 29ms
            sr * 31 / 1000,  // 31ms
            sr * 37 / 1000,  // 37ms
            sr * 41 / 1000,  // 41ms
            sr * 43 / 1000,  // 43ms
            sr * 47 / 1000,  // 47ms
            sr * 53 / 1000,  // 53ms
            sr * 59 / 1000,  // 59ms
        ];
        let comb_filters: Vec<CombFilter> = comb_delays
            .iter()
            .map(|&d| CombFilter::new(d.max(1), 0.84))
            .collect();

        let allpass_delays = [
            sr * 5 / 1000,   // 5ms
            sr * 2 / 1000,   // 2ms
            sr * 1 / 1000,   // 1ms
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
        }
    }

    fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    fn process(&mut self, input: f32) -> f32 {
        // Sum of comb filter outputs
        let mut comb_sum = 0.0f32;
        for comb in self.comb_filters.iter_mut() {
            comb_sum += comb.process(input);
        }
        comb_sum /= self.comb_filters.len() as f32;

        // Apply damping low-pass to reverb tail
        let damp_coeff = 0.3;
        self.damping_lp = self.damping_lp * damp_coeff + comb_sum * (1.0 - damp_coeff);

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
}

impl EffectChain {
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 2.0) as usize; // 2 second max delay
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
        }
    }

    pub fn set_reverb_mix(&mut self, mix: f32) {
        self.reverb_l.set_mix(mix);
        self.reverb_r.set_mix(mix);
    }

    pub fn set_delay(&mut self, time: f32, feedback: f32) {
        self.delay_l.set_delay(time, self.sample_rate);
        self.delay_r.set_delay(time, self.sample_rate);
        self.delay_l.set_feedback(feedback);
        self.delay_r.set_feedback(feedback);
        self.delay_mix = if time > 0.001 { 0.5 } else { 0.0 };
    }

    pub fn set_distortion(&mut self, amount: f32) {
        self.distortion_amount = amount.clamp(0.0, 1.0);
    }

    pub fn set_lpf(&mut self, cutoff: f32) {
        if cutoff < 19999.0 {
            self.lpf_active = true;
            self.lpf_l.set_low_pass(cutoff, self.sample_rate);
            self.lpf_r.set_low_pass(cutoff, self.sample_rate);
        } else {
            self.lpf_active = false;
        }
    }

    pub fn set_hpf(&mut self, cutoff: f32) {
        if cutoff > 21.0 {
            self.hpf_active = true;
            self.hpf_l.set_high_pass(cutoff, self.sample_rate);
            self.hpf_r.set_high_pass(cutoff, self.sample_rate);
        } else {
            self.hpf_active = false;
        }
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

        (l, r)
    }
}
