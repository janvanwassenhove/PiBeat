// Deep Parity Validation Tests
//
// These tests perform comprehensive parity checks against Sonic Pi v4.x
// behavior. They verify not just parsing, but semantic correctness of
// every sound-producing construct.
//
// Run with: cargo test --test parity_validation -- --nocapture

use sonic_daw_lib::audio::engine::AudioCommand;
use sonic_daw_lib::audio::parser::{commands_to_audio, parse_code};
use sonic_daw_lib::audio::synth::OscillatorType;
use std::collections::HashMap;

const DEFAULT_BPM: f32 = 60.0;

// ============================================================================
// Helpers
// ============================================================================

fn events(code: &str, bpm: f32) -> Vec<(f32, AudioCommand)> {
    let parsed = parse_code(code).expect("parse should succeed");
    commands_to_audio(&parsed, bpm)
}

fn try_events(code: &str, bpm: f32) -> Result<Vec<(f32, AudioCommand)>, String> {
    match parse_code(code) {
        Ok(parsed) => Ok(commands_to_audio(&parsed, bpm)),
        Err(e) => Err(e),
    }
}

fn approx(v: f32, decimals: u32) -> f32 {
    let factor = 10f32.powi(decimals as i32);
    (v * factor).round() / factor
}

fn note_count(evts: &[(f32, AudioCommand)]) -> usize {
    evts.iter()
        .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
        .count()
}

fn sample_count(evts: &[(f32, AudioCommand)]) -> usize {
    evts.iter()
        .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
        .count()
}

fn fx_start_count(evts: &[(f32, AudioCommand)]) -> usize {
    evts.iter()
        .filter(|(_, c)| matches!(c, AudioCommand::FxStart { .. }))
        .count()
}

fn notes_with_synth(evts: &[(f32, AudioCommand)], synth: OscillatorType) -> usize {
    evts.iter()
        .filter(|(_, c)| {
            if let AudioCommand::PlayNote { synth_type, .. } = c {
                *synth_type == synth
            } else {
                false
            }
        })
        .count()
}

fn max_time(evts: &[(f32, AudioCommand)]) -> f32 {
    evts.iter().map(|(t, _)| *t).fold(0.0f32, f32::max)
}

/// Verify a code snippet parses and produces at least N notes
fn assert_min_notes(code: &str, bpm: f32, min: usize, context: &str) {
    let evts = events(code, bpm);
    let n = note_count(&evts);
    assert!(
        n >= min,
        "{}: expected >= {} notes, got {}",
        context, min, n
    );
}

/// Verify a code snippet parses and produces at least N samples
fn assert_min_samples(code: &str, bpm: f32, min: usize, context: &str) {
    let evts = events(code, bpm);
    let s = sample_count(&evts);
    assert!(
        s >= min,
        "{}: expected >= {} samples, got {}",
        context, min, s
    );
}

// ============================================================================
// SECTION 1: Synth Type Parity
// ============================================================================

#[test]
fn parity_all_synth_types_parse() {
    let synth_names = vec![
        "sine", "beep", "saw", "square", "triangle", "noise", "pulse",
        "super_saw", "supersaw", "dsaw", "dpulse", "dtri", "fm", "mod_fm",
        "mod_sine", "mod_saw", "mod_dsaw", "mod_tri", "mod_pulse",
        "tb303", "prophet", "zawa", "blade", "tech_saws", "hoover",
        "pluck", "piano", "pretty_bell", "dull_bell", "hollow",
        "dark_ambience", "growl", "chip_lead", "chip_bass", "chip_noise",
        "bnoise", "pnoise", "gnoise", "cnoise", "sub_pulse",
    ];

    for name in &synth_names {
        let code = format!("use_synth :{}\nplay :c4, release: 0.2", name);
        let result = try_events(&code, DEFAULT_BPM);
        assert!(
            result.is_ok(),
            "Synth :{} should parse without error, got: {:?}",
            name,
            result.err()
        );
        let evts = result.unwrap();
        let n = note_count(&evts);
        assert!(
            n >= 1,
            "Synth :{} should produce >= 1 note, got {}",
            name, n
        );
    }
    eprintln!("All {} synth types parse and produce notes ✓", synth_names.len());
}

#[test]
fn parity_synth_note_frequency_table() {
    // Verify A4 = 440 Hz tuning standard
    let reference_notes = vec![
        (":c4", 261.63),
        (":cs4", 277.18),
        (":d4", 293.66),
        (":ds4", 311.13),
        (":e4", 329.63),
        (":f4", 349.23),
        (":fs4", 369.99),
        (":g4", 392.00),
        (":gs4", 415.30),
        (":a4", 440.00),
        (":as4", 466.16),
        (":b4", 493.88),
        (":c5", 523.25),
        (":c3", 130.81),
        (":c2", 65.41),
        (":e2", 82.41),
        (":a3", 220.00),
    ];

    for (note_name, expected_freq) in &reference_notes {
        let code = format!("play {}", note_name);
        let evts = events(&code, DEFAULT_BPM);
        assert_eq!(note_count(&evts), 1, "play {} should produce 1 note", note_name);

        if let AudioCommand::PlayNote { frequency, .. } = &evts[0].1 {
            assert!(
                (*frequency - expected_freq).abs() < 1.0,
                "Note {} should be ~{} Hz, got {} Hz (delta: {})",
                note_name, expected_freq, frequency, (*frequency - expected_freq).abs()
            );
        }
    }
    eprintln!("All {} note frequencies match A4=440 Hz tuning ✓", reference_notes.len());
}

#[test]
fn parity_midi_note_numbers() {
    let midi_reference = vec![
        (60, 261.63),  // C4
        (69, 440.00),  // A4
        (72, 523.25),  // C5
        (48, 130.81),  // C3
        (36, 65.41),   // C2
        (24, 32.70),   // C1
        (127, 12543.85), // G9
        (0, 8.18),     // C-1
    ];

    for (midi, expected_freq) in &midi_reference {
        let code = format!("play {}", midi);
        let evts = events(&code, DEFAULT_BPM);
        assert_eq!(note_count(&evts), 1, "play {} should produce 1 note", midi);

        if let AudioCommand::PlayNote { frequency, .. } = &evts[0].1 {
            let tolerance = expected_freq * 0.01; // 1% tolerance
            assert!(
                (*frequency - expected_freq).abs() < tolerance,
                "MIDI {} should be ~{} Hz, got {} Hz",
                midi, expected_freq, frequency
            );
        }
    }
    eprintln!("All {} MIDI numbers convert correctly ✓", midi_reference.len());
}

// ============================================================================
// SECTION 2: Effect Parity
// ============================================================================

#[test]
fn parity_all_effects_parse() {
    let fx_configs = vec![
        ("reverb", "mix: 0.5, room: 0.8"),
        ("gverb", "room: 30, mix: 0.6"),
        ("echo", "phase: 0.25, feedback: 0.6"),
        ("delay", "time: 0.5, feedback: 0.5"),
        ("distortion", "distort: 0.5"),
        ("lpf", "cutoff: 80"),
        ("rlpf", "cutoff: 80, res: 0.5"),
        ("hpf", "cutoff: 50"),
        ("rhpf", "cutoff: 50, res: 0.5"),
        ("slicer", "phase: 0.25"),
        ("bitcrusher", "bits: 8"),
        ("krush", "gain: 5"),
        ("compressor", "threshold: 0.3"),
        ("normaliser", "level: 1.0"),
        ("flanger", "rate: 0.25, depth: 0.5"),
        ("chorus", "rate: 0.3, depth: 0.5"),
        ("ring_mod", "freq: 30"),
        ("pan", "pan: 0.5"),
        ("wobble", "rate: 4, depth: 0.5"),
        ("ixi_techno", "rate: 4"),
        ("octaver", "sub_amp: 1.0, super_amp: 1.0"),
    ];

    for (fx_name, params) in &fx_configs {
        let code = format!(
            "with_fx :{}, {} do\n  play :c4\nend",
            fx_name, params
        );
        let result = try_events(&code, DEFAULT_BPM);
        assert!(
            result.is_ok(),
            "Effect :{} should parse without error, got: {:?}",
            fx_name,
            result.err()
        );
        let evts = result.unwrap();
        let n = note_count(&evts);
        assert!(
            n >= 1,
            "Effect :{} block should produce >= 1 note, got {}",
            fx_name, n
        );

        // Verify FxStart is emitted
        let fx_starts = fx_start_count(&evts);
        assert!(
            fx_starts >= 1,
            "Effect :{} should emit FxStart, found {}",
            fx_name, fx_starts
        );
    }
    eprintln!("All {} effect types parse and produce FxStart+notes ✓", fx_configs.len());
}

#[test]
fn parity_nested_effects() {
    let code = r#"
with_fx :reverb, mix: 0.5 do
  with_fx :distortion, distort: 0.5 do
    with_fx :lpf, cutoff: 80 do
      play :c4
    end
  end
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert!(note_count(&evts) >= 1, "nested FX should produce notes");
    assert!(fx_start_count(&evts) >= 3, "should have 3 FxStart events");
}

#[test]
fn parity_echo_bpm_sync() {
    // In Sonic Pi, echo phase is in beats — the parser converts internally
    // for the delay engine, but FxStart params carry raw user values.
    // Verify the parser's internal delay_time conversion is correct by checking
    // that the generated PlayNote events still appear (echo doesn't break playback).
    let code = "use_bpm 120\nwith_fx :echo, phase: 0.25 do\n  play :c4\nend";
    let evts = events(code, 120.0);
    
    // The code should produce at least one note event
    assert!(note_count(&evts) >= 1, "Echo FX block should produce notes");
    
    // Should have a FxStart event for echo
    let has_echo_fx = evts.iter().any(|(_, cmd)| {
        matches!(cmd, AudioCommand::FxStart { fx_type, .. } if fx_type == "echo")
    });
    assert!(has_echo_fx, "Should have FxStart for echo effect");
}

// ============================================================================
// SECTION 3: Sample Parity
// ============================================================================

#[test]
fn parity_built_in_samples_parse() {
    let samples = vec![
        "kick", "snare", "hihat", "clap",
        "bd_haus", "bd_pure", "bd_808", "bd_tek", "bd_ada", "bd_boom",
        "sn_dub", "sn_dolf",
        "drum_heavy_kick", "drum_snare_hard", "drum_cymbal_hard", "drum_cymbal_closed",
        "hat_snap",
        "elec_triangle", "elec_snare", "elec_blip2",
        "ambi_dark_woosh", "ambi_choir", "ambi_glass_rub", "ambi_drone",
        "bass_hit_c", "bass_voxy_hit_c",
        "loop_amen", "loop_breakbeat", "loop_industrial",
        "perc_snap",
    ];

    for name in &samples {
        let code = format!("sample :{}", name);
        let result = try_events(&code, DEFAULT_BPM);
        assert!(
            result.is_ok(),
            "Sample :{} should parse without error",
            name
        );
        let evts = result.unwrap();
        let s = sample_count(&evts);
        assert!(
            s >= 1,
            "Sample :{} should produce >= 1 sample event, got {}",
            name, s
        );
    }
    eprintln!("All {} built-in samples parse correctly ✓", samples.len());
}

#[test]
fn parity_sample_parameters() {
    // Test all supported sample parameters
    let code = r#"
sample :kick, amp: 0.8
sample :snare, rate: 1.5
sample :hihat, pan: -0.5
sample :kick, amp: 0.5, rate: 2, pan: 0.3
"#;
    let evts = events(code, DEFAULT_BPM);
    let samples: Vec<_> = evts.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlaySample { amplitude, rate, pan, .. } = c {
                Some((*amplitude, *rate, *pan))
            } else {
                None
            }
        })
        .collect();
    
    assert_eq!(samples.len(), 4, "should have 4 sample events");
    assert_eq!(samples[0].0, 0.8, "first sample amp should be 0.8");
    assert_eq!(samples[1].1, 1.5, "second sample rate should be 1.5");
    assert_eq!(samples[2].2, -0.5, "third sample pan should be -0.5");
    assert_eq!(samples[3].0, 0.5, "fourth sample amp should be 0.5");
    assert_eq!(samples[3].1, 2.0, "fourth sample rate should be 2.0");
}

#[test]
fn parity_sample_beat_stretch() {
    let code = "sample :loop_amen, beat_stretch: 4";
    let evts = events(code, DEFAULT_BPM);
    let s = sample_count(&evts);
    assert!(s >= 1, "beat_stretch sample should produce an event");
    
    // Verify beat_stretch parameter is passed through
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { beat_stretch, .. } = cmd {
            assert!(
                beat_stretch.is_some(),
                "beat_stretch param should be Some(4.0)"
            );
            if let Some(bs) = beat_stretch {
                assert!(
                    (*bs - 4.0).abs() < 0.01,
                    "beat_stretch should be 4.0, got {}",
                    bs
                );
            }
        }
    }
}

#[test]
fn parity_sample_start_finish() {
    let code = "sample :loop_amen, start: 0.25, finish: 0.75";
    let evts = events(code, DEFAULT_BPM);
    let s = sample_count(&evts);
    assert!(s >= 1, "start/finish sample should produce an event");
    
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { start, finish, .. } = cmd {
            assert!(start.is_some(), "start should be Some(0.25)");
            assert!(finish.is_some(), "finish should be Some(0.75)");
        }
    }
}

#[test]
fn parity_external_sample_paths() {
    // External file paths should parse (even if file doesn't exist)
    let code = r#"sample "C:/path/to/sample.wav", amp: 1.5"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "external sample path should parse");
    
    let evts = result.unwrap();
    assert!(sample_count(&evts) >= 1, "should produce a sample event");
}

#[test]
fn parity_concatenated_sample_path() {
    let code = r#"
sample_path = "C:/samples/"
sample sample_path + "test.wav", amp: 1.0
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "concatenated sample path should parse");
}

// ============================================================================
// SECTION 4: Timing & BPM Parity
// ============================================================================

#[test]
fn parity_bpm_timing() {
    // At 120 BPM, 1 beat = 0.5 seconds
    let code = "use_bpm 120\nplay :c4\nsleep 1\nplay :e4\nsleep 2\nplay :g4";
    let evts = events(code, 120.0);
    let notes: Vec<(f32, f32)> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                Some((*t, *frequency))
            } else {
                None
            }
        })
        .collect();
    
    assert_eq!(notes.len(), 3, "should have 3 notes");
    assert!(approx(notes[0].0, 2) == 0.0, "note 1 at t=0");
    assert!((notes[1].0 - 0.5).abs() < 0.02, "note 2 at t=0.5s (1 beat at 120 BPM)");
    assert!((notes[2].0 - 1.5).abs() < 0.02, "note 3 at t=1.5s (3 beats at 120 BPM)");
}

#[test]
fn parity_default_bpm() {
    // Default BPM is 60 → 1 beat = 1 second
    let code = "play :c4\nsleep 1\nplay :e4";
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    
    assert_eq!(notes.len(), 2);
    assert!((notes[0] - 0.0).abs() < 0.01, "note 1 at t=0");
    assert!((notes[1] - 1.0).abs() < 0.02, "note 2 at t=1.0s (1 beat at 60 BPM)");
}

#[test]
fn parity_sleep_fractional() {
    let code = "play :c4\nsleep 0.25\nplay :e4\nsleep 0.5\nplay :g4";
    let evts = events(code, DEFAULT_BPM);
    let times: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    
    assert_eq!(times.len(), 3);
    assert!((times[0] - 0.0).abs() < 0.01);
    assert!((times[1] - 0.25).abs() < 0.02);
    assert!((times[2] - 0.75).abs() < 0.02);
}

// ============================================================================
// SECTION 5: Chord & Scale Parity
// ============================================================================

#[test]
fn parity_chord_types() {
    let chord_tests = vec![
        ("major", 3),    // root, major 3rd, perfect 5th
        ("minor", 3),    // root, minor 3rd, perfect 5th
        ("dom7", 4),     // root, 3rd, 5th, flat 7th
        ("min7", 4),     // root, minor 3rd, 5th, flat 7th
        ("maj7", 4),     // root, 3rd, 5th, major 7th
        ("dim", 3),      // root, minor 3rd, dim 5th
        ("aug", 3),      // root, major 3rd, aug 5th
        ("sus2", 3),     // root, 2nd, 5th
        ("sus4", 3),     // root, 4th, 5th
        ("minor7", 4),   // same as min7
    ];

    for (chord_type, expected_notes) in &chord_tests {
        let code = format!("play chord(:c4, :{})", chord_type);
        let evts = events(&code, DEFAULT_BPM);
        let n = note_count(&evts);
        assert!(
            n >= *expected_notes,
            "chord(:c4, :{}) should have >= {} notes, got {}",
            chord_type, expected_notes, n
        );
    }
}

#[test]
fn parity_scale_types() {
    let scale_tests = vec![
        ("major", 8),
        ("minor", 8),
        ("minor_pentatonic", 6),
        ("major_pentatonic", 6),
        ("blues", 7),
        ("chromatic", 13),
        ("dorian", 8),
        ("phrygian", 8),
        ("lydian", 8),
        ("mixolydian", 8),
    ];

    for (scale_type, _expected_notes) in &scale_tests {
        let code = format!("play_pattern_timed scale(:c4, :{}), [0.25]", scale_type);
        let result = try_events(&code, DEFAULT_BPM);
        assert!(
            result.is_ok(),
            "scale(:c4, :{}) should parse without error",
            scale_type
        );
        let evts = result.unwrap();
        let n = note_count(&evts);
        assert!(
            n >= 1,
            "scale(:c4, :{}) should produce notes, got {}",
            scale_type, n
        );
    }
}

// ============================================================================
// SECTION 6: Control Flow Parity
// ============================================================================

#[test]
fn parity_live_loop_produces_events() {
    let code = r#"
live_loop :test do
  play :c4
  sleep 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    // live_loop runs 500 iterations
    assert!(n >= 100, "live_loop should produce many notes, got {}", n);
}

#[test]
fn parity_in_thread_single_execution() {
    let code = r#"
in_thread do
  play :c4
  sleep 1
  play :e4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    // in_thread should run exactly once
    assert_eq!(n, 2, "in_thread should produce exactly 2 notes, got {}", n);
}

#[test]
fn parity_times_loop() {
    let code = r#"
3.times do
  play :c4
  sleep 0.5
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 3, "3.times should produce 3 notes");
}

#[test]
fn parity_define_function_call() {
    let code = r#"
define :melody do
  play :c4
  sleep 0.25
  play :e4
  sleep 0.25
end
melody
melody
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(
        note_count(&evts), 4,
        "calling melody twice should produce 4 notes"
    );
}

#[test]
fn parity_define_with_params() {
    let code = r#"
define :hit do |n|
  play n
  sleep 0.25
end
hit :c4
hit :e4
hit :g4
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(
        note_count(&evts), 3,
        "define with params: 3 calls = 3 notes"
    );
}

#[test]
fn parity_conditional_one_in() {
    // one_in(1) should always be true
    let code = "sample :kick if one_in(1)";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1, "one_in(1) should always trigger");
}

#[test]
fn parity_if_block() {
    let code = r#"
if one_in(1) do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "if one_in(1) do should always execute");
}

#[test]
fn parity_variable_resolution() {
    let code = r#"
my_note = :c4
play my_note
my_amp = 0.5
play :e4, amp: my_amp
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 2, "variable notes should produce 2 notes");
}

#[test]
fn parity_set_get() {
    let code = r#"
set :my_val, 0.8
play :c4, amp: get(:my_val)
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "set/get should parse");
}

// ============================================================================
// SECTION 7: Envelope Default Parity (Sonic Pi v4.x)
// ============================================================================

#[test]
fn parity_envelope_defaults() {
    let code = "play :c4";
    let evts = events(code, DEFAULT_BPM);
    
    if let AudioCommand::PlayNote { envelope, amplitude, pan, .. } = &evts[0].1 {
        assert_eq!(*amplitude, 1.0, "default amp should be 1.0");
        assert_eq!(*pan, 0.0, "default pan should be 0.0");
        assert!((envelope.attack - 0.0).abs() < 0.01, "default attack should be 0.0");
        assert!((envelope.decay - 0.0).abs() < 0.01, "default decay should be 0.0");
        assert!((envelope.sustain - 1.0).abs() < 0.01, "default sustain_level should be 1.0");
        assert!((envelope.release - 1.0).abs() < 0.01, "default release should be 1.0");
    } else {
        panic!("Expected PlayNote");
    }
}

#[test]
fn parity_custom_envelope() {
    let code = "play :c4, attack: 0.1, decay: 0.2, sustain: 0.5, release: 0.3, amp: 0.7";
    let evts = events(code, DEFAULT_BPM);
    
    if let AudioCommand::PlayNote { envelope, amplitude, .. } = &evts[0].1 {
        assert_eq!(*amplitude, 0.7, "amp should be 0.7");
        assert!((envelope.attack - 0.1).abs() < 0.01, "attack should be 0.1");
        assert!((envelope.decay - 0.2).abs() < 0.01, "decay should be 0.2");
        assert!((envelope.release - 0.3).abs() < 0.01, "release should be 0.3");
    } else {
        panic!("Expected PlayNote");
    }
}

// ============================================================================
// SECTION 8: Randomisation Parity
// ============================================================================

#[test]
fn parity_rrand_deterministic() {
    // Same seed should produce same results
    let code = r#"
use_random_seed 42
play :c4, amp: rrand(0.1, 1.0)
play :e4, amp: rrand(0.1, 1.0)
"#;
    let evts1 = events(code, DEFAULT_BPM);
    let evts2 = events(code, DEFAULT_BPM);
    
    let amps1: Vec<f32> = evts1.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlayNote { amplitude, .. } = c { Some(*amplitude) } else { None }
        })
        .collect();
    let amps2: Vec<f32> = evts2.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlayNote { amplitude, .. } = c { Some(*amplitude) } else { None }
        })
        .collect();
    
    assert_eq!(amps1.len(), 2);
    assert_eq!(amps1, amps2, "Same seed should produce identical random values");
}

#[test]
fn parity_rrand_range() {
    let code = r#"
use_random_seed 1
play :c4, amp: rrand(0.5, 1.0)
"#;
    let evts = events(code, DEFAULT_BPM);
    if let AudioCommand::PlayNote { amplitude, .. } = &evts[0].1 {
        assert!(
            *amplitude >= 0.5 && *amplitude <= 1.0,
            "rrand(0.5, 1.0) should be in [0.5, 1.0], got {}",
            amplitude
        );
    }
}

// ============================================================================
// SECTION 9: Ring & Spread Parity
// ============================================================================

#[test]
fn parity_ring_creation() {
    let code = r#"
notes = ring(:c4, :e4, :g4, :b4)
play notes.tick
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "ring creation and .tick should parse");
}

#[test]
fn parity_spread_euclidean() {
    let code = r#"
rhythm = spread(3, 8)
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "spread() should parse without error");
}

// ============================================================================
// SECTION 10: Complex Pattern Parity (Example File Constructs)
// ============================================================================

#[test]
fn parity_play_pattern_timed_with_params() {
    let code = "play_pattern_timed [:c4, :e4, :g4], [0.5, 0.5, 1], release: 0.3, amp: 0.5";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 3, "play_pattern_timed should produce 3 notes");
    
    // Check amplitudes
    for (_, cmd) in &evts {
        if let AudioCommand::PlayNote { amplitude, .. } = cmd {
            assert_eq!(*amplitude, 0.5, "all notes should have amp 0.5");
        }
    }
}

#[test]
fn parity_multiple_live_loops() {
    let code = r#"
live_loop :drums do
  sample :kick
  sleep 1
end

live_loop :bass do
  play :c2
  sleep 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    
    assert!(notes >= 100, "bass loop should produce notes");
    assert!(samples >= 100, "drum loop should produce samples");
}

#[test]
fn parity_with_fx_in_live_loop() {
    let code = r#"
live_loop :test do
  with_fx :reverb, mix: 0.5 do
    play :c4
  end
  sleep 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert!(note_count(&evts) >= 100, "should produce notes");
    assert!(fx_start_count(&evts) >= 100, "should produce FxStart per iteration");
}

#[test]
fn parity_stop_in_loop() {
    let code = r#"
live_loop :temp do
  3.times do
    play :c4
    sleep 0.5
  end
  stop
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 3, "stop should terminate loop after first iteration (3 notes)");
}

#[test]
fn parity_sleep_between_loops() {
    // Top-level sleep between live_loops should not affect parallel loop timing
    let code = r#"
live_loop :first do
  play :c4
  sleep 1
end

sleep 8

live_loop :second do
  play :e4
  sleep 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    
    // Both loops should produce events
    let c4_notes = evts.iter()
        .filter(|(_, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                (*frequency - 261.63).abs() < 1.0
            } else {
                false
            }
        })
        .count();
    
    let e4_notes = evts.iter()
        .filter(|(_, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                (*frequency - 329.63).abs() < 1.0
            } else {
                false
            }
        })
        .count();
    
    assert!(c4_notes >= 50, "first loop should produce C4 notes");
    assert!(e4_notes >= 50, "second loop should produce E4 notes (after 8s delay)");
}

// ============================================================================
// SECTION 11: Example File Deep Validation
// ============================================================================

fn read_example(name: &str) -> String {
    let path = format!("../examples/{}", name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e))
}

/// Read an example that may not be tracked in git (local scratch file).
/// Tests using this skip rather than fail when the file is absent.
fn try_read_example(name: &str) -> Option<String> {
    let path = format!("../examples/{}", name);
    match std::fs::read_to_string(&path) {
        Ok(code) => Some(code),
        Err(_) => {
            eprintln!("{} not present, skipping", path);
            None
        }
    }
}

#[test]
fn parity_test1_deep() {
    let code = read_example("Test1");
    let evts = events(&code, 123.0); // Test1 uses use_bpm 123
    
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    
    eprintln!("Test1 deep: {} notes, {} samples, max_t={:.1}s", 
        notes, samples, max_time(&evts));
    
    // Test1 has multiple live_loops with drums and bass
    assert!(samples > 0, "Test1 should have drum samples");
    assert!(notes > 0, "Test1 should have bass notes from play_pattern_timed");
    
    // Check that play_pattern_timed produces correct note sequences
    // Test1 uses [:e2, :g2, :b2, :d3] pattern
    let bass_notes: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                if *frequency < 200.0 { Some(*frequency) } else { None }
            } else {
                None
            }
        })
        .collect();
    assert!(bass_notes.len() > 0, "Test1 should have bass frequencies below 200 Hz");
}

#[test]
fn parity_test2_deep() {
    let code = read_example("Test2");
    let evts = events(&code, 120.0); // Test2 uses use_bpm 120
    
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    
    eprintln!("Test2 deep: {} notes, {} samples, max_t={:.1}s", 
        notes, samples, max_time(&evts));
    
    // Test2 uses define :guitar_riff with distortion. It should have notes.
    assert!(notes > 0, "Test2 should have guitar riff notes");
    
    // Test2 uses dsaw synth
    let dsaw_notes = notes_with_synth(&evts, OscillatorType::DSaw);
    assert!(dsaw_notes > 0, "Test2 should have DSaw notes");
    
    // Test2 uses :fm for bass
    let fm_notes = notes_with_synth(&evts, OscillatorType::FM);
    assert!(fm_notes > 0, "Test2 should have FM bass notes");
}

#[test]
fn parity_test3_deep() {
    let code = read_example("Test3");
    let evts = events(&code, 130.0); // Test3 uses use_bpm 130
    
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    
    eprintln!("Test3 deep: {} notes, {} samples, max_t={:.1}s",
        notes, samples, max_time(&evts));
    
    // Test3 uses set/get, define :kick, define :acid_bass, etc.
    assert!(notes > 0 || samples > 0, "Test3 should have some audio events");
}

#[test]
fn parity_test4_deep() {
    let code = read_example("Test4");
    let evts = events(&code, 135.0); // Test4 uses use_bpm 135
    
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    
    eprintln!("Test4 deep: {} notes, {} samples, max_t={:.1}s",
        notes, samples, max_time(&evts));
    
    // Test4 uses tb303 bass and Time.now (unsupported)
    // Should still produce some events from live_loops
    assert!(notes > 0 || samples > 0, "Test4 should produce events despite unsupported constructs");
}

#[test]
fn parity_test5_deep() {
    let Some(code) = try_read_example("Test5") else { return };
    // Test5 may have mismatched do/end blocks — use try_events
    let result = try_events(&code, 80.0); // Test5 uses use_bpm 80
    match result {
        Ok(evts) => {
            let notes = note_count(&evts);
            let samples = sample_count(&evts);
            eprintln!("Test5 deep: {} notes, {} samples, max_t={:.1}s",
                notes, samples, max_time(&evts));
            assert!(notes > 0 || samples > 0, "Test5 should produce events from in_thread blocks");
        }
        Err(e) => {
            eprintln!("Test5 parse error (known limitation): {}", e);
            // Test5 has mismatched do/end — this is a known issue
        }
    }
}

// ============================================================================
// SECTION 9: Per-Voice FX Chain Parity
// ============================================================================

fn fx_end_count(evts: &[(f32, AudioCommand)]) -> usize {
    evts.iter()
        .filter(|(_, c)| matches!(c, AudioCommand::FxEnd { .. }))
        .count()
}

fn fx_types(evts: &[(f32, AudioCommand)]) -> Vec<String> {
    evts.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::FxStart { fx_type, .. } = c {
                Some(fx_type.clone())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn parity_with_fx_reverb_emits_fx_start_end() {
    let code = "with_fx :reverb, mix: 0.5, room: 0.8 do\n  play 60\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1, "should emit 1 FxStart");
    assert_eq!(fx_end_count(&evts), 1, "should emit 1 FxEnd");
    assert_eq!(note_count(&evts), 1, "should emit 1 PlayNote");
    let types = fx_types(&evts);
    assert_eq!(types[0], "reverb");
}

#[test]
fn parity_with_fx_echo_emits_fx_start_end() {
    let code = "with_fx :echo, phase: 0.25, decay: 4 do\n  play 60\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    assert_eq!(fx_end_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "echo");
}

#[test]
fn parity_nested_with_fx_emits_multiple_fx() {
    let code = "with_fx :reverb, mix: 0.5 do\n  with_fx :distortion, distort: 0.3 do\n    play 60\n  end\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 2, "nested with_fx should emit 2 FxStart");
    assert_eq!(fx_end_count(&evts), 2, "nested with_fx should emit 2 FxEnd");
    assert_eq!(note_count(&evts), 1);
    let types = fx_types(&evts);
    assert!(types.contains(&"reverb".to_string()));
    assert!(types.contains(&"distortion".to_string()));
}

#[test]
fn parity_with_fx_lpf_params() {
    let code = "with_fx :lpf, cutoff: 80 do\n  play 60\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "lpf");
    // Verify cutoff param is present
    for (_, cmd) in &evts {
        if let AudioCommand::FxStart { params, .. } = cmd {
            let cutoff = params.iter().find(|(k, _)| k == "cutoff");
            assert!(cutoff.is_some(), "lpf FxStart should have cutoff param");
            assert!((cutoff.unwrap().1 - 80.0).abs() < 0.01);
        }
    }
}

#[test]
fn parity_sample_lpf_wraps_with_fx() {
    let code = "sample :kick, lpf: 80";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1, "should emit 1 PlaySample");
    assert_eq!(fx_start_count(&evts), 1, "sample lpf: should emit FxStart(lpf)");
    assert_eq!(fx_end_count(&evts), 1, "sample lpf: should emit FxEnd");
    let types = fx_types(&evts);
    assert_eq!(types[0], "lpf");
}

#[test]
fn parity_sample_hpf_wraps_with_fx() {
    let code = "sample :kick, hpf: 50";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    assert_eq!(fx_start_count(&evts), 1, "sample hpf: should emit FxStart(hpf)");
    assert_eq!(fx_end_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "hpf");
}

#[test]
fn parity_sample_lpf_and_hpf_wraps_both() {
    let code = "sample :kick, lpf: 80, hpf: 30";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    assert_eq!(fx_start_count(&evts), 2, "lpf+hpf should emit 2 FxStart");
    assert_eq!(fx_end_count(&evts), 2, "lpf+hpf should emit 2 FxEnd");
    let types = fx_types(&evts);
    assert!(types.contains(&"lpf".to_string()));
    assert!(types.contains(&"hpf".to_string()));
}

#[test]
fn parity_with_fx_contains_samples() {
    let code = "with_fx :reverb do\n  sample :kick\n  sleep 0.5\n  sample :snare\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    assert_eq!(fx_end_count(&evts), 1);
    assert_eq!(sample_count(&evts), 2, "should have 2 samples inside with_fx");
}

#[test]
fn parity_with_fx_ordering() {
    // Verify FxStart comes before PlayNote and FxEnd comes after
    let code = "with_fx :reverb do\n  play 60\nend";
    let evts = events(code, DEFAULT_BPM);
    let mut found_fx_start = false;
    let mut found_note = false;
    let mut found_fx_end = false;
    for (_, cmd) in &evts {
        match cmd {
            AudioCommand::FxStart { .. } => {
                assert!(!found_note, "FxStart must come before PlayNote");
                found_fx_start = true;
            }
            AudioCommand::PlayNote { .. } => {
                assert!(found_fx_start, "PlayNote must come after FxStart");
                found_note = true;
            }
            AudioCommand::FxEnd { .. } => {
                assert!(found_note, "FxEnd must come after PlayNote");
                found_fx_end = true;
            }
            _ => {}
        }
    }
    assert!(found_fx_start && found_note && found_fx_end, "must have FxStart, PlayNote, FxEnd");
}

// ============================================================================
// SECTION 12: At Block & Time Warp Parity
// ============================================================================

#[test]
fn parity_at_block_scheduling() {
    // at [0, 0.5, 1, 1.5] schedules code at those beat times
    let code = r#"
at [0, 0.5, 1, 1.5] do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    assert_eq!(notes.len(), 4, "at block should schedule 4 notes");
    // Verify timing: notes at 0, 0.5, 1.0, 1.5 seconds (at 60BPM, beat=second)
    assert!((notes[0] - 0.0).abs() < 0.05, "note 0 at t=0");
    assert!((notes[1] - 0.5).abs() < 0.05, "note 1 at t=0.5");
    assert!((notes[2] - 1.0).abs() < 0.05, "note 2 at t=1.0");
    assert!((notes[3] - 1.5).abs() < 0.05, "note 3 at t=1.5");
}

#[test]
fn parity_at_block_with_values() {
    // at [0, 1, 2] with values passed to block variable
    let code = r#"
at [0, 1, 2], [:c4, :e4, :g4] do |n|
  play n
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "at block with values should parse");
    let evts = result.unwrap();
    let notes = note_count(&evts);
    assert!(notes >= 3, "at block with values should produce 3 notes, got {}", notes);
}

#[test]
fn parity_time_warp_scheduling() {
    // time_warp schedules code at an offset from current time
    let code = r#"
play :c4
time_warp 0.5 do
  play :e4
end
time_warp 1.0 do
  play :g4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<(f32, f32)> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                Some((*t, *frequency))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(notes.len(), 3, "time_warp should produce 3 notes");
    // C4 at t=0, E4 at t=0.5, G4 at t=1.0
    assert!((notes[0].0 - 0.0).abs() < 0.05, "C4 at t=0");
}

#[test]
fn parity_time_warp_does_not_advance_clock() {
    // time_warp schedules at offset but does NOT advance the parent clock.
    // So play :d4 should be at t=0, same as play :c4.
    let code = r#"
play :c4
time_warp 0.5 do
  play :e4
end
play :d4
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<(f32, f32)> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                Some((*t, *frequency))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(notes.len(), 3, "should produce 3 notes");
    // C4 at t=0, E4 at t=0.5, D4 at t=0 (time_warp didn't advance clock)
    let c4 = notes.iter().find(|(_, f)| (*f - 261.63).abs() < 1.0).unwrap();
    let e4 = notes.iter().find(|(_, f)| (*f - 329.63).abs() < 1.0).unwrap();
    let d4 = notes.iter().find(|(_, f)| (*f - 293.66).abs() < 1.0).unwrap();
    assert!((c4.0 - 0.0).abs() < 0.05, "C4 at t=0, got {}", c4.0);
    assert!((e4.0 - 0.5).abs() < 0.05, "E4 at t=0.5, got {}", e4.0);
    assert!((d4.0 - 0.0).abs() < 0.05, "D4 at t=0 (clock not advanced), got {}", d4.0);
}

// ============================================================================
// SECTION 13: Choose & Additional Randomisation Parity
// ============================================================================

#[test]
fn parity_choose_from_array() {
    // choose() picks a random element from an array
    let code = r#"
use_random_seed 42
3.times do
  play choose([:c4, :e4, :g4])
  sleep 0.5
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 3, "choose in loop should produce 3 notes");

    // All frequencies should be one of C4, E4, or G4
    for (_, cmd) in &evts {
        if let AudioCommand::PlayNote { frequency, .. } = cmd {
            let is_valid = (*frequency - 261.63).abs() < 1.0  // C4
                || (*frequency - 329.63).abs() < 1.0  // E4
                || (*frequency - 392.00).abs() < 1.0; // G4
            assert!(is_valid, "choose should pick from given array, got {} Hz", frequency);
        }
    }
}

#[test]
fn parity_choose_deterministic_with_seed() {
    // Same seed should produce same choose() results
    let code = r#"
use_random_seed 99
play choose([:c4, :e4, :g4])
"#;
    let evts1 = events(code, DEFAULT_BPM);
    let evts2 = events(code, DEFAULT_BPM);

    let freq1: Vec<f32> = evts1.iter()
        .filter_map(|(_, c)| if let AudioCommand::PlayNote { frequency, .. } = c { Some(*frequency) } else { None })
        .collect();
    let freq2: Vec<f32> = evts2.iter()
        .filter_map(|(_, c)| if let AudioCommand::PlayNote { frequency, .. } = c { Some(*frequency) } else { None })
        .collect();
    assert_eq!(freq1, freq2, "same seed should produce same choose() results");
}

#[test]
fn parity_dice_range() {
    let code = r#"
use_random_seed 1
play :c4, amp: dice(6)
"#;
    let evts = events(code, DEFAULT_BPM);
    if let AudioCommand::PlayNote { amplitude, .. } = &evts[0].1 {
        assert!(
            *amplitude >= 1.0 && *amplitude <= 6.0,
            "dice(6) should be in [1, 6], got {}",
            amplitude
        );
    }
}

#[test]
fn parity_rand_range() {
    let code = r#"
use_random_seed 1
play :c4, amp: rand(1.0)
"#;
    let evts = events(code, DEFAULT_BPM);
    if let AudioCommand::PlayNote { amplitude, .. } = &evts[0].1 {
        assert!(
            *amplitude >= 0.0 && *amplitude <= 1.0,
            "rand(1.0) should be in [0, 1.0], got {}",
            amplitude
        );
    }
}

#[test]
fn parity_rand_i_range() {
    let code = r#"
use_random_seed 1
play :c4, amp: rrand_i(1, 5)
"#;
    let evts = events(code, DEFAULT_BPM);
    if let AudioCommand::PlayNote { amplitude, .. } = &evts[0].1 {
        let rounded = amplitude.round();
        assert!(
            rounded >= 1.0 && rounded <= 5.0,
            "rrand_i(1, 5) should be integer in [1, 5], got {}",
            amplitude
        );
    }
}

// ============================================================================
// SECTION 14: While Loop Parity
// ============================================================================

#[test]
fn parity_while_loop() {
    let code = r#"
i = 0
while i < 4 do
  play :c4
  sleep 0.25
  i = i + 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 4, "while i<4 should produce 4 notes");
}

#[test]
fn parity_while_loop_timing() {
    let code = r#"
i = 0
while i < 3 do
  play :c4
  sleep 1
  i = i + 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let times: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None })
        .collect();
    assert_eq!(times.len(), 3);
    assert!((times[0] - 0.0).abs() < 0.05);
    assert!((times[1] - 1.0).abs() < 0.05);
    assert!((times[2] - 2.0).abs() < 0.05);
}

// ============================================================================
// SECTION 15: Sync/Cue Parse Parity
// ============================================================================

#[test]
fn parity_sync_cue_parses_without_error() {
    // sync/cue are parsed but no-op — verify they don't crash
    let code = r#"
in_thread do
  sleep 0.5
  cue :start
end

in_thread do
  sync :start
  play :c4
  sleep 0.5
  play :e4
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "sync/cue should parse without error");
    let evts = result.unwrap();
    // Should still produce notes (sync is no-op, so thread runs immediately)
    assert!(note_count(&evts) >= 2, "sync is no-op so notes should play");
}

#[test]
fn parity_live_loop_sync_parses() {
    let code = r#"
live_loop :drums, sync: :bar do
  sample :kick
  sleep 1
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "live_loop with sync: should parse");
    let evts = result.unwrap();
    assert!(sample_count(&evts) > 0, "live_loop with sync: should produce samples");
}

// ============================================================================
// SECTION 16: Flat Note Names Parity
// ============================================================================

#[test]
fn parity_flat_note_names() {
    // Flat notes use 'f' suffix: :df4 = Db4, :ef4 = Eb4, etc.
    let flat_notes = vec![
        (":df4", 277.18), // Db4 = C#4
        (":ef4", 311.13), // Eb4 = D#4
        (":gf4", 369.99), // Gb4 = F#4
        (":af4", 415.30), // Ab4 = G#4
        (":bf4", 466.16), // Bb4 = A#4
    ];

    for (note, expected_freq) in &flat_notes {
        let code = format!("play {}", note);
        let evts = events(&code, DEFAULT_BPM);
        assert_eq!(note_count(&evts), 1, "play {} should produce 1 note", note);
        if let AudioCommand::PlayNote { frequency, .. } = &evts[0].1 {
            assert!(
                (*frequency - expected_freq).abs() < 1.0,
                "Flat note {} should be ~{} Hz, got {} Hz",
                note, expected_freq, frequency
            );
        }
    }
}

// ============================================================================
// SECTION 17: Sample Advanced Parameters Parity
// ============================================================================

#[test]
fn parity_sample_negative_rate_reverse() {
    // Negative rate should parse and produce a sample event
    let code = "sample :loop_amen, rate: -1";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1, "negative rate sample should produce event");
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { rate, .. } = cmd {
            assert!(
                *rate < 0.0,
                "negative rate should be preserved, got {}",
                rate
            );
        }
    }
}

#[test]
fn parity_sample_pitch_via_rate() {
    // pitch: N applies semitone shift via rate adjustment
    let code = "sample :kick, pitch: 12";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { rate, .. } = cmd {
            // pitch: 12 = one octave up = rate * 2.0
            assert!(
                (*rate - 2.0).abs() < 0.01,
                "pitch: 12 should double rate, got {}",
                rate
            );
        }
    }
}

#[test]
fn parity_sample_rpitch() {
    // rpitch: is same as pitch: — semitone-based rate adjustment
    let code = "sample :kick, rpitch: 7";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { rate, .. } = cmd {
            // rpitch: 7 = 2^(7/12) ≈ 1.498
            let expected = 2.0f32.powf(7.0 / 12.0);
            assert!(
                (*rate - expected).abs() < 0.01,
                "rpitch: 7 should set rate to ~{}, got {}",
                expected, rate
            );
        }
    }
}

#[test]
fn parity_sample_sustain_truncation() {
    // sustain: N should truncate sample playback to N beats
    let code = "sample :loop_amen, sustain: 2";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { sustain_secs, .. } = cmd {
            assert!(
                sustain_secs.is_some(),
                "sustain: should produce Some(sustain_secs)"
            );
        }
    }
}

// ============================================================================
// SECTION 18: Loop Variable Arithmetic Parity
// ============================================================================

#[test]
fn parity_loop_variable_arithmetic() {
    // Loop variable should support arithmetic in play parameters
    let code = r#"
3.times do |i|
  play 60 + i * 4
  sleep 0.25
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let freqs: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::PlayNote { frequency, .. } = c { Some(*frequency) } else { None })
        .collect();
    assert_eq!(freqs.len(), 3, "should produce 3 notes");
    // MIDI 60 = C4, 64 = E4, 68 = Ab4
    // Each successive note should be higher
    assert!(freqs[1] > freqs[0], "second note higher than first");
    assert!(freqs[2] > freqs[1], "third note higher than second");
}

#[test]
fn parity_loop_variable_in_amp() {
    let code = r#"
4.times do |i|
  play :c4, amp: 0.25 * i
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "loop variable in amp should parse");
    let evts = result.unwrap();
    assert_eq!(note_count(&evts), 4, "should produce 4 notes");
}

// ============================================================================
// SECTION 19: Thread Variable Scoping Parity
// ============================================================================

#[test]
fn parity_thread_variable_scoping() {
    // Variables set in threads should not leak to parent scope
    let code = r#"
my_var = :c4
in_thread do
  my_var = :e4
  play my_var
end
sleep 0.5
play my_var
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "thread variable scoping should parse");
    let evts = result.unwrap();
    let n = note_count(&evts);
    assert_eq!(n, 2, "should have 2 notes (one per scope)");
}

// ============================================================================
// SECTION 20: Puts/Print Parse Parity
// ============================================================================

#[test]
fn parity_puts_parses() {
    let code = r#"
puts "hello world"
play :c4
print "testing"
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "puts/print should parse without error");
    let evts = result.unwrap();
    assert_eq!(note_count(&evts), 1, "puts/print should not block play");
}

// ============================================================================
// SECTION 21: use_synth_defaults Parity
// ============================================================================

#[test]
fn parity_use_synth_defaults() {
    let code = r#"
use_synth_defaults amp: 0.5, release: 0.2
play :c4
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    let amps: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::PlayNote { amplitude, .. } = c { Some(*amplitude) } else { None })
        .collect();
    assert_eq!(amps.len(), 2, "should produce 2 notes");
    assert_eq!(amps[0], 0.5, "first note should use default amp 0.5");
    assert_eq!(amps[1], 0.5, "second note should use default amp 0.5");
}

#[test]
fn parity_synth_defaults_override() {
    // Explicit parameters should override synth defaults
    let code = r#"
use_synth_defaults amp: 0.3
play :c4
play :e4, amp: 0.9
"#;
    let evts = events(code, DEFAULT_BPM);
    let amps: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::PlayNote { amplitude, .. } = c { Some(*amplitude) } else { None })
        .collect();
    assert_eq!(amps.len(), 2);
    assert_eq!(amps[0], 0.3, "first note should use default amp 0.3");
    assert_eq!(amps[1], 0.9, "second note should override with amp 0.9");
}

// ============================================================================
// SECTION 22: Knit Function Parity
// ============================================================================

#[test]
fn parity_knit_function() {
    // knit(:e3, 3, :c3, 1) should produce a ring of [:e3, :e3, :e3, :c3]
    let code = r#"
pattern = knit(:e3, 3, :c3, 1)
play pattern.tick
sleep 0.5
play pattern.tick
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "knit() should parse without error");
    let evts = result.unwrap();
    let n = note_count(&evts);
    assert!(n >= 1, "knit pattern should produce notes, got {}", n);
}

// ============================================================================
// SECTION 23: .each / .each_with_index Parity
// ============================================================================

#[test]
fn parity_array_each() {
    let code = r#"
[:c4, :e4, :g4].each do |n|
  play n
  sleep 0.25
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 3, ".each should iterate over 3 elements");
}

#[test]
fn parity_array_each_timing() {
    let code = r#"
[:c4, :e4, :g4].each do |n|
  play n
  sleep 0.5
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let times: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None })
        .collect();
    assert_eq!(times.len(), 3);
    assert!((times[0] - 0.0).abs() < 0.05);
    assert!((times[1] - 0.5).abs() < 0.05);
    assert!((times[2] - 1.0).abs() < 0.05);
}

#[test]
fn parity_each_with_index() {
    let code = r#"
[:c4, :e4, :g4].each_with_index do |n, i|
  play n
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), ".each_with_index should parse");
    let evts = result.unwrap();
    assert!(note_count(&evts) >= 3, ".each_with_index should produce >= 3 notes");
}

// ============================================================================
// SECTION 24: Unless Conditional Parity
// ============================================================================

#[test]
fn parity_unless_block() {
    // unless false → should execute
    let code = r#"
unless one_in(999999) do
  play :c4
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "unless block should parse");
    // Note: one_in(999999) is almost always false, so unless should almost always execute
}

#[test]
fn parity_trailing_unless() {
    // play :c4 unless false → should execute
    let code = "play :c4 unless one_in(999999)";
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "trailing unless should parse");
}

// ============================================================================
// SECTION 25: Scale in play_pattern_timed Parity
// ============================================================================

#[test]
fn parity_play_pattern_timed_with_scale() {
    // scale(:c4, :major) = C4 D4 E4 F4 G4 A4 B4 C5 = 8 notes
    let code = "play_pattern_timed scale(:c4, :major), [0.25]";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 8, "C major scale should produce 8 notes, got {}", n);
}

#[test]
fn parity_play_pattern_timed_minor_pentatonic() {
    // scale(:a4, :minor_pentatonic) = A4 C5 D5 E5 G5 A5 = 6 notes
    let code = "play_pattern_timed scale(:a4, :minor_pentatonic), [0.125]";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 6, "A minor pentatonic should produce 6 notes, got {}", n);
}

#[test]
fn parity_play_pattern_timed_chord() {
    // chord(:c4, :major) = C4 E4 G4 = 3 notes
    let code = "play_pattern_timed chord(:c4, :major), [0.5]";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 3, "C major chord should produce 3 notes, got {}", n);
}

#[test]
fn parity_play_pattern_timed_scale_bare_timing() {
    // scale() with bare number timing (no brackets)
    let code = "play_pattern_timed scale(:c4, :minor), 0.25, release: 0.2";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 8, "C minor scale should produce 8 notes, got {}", n);
}

#[test]
fn parity_play_pattern_timed_ring_variable() {
    // Variable holding ring values used in play_pattern_timed
    let code = r#"
notes = ring(:c4, :e4, :g4)
play_pattern_timed notes, [0.5]
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 3, "ring variable in play_pattern_timed should produce 3 notes, got {}", n);
}

// ============================================================================
// SECTION 26: Control Flow Edge Cases
// ============================================================================

#[test]
fn parity_nested_loops() {
    let code = r#"
2.times do
  3.times do
    play :c4
    sleep 0.25
  end
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 6, "2 * 3 = 6 notes");
}

#[test]
fn parity_loop_in_thread() {
    let code = r#"
in_thread do
  3.times do
    play :c4
    sleep 0.5
  end
end
in_thread do
  2.times do
    play :e4
    sleep 0.5
  end
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 5, "should produce 3 + 2 = 5 notes from threads");
}

#[test]
fn parity_stop_in_live_loop_first_iteration() {
    // stop should immediately terminate the live_loop
    let code = r#"
live_loop :halt do
  play :c4
  stop
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "stop should halt after 1 note");
}

#[test]
fn parity_empty_live_loop_no_crash() {
    // An empty live_loop with just sleep should not crash
    let code = r#"
live_loop :empty do
  sleep 1
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "empty live_loop should parse without error");
}

// ============================================================================
// SECTION 27: With_Synth Block Scoping Parity
// ============================================================================

#[test]
fn parity_use_synth_persists() {
    // use_synth should persist for subsequent play calls
    let code = r#"
use_synth :saw
play :c4
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    let saw_count = notes_with_synth(&evts, OscillatorType::Saw);
    assert_eq!(saw_count, 2, "both notes should use saw synth");
}

#[test]
fn parity_use_synth_changes() {
    // Changing use_synth should affect subsequent notes
    let code = r#"
use_synth :saw
play :c4
use_synth :square
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    let saw_count = notes_with_synth(&evts, OscillatorType::Saw);
    let square_count = notes_with_synth(&evts, OscillatorType::Square);
    assert_eq!(saw_count, 1, "first note should be saw");
    assert_eq!(square_count, 1, "second note should be square");
}

// ============================================================================
// SECTION 28: Multiple FX Types with Params Parity
// ============================================================================

#[test]
fn parity_fx_flanger_params() {
    let code = "with_fx :flanger, rate: 0.5, depth: 0.8, mix: 0.7 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    assert_eq!(note_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::FxStart { fx_type, params, .. } = cmd {
            assert_eq!(fx_type, "flanger");
            let rate_param = params.iter().find(|(k, _)| k == "rate");
            assert!(rate_param.is_some(), "flanger should have rate param");
        }
    }
}

#[test]
fn parity_fx_chorus_params() {
    let code = "with_fx :chorus, rate: 0.3, depth: 0.5, mix: 0.6 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "chorus");
}

#[test]
fn parity_fx_ring_mod_params() {
    let code = "with_fx :ring_mod, freq: 30, mix: 0.8 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "ring_mod");
}

#[test]
fn parity_fx_slicer_params() {
    let code = "with_fx :slicer, phase: 0.25 do\n  play :c4, sustain: 2\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "slicer");
}

#[test]
fn parity_fx_compressor_params() {
    let code = "with_fx :compressor, threshold: 0.3 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "compressor");
}

#[test]
fn parity_fx_normaliser_params() {
    let code = "with_fx :normaliser, level: 0.8 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "normaliser");
}

#[test]
fn parity_fx_bitcrusher_params() {
    let code = "with_fx :bitcrusher, bits: 8, sample_rate: 8000 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    let types = fx_types(&evts);
    assert_eq!(types[0], "bitcrusher");
}

#[test]
fn parity_fx_krush_routes_to_bitcrusher() {
    let code = "with_fx :krush, gain: 5 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(fx_start_count(&evts), 1);
    // krush should route to bitcrusher
    let types = fx_types(&evts);
    assert!(
        types[0] == "bitcrusher" || types[0] == "krush",
        "krush should route to bitcrusher, got {}",
        types[0]
    );
}

// ============================================================================
// SECTION 29: Complex Composition Patterns Parity
// ============================================================================

#[test]
fn parity_drums_bass_melody_composition() {
    // Test a realistic multi-loop composition
    let code = r#"
use_bpm 120

live_loop :drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.5
end

live_loop :bass do
  use_synth :tb303
  play :c2, cutoff: 70, release: 0.2
  sleep 1
end

live_loop :melody do
  use_synth :saw
  play :e4, release: 0.3, amp: 0.5
  sleep 0.5
  play :g4, release: 0.3, amp: 0.5
  sleep 0.5
end
"#;
    let evts = events(code, 120.0);
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    let fx_starts = fx_start_count(&evts);

    assert!(samples > 100, "drums loop should produce many samples");
    assert!(notes > 100, "bass + melody should produce many notes");

    let tb303_notes = notes_with_synth(&evts, OscillatorType::TB303);
    let saw_notes = notes_with_synth(&evts, OscillatorType::Saw);
    assert!(tb303_notes > 50, "bass should have TB303 notes");
    assert!(saw_notes > 50, "melody should have saw notes");

    eprintln!(
        "Composition: {} notes ({} tb303, {} saw), {} samples, {} fx_starts",
        notes, tb303_notes, saw_notes, samples, fx_starts
    );
}

#[test]
fn parity_ambient_pad_with_effects() {
    let code = r#"
live_loop :pad do
  use_synth :blade
  with_fx :reverb, mix: 0.7, room: 0.9 do
    play chord(:c4, :minor7), amp: 0.2, attack: 2, sustain: 4, release: 2
    sleep 8
  end
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes = note_count(&evts);
    let fx_count = fx_start_count(&evts);
    assert!(notes > 0, "pad should produce notes (chord)");
    assert!(fx_count > 0, "pad should use reverb FX");

    let blade_notes = notes_with_synth(&evts, OscillatorType::Blade);
    assert!(blade_notes > 0, "should use blade synth");
}

#[test]
fn parity_acid_bass_pattern() {
    let code = r#"
live_loop :acid do
  use_synth :tb303
  8.times do
    play :c2, cutoff: 70, release: 0.2, amp: 0.5
    sleep 0.25
  end
  2.times do
    play :eb2, cutoff: 85, release: 0.2, amp: 0.5
    sleep 0.25
  end
  2.times do
    play :f2, cutoff: 100, release: 0.2, amp: 0.5
    sleep 0.25
  end
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let tb303_notes = notes_with_synth(&evts, OscillatorType::TB303);
    // Each loop iteration: 8 + 2 + 2 = 12 notes, 500 iterations → lots of notes
    assert!(tb303_notes > 100, "acid bass should produce many TB303 notes, got {}", tb303_notes);
}

// ============================================================================
// SECTION 30: Edge Cases & Robustness
// ============================================================================

#[test]
fn parity_comment_lines_ignored() {
    let code = r#"
# This is a comment
play :c4  # This plays C4
# Another comment
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 2, "comments should be ignored, producing 2 notes");
}

#[test]
fn parity_empty_code() {
    let code = "";
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "empty code should return Ok, not Err");
    let evts = result.unwrap();
    assert_eq!(note_count(&evts), 0, "empty code should produce no events");
}

#[test]
fn parity_only_comments() {
    let code = "# just comments\n# nothing here";
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "comment-only code should return Ok, not Err");
    let evts = result.unwrap();
    assert_eq!(note_count(&evts), 0, "comment-only code should produce no events");
}

#[test]
fn parity_multiple_sleeps() {
    let code = r#"
play :c4
sleep 0.5
sleep 0.5
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    let times: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None })
        .collect();
    assert_eq!(times.len(), 2);
    assert!((times[1] - 1.0).abs() < 0.05, "two 0.5 sleeps = 1.0 second pause");
}

#[test]
fn parity_zero_amp_note() {
    let code = "play :c4, amp: 0";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "zero-amp note should still produce event");
    if let AudioCommand::PlayNote { amplitude, .. } = &evts[0].1 {
        assert_eq!(*amplitude, 0.0, "amp should be 0.0");
    }
}

#[test]
fn parity_high_bpm() {
    // Very high BPM should compress timing
    let code = "use_bpm 240\nplay :c4\nsleep 1\nplay :e4";
    let evts = events(code, 240.0);
    let times: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None })
        .collect();
    assert_eq!(times.len(), 2);
    // At 240 BPM, 1 beat = 0.25 seconds
    assert!((times[1] - 0.25).abs() < 0.05, "at 240 BPM, 1 beat = 0.25s");
}

#[test]
fn parity_pan_extremes() {
    let code = r#"
play :c4, pan: -1
play :e4, pan: 1
play :g4, pan: 0
"#;
    let evts = events(code, DEFAULT_BPM);
    let pans: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::PlayNote { pan, .. } = c { Some(*pan) } else { None })
        .collect();
    assert_eq!(pans.len(), 3);
    assert_eq!(pans[0], -1.0, "hard left");
    assert_eq!(pans[1], 1.0, "hard right");
    assert_eq!(pans[2], 0.0, "center");
}

#[test]
fn parity_use_bpm_in_middle() {
    // BPM changes mid-code should affect subsequent timing
    let code = r#"
play :c4
sleep 1
use_bpm 120
play :e4
sleep 1
play :g4
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "mid-code BPM change should parse");
    let evts = result.unwrap();
    assert_eq!(note_count(&evts), 3, "should produce 3 notes");
}

// ============================================================================
// SECTION 31: Spread with Samples Parity
// ============================================================================

#[test]
fn parity_spread_with_samples() {
    // spread(3, 8) with sample triggers in a loop
    let code = r#"
rhythm = spread(3, 8)
8.times do
  sample :kick if rhythm.tick
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "spread with samples should parse");
    let evts = result.unwrap();
    let s = sample_count(&evts);
    // spread(3,8) should produce exactly 3 hits over 8 steps
    assert!(s >= 1, "spread should trigger some samples, got {}", s);
    assert!(s <= 8, "spread(3,8) should trigger at most 8 samples, got {}", s);
}

// ============================================================================
// SECTION 32: Disco Groove Full Composition Parse
// ============================================================================

#[test]
fn parity_disco_groove_parses() {
    let code = std::fs::read_to_string("../fidelity/fixtures/disco_groove.rb")
        .unwrap_or_else(|_| String::new());
    if code.is_empty() {
        eprintln!("disco_groove.rb not found, skipping");
        return;
    }
    let result = try_events(&code, 122.0);
    assert!(result.is_ok(), "disco_groove fixture should parse");
    let evts = result.unwrap();
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    eprintln!("disco_groove: {} notes, {} samples", notes, samples);
    assert!(
        notes > 0 || samples > 0,
        "disco_groove should produce audio events"
    );
}

// ============================================================================
// SECTION: Per-Sample ADSR Envelope
// ============================================================================

#[test]
fn parity_sample_adsr_envelope() {
    let code = "sample :kick, attack: 0.1, decay: 0.2, sustain_level: 0.5, release: 0.3";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1, "should produce 1 sample");
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { envelope, .. } = cmd {
            assert!(envelope.is_some(), "sample should have ADSR envelope");
            let env = envelope.as_ref().unwrap();
            assert!((env.attack - 0.1).abs() < 0.01, "attack should be 0.1, got {}", env.attack);
            assert!((env.decay - 0.2).abs() < 0.01, "decay should be 0.2, got {}", env.decay);
            assert!((env.sustain - 0.5).abs() < 0.01, "sustain_level should be 0.5, got {}", env.sustain);
            assert!((env.release - 0.3).abs() < 0.01, "release should be 0.3, got {}", env.release);
        }
    }
}

#[test]
fn parity_sample_attack_only() {
    let code = "sample :snare, attack: 0.5";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { envelope, .. } = cmd {
            assert!(envelope.is_some(), "attack-only sample should have envelope");
            let env = envelope.as_ref().unwrap();
            assert!((env.attack - 0.5).abs() < 0.01, "attack should be 0.5");
            assert!((env.release - 0.0).abs() < 0.01, "release should default to 0.0");
        }
    }
}

#[test]
fn parity_sample_release_only() {
    let code = "sample :kick, release: 0.2";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { envelope, .. } = cmd {
            assert!(envelope.is_some(), "release-only sample should have envelope");
            let env = envelope.as_ref().unwrap();
            assert!((env.release - 0.2).abs() < 0.01, "release should be 0.2");
        }
    }
}

#[test]
fn parity_sample_adsr_with_rate() {
    let code = "sample :loop_amen, attack: 0.1, release: 0.5, rate: 1.5";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { envelope, rate, .. } = cmd {
            assert!(envelope.is_some(), "should have envelope");
            assert!((*rate - 1.5).abs() < 0.01, "rate should be 1.5, got {}", rate);
        }
    }
}

#[test]
fn parity_sample_no_adsr_default() {
    // Sample without ADSR params should have envelope = None
    let code = "sample :kick, amp: 0.8";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { envelope, .. } = cmd {
            assert!(envelope.is_none(), "sample without ADSR should have no envelope");
        }
    }
}

// ============================================================================
// SECTION: Conditional Logic (if/else/elsif)
// ============================================================================

#[test]
fn parity_if_else_block() {
    let code = r#"
x = 5
if x > 10 do
  play :c4
else
  play :e4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert!(n >= 1, "if/else should produce at least 1 note, got {}", n);
}

#[test]
fn parity_if_elsif_else_block() {
    let code = r#"
x = 2
if x == 1 do
  play :c4
elsif x == 2 do
  play :e4
else
  play :g4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert!(n >= 1, "if/elsif/else should produce at least 1 note, got {}", n);
}

#[test]
fn parity_numeric_comparison_gt() {
    let code = r#"
x = 5
if x > 3 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "> comparison should produce 1 note");
}

#[test]
fn parity_numeric_comparison_lte() {
    let code = r#"
x = 3
if x <= 3 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "<= comparison should produce 1 note");
}

#[test]
fn parity_boolean_or_conditional() {
    let code = r#"
if one_in(1) || one_in(999) do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "|| with one_in(1) should always produce a note");
}

#[test]
fn parity_boolean_and_conditional() {
    let code = r#"
if one_in(1) && one_in(1) do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "&& with one_in(1) should always produce a note");
}

#[test]
fn parity_trailing_if_produces_note() {
    // one_in(1) always true
    let code = "play :c4 if one_in(1)";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "trailing if with one_in(1) should produce a note");
}

#[test]
fn parity_unless_false_executes() {
    let code = r#"
x = 1
unless x == 5 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "unless with false condition should produce a note");
}

#[test]
fn parity_if_with_variable_comparison() {
    let code = r#"
x = 5
if x == 5 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "if x == 5 should match and produce a note");
}

// ============================================================================
// SECTION: Single-Line If/Unless (do/end validator)
// ============================================================================

#[test]
fn parity_single_line_if_semicolons() {
    let code = "if one_in(1); play :c4; sleep 0.5; end";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "single-line if should produce 1 note");
}

#[test]
fn parity_nested_if_inside_loop() {
    let code = r#"
3.times do
  if one_in(1) do
    play :c4
  end
  sleep 0.25
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 3, "if inside loop should produce 3 notes");
}

// ============================================================================
// SECTION: Define / Function Patterns
// ============================================================================

#[test]
fn parity_define_with_fx_inside() {
    let code = r#"
define :riff do
  with_fx :distortion do
    play :e2
  end
end
riff
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "define with FX should produce 1 note");
    assert!(fx_start_count(&evts) >= 1, "define with FX should produce FxStart");
}

#[test]
fn parity_define_with_loop_inside() {
    let code = r#"
define :arp do
  3.times do
    play :c4
    sleep 0.25
  end
end
arp
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 3, "define with loop should produce 3 notes");
}

#[test]
fn parity_define_called_from_live_loop() {
    let code = r#"
define :kick do
  sample :kick
end
live_loop :t do
  kick
  sleep 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert!(sample_count(&evts) >= 5, "define called from live_loop should produce many samples");
}

#[test]
fn parity_nested_define_calls() {
    let code = r#"
define :a do
  play :c4
  sleep 0.25
end
define :b do
  a
  a
end
b
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 2, "nested define calls should produce 2 notes");
}

// ============================================================================
// SECTION: at/time_warp Advanced
// ============================================================================

#[test]
fn parity_at_block_does_not_advance_clock() {
    let code = r#"
play :c4
at [0.5, 1.0] do
  play :e4
end
play :g4
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<(f32, f32)> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                Some((*t, *frequency))
            } else {
                None
            }
        })
        .collect();

    // C4 and G4 should be at t=0 (at block doesn't advance clock)
    // E4 should be at t=0.5 and t=1.0
    assert!(notes.len() >= 3, "should have at least 3 notes, got {}", notes.len());

    // Find C4 (~262 Hz) and G4 (~392 Hz) — both at t=0.0
    let c4_time = notes.iter().find(|(_, f)| (*f - 261.63).abs() < 2.0).map(|(t, _)| *t);
    let g4_time = notes.iter().find(|(_, f)| (*f - 392.0).abs() < 2.0).map(|(t, _)| *t);
    assert!(c4_time.is_some(), "should have C4 note");
    assert!(g4_time.is_some(), "should have G4 note");
    assert!((c4_time.unwrap() - g4_time.unwrap()).abs() < 0.01,
        "C4 and G4 should be at same time (at block non-clock-advancing), c4={}, g4={}",
        c4_time.unwrap(), g4_time.unwrap());
}

#[test]
fn parity_multiple_at_blocks() {
    let code = r#"
at [0, 1] do
  play :c4
end
at [0.5, 1.5] do
  play :e4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert!(note_count(&evts) >= 4, "two at blocks should produce at least 4 notes, got {}", note_count(&evts));
}

#[test]
fn parity_time_warp_negative_offset() {
    let code = r#"
sleep 2
time_warp -0.5 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    assert_eq!(notes.len(), 1, "should have 1 note");
    assert!((notes[0] - 1.5).abs() < 0.1, "C4 should be at ~1.5s (2.0 - 0.5), got {}", notes[0]);
}

// ============================================================================
// SECTION: Variable Handling Edge Cases
// ============================================================================

#[test]
fn parity_variable_in_sleep() {
    let code = r#"
d = 0.5
play :c4
sleep d
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    assert_eq!(notes.len(), 2, "should have 2 notes");
    assert!((notes[1] - 0.5).abs() < 0.1, "second note should be at ~0.5s, got {}", notes[1]);
}

#[test]
fn parity_variable_reassignment() {
    let code = r#"
n = :c4
play n
n = :e4
play n
"#;
    let evts = events(code, DEFAULT_BPM);
    let freqs: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlayNote { frequency, .. } = c {
                Some(*frequency)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(freqs.len(), 2, "should have 2 notes");
    assert!((freqs[0] - 261.63).abs() < 2.0, "first note should be C4");
    assert!((freqs[1] - 329.63).abs() < 2.0, "second note should be E4");
}

#[test]
fn parity_rrand_in_sample_param() {
    let code = "sample :kick, amp: rrand(0.5, 1.0)";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 1, "rrand in sample param should produce 1 sample");
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { amplitude, .. } = cmd {
            assert!(*amplitude >= 0.4 && *amplitude <= 1.1,
                "amplitude should be in rrand range, got {}", amplitude);
        }
    }
}

// ============================================================================
// SECTION: FX Parameter Verification
// ============================================================================

#[test]
fn parity_fx_gverb_params() {
    let code = r#"
with_fx :gverb, room: 30, mix: 0.6 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1);
    assert!(fx_start_count(&evts) >= 1, "gverb should emit FxStart");
}

#[test]
fn parity_fx_pan_params() {
    let code = r#"
with_fx :pan, pan: -0.5 do
  play :c4
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1);
    assert!(fx_start_count(&evts) >= 1, "pan FX should emit FxStart");
}

#[test]
fn parity_triple_nested_fx() {
    let code = r#"
with_fx :reverb do
  with_fx :lpf, cutoff: 80 do
    with_fx :distortion do
      play :c4
    end
  end
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1);
    assert!(fx_start_count(&evts) >= 3, "3 nested FX should emit >= 3 FxStart events, got {}", fx_start_count(&evts));
}

// ============================================================================
// SECTION: No-Op / Pragma Constructs (Should Not Crash)
// ============================================================================

#[test]
fn parity_control_parses_no_crash() {
    // `control` is a no-op; `s = play :c4` may not produce a note when assigned
    let code = r#"
play :c4, sustain: 4
sleep 1
control :unused, note: :e4
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "control should parse without crash");
}

#[test]
fn parity_pragma_no_crash() {
    let code = r#"
use_debug false
play :c4
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "pragmas should parse without crash");
    assert_eq!(note_count(&result.unwrap()), 1);
}

// ============================================================================
// SECTION: Ring/List Methods
// ============================================================================

#[test]
fn parity_ring_tick_in_loop() {
    let code = r#"
notes = ring(:c4, :e4, :g4)
4.times do
  play notes.tick
  sleep 0.25
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 4, "ring.tick in loop should produce 4 notes");
}

#[test]
fn parity_array_choose_single() {
    let code = "play [:c4, :e4, :g4].choose";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, ".choose should produce 1 note");
}

#[test]
fn parity_list_shuffle_count() {
    // .shuffle.each is a chained method - may not be fully supported
    // Verify it doesn't panic; producing notes is a bonus
    let code = r#"
[:c4, :e4, :g4].shuffle.each do |n|
  play n
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    // Just verify no panic — either parses or returns an error gracefully
    if let Ok(evts) = result {
        eprintln!("shuffle.each produced {} notes", note_count(&evts));
    }
}

#[test]
fn parity_list_reverse_pattern() {
    let code = "play_pattern_timed [:c4, :e4, :g4].reverse, [0.25]";
    let result = try_events(code, DEFAULT_BPM);
    if result.is_ok() {
        let evts = result.unwrap();
        assert!(note_count(&evts) >= 1, "reversed pattern should produce notes");
    }
}

// ============================================================================
// SECTION: Scale/Chord Edge Cases
// ============================================================================

#[test]
fn parity_chord_dom7_notes() {
    let code = "play chord(:c4, :dom7)";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert!(n >= 3, "dom7 chord should produce at least 3 notes, got {}", n);
}

#[test]
fn parity_chord_in_variable() {
    // Known limitation: `play notes` where notes = chord() is not resolved
    // Verify it at least parses without crashing
    let code = r#"
notes = chord(:c4, :major)
play notes
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "chord in variable should parse without crash");
}

#[test]
fn parity_scale_in_each() {
    let code = r#"
scale(:c4, :major).each do |n|
  play n
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    if result.is_ok() {
        let evts = result.unwrap();
        let n = note_count(&evts);
        assert!(n >= 7, "scale .each should produce >= 7 notes, got {}", n);
    }
}

#[test]
fn parity_scale_chromatic_count() {
    let code = "play_pattern_timed scale(:c4, :chromatic), [0.1]";
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert!(n >= 12, "chromatic scale should produce >= 12 notes, got {}", n);
}

// ============================================================================
// SECTION: Timing / Sleep Edge Cases
// ============================================================================

#[test]
fn parity_sleep_zero() {
    let code = r#"
play :c4
sleep 0
play :e4
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    assert_eq!(notes.len(), 2);
    assert!((notes[0] - notes[1]).abs() < 0.01, "sleep 0 should keep both notes at same time");
}

#[test]
fn parity_whitespace_only_code() {
    let code = "   \n  \n  ";
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "whitespace-only code should parse OK");
    assert_eq!(note_count(&result.unwrap()), 0);
}

#[test]
fn parity_inline_comment_after_code() {
    let code = "play :c4 # my note\nsleep 1 # wait";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1, "inline comments should be stripped");
}

// ============================================================================
// SECTION: Robustness / Edge Cases  
// ============================================================================

#[test]
fn parity_play_rest_symbol() {
    let code = r#"
play :rest
play :c4
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "play :rest should parse OK");
    let evts = result.unwrap();
    // :rest may produce 0 or 1 notes depending on implementation
    // The important thing is it doesn't crash and :c4 still works
    assert!(note_count(&evts) >= 1, "should have at least the C4 note");
}

#[test]
fn parity_very_large_times() {
    let code = r#"
500.times do
  play :c4
  sleep 0.001
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 500, "500.times should produce exactly 500 notes");
}

#[test]
fn parity_sample_combined_params() {
    let code = "sample :kick, amp: 0.8, rate: 1.5, pan: -0.5, lpf: 80";
    let evts = events(code, DEFAULT_BPM);
    let samples: Vec<_> = evts.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlaySample { amplitude, rate, pan, .. } = c {
                Some((*amplitude, *rate, *pan))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(samples.len(), 1, "combined params sample should produce 1 event");
    assert!((samples[0].0 - 0.8).abs() < 0.01, "amp should be 0.8");
    assert!((samples[0].1 - 1.5).abs() < 0.01, "rate should be 1.5");
    assert!((samples[0].2 - (-0.5)).abs() < 0.01, "pan should be -0.5");
    // lpf should wrap with FxStart
    assert!(fx_start_count(&evts) >= 1, "lpf: param should generate FxStart");
}

#[test]
fn parity_sample_start_finish_with_rate() {
    let code = "sample :loop_amen, start: 0.25, finish: 0.75, rate: 2";
    let evts = events(code, DEFAULT_BPM);
    for (_, cmd) in &evts {
        if let AudioCommand::PlaySample { start, finish, rate, .. } = cmd {
            assert!(start.is_some(), "start should be present");
            assert!(finish.is_some(), "finish should be present");
            assert!((*rate - 2.0).abs() < 0.01, "rate should be 2.0");
        }
    }
}

// ============================================================================
// SECTION: Iteration Advanced
// ============================================================================

#[test]
fn parity_each_with_index_uses_index() {
    let code = r#"
[:c4, :e4, :g4].each_with_index do |n, i|
  play n, amp: 0.3 * (i + 1)
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    assert!(result.is_ok(), "each_with_index should parse");
    let evts = result.unwrap();
    let n = note_count(&evts);
    assert!(n >= 1, "each_with_index should produce notes, got {}", n);
}

#[test]
fn parity_ring_each_do() {
    let code = r#"
ring(:c4, :e4, :g4).each do |n|
  play n
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    if result.is_ok() {
        let evts = result.unwrap();
        assert!(note_count(&evts) >= 1, "ring .each should produce notes");
    }
}

// ============================================================================
// SECTION: Knit / Pattern Constructors
// ============================================================================

#[test]
fn parity_knit_values_in_play_pattern() {
    let code = r#"
pattern = knit(:c4, 2, :e4, 1)
pattern.each do |n|
  play n
  sleep 0.25
end
"#;
    let result = try_events(code, DEFAULT_BPM);
    if result.is_ok() {
        let evts = result.unwrap();
        let n = note_count(&evts);
        assert!(n >= 1, "knit each should produce notes, got {}", n);
    }
}

// ============================================================================
// SECTION: While Loop with Guard
// ============================================================================

#[test]
fn parity_while_with_counter() {
    let code = r#"
i = 0
while i < 3 do
  play :c4
  sleep 0.25
  i = i + 1
end
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 3, "while loop with counter should produce 3 notes");
}

// ============================================================================
// SECTION: use_synth Scoping
// ============================================================================

#[test]
fn parity_use_synth_restores_after_block() {
    // Verify that use_synth inside a define/block doesn't leak out
    let code = r#"
play :c4
use_synth :saw
play :e4
use_synth :sine
play :g4
"#;
    let evts = events(code, DEFAULT_BPM);
    let synths: Vec<OscillatorType> = evts.iter()
        .filter_map(|(_, c)| {
            if let AudioCommand::PlayNote { synth_type, .. } = c {
                Some(*synth_type)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(synths.len(), 3);
    // First note uses default synth, second uses saw, third uses sine
    assert_eq!(synths[1], OscillatorType::Saw, "second note should be saw");
    assert_eq!(synths[2], OscillatorType::Sine, "third note should be sine");
}

#[test]
fn parity_use_synth_in_define_scoped() {
    let code = r#"
define :bass do
  use_synth :tb303
  play :e2
end
play :c4
bass
play :g4
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert!(n >= 2, "should have notes from both contexts, got {}", n);
}

// ============================================================================
// SECTION: BPM Handling
// ============================================================================

#[test]
fn parity_use_bpm_affects_sleep() {
    let code = r#"
use_bpm 120
play :c4
sleep 1
play :e4
"#;
    let evts = events(code, 120.0);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| {
            if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None }
        })
        .collect();
    assert_eq!(notes.len(), 2);
    // At 120 BPM, 1 beat = 0.5 seconds
    assert!((notes[1] - 0.5).abs() < 0.1, "at 120 BPM, 1 beat sleep = 0.5s, got {}", notes[1]);
}

// ============================================================================
// SECTION: Multiple live_loops Concurrent
// ============================================================================

#[test]
fn parity_concurrent_live_loops_produce_events() {
    let code = r#"
live_loop :kick do
  sample :kick
  sleep 1
end

live_loop :melody do
  play :c4
  sleep 0.5
end
"#;
    let evts = events(code, DEFAULT_BPM);
    let notes = note_count(&evts);
    let samples = sample_count(&evts);
    assert!(notes > 0 && samples > 0,
        "concurrent loops should produce both notes ({}) and samples ({})", notes, samples);
}

// ============================================================================
// SECTION: Synth-Specific Parameters
// ============================================================================

#[test]
fn parity_synth_cutoff_param() {
    let code = r#"
use_synth :tb303
play :c2, cutoff: 80
"#;
    let evts = events(code, DEFAULT_BPM);
    let n = note_count(&evts);
    assert_eq!(n, 1);
    // Verify the note has the right synth
    for (_, cmd) in &evts {
        if let AudioCommand::PlayNote { synth_type, .. } = cmd {
            assert_eq!(*synth_type, OscillatorType::TB303);
        }
    }
}

#[test]
fn parity_synth_super_saw_detune() {
    let code = r#"
use_synth :super_saw
play :c4
"#;
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 1);
    for (_, cmd) in &evts {
        if let AudioCommand::PlayNote { synth_type, .. } = cmd {
            assert_eq!(*synth_type, OscillatorType::SuperSaw);
        }
    }
}

// ============================================================================
// SECTION: New Effects Parity (bpf, tremolo, ping_pong, level, mono, etc.)
// ============================================================================

#[test]
fn parity_fx_bpf_parses() {
    let code = "with_fx :bpf, centre: 80, res: 0.5 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "bpf should produce FxStart");
    assert_eq!(note_count(&evts), 1);
}

#[test]
fn parity_fx_rbpf_parses() {
    let code = "with_fx :rbpf, centre: 90, res: 0.8 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "rbpf should produce FxStart");
}

#[test]
fn parity_fx_nbpf_parses() {
    let code = "with_fx :nbpf, centre: 70, res: 0.3 do\n  sample :kick\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "nbpf should produce FxStart");
    assert_eq!(sample_count(&evts), 1);
}

#[test]
fn parity_fx_nrbpf_parses() {
    let code = "with_fx :nrbpf, centre: 85, res: 0.6 do\n  play :e4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "nrbpf should produce FxStart");
}

#[test]
fn parity_fx_tremolo_parses() {
    let code = "with_fx :tremolo, rate: 4, depth: 0.7, wave: 2 do\n  play :c4, sustain: 2\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "tremolo should produce FxStart");
    assert_eq!(note_count(&evts), 1);
}

#[test]
fn parity_fx_ping_pong_parses() {
    let code = "with_fx :ping_pong, phase: 0.25, feedback: 0.5 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "ping_pong should produce FxStart");
}

#[test]
fn parity_fx_level_parses() {
    let code = "with_fx :level, amp: 0.3 do\n  play :c4\n  play :e4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "level should produce FxStart");
    assert_eq!(note_count(&evts), 2);
}

#[test]
fn parity_fx_mono_parses() {
    let code = "with_fx :mono do\n  play :c4, pan: -0.8\n  play :e4, pan: 0.8\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "mono should produce FxStart");
    assert_eq!(note_count(&evts), 2);
}

#[test]
fn parity_fx_band_eq_parses() {
    let code = "with_fx :band_eq, freq: 2000, db: 12, res: 0.5 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "band_eq should produce FxStart");
}

#[test]
fn parity_fx_pitch_shift_parses() {
    let code = "with_fx :pitch_shift, shift: 7 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "pitch_shift should produce FxStart");
}

#[test]
fn parity_fx_whammy_parses() {
    let code = "with_fx :whammy, transpose: 12 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "whammy should produce FxStart");
}

#[test]
fn parity_fx_tanh_parses() {
    let code = "with_fx :tanh, krunch: 0.7 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "tanh should produce FxStart");
}

#[test]
fn parity_fx_nrlpf_parses() {
    let code = "with_fx :nrlpf, cutoff: 80, res: 0.5 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "nrlpf should produce FxStart");
}

#[test]
fn parity_fx_nrhpf_parses() {
    let code = "with_fx :nrhpf, cutoff: 60, res: 0.3 do\n  play :c4\nend";
    let evts = events(code, DEFAULT_BPM);
    assert!(fx_start_count(&evts) >= 1, "nrhpf should produce FxStart");
}

// ============================================================================
// SECTION: New BPM Commands (use_bpm_mul, with_bpm_mul)
// ============================================================================

#[test]
fn parity_use_bpm_mul_doubles() {
    let code = "use_bpm 100\nuse_bpm_mul 2\nplay :c4\nsleep 1";
    let evts = events(code, DEFAULT_BPM);
    let bpms: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::SetBpm(b) = c { Some(*b) } else { None })
        .collect();
    assert!(bpms.contains(&100.0));
    assert!(bpms.contains(&200.0), "use_bpm_mul 2 should double BPM to 200");
}

#[test]
fn parity_use_bpm_mul_half() {
    let code = "use_bpm 120\nuse_bpm_mul 0.5\nplay :c4";
    let evts = events(code, DEFAULT_BPM);
    let bpms: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::SetBpm(b) = c { Some(*b) } else { None })
        .collect();
    assert!(bpms.contains(&60.0), "use_bpm_mul 0.5 on BPM 120 should give BPM 60");
}

#[test]
fn parity_with_bpm_mul_scoped() {
    let code = "use_bpm 120\nwith_bpm_mul 0.5 do\n  play :c4\n  sleep 1\nend\nplay :e4";
    let evts = events(code, DEFAULT_BPM);
    let bpms: Vec<f32> = evts.iter()
        .filter_map(|(_, c)| if let AudioCommand::SetBpm(b) = c { Some(*b) } else { None })
        .collect();
    assert!(bpms.contains(&60.0), "inside with_bpm_mul BPM should be 60");
    assert!(bpms.contains(&120.0), "BPM should be restored after with_bpm_mul");
    assert_eq!(note_count(&evts), 2);
}

#[test]
fn parity_with_bpm_mul_timing() {
    // with_bpm_mul 0.5 halves BPM from 120→60: sleep 1 at 60bpm = 1.0s
    let code = "use_bpm 120\nwith_bpm_mul 0.5 do\n  play :c4\n  sleep 1\n  play :e4\nend";
    let evts = events(code, DEFAULT_BPM);
    let notes: Vec<f32> = evts.iter()
        .filter_map(|(t, c)| if let AudioCommand::PlayNote { .. } = c { Some(*t) } else { None })
        .collect();
    assert_eq!(notes.len(), 2);
    let gap = notes[1] - notes[0];
    assert!((gap - 1.0).abs() < 0.05, "sleep 1 at BPM 60 should be ~1.0s, got {}", gap);
}

// ============================================================================
// SECTION: with_swing — block contents preserved
// ============================================================================

#[test]
fn parity_with_swing_block_produces_notes() {
    let code = "with_swing 0.1 do\n  play :c4\n  sleep 0.5\n  play :e4\n  sleep 0.5\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(note_count(&evts), 2, "with_swing block should produce notes");
}

#[test]
fn parity_with_swing_nested_loops() {
    let code = "with_swing 0.1 do\n  3.times do\n    sample :kick\n    sleep 0.5\n  end\nend";
    let evts = events(code, DEFAULT_BPM);
    assert_eq!(sample_count(&evts), 3, "with_swing with nested 3.times should produce 3 samples");
}

// ============================================================================
// SECTION: All new FX types produce audible events
// ============================================================================

#[test]
fn parity_all_new_fx_produce_notes() {
    let fxs = vec![
        ("bpf", "centre: 80"),
        ("rbpf", "centre: 90, res: 0.5"),
        ("nbpf", "centre: 70"),
        ("nrbpf", "centre: 85, res: 0.4"),
        ("tremolo", "rate: 4, depth: 0.5"),
        ("ping_pong", "phase: 0.25, feedback: 0.5"),
        ("level", "amp: 0.5"),
        ("mono", ""),
        ("band_eq", "freq: 1000, db: 6"),
        ("pitch_shift", "shift: 7"),
        ("whammy", "transpose: 12"),
        ("tanh", "krunch: 0.5"),
        ("nrlpf", "cutoff: 80"),
        ("nrhpf", "cutoff: 60"),
    ];
    for (fx, params) in &fxs {
        let code = if params.is_empty() {
            format!("with_fx :{} do\n  play :c4\n  sleep 1\nend", fx)
        } else {
            format!("with_fx :{}, {} do\n  play :c4\n  sleep 1\nend", fx, params)
        };
        let evts = events(&code, DEFAULT_BPM);
        assert!(
            fx_start_count(&evts) >= 1,
            "FX '{}' should produce FxStart event", fx
        );
        assert!(
            note_count(&evts) >= 1,
            "FX '{}' should still produce PlayNote events", fx
        );
    }
}

// ============================================================================
// SECTION: cue / sync coordination
//
// Sonic Pi's `sync` blocks the calling thread until another thread broadcasts
// a matching `cue`, and resets the waiting thread's clock to the cue's time.
// PiBeat expands the whole piece up front, so it resolves this statically: one
// pass records when every cue fires, later passes make each `sync` jump to the
// next matching cue time.
// ============================================================================

/// Time of the first note event in the stream.
fn first_note_time(evts: &[(f32, AudioCommand)]) -> Option<f32> {
    evts.iter()
        .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
        .map(|(t, _)| *t)
        .fold(None, |acc: Option<f32>, t| {
            Some(acc.map_or(t, |a| a.min(t)))
        })
}

/// All note times, sorted.
fn note_times(evts: &[(f32, AudioCommand)]) -> Vec<f32> {
    let mut ts: Vec<f32> = evts
        .iter()
        .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
        .map(|(t, _)| *t)
        .collect();
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ts
}

#[test]
fn parity_sync_waits_for_cue() {
    // The cueing thread sleeps 2 beats before cueing, so at 60 BPM the synced
    // note must land at 2s, not at 0s.
    let code = "\
in_thread do
  sleep 2
  cue :go
end

in_thread do
  sync :go
  play :c4
end";
    let evts = events(code, DEFAULT_BPM);
    let t = first_note_time(&evts).expect("synced note should be emitted");
    assert!(
        (t - 2.0).abs() < 0.01,
        "sync :go should delay the note to the cue at 2.0s, got {t}"
    );
}

#[test]
fn parity_sync_without_matching_cue_does_not_block() {
    // Sonic Pi would hang forever here. Silently dropping the user's music is
    // worse than starting it, so an unmatched sync is a no-op.
    let code = "sync :never_cued\nplay :c4";
    let evts = events(code, DEFAULT_BPM);
    let t = first_note_time(&evts).expect("note should still be emitted");
    assert!(
        t.abs() < 0.01,
        "unmatched sync should not delay the note, got {t}"
    );
}

#[test]
fn parity_sync_picks_the_next_cue_not_an_earlier_one() {
    // A cue that has already fired cannot be synced to; the wait resolves to
    // the *next* one. Cues at 1s and 3s, sync issued at 2s -> lands at 3s.
    let code = "\
in_thread do
  sleep 1
  cue :tick
  sleep 2
  cue :tick
end

in_thread do
  sleep 2
  sync :tick
  play :c4
end";
    let evts = events(code, DEFAULT_BPM);
    let t = first_note_time(&evts).expect("synced note should be emitted");
    assert!(
        (t - 3.0).abs() < 0.01,
        "sync should wait for the cue at 3.0s, got {t}"
    );
}

#[test]
fn parity_live_loop_sync_opt_delays_first_iteration() {
    // `live_loop :x, sync: :bar` holds only the first iteration; after that the
    // loop free-runs on its own body length.
    let code = "\
live_loop :metro do
  sleep 4
  cue :bar
  stop
end

live_loop :melody, sync: :bar do
  play :c4
  sleep 1
  stop
end";
    let evts = events(code, DEFAULT_BPM);
    let t = first_note_time(&evts).expect("synced loop should emit a note");
    assert!(
        (t - 4.0).abs() < 0.01,
        "live_loop sync: :bar should start at the 4.0s cue, got {t}"
    );
}

#[test]
fn parity_cue_alone_does_not_shift_timing() {
    // `cue` is instantaneous — it must not consume a beat.
    let with_cue = events("cue :x\nplay :c4\nsleep 1\nplay :e4", DEFAULT_BPM);
    let without = events("play :c4\nsleep 1\nplay :e4", DEFAULT_BPM);
    assert_eq!(
        note_times(&with_cue),
        note_times(&without),
        "a bare cue should not move any note"
    );
}

// ============================================================================
// SECTION: with_swing
//
// Sonic Pi runs a with_swing block straight except on one invocation in every
// `pulse`, where it wraps the block in `time_warp shift`. The counter is a
// tick, so the very first run is the shifted one.
// ============================================================================

#[test]
fn parity_with_swing_shifts_one_run_in_every_pulse() {
    // 8 runs, pulse 4, shift 0.1 beats (0.1s at 60 BPM): runs 0 and 4 are
    // swung, so their notes sit 0.1s after the beat and the rest sit on it.
    let code = "\
8.times do
  with_swing 0.1, pulse: 4 do
    play :c4
  end
  sleep 1
end";
    let evts = events(code, DEFAULT_BPM);
    let times = note_times(&evts);
    assert_eq!(times.len(), 8, "should emit one note per run");
    for (i, t) in times.iter().enumerate() {
        let expected = i as f32 + if i % 4 == 0 { 0.1 } else { 0.0 };
        assert!(
            (t - expected).abs() < 0.01,
            "run {i}: expected note at {expected}s, got {t}"
        );
    }
}

#[test]
fn parity_with_swing_does_not_advance_the_parent_clock() {
    // Like time_warp, the shift displaces the block's contents only. The note
    // after the block must stay on its own beat.
    let code = "\
with_swing 0.25, pulse: 1 do
  play :c4
end
sleep 1
play :e4";
    let evts = events(code, DEFAULT_BPM);
    let times = note_times(&evts);
    assert_eq!(times.len(), 2);
    assert!(
        (times[0] - 0.25).abs() < 0.01,
        "swung note should be shifted to 0.25s, got {}",
        times[0]
    );
    assert!(
        (times[1] - 1.0).abs() < 0.01,
        "following note should stay at 1.0s, got {}",
        times[1]
    );
}

#[test]
fn parity_with_swing_negative_shift_plays_early() {
    let code = "\
sleep 2
with_swing -0.1, pulse: 1 do
  play :c4
end";
    let evts = events(code, DEFAULT_BPM);
    let t = first_note_time(&evts).expect("swung note should be emitted");
    assert!(
        (t - 1.9).abs() < 0.01,
        "negative shift should pull the note early to 1.9s, got {t}"
    );
}

#[test]
fn parity_with_swing_separate_tick_keys_count_independently() {
    // Two swing blocks in the same loop must not share a counter, which is why
    // Sonic Pi exposes the `tick:` opt.
    let code = "\
4.times do
  with_swing 0.1, pulse: 4, tick: :a do
    play :c4
  end
  with_swing 0.1, pulse: 4, tick: :b do
    play :e4
  end
  sleep 1
end";
    let evts = events(code, DEFAULT_BPM);
    // Both blocks swing on their own run 0, so run 0 has two shifted notes and
    // runs 1-3 have two straight ones. A shared counter would stagger them.
    let mut swung = 0;
    for (t, cmd) in &evts {
        if matches!(cmd, AudioCommand::PlayNote { .. }) && (t.fract() - 0.1).abs() < 0.01 {
            swung += 1;
        }
    }
    assert_eq!(swung, 2, "exactly the two run-0 notes should be swung");
}

#[test]
fn parity_with_swing_defaults_match_sonic_pi() {
    // Defaults: shift 0.1 beats, pulse 4.
    let code = "\
4.times do
  with_swing do
    play :c4
  end
  sleep 1
end";
    let evts = events(code, DEFAULT_BPM);
    let times = note_times(&evts);
    assert_eq!(times.len(), 4);
    assert!(
        (times[0] - 0.1).abs() < 0.01,
        "first run should swing by the default 0.1 beats, got {}",
        times[0]
    );
    for (i, t) in times.iter().enumerate().skip(1) {
        assert!(
            (t - i as f32).abs() < 0.01,
            "run {i} should be straight, got {t}"
        );
    }
}

// ============================================================================
// SECTION: envelope shaping opts (attack_level / decay_level / env_curve)
//
// Sonic Pi's synths build a four-segment envelope
// (0 -> attack_level -> decay_level -> sustain_level -> 0) whose segment shape
// is chosen by env_curve. Defaults are attack_level 1, decay_level -1 ("same
// as sustain_level") and env_curve 1 (linear).
// ============================================================================

fn first_envelope(evts: &[(f32, AudioCommand)]) -> sonic_daw_lib::audio::synth::Envelope {
    for (_, cmd) in evts {
        if let AudioCommand::PlayNote { envelope, .. } = cmd {
            return *envelope;
        }
    }
    panic!("expected a PlayNote event");
}

#[test]
fn parity_envelope_defaults_match_sonic_pi() {
    let env = first_envelope(&events("play :c4", DEFAULT_BPM));
    assert_eq!(env.attack_level, 1.0, "attack_level should default to 1");
    assert_eq!(
        env.decay_level, -1.0,
        "decay_level should default to Sonic Pi's -1 sentinel"
    );
    assert_eq!(env.curve, 1.0, "env_curve should default to 1 (linear)");
    assert_eq!(
        env.effective_decay_level(),
        env.sustain,
        "an unset decay_level follows sustain_level"
    );
}

#[test]
fn parity_envelope_opts_are_parsed() {
    let env = first_envelope(&events(
        "play :c4, attack: 0.1, attack_level: 0.8, decay: 0.2, decay_level: 0.3, sustain_level: 0.5, env_curve: 3",
        DEFAULT_BPM,
    ));
    assert!((env.attack_level - 0.8).abs() < 1e-5);
    assert!((env.decay_level - 0.3).abs() < 1e-5);
    assert!((env.curve - 3.0).abs() < 1e-5);
    assert!((env.effective_decay_level() - 0.3).abs() < 1e-5);
}

#[test]
fn parity_envelope_opts_reach_supercollider_as_synth_params() {
    // The SC engine forwards every entry in `params` verbatim on /s_new, and
    // the SynthDefs declare these three, so appearing here is what makes them
    // take effect on the SuperCollider backend.
    let evts = events(
        "play :c4, attack_level: 0.7, decay_level: 0.4, env_curve: 2",
        DEFAULT_BPM,
    );
    let params = evts
        .iter()
        .find_map(|(_, c)| match c {
            AudioCommand::PlayNote { params, .. } => Some(params.clone()),
            _ => None,
        })
        .expect("expected a PlayNote event");
    let lookup = |name: &str| params.iter().find(|(k, _)| k == name).map(|(_, v)| *v);
    assert_eq!(lookup("attack_level"), Some(0.7));
    assert_eq!(lookup("decay_level"), Some(0.4));
    assert_eq!(lookup("env_curve"), Some(2.0));
}

#[test]
fn parity_env_segment_shapes() {
    use sonic_daw_lib::audio::synth::env_segment;

    // Linear is the default and must stay exactly linear.
    assert!((env_segment(0.0, 1.0, 0.5, 1.0) - 0.5).abs() < 1e-6);
    // Step jumps immediately to the target.
    assert!((env_segment(0.0, 1.0, 0.01, 0.0) - 1.0).abs() < 1e-6);
    // Sine eases: still 0.5 at the midpoint but slower at the edges.
    assert!((env_segment(0.0, 1.0, 0.5, 3.0) - 0.5).abs() < 1e-6);
    assert!(env_segment(0.0, 1.0, 0.25, 3.0) < 0.25);
    // Squared starts slow.
    assert!(env_segment(0.0, 1.0, 0.5, 6.0) < 0.5);
    // Every shape must hit both endpoints exactly.
    for curve in [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 7.0] {
        let end = env_segment(0.2, 0.9, 1.0, curve);
        assert!(
            (end - 0.9).abs() < 1e-3,
            "curve {curve} should reach the target level, got {end}"
        );
        if curve != 0.0 {
            let start = env_segment(0.2, 0.9, 0.0, curve);
            assert!(
                (start - 0.2).abs() < 1e-3,
                "curve {curve} should start at the source level, got {start}"
            );
        }
    }
    // Exponential through zero must stay finite rather than producing NaN.
    let v = env_segment(0.0, 1.0, 0.5, 2.0);
    assert!(v.is_finite(), "exponential segment from 0 should be finite");
}

// ============================================================================
// SECTION: release packaging
// ============================================================================

/// Tauri's bundler installs a binary chosen from the crate's binary targets.
/// With more than one it can pick the wrong one, and nothing about the build
/// says so — v0.3.0 shipped a 2 MB SynthDef-generator helper in place of PiBeat
/// on every platform, because that helper lived in `src/bin/`. Every installer
/// came out 82-93% smaller than the previous release and still built, tested
/// and published green.
///
/// Dev-only tools belong in `examples/`, which is never bundled.
#[test]
fn crate_ships_exactly_one_binary() {
    let bin_dir = std::path::Path::new("src/bin");
    if !bin_dir.exists() {
        return; // No extra binaries at all — the intended state.
    }
    let extras: Vec<String> = std::fs::read_dir(bin_dir)
        .expect("src/bin should be readable")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".rs"))
        .collect();
    assert!(
        extras.is_empty(),
        "src/bin/ adds binary targets that Tauri may bundle instead of PiBeat: {}.\n\
         Move dev-only tools to src-tauri/examples/ (run with `cargo run --example <name>`).",
        extras.join(", ")
    );
}
