use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ─────────────────────── Audio File I/O ───────────────────────

/// Load audio file (WAV or MP3) and return mono f32 samples + sample rate
pub fn load_wav(path: &str) -> Result<(Vec<f32>, u32), String> {
    let path_lower = path.to_lowercase();
    
    if path_lower.ends_with(".mp3") {
        load_mp3(path)
    } else {
        load_wav_file(path)
    }
}

/// Load WAV file and return mono f32 samples + sample rate
fn load_wav_file(path: &str) -> Result<(Vec<f32>, u32), String> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV file '{}': {}", path, e))?;

    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => {
            reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .collect()
        }
    };

    let mono: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    Ok((mono, sample_rate))
}

/// Load MP3 file and return mono f32 samples + sample rate
fn load_mp3(path: &str) -> Result<(Vec<f32>, u32), String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("Failed to read MP3 file '{}': {}", path, e))?;
    
    let mut decoder = minimp3::Decoder::new(&data[..]);
    let mut all_samples = Vec::new();
    let mut sample_rate = 44100; // Default
    let mut channels = 1;
    
    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                sample_rate = frame.sample_rate as u32;
                channels = frame.channels;
                
                // Convert i16 samples to f32
                for &sample in &frame.data {
                    all_samples.push(sample as f32 / 32768.0);
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(e) => return Err(format!("Failed to decode MP3 '{}': {:?}", path, e)),
        }
    }
    
    // Convert to mono if stereo
    let mono: Vec<f32> = if channels > 1 {
        all_samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        all_samples
    };
    
    Ok((mono, sample_rate))
}

fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Failed to create WAV: {}", e))?;
    for &s in samples {
        writer.write_sample(s).map_err(|e| format!("WAV write: {}", e))?;
    }
    writer.finalize().map_err(|e| format!("WAV finalize: {}", e))?;
    Ok(())
}

// ─────────────────────── Listing ───────────────────────

/// List all audio files (WAV and MP3) in a directory recursively
pub fn list_samples(dir: &str) -> Vec<SampleInfo> {
    let mut samples = Vec::new();
    if !Path::new(dir).exists() {
        return samples;
    }
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if ext_lower == "wav" || ext_lower == "mp3" {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let category = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "default".to_string());
                samples.push(SampleInfo { name, path: path.to_string_lossy().to_string(), category });
            }
        }
    }
    samples
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SampleInfo {
    pub name: String,
    pub path: String,
    pub category: String,
}

/// Get default samples directory
pub fn get_samples_dir() -> PathBuf {
    let mut dir = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    dir.push("samples");
    dir
}

// ─────────────────────── DSP helpers ───────────────────────

fn xorshift(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// One-pole lowpass: returns new state
fn lp1(prev: f32, input: f32, cutoff_hz: f32, sr: f32) -> f32 {
    let rc = 1.0 / (2.0 * PI * cutoff_hz);
    let dt = 1.0 / sr;
    let a = dt / (rc + dt);
    prev + a * (input - prev)
}

/// Simple bandpass via state-variable filter (returns lp, bp)
fn svf_step(lp: &mut f32, bp: &mut f32, input: f32, cutoff: f32, res: f32, sr: f32) {
    let f = 2.0 * (PI * cutoff / sr).sin();
    let q = 1.0 - res.min(0.99);
    *lp += f * *bp;
    let hp = input - *lp - q * *bp;
    *bp += f * hp;
}

/// Generate a full buffer of length n at given sample_rate
fn gen_buf(n: usize, sr: u32, f: impl Fn(usize, f32, f32) -> f32) -> Vec<f32> {
    let sr_f = sr as f32;
    (0..n).map(|i| f(i, i as f32 / sr_f, sr_f)).collect()
}

// ─────────────────────── Master generation ───────────────────────

/// Create all Sonic Pi built-in sample categories and files
pub fn ensure_default_samples(base_dir: &Path) -> Result<(), String> {
    let categories = [
        "drums", "bd", "sn", "hat", "elec", "ambi", "bass",
        "loop", "perc", "tabla", "vinyl", "glitch", "misc", "mehackit",
    ];
    for cat in &categories {
        std::fs::create_dir_all(base_dir.join(cat))
            .map_err(|e| format!("mkdir: {}", e))?;
    }

    let sr = 44100u32;

    // ────── Drum kit (drum_*) ──────
    gen_if_missing(base_dir, "drums", "drum_heavy_kick", sr, 0.6, |_i, t, _sr| {
        let freq = 55.0 + 120.0 * (-t * 12.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 6.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_bass_hard", sr, 0.5, |_, t, _| {
        let freq = 60.0 + 80.0 * (-t * 15.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 8.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_bass_soft", sr, 0.4, |_, t, _| {
        let freq = 55.0 + 50.0 * (-t * 10.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 10.0).exp() * 0.7
    })?;
    gen_noise_sample(base_dir, "drums", "drum_snare_hard", sr, 0.3, 200.0, 0.5, 15.0)?;
    gen_noise_sample(base_dir, "drums", "drum_snare_soft", sr, 0.25, 180.0, 0.3, 18.0)?;
    gen_if_missing(base_dir, "drums", "drum_cymbal_hard", sr, 0.5, |i, t, _| {
        let mut ns: u32 = 31415 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let ring = (t * 3500.0 * 2.0 * PI).sin() * 0.3 + (t * 5200.0 * 2.0 * PI).sin() * 0.2;
        (n * 0.5 + ring) * (-t * 5.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_cymbal_soft", sr, 0.4, |i, t, _| {
        let mut ns: u32 = 27182 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let ring = (t * 3200.0 * 2.0 * PI).sin() * 0.2;
        (n * 0.4 + ring) * (-t * 7.0).exp() * 0.7
    })?;
    gen_if_missing(base_dir, "drums", "drum_cymbal_open", sr, 1.0, |i, t, _| {
        let mut ns: u32 = 14142 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let ring = (t * 4000.0 * 2.0 * PI).sin() * 0.3 + (t * 6000.0 * 2.0 * PI).sin() * 0.15;
        (n * 0.5 + ring) * (-t * 2.5).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_cymbal_closed", sr, 0.15, |i, t, _| {
        let mut ns: u32 = 17320 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        (n * 0.6) * (-t * 50.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_cymbal_pedal", sr, 0.2, |i, t, _| {
        let mut ns: u32 = 22360 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        n * (-t * 35.0).exp() * 0.5
    })?;
    gen_if_missing(base_dir, "drums", "drum_tom_lo_hard", sr, 0.4, |_, t, _| {
        let freq = 80.0 + 40.0 * (-t * 12.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 8.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_tom_lo_soft", sr, 0.35, |_, t, _| {
        let freq = 80.0 + 30.0 * (-t * 10.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 10.0).exp() * 0.7
    })?;
    gen_if_missing(base_dir, "drums", "drum_tom_mid_hard", sr, 0.35, |_, t, _| {
        let freq = 140.0 + 60.0 * (-t * 12.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 9.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_tom_mid_soft", sr, 0.3, |_, t, _| {
        let freq = 140.0 + 40.0 * (-t * 10.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 11.0).exp() * 0.7
    })?;
    gen_if_missing(base_dir, "drums", "drum_tom_hi_hard", sr, 0.3, |_, t, _| {
        let freq = 220.0 + 80.0 * (-t * 14.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 10.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_tom_hi_soft", sr, 0.25, |_, t, _| {
        let freq = 220.0 + 50.0 * (-t * 12.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 12.0).exp() * 0.7
    })?;
    gen_if_missing(base_dir, "drums", "drum_splash_hard", sr, 0.8, |i, t, _| {
        let mut ns: u32 = 10001 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let shimmer = (t * 5500.0 * 2.0 * PI).sin() * 0.2;
        (n * 0.6 + shimmer) * (-t * 3.0).exp()
    })?;
    gen_if_missing(base_dir, "drums", "drum_splash_soft", sr, 0.6, |i, t, _| {
        let mut ns: u32 = 20002 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        (n * 0.5) * (-t * 4.0).exp() * 0.7
    })?;
    gen_noise_sample(base_dir, "drums", "drum_roll", sr, 1.0, 160.0, 0.2, 2.0)?;

    // ────── Bass drums (bd_*) ──────
    let bd_specs: Vec<(&str, f32, f32, f32, f32)> = vec![
        ("bd_pure",    50.0, 80.0,  0.5, 8.0),
        ("bd_808",     45.0, 160.0, 0.7, 5.0),
        ("bd_zum",     40.0, 200.0, 0.5, 6.0),
        ("bd_gas",     55.0, 100.0, 0.4, 10.0),
        ("bd_sone",    48.0, 130.0, 0.5, 7.0),
        ("bd_haus",    52.0, 110.0, 0.5, 6.5),
        ("bd_zome",    42.0, 150.0, 0.6, 5.5),
        ("bd_boom",    38.0, 180.0, 0.8, 4.0),
        ("bd_klub",    60.0, 90.0,  0.4, 9.0),
        ("bd_fat",     45.0, 200.0, 0.7, 5.0),
        ("bd_tek",     65.0, 120.0, 0.35, 12.0),
        ("bd_ada",     50.0, 140.0, 0.5, 7.0),
        ("bd_mehackit", 55.0, 100.0, 0.5, 8.0),
    ];
    for (name, base_f, sweep, dur, decay) in &bd_specs {
        gen_if_missing(base_dir, "bd", name, sr, *dur, |_, t, _| {
            let freq = *base_f + *sweep * (-t * *decay).exp();
            (t * freq * 2.0 * PI).sin() * (-t * (*decay * 0.8)).exp()
        })?;
    }

    // ────── Snare drums (sn_*) ──────
    gen_noise_sample(base_dir, "sn", "sn_dub",     sr, 0.35, 180.0, 0.4, 12.0)?;
    gen_noise_sample(base_dir, "sn", "sn_dolf",    sr, 0.3,  200.0, 0.5, 15.0)?;
    gen_noise_sample(base_dir, "sn", "sn_zome",    sr, 0.3,  160.0, 0.45, 14.0)?;
    gen_noise_sample(base_dir, "sn", "sn_generic", sr, 0.3,  190.0, 0.5, 13.0)?;

    // ────── Hi-hats (hat_*) ──────
    let hat_specs: Vec<(&str, f32, f32, u32)> = vec![
        ("hat_snap",  0.08, 60.0,  11111),
        ("hat_zild",  0.2,  20.0,  22222),
        ("hat_noiz",  0.1,  40.0,  33333),
        ("hat_raw",   0.12, 35.0,  44444),
        ("hat_gem",   0.15, 30.0,  55555),
        ("hat_cab",   0.1,  45.0,  66666),
        ("hat_cats",  0.08, 55.0,  77777),
        ("hat_metal", 0.25, 15.0,  88888),
        ("hat_star",  0.18, 22.0,  99999),
        ("hat_hier",  0.12, 38.0,  12321),
        ("hat_gnu",   0.1,  42.0,  13579),
        ("hat_psych", 0.2,  18.0,  24680),
        ("hat_bish",  0.09, 50.0,  11235),
        ("hat_zan",   0.14, 28.0,  81321),
        ("hat_zur",   0.11, 36.0,  34558),
    ];
    for (name, dur, decay, seed) in &hat_specs {
        gen_if_missing(base_dir, "hat", name, sr, *dur, |i, t, _| {
            let mut ns: u32 = *seed + i as u32;
            ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
            let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let hf = (t * 8000.0 * 2.0 * PI).sin() * 0.15;
            (n * 0.6 + hf) * (-t * *decay).exp()
        })?;
    }

    // ────── Electronic (elec_*) ──────
    gen_if_missing(base_dir, "elec", "elec_triangle", sr, 0.3, |_, t, _| {
        let p = (t * 800.0) % 1.0;
        let tri = if p < 0.5 { 4.0 * p - 1.0 } else { 3.0 - 4.0 * p };
        tri * (-t * 12.0).exp()
    })?;
    gen_noise_sample(base_dir, "elec", "elec_snare",     sr, 0.2, 250.0, 0.5, 20.0)?;
    gen_noise_sample(base_dir, "elec", "elec_lo_snare",  sr, 0.25, 150.0, 0.6, 15.0)?;
    gen_noise_sample(base_dir, "elec", "elec_hi_snare",  sr, 0.15, 400.0, 0.4, 25.0)?;
    gen_noise_sample(base_dir, "elec", "elec_mid_snare", sr, 0.2, 280.0, 0.5, 18.0)?;
    gen_if_missing(base_dir, "elec", "elec_cymbal", sr, 0.5, |i, t, _| {
        let mut ns: u32 = 41421 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        n * (-t * 5.0).exp() * 0.6
    })?;
    gen_if_missing(base_dir, "elec", "elec_soft_kick", sr, 0.4, |_, t, _| {
        let freq = 50.0 + 60.0 * (-t * 15.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 10.0).exp() * 0.6
    })?;
    gen_noise_sample(base_dir, "elec", "elec_filt_snare", sr, 0.25, 300.0, 0.4, 20.0)?;
    gen_if_missing(base_dir, "elec", "elec_fuzz_tom", sr, 0.3, |_, t, _| {
        let freq = 120.0 + 60.0 * (-t * 10.0).exp();
        let raw = (t * freq * 2.0 * PI).sin();
        (raw * 2.0).clamp(-1.0, 1.0) * (-t * 8.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_chime", sr, 0.8, |_, t, _| {
        let s = (t * 2000.0 * 2.0 * PI).sin() * 0.5
            + (t * 3011.0 * 2.0 * PI).sin() * 0.3
            + (t * 4520.0 * 2.0 * PI).sin() * 0.2;
        s * (-t * 4.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_bong", sr, 0.5, |_, t, _| {
        let freq = 300.0 + 100.0 * (-t * 20.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 6.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_twang", sr, 0.4, |_, t, _| {
        let p = (t * 250.0) % 1.0;
        let saw = 2.0 * p - 1.0;
        saw * (-t * 8.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_wood", sr, 0.15, |_, t, _| {
        let s = (t * 800.0 * 2.0 * PI).sin() + (t * 1200.0 * 2.0 * PI).sin() * 0.5;
        s * (-t * 30.0).exp() * 0.6
    })?;
    gen_if_missing(base_dir, "elec", "elec_pop", sr, 0.08, |_, t, _| {
        let freq = 400.0 + 600.0 * (-t * 80.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 50.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_beep", sr, 0.2, |_, t, _| {
        (t * 1000.0 * 2.0 * PI).sin() * (-t * 15.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_blip", sr, 0.1, |_, t, _| {
        (t * 1500.0 * 2.0 * PI).sin() * (-t * 30.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_blip2", sr, 0.12, |_, t, _| {
        (t * 2000.0 * 2.0 * PI).sin() * (-t * 25.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_ping", sr, 0.3, |_, t, _| {
        (t * 2500.0 * 2.0 * PI).sin() * (-t * 10.0).exp() * 0.5
    })?;
    gen_if_missing(base_dir, "elec", "elec_bell", sr, 0.6, |_, t, _| {
        let s = (t * 1200.0 * 2.0 * PI).sin() * 0.5
            + (t * 2412.0 * 2.0 * PI).sin() * 0.3
            + (t * 3618.0 * 2.0 * PI).sin() * 0.2;
        s * (-t * 3.5).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_flip", sr, 0.1, |_, t, _| {
        let freq = 800.0 + 2000.0 * t;
        (t * freq * 2.0 * PI).sin() * (-t * 25.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_tick", sr, 0.05, |_, t, _| {
        let freq = 3000.0;
        (t * freq * 2.0 * PI).sin() * (-t * 100.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_hollow_kick", sr, 0.5, |_, t, _| {
        let freq = 60.0 + 40.0 * (-t * 8.0).exp();
        let s = (t * freq * 2.0 * PI).sin();
        let bp = (t * 300.0 * 2.0 * PI).sin() * 0.2 * (-t * 15.0).exp();
        (s + bp) * (-t * 5.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_twip", sr, 0.08, |_, t, _| {
        let freq = 2000.0 - 1500.0 * t * 12.0;
        (t * freq.max(200.0) * 2.0 * PI).sin() * (-t * 40.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_plip", sr, 0.06, |_, t, _| {
        let freq = 1200.0 + 800.0 * (-t * 60.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 50.0).exp()
    })?;
    gen_if_missing(base_dir, "elec", "elec_blup", sr, 0.15, |_, t, _| {
        let freq = 200.0 + 400.0 * (-t * 20.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 15.0).exp()
    })?;

    // ────── Ambient (ambi_*) ──────
    gen_if_missing(base_dir, "ambi", "ambi_soft_buzz", sr, 1.5, |i, t, _| {
        let mut ns: u32 = 50001 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let buzz = (t * 120.0 * 2.0 * PI).sin() * 0.3;
        (n * 0.15 + buzz) * (-t * 1.5).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_swoosh", sr, 1.0, |i, t, _| {
        let mut ns: u32 = 50002 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let env = (t * PI / 1.0).sin(); // rise and fall
        n * env * 0.4
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_drone", sr, 2.0, |_, t, _| {
        let s = (t * 80.0 * 2.0 * PI).sin() * 0.4
            + (t * 120.0 * 2.0 * PI).sin() * 0.3
            + (t * 160.5 * 2.0 * PI).sin() * 0.2;
        s * (-(t - 1.0).abs() * 2.0).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_glass_hum", sr, 1.5, |_, t, _| {
        let s = (t * 800.0 * 2.0 * PI).sin() * 0.3
            + (t * 1205.0 * 2.0 * PI).sin() * 0.2
            + (t * 1607.0 * 2.0 * PI).sin() * 0.1;
        s * (-(t - 0.75).abs() * 3.0).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_glass_rub", sr, 1.2, |i, t, _| {
        let mut ns: u32 = 50003 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let tone = (t * 1000.0 * 2.0 * PI).sin() * 0.3;
        (n * 0.1 + tone) * (-(t - 0.6).abs() * 3.0).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_haunted_hum", sr, 2.0, |i, t, _| {
        let mut ns: u32 = 50004 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let drone = (t * 60.0 * 2.0 * PI).sin() * 0.3
            + (t * 63.0 * 2.0 * PI).sin() * 0.2;
        (drone + n * 0.08) * (-(t - 1.0).abs() * 1.5).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_piano", sr, 2.0, |_, t, _| {
        let freq = 261.63; // C4
        let s = (t * freq * 2.0 * PI).sin() * (-t * 2.0).exp()
            + (t * freq * 2.0 * 2.0 * PI).sin() * 0.5 * (-t * 3.0).exp()
            + (t * freq * 3.0 * 2.0 * PI).sin() * 0.25 * (-t * 5.0).exp();
        s * 0.5
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_lunar_land", sr, 2.0, |i, t, _| {
        let mut ns: u32 = 50005 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let shimmer = (t * 400.0 * 2.0 * PI).sin() * 0.1 * (t * 0.5 * 2.0 * PI).sin();
        (n * 0.05 + shimmer) * (-(t - 1.0).abs() * 2.0).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_dark_woosh", sr, 1.5, |i, t, _| {
        let mut ns: u32 = 50006 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let env = (t * PI / 1.5).sin();
        let sub = (t * 50.0 * 2.0 * PI).sin() * 0.2;
        (n * 0.3 + sub) * env
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_choir", sr, 2.0, |_, t, _| {
        let f1 = 260.0 + 5.0 * (t * 4.0 * 2.0 * PI).sin();
        let f2 = 390.0 + 4.0 * (t * 3.5 * 2.0 * PI).sin();
        let f3 = 520.0 + 6.0 * (t * 5.0 * 2.0 * PI).sin();
        let s = (t * f1 * 2.0 * PI).sin() * 0.35
            + (t * f2 * 2.0 * PI).sin() * 0.35
            + (t * f3 * 2.0 * PI).sin() * 0.3;
        s * (-(t - 1.0).abs() * 2.0).exp()
    })?;
    gen_if_missing(base_dir, "ambi", "ambi_sauna", sr, 2.0, |i, t, _| {
        let mut ns: u32 = 50007 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let hiss = n * 0.15;
        let warmth = (t * 100.0 * 2.0 * PI).sin() * 0.1;
        (hiss + warmth) * (-(t - 1.0).abs() * 1.5).exp()
    })?;

    // ────── Bass (bass_*) ──────
    let bass_freq_map: Vec<(&str, f32, &str)> = vec![
        ("bass_hit_c",      65.41, "hit"),
        ("bass_hard_c",     65.41, "hard"),
        ("bass_thick_c",    65.41, "thick"),
        ("bass_drop_c",     65.41, "drop"),
        ("bass_woodsy_c",   65.41, "woodsy"),
        ("bass_voxy_c",     65.41, "voxy"),
        ("bass_voxy_hit_c", 65.41, "voxy_hit"),
        ("bass_dnb_f",      87.31, "dnb"),
        ("bass_trance_c",   65.41, "trance"),
    ];
    for (name, freq, style) in &bass_freq_map {
        gen_if_missing(base_dir, "bass", name, sr, 0.6, |_i, t, _| {
            match *style {
                "hit" => {
                    (t * *freq * 2.0 * PI).sin() * (-t * 6.0).exp()
                }
                "hard" => {
                    let saw = 2.0 * ((t * *freq) % 1.0) - 1.0;
                    saw * (-t * 5.0).exp()
                }
                "thick" => {
                    let s1 = (t * *freq * 2.0 * PI).sin();
                    let s2 = (t * *freq * 2.0 * 2.0 * PI).sin() * 0.3;
                    (s1 + s2) * (-t * 4.0).exp()
                }
                "drop" => {
                    let f = *freq + 200.0 * (-t * 8.0).exp();
                    (t * f * 2.0 * PI).sin() * (-t * 5.0).exp()
                }
                "woodsy" => {
                    let tri = {
                        let p = (t * *freq) % 1.0;
                        if p < 0.5 { 4.0 * p - 1.0 } else { 3.0 - 4.0 * p }
                    };
                    tri * (-t * 5.0).exp()
                }
                "voxy" => {
                    let s = (t * *freq * 2.0 * PI).sin();
                    let formant = (t * 500.0 * 2.0 * PI).sin() * 0.3;
                    (s + formant) * (-t * 4.0).exp()
                }
                "voxy_hit" => {
                    let s = (t * *freq * 2.0 * PI).sin();
                    let formant = (t * 600.0 * 2.0 * PI).sin() * 0.4 * (-t * 15.0).exp();
                    (s * 0.7 + formant) * (-t * 5.0).exp()
                }
                "dnb" => {
                    let f = *freq + 300.0 * (-t * 20.0).exp();
                    let saw = 2.0 * ((t * f) % 1.0) - 1.0;
                    saw * (-t * 6.0).exp()
                }
                "trance" => {
                    let s1 = (t * *freq * 2.0 * PI).sin();
                    let s2 = (t * *freq * 1.005 * 2.0 * PI).sin();
                    (s1 + s2) * 0.5 * (-t * 3.5).exp()
                }
                _ => {
                    (t * *freq * 2.0 * PI).sin() * (-t * 5.0).exp()
                }
            }
        })?;
    }

    // ────── Loops (loop_*) ──────
    // Loops are rhythmic patterns — we generate short beat patterns
    gen_loop(base_dir, "loop", "loop_industrial",   sr, 2.0, 140.0, &[1,0,0,0, 1,0,1,0, 1,0,0,1, 0,1,0,0], "industrial")?;
    gen_loop(base_dir, "loop", "loop_compus",        sr, 2.0, 120.0, &[1,0,0,1, 0,0,1,0, 1,0,0,1, 0,0,1,0], "compus")?;
    gen_loop(base_dir, "loop", "loop_amen",          sr, 1.88, 136.0, &[1,0,1,0, 0,1,1,0, 1,0,0,1, 0,1,1,0], "amen")?;
    gen_loop(base_dir, "loop", "loop_amen_full",     sr, 3.76, 136.0, &[1,0,1,0, 0,1,1,0, 1,0,0,1, 0,1,1,0, 1,0,1,0, 0,1,0,1, 1,0,0,1, 0,1,1,0], "amen")?;
    gen_loop(base_dir, "loop", "loop_garzul",        sr, 2.0, 130.0, &[1,0,0,1, 0,1,0,0, 1,0,1,0, 0,0,1,0], "garzul")?;
    gen_loop(base_dir, "loop", "loop_mika",          sr, 2.0, 110.0, &[1,0,1,0, 0,0,1,0, 1,0,0,0, 1,0,1,0], "mika")?;
    gen_loop(base_dir, "loop", "loop_breakbeat",     sr, 2.0, 140.0, &[1,0,0,1, 0,1,0,0, 0,0,1,0, 1,0,0,1], "breakbeat")?;
    gen_loop(base_dir, "loop", "loop_safari",        sr, 2.0, 100.0, &[1,0,1,0, 1,0,1,0, 0,1,0,1, 0,1,0,1], "safari")?;
    gen_loop(base_dir, "loop", "loop_tabla",         sr, 2.0, 120.0, &[1,0,0,1, 0,1,0,0, 1,0,1,0, 0,1,0,1], "tabla")?;
    gen_loop(base_dir, "loop", "loop_3d_printer",    sr, 2.0, 140.0, &[1,1,0,1, 1,0,1,1, 0,1,1,0, 1,1,0,1], "printer")?;
    gen_loop(base_dir, "loop", "loop_drone_g_97",    sr, 4.0, 97.0,  &[1,0,0,0, 0,0,0,0, 1,0,0,0, 0,0,0,0], "drone")?;
    gen_loop(base_dir, "loop", "loop_electric",      sr, 2.0, 120.0, &[1,0,0,1, 0,0,1,0, 0,1,0,0, 1,0,1,0], "electric")?;
    gen_loop(base_dir, "loop", "loop_mehackit1",     sr, 2.0, 120.0, &[1,0,1,0, 0,1,0,1, 1,0,1,0, 0,1,0,1], "mehackit")?;
    gen_loop(base_dir, "loop", "loop_mehackit2",     sr, 2.0, 120.0, &[0,1,0,1, 1,0,1,0, 0,1,0,1, 1,0,1,0], "mehackit")?;
    gen_loop(base_dir, "loop", "loop_perc1",         sr, 2.0, 120.0, &[1,0,0,0, 1,0,0,0, 1,0,0,0, 1,0,0,0], "perc")?;
    gen_loop(base_dir, "loop", "loop_perc2",         sr, 2.0, 120.0, &[0,0,1,0, 0,0,1,0, 0,0,1,0, 0,0,1,0], "perc")?;
    gen_loop(base_dir, "loop", "loop_weirdo",        sr, 2.0, 130.0, &[1,1,0,1, 0,1,1,0, 0,1,0,1, 1,0,1,1], "weirdo")?;

    // ────── Percussion (perc_*) ──────
    gen_if_missing(base_dir, "perc", "perc_bell", sr, 0.8, |_, t, _| {
        let s = (t * 1500.0 * 2.0 * PI).sin() * 0.5
            + (t * 3017.0 * 2.0 * PI).sin() * 0.3
            + (t * 4522.0 * 2.0 * PI).sin() * 0.2;
        s * (-t * 4.0).exp()
    })?;
    gen_if_missing(base_dir, "perc", "perc_bell2", sr, 0.6, |_, t, _| {
        let s = (t * 2000.0 * 2.0 * PI).sin() * 0.5
            + (t * 3200.0 * 2.0 * PI).sin() * 0.3;
        s * (-t * 5.0).exp()
    })?;
    gen_noise_sample(base_dir, "perc", "perc_snap", sr, 0.08, 500.0, 0.3, 60.0)?;
    gen_noise_sample(base_dir, "perc", "perc_snap2", sr, 0.06, 600.0, 0.25, 70.0)?;
    gen_if_missing(base_dir, "perc", "perc_swash", sr, 0.5, |i, t, _| {
        let mut ns: u32 = 60001 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        n * (t * PI / 0.5).sin() * 0.4
    })?;
    gen_if_missing(base_dir, "perc", "perc_till", sr, 0.3, |_, t, _| {
        let s = (t * 3000.0 * 2.0 * PI).sin() * 0.5
            + (t * 5000.0 * 2.0 * PI).sin() * 0.3;
        s * (-t * 12.0).exp()
    })?;
    gen_if_missing(base_dir, "perc", "perc_door", sr, 0.2, |i, t, _| {
        let mut ns: u32 = 60002 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let thud = (t * 100.0 * 2.0 * PI).sin() * 0.5;
        (n * 0.3 + thud) * (-t * 20.0).exp()
    })?;
    gen_if_missing(base_dir, "perc", "perc_impact1", sr, 0.4, |i, t, _| {
        let mut ns: u32 = 60003 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let thud = (t * 60.0 * 2.0 * PI).sin() * 0.6;
        (n * 0.4 + thud) * (-t * 8.0).exp()
    })?;
    gen_if_missing(base_dir, "perc", "perc_impact2", sr, 0.5, |i, t, _| {
        let mut ns: u32 = 60004 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let thud = (t * 45.0 * 2.0 * PI).sin() * 0.7;
        (n * 0.5 + thud) * (-t * 6.0).exp()
    })?;
    gen_if_missing(base_dir, "perc", "perc_swoosh", sr, 0.6, |i, t, _| {
        let mut ns: u32 = 60005 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let env = (t * PI / 0.6).sin();
        n * env * 0.35
    })?;

    // ────── Tabla (tabla_*) ──────
    let tabla_specs: Vec<(&str, f32, f32, f32, f32)> = vec![
        ("tabla_tas1",   400.0, 0.0, 0.2, 20.0),
        ("tabla_tas2",   450.0, 0.0, 0.18, 22.0),
        ("tabla_tas3",   380.0, 0.0, 0.22, 18.0),
        ("tabla_ke1",    800.0, 0.0, 0.1, 40.0),
        ("tabla_ke2",    900.0, 0.0, 0.08, 45.0),
        ("tabla_ke3",    750.0, 0.0, 0.12, 35.0),
        ("tabla_na",     300.0, 80.0, 0.25, 12.0),
        ("tabla_na_s",   320.0, 60.0, 0.2, 15.0),
        ("tabla_tun1",   200.0, 100.0, 0.4, 6.0),
        ("tabla_tun2",   180.0, 120.0, 0.45, 5.0),
        ("tabla_tun3",   220.0, 80.0, 0.35, 7.0),
        ("tabla_te1",    500.0, 0.0, 0.15, 25.0),
        ("tabla_te2",    550.0, 0.0, 0.12, 28.0),
        ("tabla_te_ne",  480.0, 50.0, 0.2, 18.0),
        ("tabla_te_m",   520.0, 30.0, 0.18, 20.0),
        ("tabla_ghe1",   120.0, 80.0, 0.5, 5.0),
        ("tabla_ghe2",   130.0, 90.0, 0.45, 5.5),
        ("tabla_ghe3",   110.0, 100.0, 0.5, 4.5),
        ("tabla_ghe4",   140.0, 70.0, 0.4, 6.0),
        ("tabla_ghe5",   100.0, 110.0, 0.55, 4.0),
        ("tabla_ghe6",   150.0, 60.0, 0.35, 7.0),
        ("tabla_ghe7",   115.0, 95.0, 0.5, 5.0),
        ("tabla_ghe8",   125.0, 85.0, 0.48, 5.2),
        ("tabla_dhec",   250.0, 50.0, 0.3, 10.0),
        ("tabla_re",     350.0, 0.0, 0.15, 22.0),
    ];
    for (name, freq, sweep, dur, decay) in &tabla_specs {
        gen_if_missing(base_dir, "tabla", name, sr, *dur, |_, t, _| {
            let f = *freq + *sweep * (-t * *decay * 2.0).exp();
            let s = (t * f * 2.0 * PI).sin();
            let h2 = (t * f * 2.2 * 2.0 * PI).sin() * 0.3;
            (s + h2) * (-t * *decay).exp()
        })?;
    }

    // ────── Vinyl (vinyl_*) ──────
    gen_if_missing(base_dir, "vinyl", "vinyl_backspin", sr, 1.0, |i, t, _| {
        let mut ns: u32 = 70001 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let sweep = (t * (1000.0 - 800.0 * t) * 2.0 * PI).sin() * 0.2;
        (n * 0.15 + sweep) * (-(t - 0.5).abs() * 4.0).exp()
    })?;
    gen_if_missing(base_dir, "vinyl", "vinyl_rewind", sr, 0.8, |i, t, _| {
        let mut ns: u32 = 70002 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let sweep = (t * (2000.0 - 1500.0 * t) * 2.0 * PI).sin() * 0.3;
        (n * 0.2 + sweep) * (t * PI / 0.8).sin()
    })?;
    gen_if_missing(base_dir, "vinyl", "vinyl_scratch", sr, 0.5, |i, t, _| {
        let mut ns: u32 = 70003 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let wub = (t * 600.0 * (1.0 + 3.0 * (t * 8.0 * 2.0 * PI).sin()) * 2.0 * PI).sin() * 0.5;
        (n * 0.2 + wub) * (-t * 4.0).exp()
    })?;
    gen_if_missing(base_dir, "vinyl", "vinyl_hiss", sr, 2.0, |i, _t, _| {
        let mut ns: u32 = 70004 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        n * 0.08
    })?;

    // ────── Glitch (glitch_*) ──────
    gen_if_missing(base_dir, "glitch", "glitch_bass_g", sr, 0.4, |_, t, _| {
        let freq = 49.0 + 200.0 * (-t * 15.0).exp(); // G1
        let saw = 2.0 * ((t * freq) % 1.0) - 1.0;
        let glitch = ((t * 8000.0) as u32 % 7) as f32 / 7.0 * 2.0 - 1.0;
        (saw * 0.6 + glitch * 0.2) * (-t * 6.0).exp()
    })?;
    for idx in 1..=5 {
        let name = format!("glitch_perc{}", idx);
        let seed = 80000 + idx as u32 * 1000;
        let decay_rate = 15.0 + idx as f32 * 5.0;
        gen_if_missing(base_dir, "glitch", &name, sr, 0.15, |i, t, _| {
            let mut ns: u32 = seed + i as u32;
            ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
            let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let bit = ((t * 4000.0) as u32 % 3) as f32 / 3.0 * 2.0 - 1.0;
            (n * 0.4 + bit * 0.3) * (-t * decay_rate).exp()
        })?;
    }
    gen_if_missing(base_dir, "glitch", "glitch_robot1", sr, 0.3, |_, t, _| {
        let freq = 200.0 + 100.0 * (t * 15.0 * 2.0 * PI).sin();
        let s = if ((t * freq) % 1.0) < 0.5 { 1.0f32 } else { -1.0 };
        s * (-t * 8.0).exp() * 0.5
    })?;
    gen_if_missing(base_dir, "glitch", "glitch_robot2", sr, 0.4, |_, t, _| {
        let freq = 150.0 + 200.0 * (t * 10.0 * 2.0 * PI).sin();
        let s = 2.0 * ((t * freq) % 1.0) - 1.0;
        s * (-t * 6.0).exp() * 0.5
    })?;

    // ────── Misc (misc_*) ──────
    gen_if_missing(base_dir, "misc", "misc_burp", sr, 0.4, |_, t, _| {
        let freq = 80.0 + 100.0 * (t * 5.0 * 2.0 * PI).sin();
        (t * freq * 2.0 * PI).sin() * (-t * 6.0).exp()
    })?;
    gen_if_missing(base_dir, "misc", "misc_cineboom", sr, 1.5, |i, t, _| {
        let mut ns: u32 = 90001 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let sub = (t * 30.0 * 2.0 * PI).sin() * 0.6;
        (n * 0.4 + sub) * (-t * 2.0).exp()
    })?;
    gen_if_missing(base_dir, "misc", "misc_crow", sr, 1.0, |i, t, _| {
        let mut ns: u32 = 90002 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let caw = (t * 800.0 * (1.0 + 0.5 * (t * 6.0 * 2.0 * PI).sin()) * 2.0 * PI).sin();
        let env = if t < 0.1 { t / 0.1 } else if t < 0.3 { 1.0 } else { (-(t - 0.3) * 4.0).exp() };
        (caw * 0.5 + n * 0.1) * env
    })?;

    // ────── Mehackit (mehackit_*) ──────
    for idx in 1..=4 {
        let name = format!("mehackit_phone{}", idx);
        let freq = 400.0 + idx as f32 * 100.0;
        gen_if_missing(base_dir, "mehackit", &name, sr, 0.5, |_, t, _| {
            let s = (t * freq * 2.0 * PI).sin() * 0.5
                + (t * freq * 1.5 * 2.0 * PI).sin() * 0.3;
            let trem = 0.5 + 0.5 * (t * 20.0 * 2.0 * PI).sin();
            s * trem * (-t * 4.0).exp()
        })?;
    }
    for idx in 1..=7 {
        let name = format!("mehackit_robot{}", idx);
        let base_freq = 100.0 + idx as f32 * 50.0;
        let mod_rate = 5.0 + idx as f32 * 3.0;
        gen_if_missing(base_dir, "mehackit", &name, sr, 0.5, |_, t, _| {
            let freq = base_freq + 80.0 * (t * mod_rate * 2.0 * PI).sin();
            let s = if ((t * freq) % 1.0) < 0.5 { 1.0f32 } else { -1.0 };
            s * (-t * 5.0).exp() * 0.4
        })?;
    }

    // ────── Legacy aliases (kick, snare, hihat, clap) ──────
    gen_if_missing(base_dir, "drums", "kick", sr, 0.5, |_, t, _| {
        let freq = 50.0 + 100.0 * (-t * 10.0).exp();
        (t * freq * 2.0 * PI).sin() * (-t * 8.0).exp()
    })?;
    gen_noise_sample(base_dir, "drums", "snare", sr, 0.3, 200.0, 0.5, 15.0)?;
    gen_if_missing(base_dir, "drums", "hihat", sr, 0.15, |i, t, _| {
        let mut ns: u32 = 99 + i as u32;
        ns ^= ns << 13; ns ^= ns >> 17; ns ^= ns << 5;
        let n = (ns as f32 / u32::MAX as f32) * 2.0 - 1.0;
        n * (-t * 40.0).exp() * 0.6
    })?;
    gen_clap(base_dir)?;

    Ok(())
}

// ─────────────────────── Generation Helpers ───────────────────────

/// Generate a sample if the WAV file doesn't already exist
fn gen_if_missing(
    base_dir: &Path,
    category: &str,
    name: &str,
    sr: u32,
    duration: f32,
    f: impl Fn(usize, f32, f32) -> f32,
) -> Result<(), String> {
    let path = base_dir.join(category).join(format!("{}.wav", name));
    if path.exists() {
        return Ok(());
    }
    let n = (sr as f32 * duration) as usize;
    let samples = gen_buf(n, sr, |i, t, sr_f| f(i, t, sr_f));
    write_wav(&path, &samples, sr)
}

/// Generate a noise+tone sample (snare-like)
fn gen_noise_sample(
    base_dir: &Path,
    category: &str,
    name: &str,
    sr: u32,
    duration: f32,
    tone_freq: f32,
    tone_mix: f32,
    decay: f32,
) -> Result<(), String> {
    let path = base_dir.join(category).join(format!("{}.wav", name));
    if path.exists() {
        return Ok(());
    }
    let n = (sr as f32 * duration) as usize;
    let mut noise_state: u32 = name.bytes().fold(42u32, |a, b| a.wrapping_add(b as u32).wrapping_mul(31));
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / sr as f32;
            let tone = (t * tone_freq * 2.0 * PI).sin() * tone_mix;
            let noise = xorshift(&mut noise_state) * (1.0 - tone_mix);
            (tone + noise) * (-t * decay).exp()
        })
        .collect();
    write_wav(&path, &samples, sr)
}

/// Generate a clap sample (multiple noise bursts)
fn gen_clap(base_dir: &Path) -> Result<(), String> {
    let path = base_dir.join("drums").join("clap.wav");
    if path.exists() {
        return Ok(());
    }
    let sr = 44100u32;
    let n = (sr as f32 * 0.25) as usize;
    let mut ns: u32 = 77;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / sr as f32;
            let burst1 = if t < 0.01 { 1.0 } else { 0.0 };
            let burst2 = if t > 0.015 && t < 0.025 { 0.8 } else { 0.0 };
            let burst3 = if t > 0.03 && t < 0.04 { 0.6 } else { 0.0 };
            let tail = if t > 0.04 { (-((t - 0.04) * 20.0)).exp() } else { 0.0 };
            let envelope = burst1 + burst2 + burst3 + tail;
            xorshift(&mut ns) * envelope * 0.7
        })
        .collect();
    write_wav(&path, &samples, sr)
}

/// Generate a rhythmic loop from a hit pattern
fn gen_loop(
    base_dir: &Path,
    category: &str,
    name: &str,
    sr: u32,
    duration: f32,
    bpm: f32,
    pattern: &[u8],
    _style: &str,
) -> Result<(), String> {
    let path = base_dir.join(category).join(format!("{}.wav", name));
    if path.exists() {
        return Ok(());
    }
    let n = (sr as f32 * duration) as usize;
    let step_dur = 60.0 / bpm / 4.0; // 16th notes
    let step_samples = (sr as f32 * step_dur) as usize;
    let mut noise_state: u32 = name.bytes().fold(12345u32, |a, b| a.wrapping_add(b as u32).wrapping_mul(37));

    let mut samples = vec![0.0f32; n];
    for (step_idx, &hit) in pattern.iter().enumerate() {
        if hit == 0 { continue; }
        let start = step_idx * step_samples;
        let is_kick_pos = step_idx % 4 == 0;
        let is_snare_pos = step_idx % 8 == 4;

        for j in 0..step_samples.min(n.saturating_sub(start)) {
            let idx = start + j;
            if idx >= n { break; }
            let t = j as f32 / sr as f32;

            let s = if is_kick_pos {
                // Kick
                let freq = 50.0 + 100.0 * (-t * 12.0).exp();
                (t * freq * 2.0 * PI).sin() * (-t * 8.0).exp() * 0.7
            } else if is_snare_pos {
                // Snare
                let tone = (t * 200.0 * 2.0 * PI).sin() * 0.3;
                let noise = xorshift(&mut noise_state) * 0.5;
                (tone + noise) * (-t * 15.0).exp() * 0.6
            } else {
                // Hi-hat
                let noise = xorshift(&mut noise_state) * 0.4;
                noise * (-t * 40.0).exp() * 0.5
            };
            samples[idx] += s;
        }
    }

    // Normalize
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak > 0.0 {
        let scale = 0.9 / peak;
        for s in &mut samples {
            *s *= scale;
        }
    }

    write_wav(&path, &samples, sr)
}
