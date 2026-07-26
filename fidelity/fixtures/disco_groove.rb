# Disco groove — full-composition fixture.
#
# Exercises a broad slice of the supported Sonic Pi surface in one file:
# use_bpm, live_loop, in_thread, with_fx (nested), define, set/get,
# play_pattern_timed, ring/.tick, spread, samples with params, and cue/sync.
#
# Read by:
#   src-tauri/tests/disco_groove_test.rs
#   src-tauri/tests/fidelity_snapshots.rs   (snapshot_disco_groove)
#   src-tauri/tests/parity_validation.rs    (parity_disco_groove_parses)

use_bpm 122
use_random_seed 4242

set :master_amp, 1.0

define :amp_mod do |v|
  v * get(:master_amp)
end

bass_line = ring(:e2, :e2, :g2, :a2, :b2, :a2, :g2, :e2)
stabs = ring(:e4, :g4, :b4, :d5)

live_loop :metro do
  cue :bar
  sleep 4
end

live_loop :kick do
  sample :bd_haus, amp: amp_mod(1.6), rate: 1
  sleep 1
end

live_loop :hats do
  8.times do |i|
    sample :drum_cymbal_closed, amp: amp_mod(0.5), rate: 1.1
    sleep 0.5
  end
end

live_loop :snare do
  sleep 1
  sample :sn_dolf, amp: amp_mod(1.1)
  sleep 1
end

live_loop :bass, sync: :bar do
  use_synth :tb303
  8.times do |i|
    play bass_line.tick, amp: amp_mod(0.8), release: 0.22,
      cutoff: 70 + (i * 4), res: 0.8
    sleep 0.5
  end
end

live_loop :chords, sync: :bar do
  with_fx :reverb, room: 0.7, mix: 0.35 do
    use_synth :prophet
    play chord(:e3, :minor7), amp: amp_mod(0.4), release: 1.2
    sleep 2
    play chord(:a3, :minor7), amp: amp_mod(0.4), release: 1.2
    sleep 2
  end
end

live_loop :lead, sync: :bar do
  with_fx :echo, phase: 0.375, mix: 0.3 do
    with_fx :lpf, cutoff: 105 do
      use_synth :blade
      play_pattern_timed [stabs.tick, stabs.tick, stabs.tick],
        [0.25, 0.25, 0.5], amp: amp_mod(0.35), release: 0.4
      sleep 1
    end
  end
end

in_thread do
  sleep 32
  set :master_amp, 0.6
end
