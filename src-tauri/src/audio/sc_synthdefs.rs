/// SuperCollider SynthDef source code for all Sonic Pi-compatible synths.
///
/// These SynthDef definitions are written in SuperCollider language and compiled
/// by sclang at boot time. They are designed to produce the same sound as
/// Sonic Pi's built-in synths.
use std::path::Path;

/// SynthDef version — bump this when SynthDef source changes to force recompilation
///
/// v4: added `sonic_mixer` (Sonic Pi's master limiter chain) and switched every
/// synth's amplitude envelope to Sonic Pi's `shapedAdsr` form so `env_curve`,
/// `attack_level` and `decay_level` behave as documented.
pub const SYNTHDEF_VERSION: &str = "4";

/// Map our OscillatorType enum to the SC SynthDef name
pub fn synthdef_name(synth_type: &super::synth::OscillatorType) -> &'static str {
    use super::synth::OscillatorType::*;
    match synth_type {
        Sine => "sonic_beep",
        Saw => "sonic_saw",
        Square => "sonic_square",
        Triangle => "sonic_tri",
        Noise => "sonic_noise",
        Pulse => "sonic_pulse",
        SuperSaw => "sonic_supersaw",
        DSaw => "sonic_dsaw",
        DPulse => "sonic_dpulse",
        DTri => "sonic_dtri",
        FM => "sonic_fm",
        ModFM => "sonic_mod_fm",
        ModSine => "sonic_mod_sine",
        ModSaw => "sonic_mod_saw",
        ModDSaw => "sonic_mod_dsaw",
        ModTri => "sonic_mod_tri",
        ModPulse => "sonic_mod_pulse",
        TB303 => "sonic_tb303",
        Prophet => "sonic_prophet",
        Zawa => "sonic_zawa",
        Blade => "sonic_blade",
        TechSaws => "sonic_tech_saws",
        Hoover => "sonic_hoover",
        Pluck => "sonic_pluck",
        Piano => "sonic_piano",
        PrettyBell => "sonic_pretty_bell",
        DullBell => "sonic_dull_bell",
        Hollow => "sonic_hollow",
        DarkAmbience => "sonic_dark_ambience",
        Growl => "sonic_growl",
        ChipLead => "sonic_chip_lead",
        ChipBass => "sonic_chip_bass",
        ChipNoise => "sonic_chip_noise",
        BNoise => "sonic_bnoise",
        PNoise => "sonic_pnoise",
        GNoise => "sonic_gnoise",
        CNoise => "sonic_cnoise",
        SubPulse => "sonic_subpulse",
        GabberKick => "sonic_gabberkick",
    }
}

/// Generate the full SuperCollider SynthDef compilation script.
/// When run through sclang, this will write compiled .scsyndef files
/// to the specified directory.
pub fn generate_synthdef_script(output_dir: &Path) -> String {
    let dir = output_dir.to_string_lossy().replace('\\', "/");
    format!(
        r#"(
var dir = "{dir}";

// ============================================================
// SHARED AMPLITUDE ENVELOPE
// ============================================================
//
// Reproduces Sonic Pi's `shapedAdsr` (etc/synthdefs/designs/supercollider/
// beep.scd) exactly, using the raw envelope-array form:
//
//   [initialLevel, numSegments, releaseNode, loopNode,
//    level, time, shape, curve, ...]
//
// Building the array from synth arguments (rather than using `Env`) is what
// lets `env_curve` stay a runtime parameter — `Env` bakes the segment shape
// in at compile time. Sonic Pi's shape numbering applies: 1=linear (the
// default), 2=exponential, 3=sine, 4=welch, 6=squared, 7=cubed.
//
// `decay_level` defaults to -1, the sentinel Sonic Pi uses for "same as
// sustain_level", so a plain `play :c4` produces the identical four-segment
// 0 -> 1 -> sustain_level -> sustain_level -> 0 shape as before.
var spEnv = {{ |attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve|
    var dl = Select.kr(decay_level < 0, [decay_level, sustain_level]);
    EnvGen.kr(
        [0, 4, -99, -99,
            attack_level, attack, env_curve, 0,
            dl, decay, env_curve, 0,
            sustain_level, sustain, env_curve, 0,
            0, release, env_curve, 0],
        1, 1, 0, 1, 2);
}};

// ============================================================
// SYNTH DEFINITIONS - Matching Sonic Pi's built-in synths
// ============================================================

// Beep / Sine
SynthDef(\sonic_beep, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1|
    var sig = SinOsc.ar(freq);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Saw
SynthDef(\sonic_saw, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=100, res=0.3|
    var sig = Saw.ar(freq);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Square
SynthDef(\sonic_square, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=100, res=0.3|
    var sig = Pulse.ar(freq, 0.5);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Triangle
SynthDef(\sonic_tri, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1|
    var sig = LFTri.ar(freq);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Noise
SynthDef(\sonic_noise, {{ |out=0, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=110, freq=0|
    var sig = WhiteNoise.ar;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), 0.3);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Pulse (variable width)
SynthDef(\sonic_pulse, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, pulse_width=0.5, cutoff=100, res=0.3|
    var sig = Pulse.ar(freq, pulse_width);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Super Saw (7 detuned saws — fat unison supersaw)
SynthDef(\sonic_supersaw, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=130, res=0.7|
    var detunes = [-0.012, -0.008, -0.004, 0, 0.004, 0.008, 0.012];
    var sig = Mix.ar(Array.fill(7, {{ |i| Saw.ar(freq * (1 + detunes[i])) }})) / 4;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), (1 - res).clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Detuned Saw
SynthDef(\sonic_dsaw, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, detune=0.1, cutoff=100, res=0.3|
    var sig = Mix.ar([Saw.ar(freq), Saw.ar(freq * (1 + (detune * 0.01)))]) * 0.5;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Detuned Pulse
SynthDef(\sonic_dpulse, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, detune=0.1, cutoff=100, res=0.3|
    var sig = Mix.ar([Pulse.ar(freq, 0.5), Pulse.ar(freq * (1 + (detune * 0.01)), 0.5)]) * 0.5;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Detuned Tri
SynthDef(\sonic_dtri, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, detune=0.1|
    var sig = Mix.ar([LFTri.ar(freq), LFTri.ar(freq * (1 + (detune * 0.01)))]) * 0.5;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// FM Synthesis
SynthDef(\sonic_fm, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, divisor=2, depth=1|
    var modFreq = freq / divisor;
    var modulator = SinOsc.ar(modFreq) * depth * modFreq;
    var sig = SinOsc.ar(freq + modulator);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Mod FM
SynthDef(\sonic_mod_fm, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, mod_phase=1, mod_range=5, mod_pulse_width=0.5, mod_phase_offset=0, mod_invert_wave=0, mod_wave=0, divisor=2, depth=1|
    var modFreq = freq / divisor;
    var lfo = Select.kr(mod_wave, [
        SinOsc.kr(mod_phase, mod_phase_offset),
        LFSaw.kr(mod_phase, mod_phase_offset),
        LFPulse.kr(mod_phase, 0, mod_pulse_width) * 2 - 1,
        LFTri.kr(mod_phase, mod_phase_offset)
    ]);
    lfo = lfo.linlin(-1, 1, freq, freq * mod_range);
    var modulator = SinOsc.ar(modFreq) * depth * modFreq;
    var sig = SinOsc.ar(lfo + modulator);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Mod Sine
SynthDef(\sonic_mod_sine, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, mod_phase=1, mod_range=5, mod_pulse_width=0.5, mod_phase_offset=0, mod_wave=0|
    var lfo = Select.kr(mod_wave, [
        SinOsc.kr(mod_phase, mod_phase_offset),
        LFSaw.kr(mod_phase, mod_phase_offset),
        LFPulse.kr(mod_phase, 0, mod_pulse_width) * 2 - 1,
        LFTri.kr(mod_phase, mod_phase_offset)
    ]);
    var modulated_freq = freq * (1 + (lfo * mod_range * 0.01));
    var sig = SinOsc.ar(modulated_freq);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Mod Saw
SynthDef(\sonic_mod_saw, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, mod_phase=1, mod_range=5, mod_pulse_width=0.5, mod_phase_offset=0, mod_wave=0, cutoff=100, res=0.3|
    var lfo = Select.kr(mod_wave, [
        SinOsc.kr(mod_phase, mod_phase_offset),
        LFSaw.kr(mod_phase, mod_phase_offset),
        LFPulse.kr(mod_phase, 0, mod_pulse_width) * 2 - 1,
        LFTri.kr(mod_phase, mod_phase_offset)
    ]);
    var modulated_freq = freq * (1 + (lfo * mod_range * 0.01));
    var sig = Saw.ar(modulated_freq);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Mod DSaw
SynthDef(\sonic_mod_dsaw, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, mod_phase=1, mod_range=5, mod_pulse_width=0.5, mod_phase_offset=0, mod_wave=0, detune=0.1, cutoff=100, res=0.3|
    var lfo = Select.kr(mod_wave, [
        SinOsc.kr(mod_phase, mod_phase_offset),
        LFSaw.kr(mod_phase, mod_phase_offset),
        LFPulse.kr(mod_phase, 0, mod_pulse_width) * 2 - 1,
        LFTri.kr(mod_phase, mod_phase_offset)
    ]);
    var modulated_freq = freq * (1 + (lfo * mod_range * 0.01));
    var sig = Mix.ar([Saw.ar(modulated_freq), Saw.ar(modulated_freq * (1 + (detune * 0.01)))]) * 0.5;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Mod Tri
SynthDef(\sonic_mod_tri, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, mod_phase=1, mod_range=5, mod_pulse_width=0.5, mod_phase_offset=0, mod_wave=0|
    var lfo = Select.kr(mod_wave, [
        SinOsc.kr(mod_phase, mod_phase_offset),
        LFSaw.kr(mod_phase, mod_phase_offset),
        LFPulse.kr(mod_phase, 0, mod_pulse_width) * 2 - 1,
        LFTri.kr(mod_phase, mod_phase_offset)
    ]);
    var modulated_freq = freq * (1 + (lfo * mod_range * 0.01));
    var sig = LFTri.ar(modulated_freq);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Mod Pulse
SynthDef(\sonic_mod_pulse, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, mod_phase=1, mod_range=5, mod_pulse_width=0.5, mod_phase_offset=0, mod_wave=0, cutoff=100, res=0.3|
    var lfo = Select.kr(mod_wave, [
        SinOsc.kr(mod_phase, mod_phase_offset),
        LFSaw.kr(mod_phase, mod_phase_offset),
        LFPulse.kr(mod_phase, 0, mod_pulse_width) * 2 - 1,
        LFTri.kr(mod_phase, mod_phase_offset)
    ]);
    var modulated_freq = freq * (1 + (lfo * mod_range * 0.01));
    var sig = Pulse.ar(modulated_freq, mod_pulse_width);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// TB-303 (acid bass)
SynthDef(\sonic_tb303, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=100, res=0.8, wave=0|
    var sig = Select.ar(wave, [Saw.ar(freq), Pulse.ar(freq, 0.5)]);
    var fenv = EnvGen.kr(Env.perc(0.001, release * 2), 1, cutoff.midicps * 2, cutoff.midicps * 0.5);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, fenv.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Prophet (detuned saws + pulse)
SynthDef(\sonic_prophet, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=110, res=0.7|
    var sig = Mix.ar([
        Saw.ar(freq, 0.5),
        Pulse.ar(freq * 1.002, 0.4, 0.4),
        Pulse.ar(freq * 0.998, 0.6, 0.3)
    ]);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Zawa (phase modulation synth)
SynthDef(\sonic_zawa, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=100, res=0.9, phase=1, wave=3|
    var modulator = SinOsc.ar(freq * phase) * 2pi;
    var sig = Select.ar(wave, [
        SinOsc.ar(freq, modulator),
        Saw.ar(freq),
        Pulse.ar(freq, 0.5),
        LFTri.ar(freq)
    ]);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Blade (thick detuned saws)
SynthDef(\sonic_blade, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=100, res=0.5|
    var sig = Mix.ar(Array.fill(8, {{ |i|
        var detune = (i - 3.5) * 0.007;
        Saw.ar(freq * (1 + detune));
    }})) / 4;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Tech Saws (5 layered saws)
SynthDef(\sonic_tech_saws, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=130, res=0.3|
    var sig = Mix.ar(Array.fill(5, {{ |i|
        Saw.ar(freq * (1 + (i * 0.01)));
    }})) / 3;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Hoover (classic rave synth)
SynthDef(\sonic_hoover, {{ |out=0, freq=440, amp=1, pan=0, attack=0.05, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=130|
    var sig = Mix.ar([
        Saw.ar(freq, 0.3),
        Saw.ar(freq * 1.01, 0.3),
        Saw.ar(freq * 0.99, 0.3),
        Pulse.ar(freq * 0.5, 0.5, 0.2)
    ]);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), 0.5);
    sig = FreeVerb.ar(sig, 0.3, 0.5, 0.5);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Pluck (Karplus-Strong)
SynthDef(\sonic_pluck, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, coef=0.3|
    var sig = Pluck.ar(WhiteNoise.ar, 1, 0.2, freq.reciprocal, release * 5, coef);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Piano (additive harmonics)
SynthDef(\sonic_piano, {{ |out=0, freq=440, amp=1, pan=0, attack=0.01, decay=0, sustain=0, release=0.5, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, vel=0.8|
    var sig = Mix.ar(Array.fill(8, {{ |i|
        var partial = i + 1;
        SinOsc.ar(freq * partial, 0, 1.0 / (partial * partial));
    }}));
    var env = EnvGen.kr(Env.perc(attack, release), doneAction: 2);
    sig = sig * env * vel * amp;
    Out.ar(out, Pan2.ar(sig, pan));
}}).writeDefFile(dir);

// Pretty Bell (inharmonic partials)
SynthDef(\sonic_pretty_bell, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1.5, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1|
    var partials = [1, 2.4, 3.1, 4.7, 6.2];
    var sig = Mix.ar(partials.collect({{ |p|
        SinOsc.ar(freq * p, 0, 1.0 / p);
    }}));
    var env = EnvGen.kr(Env.perc(attack, release), doneAction: 2);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Dull Bell
SynthDef(\sonic_dull_bell, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1.5, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1|
    var partials = [1, 2.0, 2.5, 3.2, 4.0];
    var sig = Mix.ar(partials.collect({{ |p|
        SinOsc.ar(freq * p, 0, 1.0 / (p * p));
    }}));
    var env = EnvGen.kr(Env.perc(attack, release), doneAction: 2);
    sig = LPF.ar(sig, 2000);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Hollow (band-pass filtered)
SynthDef(\sonic_hollow, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=90, res=0.99|
    var sig = Mix.ar([SinOsc.ar(freq), PinkNoise.ar(0.3)]);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = BPF.ar(sig, freq, 1 - res.clip(0.01, 0.99));
    sig = sig * 4;
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Dark Ambience (atmospheric pad)
SynthDef(\sonic_dark_ambience, {{ |out=0, freq=52, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=90, res=0.7, detune=12, noise=0, room=70, reverb_time=100|
    var sig = Mix.ar([
        Saw.ar(freq * (1 + (detune * 0.001)), 0.3),
        Saw.ar(freq * (1 - (detune * 0.001)), 0.3),
        SinOsc.ar(freq * 0.5, 0, 0.2),
        PinkNoise.ar(0.08 + (noise * 0.1))
    ]);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    sig = FreeVerb.ar(sig, 0.7, room / 100, 0.5);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Growl (ring modulated)
SynthDef(\sonic_growl, {{ |out=0, freq=440, amp=1, pan=0, attack=0.1, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=130|
    var mod = SinOsc.ar(freq * 0.5);
    var sig = SinOsc.ar(freq) * mod;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), 0.3);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Chip Lead
SynthDef(\sonic_chip_lead, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, width=0|
    var sig = Pulse.ar(freq, (width * 0.5) + 0.5);
    sig = sig.round(0.125);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Chip Bass
SynthDef(\sonic_chip_bass, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1|
    var sig = Pulse.ar(freq, 0.5) + Pulse.ar(freq * 0.5, 0.5);
    sig = sig.round(0.125) * 0.5;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Chip Noise
SynthDef(\sonic_chip_noise, {{ |out=0, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, freq=440|
    var sig = LFNoise0.ar(freq * 4);
    sig = sig.round(0.125);
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Brown Noise
SynthDef(\sonic_bnoise, {{ |out=0, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, freq=0|
    var sig = BrownNoise.ar;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Pink Noise
SynthDef(\sonic_pnoise, {{ |out=0, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, freq=0|
    var sig = PinkNoise.ar;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Grey Noise
SynthDef(\sonic_gnoise, {{ |out=0, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, freq=0|
    var sig = GrayNoise.ar;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Clip Noise
SynthDef(\sonic_cnoise, {{ |out=0, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, freq=0|
    var sig = ClipNoise.ar;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);

// Sub Pulse
SynthDef(\sonic_subpulse, {{ |out=0, freq=440, amp=1, pan=0, attack=0, decay=0, sustain=0, release=1, sustain_level=1, attack_level=1, decay_level=(-1), env_curve=1, cutoff=100, res=0.3|
    var sig = Pulse.ar(freq, 0.5) + SinOsc.ar(freq * 0.5, 0, 0.6);
    sig = sig * 0.5;
    var env = spEnv.(attack, decay, sustain, release, sustain_level, attack_level, decay_level, env_curve);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    Out.ar(out, Pan2.ar(sig * env * amp, pan));
}}).writeDefFile(dir);


// ============================================================
// SAMPLE PLAYBACK SYNTHDEFS
// ============================================================

// Mono sample player
SynthDef(\sonic_playbuf, {{ |out=0, buf=0, amp=1, rate=1, pan=0|
    var sig = PlayBuf.ar(1, buf, BufRateScale.kr(buf) * rate, doneAction: 2);
    Out.ar(out, Pan2.ar(sig * amp, pan));
}}).writeDefFile(dir);

// Stereo sample player
SynthDef(\sonic_playbuf2, {{ |out=0, buf=0, amp=1, rate=1, pan=0|
    var sig = PlayBuf.ar(2, buf, BufRateScale.kr(buf) * rate, doneAction: 2);
    sig = Balance2.ar(sig[0], sig[1], pan) * amp;
    Out.ar(out, sig);
}}).writeDefFile(dir);


// ============================================================
// FX SYNTHDEFS
// ============================================================

// Reverb (FreeVerb2 - high quality stereo reverb)
SynthDef(\sonic_fx_reverb, {{ |out=0, in_bus=0, mix=0.4, room=0.6, damp=0.5|
    var sig = In.ar(in_bus, 2);
    var wet = FreeVerb2.ar(sig[0], sig[1], mix, room, damp);
    DetectSilence.ar(Mix.ar(wet), 0.0001, 2, doneAction: 2);
    Out.ar(out, wet);
}}).writeDefFile(dir);

// Slicer (rhythmic gating)
SynthDef(\sonic_fx_slicer, {{ |out=0, in_bus=0, phase=0.25, wave=0, probability=1, smooth=0, amp=1|
    var sig = In.ar(in_bus, 2);
    var rate = phase.reciprocal;
    var lfo = Select.kr(wave, [
        LFSaw.kr(rate, 1).range(0, 1),
        LFPulse.kr(rate),
        SinOsc.kr(rate).range(0, 1),
        LFTri.kr(rate).range(0, 1)
    ]);
    lfo = lfo.lag(smooth);
    sig = sig * lfo * amp;
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Distortion (soft clipping)
SynthDef(\sonic_fx_distortion, {{ |out=0, in_bus=0, distort=0.5|
    var sig = In.ar(in_bus, 2);
    sig = (sig * (1 + (distort * 50))).tanh;
    sig = sig * (1 + distort).reciprocal;
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Echo / Delay
SynthDef(\sonic_fx_echo, {{ |out=0, in_bus=0, phase=0.25, decay=2, mix=1|
    var sig = In.ar(in_bus, 2);
    var delayed = CombL.ar(sig, 2, phase, decay);
    var mixed = ((1 - mix) * sig) + (mix * delayed);
    DetectSilence.ar(Mix.ar(mixed), 0.0001, 3, doneAction: 2);
    Out.ar(out, mixed);
}}).writeDefFile(dir);

// Low-pass filter
SynthDef(\sonic_fx_lpf, {{ |out=0, in_bus=0, cutoff=100|
    var sig = In.ar(in_bus, 2);
    sig = RLPF.ar(sig, cutoff.midicps.min(SampleRate.ir * 0.45), 0.5);
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// High-pass filter
SynthDef(\sonic_fx_hpf, {{ |out=0, in_bus=0, cutoff=0|
    var sig = In.ar(in_bus, 2);
    sig = RHPF.ar(sig, cutoff.midicps.max(20), 0.5);
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Flanger
SynthDef(\sonic_fx_flanger, {{ |out=0, in_bus=0, phase=4, depth=5, feedback=0, decay=2|
    var sig = In.ar(in_bus, 2);
    var delay = SinOsc.kr(phase.reciprocal).range(0.001, depth * 0.001);
    var delayed = CombL.ar(sig, 0.02, delay, decay * feedback);
    var out_sig = sig + delayed;
    DetectSilence.ar(Mix.ar(out_sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, out_sig);
}}).writeDefFile(dir);

// Compressor
SynthDef(\sonic_fx_compressor, {{ |out=0, in_bus=0, threshold=0.2, clamp_time=0.01, slope_above=0.5, relax_time=0.01|
    var sig = In.ar(in_bus, 2);
    sig = Compander.ar(sig, sig, threshold, 1, slope_above, clamp_time, relax_time);
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Bitcrusher (sample rate and bit depth reduction)
SynthDef(\sonic_fx_bitcrusher, {{ |out=0, in_bus=0, bits=8, sample_rate=8000|
    var sig = In.ar(in_bus, 2);
    var crushed = sig.round(2.pow(1 - bits));
    sig = Latch.ar(crushed, Impulse.ar(sample_rate));
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Pan (stereo panning effect)
SynthDef(\sonic_fx_pan, {{ |out=0, in_bus=0, pan=0|
    var sig = In.ar(in_bus, 2);
    var mono = Mix.ar(sig);
    var panned = Pan2.ar(mono, pan);
    DetectSilence.ar(mono, 0.0001, 2, doneAction: 2);
    Out.ar(out, panned);
}}).writeDefFile(dir);

// Wobble (LFO-modulated filter)
SynthDef(\sonic_fx_wobble, {{ |out=0, in_bus=0, phase=0.5, cutoff_min=60, cutoff_max=120, res=0.3|
    var sig = In.ar(in_bus, 2);
    var lfo = SinOsc.kr(phase.reciprocal).range(cutoff_min.midicps, cutoff_max.midicps);
    sig = RLPF.ar(sig, lfo.min(SampleRate.ir * 0.45), res.clip(0.01, 0.99));
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Tremolo (amplitude modulation)
SynthDef(\sonic_fx_tremolo, {{ |out=0, in_bus=0, phase=4, depth=0.5, wave=2|
    var sig = In.ar(in_bus, 2);
    var rate = phase.reciprocal;
    var lfo = Select.kr(wave, [
        SinOsc.kr(rate).range(1 - depth, 1),
        LFSaw.kr(rate).range(1 - depth, 1),
        LFTri.kr(rate).range(1 - depth, 1),
        LFPulse.kr(rate).range(1 - depth, 1)
    ]);
    sig = sig * lfo;
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Normaliser (peak limiter / normalizer)
SynthDef(\sonic_fx_normaliser, {{ |out=0, in_bus=0, level=1|
    var sig = In.ar(in_bus, 2);
    sig = Normalizer.ar(sig, level);
    DetectSilence.ar(Mix.ar(sig), 0.0001, 2, doneAction: 2);
    Out.ar(out, sig);
}}).writeDefFile(dir);

// Chorus (3-voice detuned delays)
SynthDef(\sonic_fx_chorus, {{ |out=0, in_bus=0, rate=0.3, depth=0.5, mix=1|
    var sig = In.ar(in_bus, 2);
    var chorus = Array.fill(3, {{ |i|
        var delay_t = SinOsc.kr(rate * (1 + (i * 0.1)), i * 2pi/3).range(0.015, 0.015 + (depth * 0.01));
        DelayL.ar(sig, 0.05, delay_t);
    }}).sum / 3;
    var mixed = ((1 - mix) * sig) + (mix * chorus);
    DetectSilence.ar(Mix.ar(mixed), 0.0001, 2, doneAction: 2);
    Out.ar(out, mixed);
}}).writeDefFile(dir);

// Ring modulator
SynthDef(\sonic_fx_ring_mod, {{ |out=0, in_bus=0, freq=30, mix=1|
    var sig = In.ar(in_bus, 2);
    var mod = SinOsc.ar(freq);
    var ring = sig * mod;
    var mixed = ((1 - mix) * sig) + (mix * ring);
    DetectSilence.ar(Mix.ar(mixed), 0.0001, 2, doneAction: 2);
    Out.ar(out, mixed);
}}).writeDefFile(dir);

// Octaver (sub-octave + super-octave)
SynthDef(\sonic_fx_octaver, {{ |out=0, in_bus=0, mix=1, sub_amp=0.5, super_amp=0|
    var sig = In.ar(in_bus, 2);
    var mono = Mix.ar(sig);
    var sub = (PulseDivider.ar(mono, 2) * 2 - 1) * sub_amp;
    var sup = (mono * mono * 4 - 1).clip2(1) * super_amp;
    var oct = Pan2.ar(mono + sub + sup, 0);
    var mixed = ((1 - mix) * sig) + (mix * oct);
    DetectSilence.ar(Mix.ar(mixed), 0.0001, 2, doneAction: 2);
    Out.ar(out, mixed);
}}).writeDefFile(dir);

// ============================================================
// MASTER MIXER
// ============================================================
//
// Port of Sonic Pi's `sonic-pi-mixer` master chain. Sonic Pi never routes a
// synth straight to the sound card — the whole mix goes through this stage,
// and its limiter is a large part of why dense Sonic Pi material stays
// composed where a raw sum would clip.
//
// Chain (matching sonic-pi-mixer's ordering):
//   sanitize (NaN/Inf -> silence) -> pre_amp -> amp -> LeakDC
//     -> Limiter(0.99, 0.01) -> clip2(1) -> HPF 10Hz -> LPF 20.5kHz
//
// Differences from upstream, all forced by staying on stock UGens:
//   * `Sanitize` is a Sonic Pi UGen; CheckBadValues + Select reproduces it.
//   * `ScopeOut2` (the pre-limiter scope tap) is dropped — PiBeat's scope
//     reads the post-mixer bus instead.
//   * The switchable master HPF/LPF and force_mono/invert_stereo controls are
//     omitted; PiBeat exposes none of them.
//
// Reads and ReplaceOut's the same bus, so it must sit after every source and
// FX node and before the scope/meter.
SynthDef(\sonic_mixer, {{ |bus=0, out=0, pre_amp=1, amp=1, limiter_bypass=0, leak_dc_bypass=0|
    var sanitize = {{ |sig| Select.ar(CheckBadValues.ar(sig, 0, 0) > 0, [sig, DC.ar(0)]) }};
    var l = sanitize.(In.ar(bus)) * pre_amp;
    var r = sanitize.(In.ar(bus + 1)) * pre_amp;

    l = l * amp;
    r = r * amp;

    l = Select.ar(leak_dc_bypass, [LeakDC.ar(l), l]);
    r = Select.ar(leak_dc_bypass, [LeakDC.ar(r), r]);

    l = Select.ar(limiter_bypass, [Limiter.ar(l, 0.99, 0.01), l]);
    r = Select.ar(limiter_bypass, [Limiter.ar(r, 0.99, 0.01), r]);

    l = l.clip2(1);
    r = r.clip2(1);

    l = LPF.ar(HPF.ar(l, 10), 20500);
    r = LPF.ar(HPF.ar(r, 10), 20500);

    ReplaceOut.ar(bus, [l, r]);
}}).writeDefFile(dir);

// Waveform monitor - writes output to a buffer for visualization
SynthDef(\sonic_scope, {{ |out=0, buf=0|
    var sig = In.ar(0, 1);
    BufWr.ar(sig, buf, Phasor.ar(0, 1, 0, BufFrames.kr(buf)));
}}).writeDefFile(dir);

// Amplitude monitor - sends amplitude back via OSC for is_playing detection
SynthDef(\sonic_meter, {{ |out=0|
    var sig = In.ar(0, 2);
    var amp_l = Amplitude.kr(sig[0], 0.01, 0.1);
    var amp_r = Amplitude.kr(sig[1], 0.01, 0.1);
    SendReply.kr(Impulse.kr(30), '/sonic/meter', [amp_l, amp_r]);
}}).writeDefFile(dir);


"SynthDefs compiled successfully".postln;
0.exit;
)
"#
    )
}

/// Check if compiled SynthDef files already exist in the directory
/// and are up-to-date (matching the current SYNTHDEF_VERSION).
pub fn synthdefs_exist(dir: &Path) -> bool {
    if !dir.exists() {
        eprintln!("[SC] SynthDefs dir does not exist: {}", dir.display());
        return false;
    }
    // Check version marker — if outdated, force recompilation
    let version_file = dir.join("pibeat_synthdef_version.txt");
    let current_version = std::fs::read_to_string(&version_file).unwrap_or_default();
    if current_version.trim() != SYNTHDEF_VERSION {
        eprintln!(
            "[SC] SynthDef version mismatch (have '{}', need '{}') — recompilation needed",
            current_version.trim(),
            SYNTHDEF_VERSION
        );
        return false;
    }
    // Check for at least a few core SynthDef files
    let required = [
        "sonic_beep.scsyndef",
        "sonic_saw.scsyndef",
        "sonic_tb303.scsyndef",
    ];
    let all_exist = required.iter().all(|name| dir.join(name).exists());
    if !all_exist {
        let missing: Vec<_> = required
            .iter()
            .filter(|name| !dir.join(name).exists())
            .collect();
        eprintln!("[SC] Missing SynthDefs in {}: {:?}", dir.display(), missing);
    }
    all_exist
}

/// Write the version marker after successful compilation
pub fn write_version_marker(dir: &Path) {
    let version_file = dir.join("pibeat_synthdef_version.txt");
    if let Err(e) = std::fs::write(&version_file, SYNTHDEF_VERSION) {
        eprintln!("[SC] Failed to write SynthDef version marker: {}", e);
    }
}
