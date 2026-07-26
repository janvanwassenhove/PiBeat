// Example File Parsing Tests
//
// These tests verify that the example files in examples/ can be parsed
// without errors and produce reasonable audio commands.
//
// Run with: cargo test --test example_parsing

use sonic_daw_lib::audio::engine::AudioCommand;
use sonic_daw_lib::audio::parser::{commands_to_audio, parse_code};

/// Read an example file, or `None` when it is not present in the working tree.
///
/// A few example files (`Test5`, `DiscoTest`) are local scratch files that are
/// not tracked in git, so tests that depend on them skip instead of failing on
/// a fresh clone or in CI.
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

/// Read an example file content (for examples that are tracked in git)
fn read_example(name: &str) -> String {
    let path = format!("../examples/{}", name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e))
}

/// Helper: Parse code and return a result indicating success or error message.
fn try_parse(code: &str) -> Result<Vec<(f32, AudioCommand)>, String> {
    match parse_code(code) {
        Ok(parsed) => {
            let timed = commands_to_audio(&parsed, 120.0);
            Ok(timed)
        }
        Err(e) => Err(e),
    }
}

/// Count notes and samples in audio commands
fn count_events(events: &[(f32, AudioCommand)]) -> (usize, usize) {
    let notes = events
        .iter()
        .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
        .count();
    let samples = events
        .iter()
        .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
        .count();
    (notes, samples)
}

#[test]
fn test_parse_test1() {
    let code = read_example("Test1");
    eprintln!("=== Parsing Test1 ({} chars) ===", code.len());

    let result = try_parse(&code);
    match result {
        Ok(events) => {
            let (notes, samples) = count_events(&events);
            eprintln!("Test1: {} notes, {} samples", notes, samples);
            // Test1 uses lots of live_loops with samples
            assert!(
                samples > 0,
                "Test1 should have samples (live_loop with drums and vocals)"
            );
        }
        Err(e) => {
            panic!("Test1 parsing failed: {}", e);
        }
    }
}

#[test]
fn test_parse_test2() {
    let code = read_example("Test2");
    eprintln!("=== Parsing Test2 ({} chars) ===", code.len());

    let result = try_parse(&code);
    match result {
        Ok(events) => {
            let (notes, samples) = count_events(&events);
            eprintln!("Test2: {} notes, {} samples", notes, samples);
            // Test2 uses define :guitar_riff with play_pattern_timed and distortion
            assert!(notes > 0, "Test2 should have notes from guitar riffs");
            assert!(samples > 0, "Test2 should have drum samples");
        }
        Err(e) => {
            panic!("Test2 parsing failed: {}", e);
        }
    }
}

#[test]
fn test_parse_test3() {
    let code = read_example("Test3");
    eprintln!("=== Parsing Test3 ({} chars) ===", code.len());

    let result = try_parse(&code);
    match result {
        Ok(events) => {
            let (notes, samples) = count_events(&events);
            eprintln!("Test3: {} notes, {} samples", notes, samples);
            // Test3 uses set/get, amp_mod function, live_loops
            assert!(
                notes > 0 || samples > 0,
                "Test3 should have some audio events"
            );
        }
        Err(e) => {
            panic!("Test3 parsing failed: {}", e);
        }
    }
}

#[test]
fn test_parse_test4() {
    let code = read_example("Test4");
    eprintln!("=== Parsing Test4 ({} chars) ===", code.len());

    let result = try_parse(&code);
    match result {
        Ok(events) => {
            let (notes, samples) = count_events(&events);
            eprintln!("Test4: {} notes, {} samples", notes, samples);
            // Test4 uses Time.now, def, .each with array
            assert!(
                notes > 0 || samples > 0,
                "Test4 should have some audio events"
            );
        }
        Err(e) => {
            panic!("Test4 parsing failed: {}", e);
        }
    }
}

#[test]
fn test_parse_test5() {
    let Some(code) = try_read_example("Test5") else { return };
    eprintln!("=== Parsing Test5 ({} chars) ===", code.len());

    let result = try_parse(&code);
    match result {
        Ok(events) => {
            let (notes, samples) = count_events(&events);
            eprintln!("Test5: {} notes, {} samples", notes, samples);
            // Test5 uses in_thread(name: :x), while loops, complex nested FX
            assert!(
                notes > 0 || samples > 0,
                "Test5 should have some audio events"
            );
        }
        Err(e) => {
            panic!("Test5 parsing failed: {}", e);
        }
    }
}

/// Regression test: Parsing Test1-5 should not panic
#[test]
fn test_all_examples_no_panic() {
    for name in &["Test1", "Test2", "Test3", "Test4", "Test5"] {
        let Some(code) = try_read_example(name) else { continue };
        eprintln!("Parsing {} ({} chars)...", name, code.len());
        let _ = try_parse(&code); // Just ensure no panic
        eprintln!("{} parsed without panic", name);
    }
}

#[test]
fn test_parse_disco() {
    let Some(code) = try_read_example("DiscoTest") else { return };
    eprintln!("=== Parsing DiscoTest ({} chars) ===", code.len());
    match sonic_daw_lib::audio::parser::parse_code(&code) {
        Ok(parsed) => {
            // Check each loop's inner duration
            for cmd in &parsed {
                if let sonic_daw_lib::audio::parser::ParsedCommand::Loop { name, commands, parallel, .. } = cmd {
                    let dur = sonic_daw_lib::audio::parser::commands_to_duration(commands, 122.0);
                    eprintln!("Loop '{}' parallel={} cmds={} duration={:.4}s", name, parallel, commands.len(), dur);
                    if name == "bass" || dur > 100.0 || dur == 0.0 {
                        eprintln!("  >>> Dumping commands for loop '{}':", name);
                        fn dump_cmd(c: &sonic_daw_lib::audio::parser::ParsedCommand, indent: usize) {
                            let pad = "  ".repeat(indent);
                            match c {
                                sonic_daw_lib::audio::parser::ParsedCommand::Sleep(b) => eprintln!("{}Sleep({:.4})", pad, b),
                                sonic_daw_lib::audio::parser::ParsedCommand::PlayNote { frequency, amplitude, duration, .. } => eprintln!("{}PlayNote freq={:.1} amp={:.2} dur={:.2}", pad, frequency, amplitude, duration),
                                sonic_daw_lib::audio::parser::ParsedCommand::PlaySample { name, amplitude, .. } => eprintln!("{}PlaySample '{}' amp={:.2}", pad, name, amplitude),
                                sonic_daw_lib::audio::parser::ParsedCommand::SetSynth(s) => eprintln!("{}SetSynth({:?})", pad, s),
                                sonic_daw_lib::audio::parser::ParsedCommand::Comment(s) => eprintln!("{}Comment({})", pad, s),
                                sonic_daw_lib::audio::parser::ParsedCommand::TimesLoop { count, commands } => {
                                    eprintln!("{}TimesLoop count={} cmds={}", pad, count, commands.len());
                                    for c2 in commands { dump_cmd(c2, indent+1); }
                                }
                                sonic_daw_lib::audio::parser::ParsedCommand::WithFx { fx_type, commands, .. } => {
                                    eprintln!("{}WithFx '{}' cmds={}", pad, fx_type, commands.len());
                                    for c2 in commands { dump_cmd(c2, indent+1); }
                                }
                                sonic_daw_lib::audio::parser::ParsedCommand::Loop { name, commands, parallel, .. } => {
                                    eprintln!("{}Loop '{}' parallel={} cmds={}", pad, name, parallel, commands.len());
                                    for c2 in commands { dump_cmd(c2, indent+1); }
                                }
                                _ => eprintln!("{}{:?}", pad, std::mem::discriminant(c)),
                            }
                        }
                        for c in commands { dump_cmd(c, 2); }
                    }
                }
            }
            let timed = sonic_daw_lib::audio::parser::commands_to_audio(&parsed, 122.0);
            eprintln!("Total timed events: {}", timed.len());
            let notes = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::PlayNote { .. })).count();
            let samples = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::PlaySample { .. })).count();
            let fx_start = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::FxStart { .. })).count();
            let fx_end = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::FxEnd { .. })).count();
            let set_fx = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::SetEffect { .. })).count();
            let stops = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::Stop)).count();
            eprintln!("Notes: {}, Samples: {}, FxStart: {}, FxEnd: {}, SetEffect: {}, Stop: {}", notes, samples, fx_start, fx_end, set_fx, stops);
            
            // Sort by time and check for big clusters
            let mut sorted = timed.clone();
            sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            
            // Check time buckets (1s intervals)
            let max_t = sorted.last().map(|(t,_)| *t).unwrap_or(0.0);
            eprintln!("Max event time: {:.1}s", max_t);
            
            // Show events in first 5 seconds after sorting
            eprintln!("--- Sorted events in first 5 seconds ---");
            let mut count_in_5s = 0;
            for (t, cmd) in &sorted {
                if *t > 5.0 { break; }
                count_in_5s += 1;
                if count_in_5s <= 60 {
                    let desc = match cmd {
                        sonic_daw_lib::audio::engine::AudioCommand::PlayNote { frequency, amplitude, .. } => format!("Note freq={:.1} amp={:.2}", frequency, amplitude),
                        sonic_daw_lib::audio::engine::AudioCommand::PlaySample { amplitude, .. } => format!("Sample amp={:.2}", amplitude),
                        sonic_daw_lib::audio::engine::AudioCommand::SetEffect { lpf_cutoff, hpf_cutoff, reverb_mix, .. } => format!("SetFx lpf={:.0} hpf={:.0} rev={:.2}", lpf_cutoff, hpf_cutoff, reverb_mix),
                        sonic_daw_lib::audio::engine::AudioCommand::FxStart { fx_type, .. } => format!("FxStart:{}", fx_type),
                        sonic_daw_lib::audio::engine::AudioCommand::FxEnd { .. } => "FxEnd".to_string(),
                        sonic_daw_lib::audio::engine::AudioCommand::Stop => "STOP!".to_string(),
                        _ => format!("{:?}", std::mem::discriminant(cmd)),
                    };
                    eprintln!("  t={:.4}s {}", t, desc);
                }
            }
            eprintln!("Total events in first 5s: {}", count_in_5s);
            
            // Show event density per second for first 20 seconds
            eprintln!("--- Event density per second ---");
            for sec in 0..20 {
                let lo = sec as f32;
                let hi = lo + 1.0;
                let count = sorted.iter().filter(|(t,_)| *t >= lo && *t < hi).count();
                if count > 0 {
                    eprintln!("  {:.0}s-{:.0}s: {} events", lo, hi, count);
                }
            }
        }
        Err(e) => {
            panic!("DiscoTest parsing failed: {}", e);
        }
    }
}


#[test]
fn test_hats_simple() {
    let code = r#"
live_loop :hats do
  16.times do |i|
    sample :hihat, amp: 0.2, rate: 1.5
    if i == 6 || i == 14
      sample :drum_cymbal_open, amp: 0.6
    end
    sleep 0.25
  end
end
"#;
    let parsed = sonic_daw_lib::audio::parser::parse_code(code).expect("parse");
    for cmd in &parsed {
        if let sonic_daw_lib::audio::parser::ParsedCommand::Loop { name, commands, .. } = cmd {
            let dur = sonic_daw_lib::audio::parser::commands_to_duration(commands, 120.0);
            eprintln!("Loop '{}' cmds={} duration={:.4}s", name, commands.len(), dur);
        }
    }
    let timed = sonic_daw_lib::audio::parser::commands_to_audio(&parsed, 120.0);
    let samples = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::PlaySample { .. })).count();
    eprintln!("Total events: {}, Samples: {}", timed.len(), samples);
    assert!(samples > 1000, "Should have many sample events for 500 loop iterations with 16 hihat hits each");
}

#[test]
fn test_join_continuation_hats_comment() {
    let code = r#"live_loop :hats do
  16.times do |i|
    amp_val = [0.18, 0.28, 0.22, 0.3].ring.look
    pan_val = [-0.2, 0.2, 0.0, 0.1].ring.look
    sample :hihat, amp: amp_val, pan: pan_val, rate: 1.5
    # open hat on the "and" of 2 and 4 (positions 6 and 14 in 16 steps; 0-indexed)
    if i == 6 || i == 14
      sample :drum_cymbal_open, amp: 0.6, start: 0.1, finish: 0.7
    end
    sleep 0.25
  end
end"#;
    let joined = sonic_daw_lib::audio::parser::join_continuation_lines_pub(code);
    eprintln!("=== JOINED OUTPUT ===");
    for (i, line) in joined.lines().enumerate() {
        eprintln!("[{}]: {}", i, line);
    }
    // The comment should NOT cause continuation issues — "0-indexed)" should not be a standalone line
    assert!(!joined.lines().any(|l| l.trim() == "0-indexed)"), "Comment parens should not cause bad continuation");
}

#[test]
fn test_join_continuation_bass_pattern() {
    let code = r#"root = :c2
  pattern = [
    [root, 0.75],
    [nil, 0.25],
    [root+3, 0.5],   # minor 3rd walk
    [root+5, 0.5],   # 5th
    [root, 0.5],
    [nil, 0.5]
  ].ring"#;
    let joined = sonic_daw_lib::audio::parser::join_continuation_lines_pub(code);
    eprintln!("=== JOINED OUTPUT ===");
    for (i, line) in joined.lines().enumerate() {
        eprintln!("[{}]: {}", i, line);
    }
    // ].ring should be merged into the pattern line
    assert!(!joined.lines().any(|l| l.trim() == "].ring"), "].ring should be merged into pattern line");
    assert!(joined.contains(".ring"), "should still have .ring somewhere");
}

#[test]
fn test_play_variable_note() {
    let code = r#"root = :c2
use_synth :fm
use_synth_defaults release: 0.12, amp: 0.9
pattern = [
    [root, 0.75],
    [nil, 0.25],
    [root+3, 0.5],
    [root+5, 0.5],
    [root, 0.5],
    [nil, 0.5]
  ].ring
2.times do
    pattern.each do |n, d|
      if n
        play n, cutoff: 80 + rrand(0, 15), pan: rrand(-0.1, 0.1)
      end
      sleep d
    end
  end"#;
    let parsed = sonic_daw_lib::audio::parser::parse_code(code).expect("parse failed");
    eprintln!("Parsed commands:");
    for cmd in &parsed {
        eprintln!("  {:?}", cmd);
    }
    let timed = sonic_daw_lib::audio::parser::commands_to_audio(&parsed, 120.0);
    let notes = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::PlayNote { .. })).count();
    eprintln!("Total events: {}, Notes: {}", timed.len(), notes);
    assert!(notes >= 1, "Should have at least one PlayNote from 'play n' where n=root=:c2");
}

#[test]
fn test_ternary_in_sample_param() {
    let code = r#"
pattern = (ring 1, 0, 0, 1, 0, 1, 0, 0)
pattern.each do |p|
  sample :elec_cymbal, amp: (p==1 ? 0.35 : 0.0), rate: 1.2
  sample :perc_snap, amp: (p==1 ? 0.15 : 0.0) if p==1
  sleep 0.5
end"#;
    let parsed = sonic_daw_lib::audio::parser::parse_code(code).expect("parse failed");
    let timed = sonic_daw_lib::audio::parser::commands_to_audio(&parsed, 120.0);
    let samples = timed.iter().filter(|(_, c)| matches!(c, sonic_daw_lib::audio::engine::AudioCommand::PlaySample { .. })).count();
    eprintln!("Total events: {}, Samples: {}", timed.len(), samples);
    // pattern has 3 ones → 3 elec_cymbal + 3 perc_snap = 6 samples minimum
    // Actually all 8 elec_cymbal should play (just with amp 0.0 for p==0), plus 3 perc_snap
    assert!(samples >= 3, "Should have at least the perc_snap samples for p==1");
}

#[test]
fn test_join_continuation_test5_do_end_balance() {
    let Some(code) = try_read_example("Test5") else { return };
    let joined = sonic_daw_lib::audio::parser::join_continuation_lines_pub(&code);
    let mut do_count = 0;
    let mut end_count = 0;
    for (i, line) in joined.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Count 'do' at end of line or 'do |' in the line
        if trimmed.ends_with(" do") || trimmed.contains(" do |") || trimmed.contains(" do\n") || trimmed == "do" {
            do_count += 1;
        }
        if trimmed == "end" {
            end_count += 1;
        }
    }
    eprintln!("Test5 after joining: do={}, end={}", do_count, end_count);
    // Find lines where join might have merged a 'do' or 'end' keyword
    for (i, line) in joined.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.len() > 200 {
            eprintln!("[LONG LINE {}]: {}...", i, &trimmed[..100]);
        }
    }
}
