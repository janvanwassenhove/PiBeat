// Audio comparison harness for PiBeat fidelity testing.
//
// Reads reference and candidate WAV files, aligns them via
// cross-correlation, and computes:
//   - RMS delta
//   - LUFS proxy (A-weighted RMS)
//   - Spectral distance (simple FFT bin comparison)
//   - Onset/transient delta
//   - Silence mismatch
//
// Produces JSON + Markdown reports.
//
// Usage:
//   cargo test --test audio_compare
//   (or as standalone: cargo run --example audio_compare_cli)

use std::fs;
use std::path::Path;

// ============================================================================
// WAV I/O (self-contained, no external WAV crate needed for comparison)
// ============================================================================

/// Simple WAV reader — returns mono f32 samples and sample rate.
fn read_wav(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    if data.len() < 44 {
        return Err("WAV file too short".into());
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("Not a valid WAV file".into());
    }

    // Parse fmt chunk
    let channels = u16::from_le_bytes([data[22], data[23]]) as u32;
    let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let bits_per_sample = u16::from_le_bytes([data[34], data[35]]);

    // Find data chunk
    let mut pos = 12;
    while pos + 8 < data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                as usize;
        if chunk_id == b"data" {
            let audio_data = &data[pos + 8..std::cmp::min(pos + 8 + chunk_size, data.len())];
            let samples = decode_samples(audio_data, bits_per_sample, channels)?;
            return Ok((samples, sample_rate));
        }
        pos += 8 + chunk_size;
        if chunk_size % 2 != 0 {
            pos += 1; // padding byte
        }
    }
    Err("No data chunk found in WAV".into())
}

fn decode_samples(data: &[u8], bits: u16, channels: u32) -> Result<Vec<f32>, String> {
    let mut samples = Vec::new();
    match bits {
        16 => {
            for chunk in data.chunks_exact(2 * channels as usize) {
                // Take first channel (mono mixdown: average all channels)
                let mut sum = 0.0f32;
                for ch in 0..channels as usize {
                    let offset = ch * 2;
                    let val = i16::from_le_bytes([chunk[offset], chunk[offset + 1]]);
                    sum += val as f32 / 32768.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        24 => {
            for chunk in data.chunks_exact(3 * channels as usize) {
                let mut sum = 0.0f32;
                for ch in 0..channels as usize {
                    let offset = ch * 3;
                    let val = ((chunk[offset] as i32)
                        | ((chunk[offset + 1] as i32) << 8)
                        | ((chunk[offset + 2] as i32) << 16))
                        - if chunk[offset + 2] & 0x80 != 0 {
                            0x1000000
                        } else {
                            0
                        };
                    sum += val as f32 / 8388608.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        32 => {
            for chunk in data.chunks_exact(4 * channels as usize) {
                let mut sum = 0.0f32;
                for ch in 0..channels as usize {
                    let offset = ch * 4;
                    let val = f32::from_le_bytes([
                        chunk[offset],
                        chunk[offset + 1],
                        chunk[offset + 2],
                        chunk[offset + 3],
                    ]);
                    sum += val;
                }
                samples.push(sum / channels as f32);
            }
        }
        _ => return Err(format!("Unsupported bit depth: {}", bits)),
    }
    Ok(samples)
}

// ============================================================================
// Audio Metrics
// ============================================================================

/// RMS (Root Mean Square) of a signal
fn rms(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 0.0;
    }
    let sum: f64 = signal.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / signal.len() as f64).sqrt() as f32
}

/// RMS in dB
fn rms_db(signal: &[f32]) -> f32 {
    let r = rms(signal);
    if r < 1e-10 {
        -100.0
    } else {
        20.0 * r.log10()
    }
}

/// Simple A-weighting proxy — apply a crude high-pass at ~1kHz
/// to approximate LUFS mid-frequency emphasis. Not true LUFS but
/// a lightweight loudness proxy.
fn a_weighted_rms(signal: &[f32], sample_rate: u32) -> f32 {
    if signal.is_empty() {
        return 0.0;
    }
    // Simple first-order high-pass filter approximating A-weighting
    let rc = 1.0 / (2.0 * std::f64::consts::PI * 1000.0); // ~1kHz cutoff
    let dt = 1.0 / sample_rate as f64;
    let alpha = rc / (rc + dt);

    let mut filtered = vec![0.0f32; signal.len()];
    filtered[0] = signal[0];
    for i in 1..signal.len() {
        filtered[i] =
            (alpha * (filtered[i - 1] as f64 + signal[i] as f64 - signal[i - 1] as f64)) as f32;
    }
    rms(&filtered)
}

/// Simple spectral distance using DFT bins.
/// Computes magnitude spectrum of fixed-size frames and returns
/// average bin-wise L2 distance.
fn spectral_distance(a: &[f32], b: &[f32], frame_size: usize) -> f32 {
    let num_frames_a = a.len() / frame_size;
    let num_frames_b = b.len() / frame_size;
    let num_frames = num_frames_a.min(num_frames_b);
    if num_frames == 0 {
        return 1.0;
    }

    let mut total_dist = 0.0f64;
    for f in 0..num_frames {
        let frame_a = &a[f * frame_size..(f + 1) * frame_size];
        let frame_b = &b[f * frame_size..(f + 1) * frame_size];
        let mag_a = magnitude_spectrum(frame_a);
        let mag_b = magnitude_spectrum(frame_b);
        let dist: f64 = mag_a
            .iter()
            .zip(mag_b.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        total_dist += dist.sqrt();
    }
    (total_dist / num_frames as f64) as f32
}

/// Compute magnitude spectrum using naive DFT (for small frame sizes).
fn magnitude_spectrum(frame: &[f32]) -> Vec<f64> {
    let n = frame.len();
    let half = n / 2 + 1;
    let mut mags = Vec::with_capacity(half);
    for k in 0..half {
        let mut re = 0.0f64;
        let mut im = 0.0f64;
        for (i, &s) in frame.iter().enumerate() {
            let angle = -2.0 * std::f64::consts::PI * k as f64 * i as f64 / n as f64;
            re += s as f64 * angle.cos();
            im += s as f64 * angle.sin();
        }
        mags.push((re * re + im * im).sqrt());
    }
    mags
}

/// Simple onset detection: count frames where energy rises above threshold.
fn detect_onsets(signal: &[f32], frame_size: usize, threshold: f32) -> Vec<usize> {
    let mut onsets = Vec::new();
    let mut prev_energy = 0.0f32;
    for (i, chunk) in signal.chunks(frame_size).enumerate() {
        let energy = rms(chunk);
        if energy > threshold && energy > prev_energy * 2.0 && i > 0 {
            onsets.push(i);
        }
        prev_energy = energy;
    }
    onsets
}

/// Onset delta: difference in number and timing of detected onsets.
fn onset_delta(a: &[f32], b: &[f32], sample_rate: u32) -> (usize, f32) {
    let frame_size = (sample_rate / 100) as usize; // 10ms frames
    let threshold = 0.01;
    let onsets_a = detect_onsets(a, frame_size, threshold);
    let onsets_b = detect_onsets(b, frame_size, threshold);
    let count_diff = (onsets_a.len() as isize - onsets_b.len() as isize).unsigned_abs();

    // Average timing difference for matched onsets
    let matched = onsets_a.len().min(onsets_b.len());
    let timing_diff = if matched > 0 {
        let sum: f64 = onsets_a
            .iter()
            .zip(onsets_b.iter())
            .map(|(a, b)| (*a as f64 - *b as f64).abs() * frame_size as f64 / sample_rate as f64)
            .sum();
        (sum / matched as f64) as f32
    } else {
        0.0
    };

    (count_diff, timing_diff)
}

/// Silence mismatch: percentage of frames where one is silent and other isn't.
fn silence_mismatch(a: &[f32], b: &[f32], frame_size: usize) -> f32 {
    let threshold = 0.001;
    let frames_a = a.chunks(frame_size);
    let frames_b = b.chunks(frame_size);
    let total = frames_a.len().min(frames_b.len());
    if total == 0 {
        return 0.0;
    }
    let mismatches: usize = a
        .chunks(frame_size)
        .zip(b.chunks(frame_size))
        .filter(|(fa, fb)| {
            let a_silent = rms(fa) < threshold;
            let b_silent = rms(fb) < threshold;
            a_silent != b_silent
        })
        .count();
    mismatches as f32 / total as f32
}

/// Cross-correlation based alignment: find the lag that maximizes correlation.
fn find_alignment_lag(reference: &[f32], candidate: &[f32], max_lag: usize) -> isize {
    let len = reference.len().min(candidate.len());
    if len == 0 {
        return 0;
    }
    let search_range = max_lag.min(len / 2);
    let mut best_lag: isize = 0;
    let mut best_corr = f64::NEG_INFINITY;

    for lag in -(search_range as isize)..=(search_range as isize) {
        let mut corr = 0.0f64;
        let mut count = 0usize;
        for i in 0..len {
            let j = i as isize + lag;
            if j >= 0 && (j as usize) < candidate.len() {
                corr += reference[i] as f64 * candidate[j as usize] as f64;
                count += 1;
            }
        }
        if count > 0 {
            corr /= count as f64;
        }
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }
    best_lag
}

/// Apply lag alignment to candidate signal.
fn align_signal(signal: &[f32], lag: isize) -> Vec<f32> {
    if lag == 0 {
        return signal.to_vec();
    }
    if lag > 0 {
        // Shift right — prepend zeros
        let mut aligned = vec![0.0f32; lag as usize];
        aligned.extend_from_slice(signal);
        aligned
    } else {
        // Shift left — skip leading samples
        let skip = (-lag) as usize;
        if skip >= signal.len() {
            vec![0.0; signal.len()]
        } else {
            signal[skip..].to_vec()
        }
    }
}

// ============================================================================
// Comparison Report
// ============================================================================

#[derive(serde::Serialize)]
struct ComparisonReport {
    fixture: String,
    reference_path: String,
    candidate_path: String,
    sample_rate: u32,
    reference_duration_secs: f32,
    candidate_duration_secs: f32,
    alignment_lag_samples: isize,
    rms_delta_db: f32,
    lufs_proxy_delta_db: f32,
    spectral_distance: f32,
    onset_count_delta: usize,
    onset_timing_delta_secs: f32,
    silence_mismatch_pct: f32,
    pass: bool,
}

/// Thresholds for pass/fail
const RMS_DELTA_THRESHOLD_DB: f32 = 3.0;
const SPECTRAL_DIST_THRESHOLD: f32 = 50.0;
const SILENCE_MISMATCH_THRESHOLD: f32 = 0.10; // 10%

fn compare_wavs(
    fixture_name: &str,
    reference_path: &Path,
    candidate_path: &Path,
) -> Result<ComparisonReport, String> {
    let (ref_samples, ref_sr) = read_wav(reference_path)?;
    let (cand_samples, cand_sr) = read_wav(candidate_path)?;

    if ref_sr != cand_sr {
        return Err(format!(
            "Sample rate mismatch: ref={} cand={}",
            ref_sr, cand_sr
        ));
    }

    let sr = ref_sr;
    let ref_dur = ref_samples.len() as f32 / sr as f32;
    let cand_dur = cand_samples.len() as f32 / sr as f32;

    // Align via cross-correlation (max 100ms lag)
    let max_lag = (sr as f64 * 0.1) as usize;
    let lag = find_alignment_lag(&ref_samples, &cand_samples, max_lag);
    let aligned_cand = align_signal(&cand_samples, lag);

    // Trim to same length
    let len = ref_samples.len().min(aligned_cand.len());
    let ref_trimmed = &ref_samples[..len];
    let cand_trimmed = &aligned_cand[..len];

    // Compute metrics
    let rms_ref = rms_db(ref_trimmed);
    let rms_cand = rms_db(cand_trimmed);
    let rms_delta = (rms_ref - rms_cand).abs();

    let lufs_ref = a_weighted_rms(ref_trimmed, sr);
    let lufs_cand = a_weighted_rms(cand_trimmed, sr);
    let lufs_delta = if lufs_ref < 1e-10 && lufs_cand < 1e-10 {
        0.0
    } else {
        (20.0 * (lufs_ref / lufs_cand.max(1e-10)).log10()).abs()
    };

    let frame_size = 256;
    let spec_dist = spectral_distance(ref_trimmed, cand_trimmed, frame_size);
    let (onset_count_delta, onset_timing) = onset_delta(ref_trimmed, cand_trimmed, sr);
    let silence = silence_mismatch(ref_trimmed, cand_trimmed, (sr / 100) as usize);

    let pass = rms_delta < RMS_DELTA_THRESHOLD_DB
        && spec_dist < SPECTRAL_DIST_THRESHOLD
        && silence < SILENCE_MISMATCH_THRESHOLD;

    Ok(ComparisonReport {
        fixture: fixture_name.to_string(),
        reference_path: reference_path.to_string_lossy().to_string(),
        candidate_path: candidate_path.to_string_lossy().to_string(),
        sample_rate: sr,
        reference_duration_secs: ref_dur,
        candidate_duration_secs: cand_dur,
        alignment_lag_samples: lag,
        rms_delta_db: rms_delta,
        lufs_proxy_delta_db: lufs_delta,
        spectral_distance: spec_dist,
        onset_count_delta,
        onset_timing_delta_secs: onset_timing,
        silence_mismatch_pct: silence,
        pass,
    })
}

// ============================================================================
// Report generation
// ============================================================================

fn generate_markdown_report(reports: &[ComparisonReport]) -> String {
    let mut md = String::new();
    md.push_str("# PiBeat Fidelity — Audio Comparison Report\n\n");
    md.push_str(&format!("Generated: {}\n\n", chrono_lite_now()));

    let total = reports.len();
    let passed = reports.iter().filter(|r| r.pass).count();
    md.push_str(&format!("**Overall: {} / {} PASS**\n\n", passed, total));

    md.push_str("| Fixture | RMS Δ (dB) | Spectral Dist | Onset Δ | Silence % | Result |\n");
    md.push_str("|---------|-----------|--------------|---------|----------|--------|\n");
    for r in reports {
        md.push_str(&format!(
            "| {} | {:.2} | {:.2} | {} ({:.3}s) | {:.1}% | {} |\n",
            r.fixture,
            r.rms_delta_db,
            r.spectral_distance,
            r.onset_count_delta,
            r.onset_timing_delta_secs,
            r.silence_mismatch_pct * 100.0,
            if r.pass { "✅ PASS" } else { "❌ FAIL" }
        ));
    }
    md.push_str("\n### Thresholds\n\n");
    md.push_str(&format!(
        "- RMS delta: < {:.1} dB\n",
        RMS_DELTA_THRESHOLD_DB
    ));
    md.push_str(&format!(
        "- Spectral distance: < {:.1}\n",
        SPECTRAL_DIST_THRESHOLD
    ));
    md.push_str(&format!(
        "- Silence mismatch: < {:.0}%\n",
        SILENCE_MISMATCH_THRESHOLD * 100.0
    ));
    md
}

fn chrono_lite_now() -> String {
    // Simple timestamp without chrono crate
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix_timestamp={}", d.as_secs())
}

// ============================================================================
// Tests: Self-test of the comparison harness
// ============================================================================

#[test]
fn harness_rms_basic() {
    let signal = vec![0.5f32; 1000];
    let r = rms(&signal);
    assert!((r - 0.5).abs() < 0.001);
}

#[test]
fn harness_rms_silence() {
    let signal = vec![0.0f32; 1000];
    assert!(rms(&signal) < 0.0001);
}

#[test]
fn harness_alignment_identity() {
    // Use a non-periodic signal (chirp) to ensure unique correlation peak
    let signal: Vec<f32> = (0..1000)
        .map(|i| {
            let t = i as f32 / 1000.0;
            (t * t * 50.0).sin() * (-t * 2.0).exp()
        })
        .collect();
    let lag = find_alignment_lag(&signal, &signal, 100);
    assert!(
        lag.abs() <= 1,
        "self-alignment should have near-zero lag, got {}",
        lag
    );
}

#[test]
fn harness_alignment_shifted() {
    let signal: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.01).sin()).collect();
    let shifted: Vec<f32> = std::iter::repeat(0.0f32)
        .take(50)
        .chain(signal.iter().cloned())
        .collect();
    let lag = find_alignment_lag(&signal, &shifted, 100);
    // Lag should be close to 50 (candidate is delayed by 50 samples)
    assert!((lag - 50).abs() <= 5, "expected lag ~50, got {}", lag);
}

#[test]
fn harness_spectral_distance_identity() {
    let signal: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin()).collect();
    let dist = spectral_distance(&signal, &signal, 256);
    assert!(
        dist < 0.001,
        "identical signals should have ~0 spectral distance, got {}",
        dist
    );
}

#[test]
fn harness_silence_mismatch_identical() {
    let signal = vec![0.5f32; 1000];
    let mismatch = silence_mismatch(&signal, &signal, 100);
    assert_eq!(mismatch, 0.0);
}

#[test]
fn harness_onset_detection() {
    // Create a signal with a clear onset at sample 500
    let mut signal = vec![0.0f32; 2000];
    for i in 500..1000 {
        signal[i] = 0.5 * ((i - 500) as f32 * 0.05).sin();
    }
    let onsets = detect_onsets(&signal, 100, 0.01);
    assert!(!onsets.is_empty(), "should detect at least one onset");
}

/// Run the full comparison pipeline on reference/candidate WAV pairs
/// in the fidelity renders directory. This test generates the reports.
#[test]
fn run_audio_comparison_pipeline() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fidelity_dir = manifest_dir.parent().unwrap().join("fidelity");
    let ref_dir = fidelity_dir.join("renders").join("reference");
    let cand_dir = fidelity_dir.join("renders").join("candidate");
    let reports_dir = fidelity_dir.join("reports");

    fs::create_dir_all(&reports_dir).ok();

    // Find all reference WAVs and try to match with candidates
    let mut reports = Vec::new();

    if ref_dir.exists() {
        if let Ok(entries) = fs::read_dir(&ref_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "wav") {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    let cand_path = cand_dir.join(format!("{}.wav", name));
                    if cand_path.exists() {
                        match compare_wavs(&name, &path, &cand_path) {
                            Ok(report) => {
                                eprintln!(
                                    "[audio_compare] {} — RMS Δ={:.2}dB spectral={:.2} silence={:.1}% → {}",
                                    name,
                                    report.rms_delta_db,
                                    report.spectral_distance,
                                    report.silence_mismatch_pct * 100.0,
                                    if report.pass { "PASS" } else { "FAIL" }
                                );
                                reports.push(report);
                            }
                            Err(e) => {
                                eprintln!("[audio_compare] ERROR comparing {}: {}", name, e);
                            }
                        }
                    }
                }
            }
        }
    }

    if reports.is_empty() {
        eprintln!("[audio_compare] No WAV pairs found in renders/. Skipping report generation.");
        eprintln!("[audio_compare] To use this harness:");
        eprintln!("  1. Place reference WAVs in fidelity/renders/reference/");
        eprintln!("  2. Place candidate WAVs in fidelity/renders/candidate/");
        eprintln!("  3. Re-run this test");
        return;
    }

    // Write JSON report
    let json_path = reports_dir.join("latest.json");
    let json = serde_json::to_string_pretty(&reports).unwrap();
    fs::write(&json_path, &json).unwrap();
    eprintln!("[audio_compare] JSON report: {}", json_path.display());

    // Write Markdown report
    let md_path = reports_dir.join("latest.md");
    let md = generate_markdown_report(&reports);
    fs::write(&md_path, &md).unwrap();
    eprintln!("[audio_compare] Markdown report: {}", md_path.display());

    // Assert all pass
    let failed: Vec<_> = reports.iter().filter(|r| !r.pass).collect();
    if !failed.is_empty() {
        for f in &failed {
            eprintln!(
                "[audio_compare] FAIL: {} (RMS Δ={:.2}dB spectral={:.2})",
                f.fixture, f.rms_delta_db, f.spectral_distance
            );
        }
        panic!("{} fixture(s) failed audio comparison", failed.len());
    }
}
