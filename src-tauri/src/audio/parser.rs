use std::collections::HashMap;
use rand::Rng;
use super::engine::AudioCommand;
use super::synth::{midi_to_freq, note_name_to_midi, Envelope, OscillatorType};

/// Represents a parsed command from user code
#[derive(Debug, Clone)]
pub enum ParsedCommand {
    PlayNote {
        synth_type: OscillatorType,
        frequency: f32,
        amplitude: f32,
        duration: f32,
        pan: f32,
        envelope: Envelope,
        /// Synth-specific parameters (cutoff, res, detune, depth, etc.)
        params: Vec<(String, f32)>,
    },
    PlaySample {
        name: String,
        rate: f32,
        amplitude: f32,
        pan: f32,
    },
    Sleep(f32),
    SetBpm(f32),
    SetVolume(f32),
    SetSynth(OscillatorType),
    WithFx {
        fx_type: String,
        params: Vec<(String, f32)>,
        commands: Vec<ParsedCommand>,
    },
    Loop {
        name: String,
        commands: Vec<ParsedCommand>,
        /// If true, the loop runs in parallel (live_loop, in_thread) and does
        /// NOT advance the parent time offset. If false (loop do, uncomment, density),
        /// it advances time sequentially.
        parallel: bool,
    },
    TimesLoop {
        count: usize,
        commands: Vec<ParsedCommand>,
    },
    Stop,
    Comment(String),
    Log(String),
}

/// Parser context that tracks variables, functions, and synth state
struct ParseContext {
    variables: HashMap<String, String>,
    current_synth: OscillatorType,
    /// Stored function definitions from `define :name do ... end`
    functions: HashMap<String, String>,
    /// Ring buffer values: variable name -> list of values
    ring_values: HashMap<String, Vec<String>>,
    /// Ring tick counters: variable name -> current index
    ring_counters: HashMap<String, usize>,
    /// Default params applied to every `play` / `synth` command
    synth_defaults: HashMap<String, f32>,
    /// Default params applied to every `sample` command
    sample_defaults: HashMap<String, f32>,
    /// Global tick counter (used by standalone `tick` / `look`)
    global_tick: usize,
}

impl ParseContext {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            current_synth: OscillatorType::Sine,
            functions: HashMap::new(),
            ring_values: HashMap::new(),
            ring_counters: HashMap::new(),
            synth_defaults: HashMap::new(),
            sample_defaults: HashMap::new(),
            global_tick: 0,
        }
    }

    /// Resolve a value that may reference a variable or use string concatenation
    fn resolve_string(&self, raw: &str) -> String {
        let trimmed = raw.trim();

        // Handle string concatenation: expr + expr + ...
        if trimmed.contains('+') {
            let parts: Vec<&str> = trimmed.split('+').collect();
            let mut result = String::new();
            for part in parts {
                let p = part.trim();
                if p.starts_with('"') && p.ends_with('"') {
                    // String literal
                    result.push_str(&p[1..p.len() - 1]);
                } else if let Some(val) = self.variables.get(p) {
                    // Variable reference
                    eprintln!("[resolve_string] var '{}' = '{}'", p, val);
                    result.push_str(val);
                } else {
                    eprintln!("[resolve_string] UNRESOLVED var '{}' (known vars: {:?})", p, self.variables.keys().collect::<Vec<_>>());
                    result.push_str(p);
                }
            }
            return result;
        }

        // Simple string literal
        if trimmed.starts_with('"') && trimmed.ends_with('"') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }

        // Variable reference
        if let Some(val) = self.variables.get(trimmed) {
            return val.clone();
        }

        trimmed.to_string()
    }

    /// Resolve a numeric expression that may contain rrand(), rand(), dice(), etc.
    fn resolve_numeric(&self, expr: &str) -> Option<f32> {
        let trimmed = expr.trim();
        let mut rng = rand::thread_rng();

        // rrand(min, max)
        if let Some(inner) = extract_func_args(trimmed, "rrand") {
            let args: Vec<&str> = inner.split(',').collect();
            if args.len() == 2 {
                let min: f32 = args[0].trim().parse().ok()?;
                let max: f32 = args[1].trim().parse().ok()?;
                return Some(rng.gen_range(min..=max));
            }
        }

        // rrand_i(min, max)
        if let Some(inner) = extract_func_args(trimmed, "rrand_i") {
            let args: Vec<&str> = inner.split(',').collect();
            if args.len() == 2 {
                let min: i32 = args[0].trim().parse().ok()?;
                let max: i32 = args[1].trim().parse().ok()?;
                return Some(rng.gen_range(min..=max) as f32);
            }
        }

        // rand(max) or rand()
        if let Some(inner) = extract_func_args(trimmed, "rand") {
            let max: f32 = if inner.trim().is_empty() {
                1.0
            } else {
                inner.trim().parse().unwrap_or(1.0)
            };
            return Some(rng.gen_range(0.0..max));
        }

        // rand_i(max)
        if let Some(inner) = extract_func_args(trimmed, "rand_i") {
            let max: i32 = inner.trim().parse().unwrap_or(2);
            return Some(rng.gen_range(0..max) as f32);
        }

        // dice(n) - random integer 1..n
        if let Some(inner) = extract_func_args(trimmed, "dice") {
            let n: i32 = inner.trim().parse().unwrap_or(6);
            return Some(rng.gen_range(1..=n) as f32);
        }

        // Expression with arithmetic: e.g. "1 + rrand(-0.02, 0.03)"
        if trimmed.contains('+') || trimmed.contains('-') {
            // Try to evaluate simple arithmetic with rrand
            if let Some(result) = self.eval_simple_arithmetic(trimmed) {
                return Some(result);
            }
        }

        // Plain number
        trimmed.parse::<f32>().ok()
    }

    /// Evaluate simple arithmetic expressions like "1 + rrand(-0.02, 0.03)"
    fn eval_simple_arithmetic(&self, expr: &str) -> Option<f32> {
        let trimmed = expr.trim();

        // Look for rrand/rand function calls in the expression
        for func_name in &["rrand", "rrand_i", "rand", "rand_i", "dice"] {
            if let Some(func_pos) = trimmed.find(&format!("{}(", func_name)) {
                // Find the matching closing paren
                let open_paren = func_pos + func_name.len();
                let mut depth = 0;
                let mut close_paren = open_paren;
                for (i, ch) in trimmed[open_paren..].chars().enumerate() {
                    if ch == '(' { depth += 1; }
                    if ch == ')' { depth -= 1; if depth == 0 { close_paren = open_paren + i; break; } }
                }

                let func_call = &trimmed[func_pos..=close_paren];
                let func_val = self.resolve_numeric(func_call)?;

                let before = trimmed[..func_pos].trim();
                let after = trimmed[close_paren + 1..].trim();

                // Parse what's before: could be "1 +" or "0.5 -" etc.
                let mut result = func_val;
                if !before.is_empty() {
                    if let Some(stripped) = before.strip_suffix('+') {
                        let left: f32 = stripped.trim().parse().ok()?;
                        result = left + func_val;
                    } else if let Some(stripped) = before.strip_suffix('-') {
                        let left: f32 = stripped.trim().parse().ok()?;
                        result = left - func_val;
                    } else if let Some(stripped) = before.strip_suffix('*') {
                        let left: f32 = stripped.trim().parse().ok()?;
                        result = left * func_val;
                    }
                }

                // Parse what's after: could be "+ 0.5" or "* 2" etc.
                if !after.is_empty() {
                    if let Some(stripped) = after.strip_prefix('+') {
                        let right: f32 = stripped.trim().parse().unwrap_or(0.0);
                        result += right;
                    } else if let Some(stripped) = after.strip_prefix('-') {
                        let right: f32 = stripped.trim().parse().unwrap_or(0.0);
                        result -= right;
                    } else if let Some(stripped) = after.strip_prefix('*') {
                        let right: f32 = stripped.trim().parse().unwrap_or(1.0);
                        result *= right;
                    }
                }

                return Some(result);
            }
        }

        None
    }

    /// Evaluate one_in(n) - returns true with probability 1/n
    fn eval_one_in(&self, expr: &str) -> Option<bool> {
        if let Some(inner) = extract_func_args(expr, "one_in") {
            let n: u32 = inner.trim().parse().ok()?;
            if n == 0 { return Some(false); }
            let mut rng = rand::thread_rng();
            return Some(rng.gen_ratio(1, n));
        }
        None
    }

    /// Get the next tick value from a ring buffer
    fn tick_ring(&mut self, var_name: &str) -> Option<String> {
        let values = self.ring_values.get(var_name)?.clone();
        if values.is_empty() { return None; }
        let counter = self.ring_counters.entry(var_name.to_string()).or_insert(0);
        let val = values[*counter % values.len()].clone();
        *counter += 1;
        Some(val)
    }

    /// Get the current look value from a ring buffer (no advance)
    fn look_ring(&self, var_name: &str) -> Option<String> {
        let values = self.ring_values.get(var_name)?;
        if values.is_empty() { return None; }
        let counter = self.ring_counters.get(var_name).copied().unwrap_or(0);
        Some(values[counter % values.len()].clone())
    }

    /// Advance the global tick counter by 1 and return the previous value
    fn tick(&mut self) -> usize {
        let val = self.global_tick;
        self.global_tick += 1;
        val
    }

    /// Return the current global tick value without advancing
    fn look(&self) -> usize {
        self.global_tick
    }

    /// Evaluate a list expression that may have method calls:
    ///   `[:c4, :e4, :g4].choose`
    ///   `scale(:c4, :minor).choose`
    ///   `(ring 1, 0, 1, 0).tick`
    ///   `var_name.tick`
    fn resolve_list_value(&mut self, expr: &str) -> Option<String> {
        let trimmed = expr.trim();
        let mut rng = rand::thread_rng();

        // Check for method calls: .choose, .pick, .shuffle, .reverse, .tick, .look, .first, .last
        for method in &[".choose", ".pick(", ".pick", ".shuffle", ".reverse",
                        ".tick", ".look", ".first", ".last", ".ring",
                        ".min", ".max", ".sort", ".mirror", ".stretch(", ".repeat("] {
            if let Some(dot_pos) = trimmed.rfind(method) {
                let base_expr = &trimmed[..dot_pos];
                let method_name = &trimmed[dot_pos + 1..];

                // Resolve the base to a list of values
                let values = self.resolve_to_list(base_expr)?;
                if values.is_empty() { return None; }

                // Apply the method
                if method_name.starts_with("choose") {
                    let idx = rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name.starts_with("pick(") {
                    // .pick(n) — pick n random elements
                    if let Some(inner) = extract_func_args(method_name, "pick") {
                        let n: usize = inner.trim().parse().unwrap_or(1);
                        let picked: Vec<String> = (0..n)
                            .map(|_| values[rng.gen_range(0..values.len())].clone())
                            .collect();
                        // Return as first element for single note context
                        return picked.first().cloned();
                    }
                    let idx = rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name == "pick" {
                    let idx = rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name.starts_with("tick") {
                    // Use the base expression as the key for tick counter
                    let key = base_expr.to_string();
                    let counter = self.ring_counters.entry(key.clone()).or_insert(0);
                    let val = values[*counter % values.len()].clone();
                    *counter += 1;
                    return Some(val);
                }
                if method_name.starts_with("look") {
                    let key = base_expr.to_string();
                    let counter = self.ring_counters.get(&key).copied().unwrap_or(0);
                    return Some(values[counter % values.len()].clone());
                }
                if method_name == "first" {
                    return values.first().cloned();
                }
                if method_name == "last" {
                    return values.last().cloned();
                }
                if method_name == "reverse" {
                    let mut rev = values;
                    rev.reverse();
                    return rev.first().cloned();
                }
                if method_name == "shuffle" {
                    // Shuffle and return first
                    let idx = rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name == "min" {
                    return values.iter()
                        .filter_map(|v| v.parse::<f32>().ok())
                        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|v| v.to_string())
                        .or_else(|| values.first().cloned());
                }
                if method_name == "max" {
                    return values.iter()
                        .filter_map(|v| v.parse::<f32>().ok())
                        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|v| v.to_string())
                        .or_else(|| values.first().cloned());
                }
                if method_name == "ring" {
                    // .ring just wraps as a ring – return first for scalar context
                    return values.first().cloned();
                }
                return values.first().cloned();
            }
        }

        None
    }

    /// Resolve an expression to a list of string values
    fn resolve_to_list(&self, expr: &str) -> Option<Vec<String>> {
        let trimmed = expr.trim();

        // Inline array: [:c4, :e4, :g4]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len()-1];
            let items: Vec<String> = inner.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            return Some(items);
        }

        // (ring 1, 0, 1, 0) syntax
        if trimmed.starts_with("(ring") || trimmed.starts_with("( ring") {
            let inner = trimmed.trim_start_matches('(').trim_end_matches(')').trim();
            let inner = inner.strip_prefix("ring").unwrap_or(inner).trim();
            let items: Vec<String> = inner.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            return Some(items);
        }

        // scale(:c4, :minor) or scale :c4, :minor
        if trimmed.starts_with("scale(") || trimmed.starts_with("scale ") || trimmed.starts_with("scale\t") {
            return self.resolve_scale_expr(trimmed);
        }

        // chord(:c4, :minor) or chord :c4, :minor (as standalone list)
        if trimmed.starts_with("chord(") || trimmed.starts_with("chord ") || trimmed.starts_with("chord\t") {
            return self.resolve_chord_expr(trimmed);
        }

        // ring(1, 0, 1, 0)
        if let Some(inner) = extract_func_args(trimmed, "ring") {
            let items: Vec<String> = inner.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            return Some(items);
        }

        // knit(:e3, 3, :c3, 1) → [:e3, :e3, :e3, :c3]
        if let Some(inner) = extract_func_args(trimmed, "knit") {
            return Some(eval_knit(inner));
        }

        // range(start, end, step)
        if let Some(inner) = extract_func_args(trimmed, "range") {
            return Some(eval_range(inner));
        }

        // line(start, finish, steps: n) — linear interpolation
        if let Some(inner) = extract_func_args(trimmed, "line") {
            return Some(eval_line(inner));
        }

        // spread(pulses, steps) — Euclidean rhythm
        if let Some(inner) = extract_func_args(trimmed, "spread") {
            let args: Vec<&str> = inner.split(',').collect();
            if args.len() >= 2 {
                let pulses: usize = args[0].trim().parse().unwrap_or(0);
                let steps: usize = args[1].trim().parse().unwrap_or(0);
                let pattern = euclidean_rhythm(pulses, steps);
                return Some(pattern.iter()
                    .map(|b| if *b { "true".to_string() } else { "false".to_string() })
                    .collect());
            }
        }

        // Variable reference (ring/list variable)
        if let Some(values) = self.ring_values.get(trimmed) {
            return Some(values.clone());
        }

        // Variable that might be a comma-separated list in a simple variable
        if let Some(val) = self.variables.get(trimmed) {
            // Check if it looks like a list
            if val.starts_with('[') && val.ends_with(']') {
                let inner = &val[1..val.len()-1];
                let items: Vec<String> = inner.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                return Some(items);
            }
        }

        None
    }

    /// Resolve scale(:root, :type) to list of note symbols
    fn resolve_scale_expr(&self, expr: &str) -> Option<Vec<String>> {
        // Parse: scale(:c4, :minor) or scale(:c4, :minor, num_octaves: 2)
        let args_str = if let Some(inner) = extract_func_args(expr, "scale") {
            inner.to_string()
        } else {
            // scale :c4, :minor form
            expr.strip_prefix("scale")?.trim().to_string()
        };
        let args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
        if args.len() < 2 { return None; }

        let root_str = args[0].trim_start_matches(':');
        let scale_type = args[1].trim().trim_start_matches(':');

        let root_midi = note_name_to_midi(&root_str.to_uppercase())?;
        let intervals = scale_intervals(scale_type);

        // Check for num_octaves parameter
        let num_octaves = args.iter()
            .find(|a| a.contains("num_octaves"))
            .and_then(|a| a.split(':').last())
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(1);

        let mut notes = Vec::new();
        for octave in 0..num_octaves {
            for &interval in &intervals {
                let midi = root_midi as i32 + interval as i32 + (octave as i32 * 12);
                if midi >= 0 && midi <= 127 {
                    notes.push(format!("{}", midi));
                }
            }
        }
        // Add the top note of the last octave
        let top = root_midi as i32 + (num_octaves as i32 * 12);
        if top >= 0 && top <= 127 {
            notes.push(format!("{}", top));
        }

        Some(notes)
    }

    /// Resolve chord(:root, :type) to list of MIDI note numbers
    fn resolve_chord_expr(&self, expr: &str) -> Option<Vec<String>> {
        let args_str = if let Some(inner) = extract_func_args(expr, "chord") {
            inner.to_string()
        } else {
            expr.strip_prefix("chord")?.trim().to_string()
        };
        let args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
        if args.is_empty() { return None; }

        let root_str = args[0].trim_start_matches(':');
        let chord_type = args.get(1).map(|s| s.trim().trim_start_matches(':')).unwrap_or("major");
        let root_midi = note_name_to_midi(&root_str.to_uppercase())?;
        let intervals = chord_intervals(chord_type);

        let notes: Vec<String> = intervals.iter()
            .map(|&interval| format!("{}", root_midi as i32 + interval as i32))
            .collect();

        Some(notes)
    }
}

/// Extract function arguments from "func_name(args)" pattern
fn extract_func_args<'a>(expr: &'a str, func_name: &str) -> Option<&'a str> {
    let pattern = format!("{}(", func_name);
    let start = expr.find(&pattern)?;
    let inner_start = start + pattern.len();
    // Find matching close paren
    let mut depth = 1;
    let mut end = inner_start;
    for (i, ch) in expr[inner_start..].chars().enumerate() {
        if ch == '(' { depth += 1; }
        if ch == ')' { depth -= 1; if depth == 0 { end = inner_start + i; break; } }
    }
    if depth == 0 {
        Some(&expr[inner_start..end])
    } else {
        None
    }
}

/// Generate a Euclidean/Bjorklund rhythm pattern (spread)
fn euclidean_rhythm(pulses: usize, steps: usize) -> Vec<bool> {
    if steps == 0 { return vec![]; }
    if pulses >= steps { return vec![true; steps]; }
    if pulses == 0 { return vec![false; steps]; }

    let mut pattern = vec![false; steps];
    let mut bucket = 0i32;
    for i in 0..steps {
        bucket += pulses as i32;
        if bucket >= steps as i32 {
            bucket -= steps as i32;
            pattern[i] = true;
        }
    }
    pattern
}

/// Get scale intervals for a given scale type
fn scale_intervals(scale_type: &str) -> Vec<i32> {
    match scale_type {
        "major" | "ionian" => vec![0, 2, 4, 5, 7, 9, 11],
        "minor" | "aeolian" | "natural_minor" => vec![0, 2, 3, 5, 7, 8, 10],
        "harmonic_minor" => vec![0, 2, 3, 5, 7, 8, 11],
        "melodic_minor" | "melodic_minor_asc" => vec![0, 2, 3, 5, 7, 9, 11],
        "dorian" => vec![0, 2, 3, 5, 7, 9, 10],
        "phrygian" => vec![0, 1, 3, 5, 7, 8, 10],
        "lydian" => vec![0, 2, 4, 6, 7, 9, 11],
        "mixolydian" => vec![0, 2, 4, 5, 7, 9, 10],
        "locrian" => vec![0, 1, 3, 5, 6, 8, 10],
        "minor_pentatonic" | "minor_penta" => vec![0, 3, 5, 7, 10],
        "major_pentatonic" | "major_penta" => vec![0, 2, 4, 7, 9],
        "pentatonic" => vec![0, 2, 4, 7, 9],
        "blues" | "blues_minor" => vec![0, 3, 5, 6, 7, 10],
        "blues_major" => vec![0, 2, 3, 4, 7, 9],
        "whole_tone" | "whole" => vec![0, 2, 4, 6, 8, 10],
        "chromatic" => vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        "diminished" | "octatonic" => vec![0, 2, 3, 5, 6, 8, 9, 11],
        "hex_major6" => vec![0, 2, 4, 5, 7, 9],
        "hex_dorian" => vec![0, 2, 3, 5, 7, 10],
        "hex_phrygian" => vec![0, 1, 3, 5, 8, 10],
        "hex_major7" => vec![0, 2, 4, 5, 7, 11],
        "hex_sus" => vec![0, 2, 5, 7, 9, 10],
        "hex_aeolian" => vec![0, 3, 5, 7, 8, 10],
        "hungarian_minor" => vec![0, 2, 3, 6, 7, 8, 11],
        "diatonic" => vec![0, 2, 4, 7, 9],
        "hirajoshi" => vec![0, 2, 3, 7, 8],
        "iwato" => vec![0, 1, 5, 6, 10],
        "kumoi" => vec![0, 2, 3, 7, 9],
        "in_sen" | "in" => vec![0, 1, 5, 7, 10],
        "yo" => vec![0, 3, 5, 7, 10],
        "pelog" => vec![0, 1, 3, 7, 8],
        "chinese" => vec![0, 4, 6, 7, 11],
        "egyptian" => vec![0, 2, 5, 7, 10],
        "enigmatic" => vec![0, 1, 4, 6, 8, 10, 11],
        "spanish" => vec![0, 1, 3, 4, 5, 7, 8, 10],
        "gypsy" => vec![0, 2, 3, 6, 7, 8, 11],
        "super_locrian" => vec![0, 1, 3, 4, 6, 8, 10],
        "prometheus" => vec![0, 2, 4, 6, 9, 10],
        "neapolitan_minor" => vec![0, 1, 3, 5, 7, 8, 11],
        "neapolitan_major" => vec![0, 1, 3, 5, 7, 9, 11],
        "bartok" => vec![0, 2, 4, 6, 7, 9, 10],
        "bhairav" => vec![0, 1, 4, 5, 7, 8, 11],
        "ahirbhairav" => vec![0, 1, 4, 5, 7, 9, 10],
        "marva" => vec![0, 1, 4, 6, 7, 9, 11],
        "todi" => vec![0, 1, 3, 6, 7, 8, 11],
        "purvi" => vec![0, 1, 4, 6, 7, 8, 11],
        _ => vec![0, 2, 4, 5, 7, 9, 11], // default to major
    }
}

/// knit(:e3, 3, :c3, 1) → [":e3", ":e3", ":e3", ":c3"]
fn eval_knit(args: &str) -> Vec<String> {
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < parts.len() {
        let value = parts[i].to_string();
        let count: usize = parts[i + 1].parse().unwrap_or(1);
        for _ in 0..count {
            result.push(value.clone());
        }
        i += 2;
    }
    result
}

/// range(start, end, step) → list of numbers
fn eval_range(args: &str) -> Vec<String> {
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
    if parts.is_empty() { return vec![]; }

    let start: f32 = parts[0].parse().unwrap_or(0.0);
    let end: f32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(start + 10.0);
    let step: f32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    if step == 0.0 { return vec![]; }

    let mut result = Vec::new();
    let mut val = start;
    if step > 0.0 {
        while val < end {
            result.push(format!("{}", val));
            val += step;
        }
    } else {
        while val > end {
            result.push(format!("{}", val));
            val += step;
        }
    }
    result
}

/// line(start, finish, steps: n) → linear interpolation from start to finish
fn eval_line(args: &str) -> Vec<String> {
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 { return vec![]; }

    let start: f32 = parts[0].parse().unwrap_or(0.0);
    // Second arg might be "finish" or a named param
    let finish: f32 = parts[1].split(':').last()
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| parts[1].parse().ok())
        .unwrap_or(1.0);

    // Look for steps: n
    let steps: usize = parts.iter()
        .find(|p| p.contains("steps"))
        .and_then(|p| p.split(':').last())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(10);

    if steps <= 1 { return vec![format!("{}", start)]; }

    let mut result = Vec::new();
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let val = start + t * (finish - start);
        result.push(format!("{:.4}", val));
    }
    result
}

/// Parse Sonic Pi-like code into commands
pub fn parse_code(code: &str) -> Result<Vec<ParsedCommand>, String> {
    let mut ctx = ParseContext::new();
    parse_code_with_context(code, &mut ctx)
}

/// Pre-process code to join continuation lines (lines ending with `,` or `\`)
fn join_continuation_lines(code: &str) -> String {
    let raw_lines: Vec<&str> = code.lines().collect();
    let mut joined = Vec::new();
    let mut i = 0;
    while i < raw_lines.len() {
        let mut current = raw_lines[i].to_string();
        // Keep joining while the trimmed line ends with ',' or '\'
        while i + 1 < raw_lines.len() {
            let trimmed = current.trim_end();
            if trimmed.ends_with(',') || trimmed.ends_with('\\') {
                let next = raw_lines[i + 1].trim();
                if trimmed.ends_with('\\') {
                    // Remove the trailing backslash and append next line
                    current = format!("{} {}", trimmed.trim_end_matches('\\').trim_end(), next);
                } else {
                    // Trailing comma — append next line after the comma
                    current = format!("{} {}", trimmed, next);
                }
                i += 1;
            } else {
                break;
            }
        }
        joined.push(current);
        i += 1;
    }
    joined.join("\n")
}

fn parse_code_with_context(
    code: &str,
    ctx: &mut ParseContext,
) -> Result<Vec<ParsedCommand>, String> {
    let mut commands = Vec::new();
    // Pre-process: join continuation lines
    let preprocessed = join_continuation_lines(code);
    let lines: Vec<&str> = preprocessed.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let raw_line = lines[i];
        // Strip inline comments (but not inside strings)
        let line = strip_inline_comment(raw_line.trim());

        if line.is_empty() {
            i += 1;
            continue;
        }

        // Full-line comments
        if line.starts_with('#') {
            commands.push(ParsedCommand::Comment(line.to_string()));
            i += 1;
            continue;
        }

        // Handle Time.now.to_f and similar Ruby constants
        if line.contains("Time.now") {
            // Time.now.to_f → treat as 0.0 (we simulate with elapsed time = 0)
            if let Some((var_name, _)) = try_parse_assignment(&line) {
                ctx.variables.insert(var_name, "0.0".to_string());
            }
            i += 1;
            continue;
        }

        // Variable assignment: var_name = "value" or var_name = expression
        if let Some(caps) = try_parse_assignment(&line) {
            let (var_name, var_value) = caps;

            // Check if value is a ring() call
            if let Some(ring_args) = extract_func_args(&var_value, "ring") {
                let items: Vec<String> = ring_args.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                eprintln!("[parse] ring '{}' = {:?}", var_name, items);
                ctx.ring_values.insert(var_name.clone(), items);
                ctx.ring_counters.insert(var_name, 0);
                i += 1;
                continue;
            }

            // Check if value is a spread() call
            if let Some(spread_args) = extract_func_args(&var_value, "spread") {
                let args: Vec<&str> = spread_args.split(',').collect();
                if args.len() >= 2 {
                    let pulses: usize = args[0].trim().parse().unwrap_or(0);
                    let steps: usize = args[1].trim().parse().unwrap_or(0);
                    let pattern = euclidean_rhythm(pulses, steps);
                    let items: Vec<String> = pattern.iter()
                        .map(|b| if *b { "true".to_string() } else { "false".to_string() })
                        .collect();
                    eprintln!("[parse] spread({}, {}) '{}' = {:?}", pulses, steps, var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.insert(var_name, 0);
                }
                i += 1;
                continue;
            }

            // Check if value is (ring ...) (Sonic Pi alternate syntax)
            if var_value.starts_with("(ring") || var_value.starts_with("( ring") {
                let inner = var_value.trim_start_matches('(').trim_end_matches(')').trim();
                let inner = inner.strip_prefix("ring").unwrap_or(inner).trim();
                let items: Vec<String> = inner.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                ctx.ring_values.insert(var_name.clone(), items);
                ctx.ring_counters.insert(var_name, 0);
                i += 1;
                continue;
            }

            // Check if value is a scale() call → store as ring
            if var_value.starts_with("scale(") || var_value.starts_with("scale ") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] scale '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.insert(var_name, 0);
                }
                i += 1;
                continue;
            }

            // Check if value is a chord() call → store as ring
            if var_value.starts_with("chord(") || var_value.starts_with("chord ") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] chord '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.insert(var_name, 0);
                }
                i += 1;
                continue;
            }

            // Check if value is a knit() call → store as ring
            if var_value.starts_with("knit(") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] knit '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.insert(var_name, 0);
                }
                i += 1;
                continue;
            }

            // Check if value is a range() call → store as ring
            if var_value.starts_with("range(") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] range '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.insert(var_name, 0);
                }
                i += 1;
                continue;
            }

            // Check if value is a line() call → store as ring
            if var_value.starts_with("line(") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] line '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.insert(var_name, 0);
                }
                i += 1;
                continue;
            }

            // Check if value is an inline array: [:c4, :e4, :g4]
            if var_value.starts_with('[') && var_value.ends_with(']') {
                let inner = &var_value[1..var_value.len()-1];
                let items: Vec<String> = inner.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                ctx.ring_values.insert(var_name.clone(), items);
                ctx.ring_counters.insert(var_name, 0);
                i += 1;
                continue;
            }

            // Resolve the value (could reference other vars)
            let resolved = ctx.resolve_string(&var_value);
            ctx.variables.insert(var_name, resolved);
            i += 1;
            continue;
        }

        // Block structures: live_loop, N.times do, with_fx, in_thread, define, if, etc.
        if let Some(block_result) = try_parse_block(&line, &lines, i, ctx)? {
            let (cmd, new_i) = block_result;
            commands.push(cmd);
            i = new_i + 1;
            continue;
        }

        // Single-line commands
        if let Some(cmd) = parse_line(&line, ctx) {
            match &cmd {
                ParsedCommand::SetSynth(s) => {
                    ctx.current_synth = *s;
                }
                _ => {}
            }
            commands.push(cmd);
        } else {
            // Check if this is a function call to a defined function
            // Support both plain names and names with ? or !
            let func_name_raw = line.split_whitespace().next()
                .or_else(|| line.split('(').next())
                .unwrap_or("");
            // Also try stripping args: "should_stop?(x, y)" -> "should_stop?"
            let func_name = func_name_raw.split('(').next().unwrap_or(func_name_raw);
            if ctx.functions.contains_key(func_name) {
                let body = ctx.functions.get(func_name).unwrap().clone();
                eprintln!("[parse] Expanding function '{}' ({} chars)", func_name, body.len());
                let sub = parse_code_with_context(&body, ctx)?;
                commands.extend(sub);
            }
            // else: silently skip truly unrecognized lines
        }

        i += 1;
    }

    Ok(commands)
}

/// Try to parse a variable assignment like `sample_path = "..."`
fn try_parse_assignment(line: &str) -> Option<(String, String)> {
    // Match: identifier = value (but NOT ==)
    // Must not start with a keyword
    let keywords = [
        "play", "sample", "sleep", "use_bpm", "use_synth", "live_loop", "with_fx",
        "puts", "print", "log", "stop", "end", "do", "loop", "define", "def", "in_thread",
        "set_volume", "set_volume!", "comment", "uncomment", "density", "at", "cue", "sync",
    ];

    let eq_pos = line.find('=')?;
    // Make sure it's not == or =>
    if eq_pos + 1 < line.len() {
        let next_char = line.as_bytes().get(eq_pos + 1)?;
        if *next_char == b'=' || *next_char == b'>' {
            return None;
        }
    }
    if eq_pos > 0 {
        let prev_char = line.as_bytes().get(eq_pos - 1)?;
        if *prev_char == b'!' || *prev_char == b'<' || *prev_char == b'>' {
            return None;
        }
    }

    let var_name = line[..eq_pos].trim().to_string();
    let var_value = line[eq_pos + 1..].trim().to_string();

    // Variable names must be valid identifiers
    if var_name.is_empty()
        || !var_name.chars().next().unwrap_or(' ').is_alphabetic()
        || var_name.contains(' ')
    {
        return None;
    }

    // Don't treat keywords as variable names
    if keywords.contains(&var_name.as_str()) {
        return None;
    }

    Some((var_name, var_value))
}

/// Try to parse a block structure (live_loop, N.times do, with_fx, in_thread, define, etc.)
fn try_parse_block(
    line: &str,
    lines: &[&str],
    start_i: usize,
    ctx: &mut ParseContext,
) -> Result<Option<(ParsedCommand, usize)>, String> {
    // live_loop :name do
    if line.starts_with("live_loop") {
        let name = extract_symbol(line).unwrap_or_else(|| "loop".to_string());
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::Loop {
                name,
                commands: sub,
                parallel: true,
            },
            end_i,
        )));
    }

    // loop do
    if line == "loop do" || line.starts_with("loop do") {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::Loop {
                name: "loop".to_string(),
                commands: sub,
                parallel: false,
            },
            end_i,
        )));
    }

    // N.times do (e.g., 8.times do, 16.times do)
    if let Some(count) = try_extract_times_count(line) {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::TimesLoop {
                count,
                commands: sub,
            },
            end_i,
        )));
    }

    // with_fx :effect, params do
    if line.starts_with("with_fx") {
        let fx_type = extract_symbol(line).unwrap_or_else(|| "reverb".to_string());
        let params = extract_fx_params(line);
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::WithFx {
                fx_type,
                params,
                commands: sub,
            },
            end_i,
        )));
    }

    // in_thread do
    if line.starts_with("in_thread") {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::Loop {
                name: "thread".to_string(),
                commands: sub,
                parallel: true,
            },
            end_i,
        )));
    }

    // define :name do ... end — store function body for later expansion
    if line.starts_with("define") {
        let func_name = extract_symbol(line).unwrap_or_else(|| "unnamed".to_string());
        let (body, end_i) = collect_block_body(lines, start_i)?;
        eprintln!("[parse] Storing define :{} ({} chars)", func_name, body.len());
        ctx.functions.insert(func_name.clone(), body);
        return Ok(Some((
            ParsedCommand::Comment(format!("# define :{} (stored)", func_name)),
            end_i,
        )));
    }

    // Ruby-style def name(args) ... end — store function body
    if line.starts_with("def ") {
        let rest = line[4..].trim();
        // Extract function name (may contain ? or !)
        let name_end = rest.find('(').or_else(|| rest.find(' ')).unwrap_or(rest.len());
        let func_name = rest[..name_end].trim().to_string();
        let (body, end_i) = collect_block_body_for_def(lines, start_i)?;
        eprintln!("[parse] Storing def {} ({} chars)", func_name, body.len());
        ctx.functions.insert(func_name.clone(), body);
        return Ok(Some((
            ParsedCommand::Comment(format!("# def {} (stored)", func_name)),
            end_i,
        )));
    }

    // if ... do ... end / if ... (single-line trailing if handled elsewhere)
    if line.starts_with("if ") {
        let condition = line.strip_prefix("if ").unwrap_or("").trim();
        // Check if it's a block (ends with "do" on this line or next)
        let is_block = line.ends_with("do") || line.ends_with("then");
        if is_block {
            let cond_str = condition.trim_end_matches(" do").trim_end_matches(" then");
            let (body, end_i) = collect_block_body_with_else(lines, start_i)?;

            // body may contain elsif / else branches
            let branches = split_if_branches(&body);
            let condition_result = evaluate_condition(cond_str, ctx);

            if condition_result {
                // Execute the first (if) branch
                let sub = parse_code_with_context(&branches.if_body, ctx)?;
                return Ok(Some((
                    ParsedCommand::TimesLoop {
                        count: 1,
                        commands: sub,
                    },
                    end_i,
                )));
            } else {
                // Try elsif branches
                for (elsif_cond, elsif_body) in &branches.elsif_branches {
                    if evaluate_condition(elsif_cond, ctx) {
                        let sub = parse_code_with_context(elsif_body, ctx)?;
                        return Ok(Some((
                            ParsedCommand::TimesLoop {
                                count: 1,
                                commands: sub,
                            },
                            end_i,
                        )));
                    }
                }
                // Try else branch
                if let Some(else_body) = &branches.else_body {
                    let sub = parse_code_with_context(else_body, ctx)?;
                    return Ok(Some((
                        ParsedCommand::TimesLoop {
                            count: 1,
                            commands: sub,
                        },
                        end_i,
                    )));
                }
                return Ok(Some((
                    ParsedCommand::Comment(format!("# if (skipped): {}", condition)),
                    end_i,
                )));
            }
        }
        // Single-line if without do/then - skip for now
        return Ok(Some((
            ParsedCommand::Comment(format!("# if: {}", line)),
            start_i,
        )));
    }

    // unless ... do ... end / unless trailing
    if line.starts_with("unless ") {
        let condition = line.strip_prefix("unless ").unwrap_or("").trim();
        let is_block = line.ends_with("do") || line.ends_with("then");
        if is_block {
            let cond_str = condition.trim_end_matches(" do").trim_end_matches(" then");
            let (body, end_i) = collect_block_body(lines, start_i)?;
            let condition_result = evaluate_condition(cond_str, ctx);
            if !condition_result {
                // unless is negated if
                let sub = parse_code_with_context(&body, ctx)?;
                return Ok(Some((
                    ParsedCommand::TimesLoop {
                        count: 1,
                        commands: sub,
                    },
                    end_i,
                )));
            } else {
                return Ok(Some((
                    ParsedCommand::Comment(format!("# unless (skipped): {}", condition)),
                    end_i,
                )));
            }
        }
        return Ok(Some((
            ParsedCommand::Comment(format!("# unless: {}", line)),
            start_i,
        )));
    }

    // with_synth :synth_name do ... end
    if line.starts_with("with_synth") {
        let synth_name = extract_symbol(line).unwrap_or_else(|| "sine".to_string());
        let old_synth = ctx.current_synth;
        ctx.current_synth = parse_synth_name(&synth_name);
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        ctx.current_synth = old_synth; // restore after block
        return Ok(Some((
            ParsedCommand::TimesLoop {
                count: 1,
                commands: sub,
            },
            end_i,
        )));
    }

    // with_bpm N do ... end
    if line.starts_with("with_bpm") {
        let bpm_str = line.strip_prefix("with_bpm").unwrap_or("120").trim()
            .trim_end_matches("do").trim_end_matches("then").trim();
        let bpm: f32 = bpm_str.parse().unwrap_or(120.0);
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let mut sub = vec![ParsedCommand::SetBpm(bpm)];
        sub.extend(parse_code_with_context(&body, ctx)?);
        return Ok(Some((
            ParsedCommand::TimesLoop {
                count: 1,
                commands: sub,
            },
            end_i,
        )));
    }

    // with_bpm_mul N do ... end
    if line.starts_with("with_bpm_mul") {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::TimesLoop {
                count: 1,
                commands: sub,
            },
            end_i,
        )));
    }

    // .each do |x| ... end  (e.g., [:c4, :e4, :g4].each do |n|)
    // Also handles: var_name.each do |x|
    if line.contains(".each") && (line.ends_with("do") || line.contains("do |")) {
        let dot_pos = line.find(".each").unwrap();
        let list_expr = &line[..dot_pos];

        // Extract block variable name from |var|
        let block_var = line.find('|')
            .and_then(|start| {
                let after = &line[start + 1..];
                after.find('|').map(|end| after[..end].trim().to_string())
            })
            .unwrap_or_else(|| "x".to_string());

        let (body, end_i) = collect_block_body(lines, start_i)?;

        // Resolve the list
        if let Some(values) = ctx.resolve_to_list(list_expr) {
            let mut all_commands = Vec::new();
            for val in &values {
                // Set the block variable to the current value
                let old_val = ctx.variables.get(&block_var).cloned();
                ctx.variables.insert(block_var.clone(), val.clone());
                let sub = parse_code_with_context(&body, ctx)?;
                all_commands.extend(sub);
                // Restore old value
                if let Some(ov) = old_val {
                    ctx.variables.insert(block_var.clone(), ov);
                } else {
                    ctx.variables.remove(&block_var);
                }
            }
            return Ok(Some((
                ParsedCommand::TimesLoop {
                    count: 1,
                    commands: all_commands,
                },
                end_i,
            )));
        }

        // If we can't resolve the list, just skip the block
        return Ok(Some((
            ParsedCommand::Comment(format!("# each: {}", line)),
            end_i,
        )));
    }

    // .each_with_index do |x, i| ... end
    if line.contains(".each_with_index") && (line.ends_with("do") || line.contains("do |")) {
        let dot_pos = line.find(".each_with_index").unwrap();
        let list_expr = &line[..dot_pos];

        let (body, end_i) = collect_block_body(lines, start_i)?;

        if let Some(values) = ctx.resolve_to_list(list_expr) {
            let mut all_commands = Vec::new();
            for (_idx, val) in values.iter().enumerate() {
                ctx.variables.insert("__each_val".to_string(), val.clone());
                let sub = parse_code_with_context(&body, ctx)?;
                all_commands.extend(sub);
            }
            return Ok(Some((
                ParsedCommand::TimesLoop {
                    count: 1,
                    commands: all_commands,
                },
                end_i,
            )));
        }

        return Ok(Some((
            ParsedCommand::Comment(format!("# each_with_index: {}", line)),
            end_i,
        )));
    }

    // comment do ... end (ignore contents)
    if line == "comment do" || line.starts_with("comment do") {
        let (_body, end_i) = collect_block_body(lines, start_i)?;
        return Ok(Some((
            ParsedCommand::Comment("# commented out block".to_string()),
            end_i,
        )));
    }

    // uncomment do ... end (include contents)
    if line == "uncomment do" || line.starts_with("uncomment do") {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::Loop {
                name: "uncomment".to_string(),
                commands: sub,
                parallel: false,
            },
            end_i,
        )));
    }

    // density N do ... end
    if line.starts_with("density") {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        return Ok(Some((
            ParsedCommand::Loop {
                name: "density".to_string(),
                commands: sub,
                parallel: false,
            },
            end_i,
        )));
    }

    Ok(None)
}

/// Evaluate a condition expression (for if blocks)
fn evaluate_condition(condition: &str, ctx: &ParseContext) -> bool {
    let trimmed = condition.trim();

    // one_in(n)
    if let Some(result) = ctx.eval_one_in(trimmed) {
        return result;
    }

    // Numeric comparisons: val1 > val2, val1 < val2, val1 >= val2, val1 <= val2, val1 == val2, val1 != val2
    for op in &[">=", "<=", "!=", "==", ">", "<"] {
        if let Some(op_pos) = trimmed.find(op) {
            let left_str = trimmed[..op_pos].trim();
            let right_str = trimmed[op_pos + op.len()..].trim();

            // Try to resolve both sides as numbers
            let left = ctx.resolve_numeric(left_str)
                .or_else(|| left_str.parse::<f32>().ok());
            let right = ctx.resolve_numeric(right_str)
                .or_else(|| right_str.parse::<f32>().ok());

            if let (Some(l), Some(r)) = (left, right) {
                return match *op {
                    ">=" => l >= r,
                    "<=" => l <= r,
                    "!=" => (l - r).abs() > f32::EPSILON,
                    "==" => (l - r).abs() < f32::EPSILON,
                    ">" => l > r,
                    "<" => l < r,
                    _ => true,
                };
            }
            // String/symbol comparison
            let left_s = left_str.trim_start_matches(':');
            let right_s = right_str.trim_start_matches(':');
            return match *op {
                "==" => left_s == right_s,
                "!=" => left_s != right_s,
                _ => true,
            };
        }
    }

    // var.tick == value (ring tick comparison)
    if trimmed.contains(".tick") {
        // For ring tick patterns like "kick_pat.tick == 1"
        if let Some(dot_pos) = trimmed.find(".tick") {
            let var_name = trimmed[..dot_pos].trim();
            if let Some(values) = ctx.ring_values.get(var_name) {
                if let Some(eq_pos) = trimmed.find("==") {
                    let expected = trimmed[eq_pos + 2..].trim();
                    let match_count = values.iter().filter(|v| v.trim() == expected).count();
                    if values.is_empty() { return false; }
                    let probability = match_count as f64 / values.len() as f64;
                    let mut rng = rand::thread_rng();
                    return rng.gen_bool(probability.min(1.0));
                }
                return true;
            }
        }
        let mut rng = rand::thread_rng();
        return rng.gen_bool(0.5);
    }

    // true/false literals
    if trimmed == "true" { return true; }
    if trimmed == "false" { return false; }

    // Function call as condition: func_name(args) or func_name?(args)
    // If the function is defined, try to evaluate its body.
    // We can't fully evaluate Ruby return values, so for defined functions
    // whose body contains comparison operators, attempt a rough evaluation.
    // For time-based functions (referencing Time), default to false (time hasn't elapsed).
    let func_call_name = trimmed.split('(').next().unwrap_or("").trim();
    if ctx.functions.contains_key(func_call_name) {
        let body = ctx.functions.get(func_call_name).unwrap().clone();
        // If the function body references Time or time-based calculations, return false
        // since at parse time no real time has elapsed
        if body.contains("Time.now") || body.contains("start_time") || body.contains("stop_time") {
            eprintln!("[eval_condition] Function '{}' is time-based, defaulting to false", func_call_name);
            return false;
        }
        // For other defined functions, default to true
        return true;
    }

    // Default: true (include the block)
    true
}

/// Extract count from "N.times do" patterns
fn try_extract_times_count(line: &str) -> Option<usize> {
    // Match: 8.times do, 16.times do, etc.
    let line = line.trim();
    if let Some(dot_pos) = line.find(".times") {
        let num_str = line[..dot_pos].trim();
        if let Ok(n) = num_str.parse::<usize>() {
            // Ensure it ends with "do"
            if line.trim_end().ends_with("do") {
                return Some(n);
            }
        }
    }
    None
}
/// Collect block body lines between the opening line and matching 'end'
fn collect_block_body(lines: &[&str], start_i: usize) -> Result<(String, usize), String> {
    let mut depth = 1;
    let mut body_lines = Vec::new();
    let mut i = start_i + 1;

    while i < lines.len() {
        let l = lines[i].trim();

        // Check for 'end' (possibly with trailing comments)
        let l_no_comment = strip_inline_comment(l);
        if l_no_comment == "end" {
            depth -= 1;
            if depth == 0 {
                return Ok((body_lines.join("\n"), i));
            }
        }

        // Check for new blocks opening
        if is_block_opener(l) {
            depth += 1;
        }

        body_lines.push(lines[i]); // preserve original indentation
        i += 1;
    }

    // If we never found matching end, return what we have
    Ok((body_lines.join("\n"), i.saturating_sub(1)))
}

/// Collect block body for Ruby-style `def name(args) ... end` blocks.
/// These don't use `do` as the opener — the opening line IS the `def` line itself.
fn collect_block_body_for_def(lines: &[&str], start_i: usize) -> Result<(String, usize), String> {
    let mut depth = 1;
    let mut body_lines = Vec::new();
    let mut i = start_i + 1;

    while i < lines.len() {
        let l = lines[i].trim();
        let l_no_comment = strip_inline_comment(l);

        if l_no_comment == "end" {
            depth -= 1;
            if depth == 0 {
                return Ok((body_lines.join("\n"), i));
            }
        }

        if is_block_opener(l) {
            depth += 1;
        }

        body_lines.push(lines[i]);
        i += 1;
    }

    Ok((body_lines.join("\n"), i.saturating_sub(1)))
}

/// Collect block body for if/elsif/else blocks, preserving elsif/else markers
fn collect_block_body_with_else(lines: &[&str], start_i: usize) -> Result<(String, usize), String> {
    let mut depth = 1;
    let mut body_lines = Vec::new();
    let mut i = start_i + 1;

    while i < lines.len() {
        let l = lines[i].trim();
        let l_no_comment = strip_inline_comment(l);

        if l_no_comment == "end" {
            depth -= 1;
            if depth == 0 {
                return Ok((body_lines.join("\n"), i));
            }
        }

        if is_block_opener(l) {
            // Don't increase depth for elsif/else at our level
            let is_elsif_else = (l_no_comment.starts_with("elsif") || l_no_comment == "else") && depth == 1;
            if !is_elsif_else {
                depth += 1;
            }
        }

        body_lines.push(lines[i]);
        i += 1;
    }

    Ok((body_lines.join("\n"), i.saturating_sub(1)))
}

/// Parsed if/elsif/else branches
struct IfBranches {
    if_body: String,
    elsif_branches: Vec<(String, String)>, // (condition, body)
    else_body: Option<String>,
}

/// Split a block body containing elsif/else markers into branches
fn split_if_branches(body: &str) -> IfBranches {
    let lines: Vec<&str> = body.lines().collect();
    let mut if_lines = Vec::new();
    let mut elsif_branches: Vec<(String, String)> = Vec::new();
    let mut else_lines: Option<Vec<&str>> = None;
    let mut current_elsif_cond: Option<String> = None;
    let mut current_elsif_lines: Vec<&str> = Vec::new();
    let mut depth = 0;

    for line in &lines {
        let trimmed = strip_inline_comment(line.trim());

        // Track nested blocks
        if is_block_opener(line.trim()) {
            depth += 1;
        }
        if trimmed == "end" {
            depth -= 1;
        }

        // Only handle elsif/else at depth 0 (top level of the if body)
        if depth == 0 || (depth == 1 && is_block_opener(line.trim())) {
            if trimmed.starts_with("elsif ") {
                // Save current elsif branch if any
                if let Some(cond) = current_elsif_cond.take() {
                    elsif_branches.push((cond, current_elsif_lines.join("\n")));
                    current_elsif_lines.clear();
                }
                let cond = trimmed.strip_prefix("elsif ").unwrap_or("")
                    .trim_end_matches(" do").trim_end_matches(" then").trim().to_string();
                current_elsif_cond = Some(cond);
                continue;
            }
            if trimmed == "else" {
                // Save current elsif branch if any
                if let Some(cond) = current_elsif_cond.take() {
                    elsif_branches.push((cond, current_elsif_lines.join("\n")));
                    current_elsif_lines.clear();
                }
                else_lines = Some(Vec::new());
                continue;
            }
        }

        // Route lines to the right branch
        if let Some(ref mut el) = else_lines {
            el.push(line);
        } else if current_elsif_cond.is_some() {
            current_elsif_lines.push(line);
        } else {
            if_lines.push(*line);
        }
    }

    // Save last elsif if pending
    if let Some(cond) = current_elsif_cond.take() {
        elsif_branches.push((cond, current_elsif_lines.join("\n")));
    }

    IfBranches {
        if_body: if_lines.join("\n"),
        elsif_branches,
        else_body: else_lines.map(|l| l.join("\n")),
    }
}

/// Check if a line opens a new block (ends with 'do' or 'do |...|' or 'then')
fn is_block_opener(line: &str) -> bool {
    let trimmed = strip_inline_comment(line.trim());
    // Ends with "do" or "do |var|" or "do |var, var|"
    if trimmed.ends_with("do") {
        return true;
    }
    // Ends with "then" (if/elsif blocks)
    if trimmed.ends_with("then") {
        return true;
    }
    // "do |x|" pattern
    if let Some(do_pos) = trimmed.rfind(" do ") {
        let after = trimmed[do_pos + 4..].trim();
        if after.starts_with('|') && after.ends_with('|') {
            return true;
        }
    }
    // Also handle block openers like "begin"
    if trimmed == "begin" {
        return true;
    }
    // Ruby-style def name(args) ... end
    if trimmed.starts_with("def ") {
        return true;
    }
    false
}

/// Strip inline comment from a line (outside of strings)
fn strip_inline_comment(line: &str) -> String {
    let mut in_string = false;
    let mut string_char = ' ';
    let chars: Vec<char> = line.chars().collect();
    for i in 0..chars.len() {
        if in_string {
            if chars[i] == string_char && (i == 0 || chars[i - 1] != '\\') {
                in_string = false;
            }
        } else if chars[i] == '"' || chars[i] == '\'' {
            in_string = true;
            string_char = chars[i];
        } else if chars[i] == '#' {
            return line[..i].trim().to_string();
        }
    }
    line.trim().to_string()
}

/// Find a trailing `if` condition in a line (outside of strings).
/// Returns the byte position of the ` if ` keyword, or None.
/// Example: "sample :bd, amp: 2 if one_in(3)" -> Some(19)
fn find_trailing_if(line: &str) -> Option<usize> {
    let mut in_string = false;
    let mut string_char = ' ';
    let chars: Vec<char> = line.chars().collect();
    let mut byte_pos = 0usize;

    for i in 0..chars.len() {
        if in_string {
            if chars[i] == string_char && (i == 0 || chars[i - 1] != '\\') {
                in_string = false;
            }
        } else if chars[i] == '"' || chars[i] == '\'' {
            in_string = true;
            string_char = chars[i];
        } else if chars[i] == ' ' {
            // Check if " if " follows
            let remaining = &line[byte_pos..];
            if remaining.starts_with(" if ") {
                // Make sure it's a trailing if, not "if" at start or part of another word
                // It should come after a command, not at the start
                if byte_pos > 0 {
                    return Some(byte_pos + 1); // +1 to skip the leading space, point to 'i' in 'if'
                }
            }
        }
        byte_pos += chars[i].len_utf8();
    }
    None
}

/// Find a trailing `unless` condition in a line (outside of strings).
fn find_trailing_unless(line: &str) -> Option<usize> {
    let mut in_string = false;
    let mut string_char = ' ';
    let chars: Vec<char> = line.chars().collect();
    let mut byte_pos = 0usize;

    for i in 0..chars.len() {
        if in_string {
            if chars[i] == string_char && (i == 0 || chars[i - 1] != '\\') {
                in_string = false;
            }
        } else if chars[i] == '"' || chars[i] == '\'' {
            in_string = true;
            string_char = chars[i];
        } else if chars[i] == ' ' {
            let remaining = &line[byte_pos..];
            if remaining.starts_with(" unless ") {
                if byte_pos > 0 {
                    return Some(byte_pos + 1);
                }
            }
        }
        byte_pos += chars[i].len_utf8();
    }
    None
}

/// Try to resolve a note expression that involves list methods like .choose, .tick, etc.
/// Returns a resolved note string if the expression matches, None otherwise.
fn try_resolve_list_method(expr: &str, ctx: &mut ParseContext) -> Option<String> {
    let trimmed = expr.trim();

    // Split off params: "scale(:c4, :minor).choose, amp: 0.5" → separate at first comma after method
    let note_part = if let Some(method_end) = find_method_end(trimmed) {
        &trimmed[..method_end]
    } else {
        trimmed
    };

    // Check if it has a method call
    for method in &[".choose", ".pick", ".tick", ".look", ".first", ".last",
                    ".shuffle", ".reverse", ".min", ".max", ".sample"] {
        if note_part.contains(method) {
            return ctx.resolve_list_value(note_part);
        }
    }

    None
}

/// Find where a method call expression ends (before params)
fn find_method_end(expr: &str) -> Option<usize> {
    let mut paren_depth = 0;
    let mut found_method = false;

    for (i, ch) in expr.chars().enumerate() {
        if ch == '(' { paren_depth += 1; }
        if ch == ')' { paren_depth -= 1; }
        if ch == '.' && paren_depth == 0 { found_method = true; }
        if ch == ',' && paren_depth == 0 && found_method {
            return Some(i);
        }
    }
    None
}

/// Extract param with defaults fallback
fn extract_param_with_defaults(line: &str, param: &str, defaults: &HashMap<String, f32>, fallback: f32) -> f32 {
    extract_param(line, param)
        .or_else(|| defaults.get(param).copied())
        .unwrap_or(fallback)
}

/// Parse a defaults line like "use_synth_defaults attack: 0.1, release: 0.5"
fn parse_defaults_line(line: &str, prefix: &str, defaults: &mut HashMap<String, f32>) {
    let rest = line.strip_prefix(prefix).unwrap_or("").trim();
    // Parse key: value pairs
    for pair in rest.split(',') {
        let pair = pair.trim();
        if let Some(colon_pos) = pair.find(':') {
            let key = pair[..colon_pos].trim().to_string();
            let val_str = pair[colon_pos + 1..].trim();
            if let Ok(val) = val_str.parse::<f32>() {
                defaults.insert(key, val);
            }
        }
    }
}

fn parse_line(line: &str, ctx: &mut ParseContext) -> Option<ParsedCommand> {
    // Handle trailing `if one_in(n)` or `if condition`
    // e.g., "sample :drum_cymbal_hard, sustain: 0.2, amp: 2 if one_in(3)"
    if let Some(if_pos) = find_trailing_if(line) {
        let main_part = line[..if_pos].trim();
        let condition = line[if_pos + 3..].trim();  // skip "if "
        let condition_result = evaluate_condition(condition, ctx);
        if condition_result {
            return parse_line(main_part, ctx);
        } else {
            return None;  // Condition false, skip this line
        }
    }

    // Handle trailing `unless condition`
    if let Some(unless_pos) = find_trailing_unless(line) {
        let main_part = line[..unless_pos].trim();
        let condition = line[unless_pos + 7..].trim();  // skip "unless "
        let condition_result = evaluate_condition(condition, ctx);
        if !condition_result {
            return parse_line(main_part, ctx);
        } else {
            return None;
        }
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "play" => {
            // Check for chord: play chord(:e3, :minor7), ...
            if line.contains("chord(") || line.contains("chord (") {
                return parse_play_chord(line, ctx);
            }

            // Check for list/ring method calls: play scale(:c4, :minor).choose
            // or play notes.tick
            let note_expr = &line["play".len()..].trim();
            if let Some(note_str) = try_resolve_list_method(note_expr, ctx) {
                let note = parse_note_value(&note_str)?;
                let amplitude = extract_param_with_defaults(line, "amp", &ctx.synth_defaults, 0.5);
                let duration = extract_param_with_defaults(line, "sustain", &ctx.synth_defaults, 0.5);
                let pan = extract_param_with_defaults(line, "pan", &ctx.synth_defaults, 0.0);
                let attack = extract_param_with_defaults(line, "attack", &ctx.synth_defaults, 0.01);
                let decay = extract_param_with_defaults(line, "decay", &ctx.synth_defaults, 0.1);
                let sustain_level = extract_param_with_defaults(line, "sustain_level", &ctx.synth_defaults, 0.7);
                let release = extract_param_with_defaults(line, "release", &ctx.synth_defaults, 0.3);

                return Some(ParsedCommand::PlayNote {
                    synth_type: ctx.current_synth,
                    frequency: note,
                    amplitude,
                    duration,
                    pan,
                    envelope: Envelope {
                        attack,
                        decay,
                        sustain: sustain_level,
                        release,
                    },
                    params: extract_synth_params(line),
                });
            }

            let note_str = parts.get(1)?;
            let note = parse_note_value(note_str)?;
            let amplitude = extract_param_with_defaults(line, "amp", &ctx.synth_defaults, 0.5);
            let duration = extract_param(line, "sustain")
                .or_else(|| extract_param(line, "duration"))
                .or_else(|| ctx.synth_defaults.get("sustain").copied())
                .unwrap_or(0.5);
            let pan = extract_param_with_defaults(line, "pan", &ctx.synth_defaults, 0.0);
            let attack = extract_param_with_defaults(line, "attack", &ctx.synth_defaults, 0.01);
            let decay = extract_param_with_defaults(line, "decay", &ctx.synth_defaults, 0.1);
            let sustain_level = extract_param_with_defaults(line, "sustain_level", &ctx.synth_defaults, 0.7);
            let release = extract_param_with_defaults(line, "release", &ctx.synth_defaults, 0.3);

            Some(ParsedCommand::PlayNote {
                synth_type: ctx.current_synth,
                frequency: note,
                amplitude,
                duration,
                pan,
                envelope: Envelope {
                    attack,
                    decay,
                    sustain: sustain_level,
                    release,
                },
                params: extract_synth_params(line),
            })
        }
        "play_pattern_timed" => parse_play_pattern_timed(line, ctx),
        "play_pattern" => parse_play_pattern(line, ctx),
        "sample" => {
            // Sample can be: sample :name, sample path, sample var + "str"
            let rest = line["sample".len()..].trim();
            let (sample_expr, params_str) = split_sample_and_params(rest);
            let resolved = resolve_sample_name(sample_expr, ctx);
            eprintln!("[parse] sample expr='{}' -> resolved='{}'", sample_expr, resolved);

            let rate = extract_param_with_defaults(params_str, "rate", &ctx.sample_defaults, 1.0);
            let amplitude = extract_param_with_defaults(params_str, "amp", &ctx.sample_defaults, 1.0);
            let pan = extract_param_with_defaults(params_str, "pan", &ctx.sample_defaults, 0.0);

            // These are parsed but applied as rate modifiers where possible
            let rpitch = extract_param(params_str, "rpitch");
            let beat_stretch = extract_param(params_str, "beat_stretch");
            let _start = extract_param(params_str, "start"); // 0.0-1.0 range
            let _finish = extract_param(params_str, "finish"); // 0.0-1.0 range
            let _pitch_stretch = extract_param(params_str, "pitch_stretch");

            // Apply rpitch as rate modifier (semitone shift)
            let mut final_rate = rate;
            if let Some(rp) = rpitch {
                final_rate *= 2.0f32.powf(rp / 12.0);
            }
            // beat_stretch adjusts rate based on BPM — approximate
            if let Some(_bs) = beat_stretch {
                // beat_stretch needs sample duration knowledge,
                // approximate by just noting the param for now
                eprintln!("[parse] beat_stretch: {} (approximated)", _bs);
            }

            Some(ParsedCommand::PlaySample {
                name: resolved,
                rate: final_rate,
                amplitude,
                pan,
            })
        }
        "sleep" => {
            let duration: f32 = parts.get(1)?.parse().ok()?;
            Some(ParsedCommand::Sleep(duration))
        }
        "wait" => {
            let duration: f32 = parts.get(1)?.parse().ok()?;
            Some(ParsedCommand::Sleep(duration))
        }
        "use_bpm" => {
            let bpm: f32 = parts.get(1)?.parse().ok()?;
            Some(ParsedCommand::SetBpm(bpm))
        }
        "set_volume!" | "set_volume" => {
            let vol: f32 = parts.get(1)?.parse().ok()?;
            Some(ParsedCommand::SetVolume(vol))
        }
        "use_synth" => {
            let synth_name = parts.get(1)?.trim_start_matches(':');
            let synth_type = parse_synth_name(synth_name);
            Some(ParsedCommand::SetSynth(synth_type))
        }
        "synth" => {
            // synth :saw, note: :c4, release: 0.2
            let synth_name = parts.get(1).map(|s| s.trim_start_matches(':').trim_end_matches(','))
                .unwrap_or("sine");
            let synth_type = parse_synth_name(synth_name);

            // Try to resolve note as a list method expression
            let note = extract_param(line, "note")
                .or_else(|| extract_note_param(line, "note"))
                .or_else(|| {
                    // Check if note param uses a list method: note: scale(:c4, :minor).choose
                    if let Some(pos) = line.find("note:") {
                        let after = &line[pos + 5..].trim();
                        let note_expr: String = after.chars()
                            .take_while(|c| *c != ',' || after[..after.find(*c).unwrap_or(0)].matches('(').count()
                                > after[..after.find(*c).unwrap_or(0)].matches(')').count())
                            .collect();
                        if let Some(resolved) = try_resolve_list_method(&note_expr, ctx) {
                            parse_note_value(&resolved)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .unwrap_or(261.63);

            let amplitude = extract_param_with_defaults(line, "amp", &ctx.synth_defaults, 0.5);
            let duration = extract_param_with_defaults(line, "sustain", &ctx.synth_defaults, 0.5);
            let pan = extract_param_with_defaults(line, "pan", &ctx.synth_defaults, 0.0);
            let attack = extract_param_with_defaults(line, "attack", &ctx.synth_defaults, 0.01);
            let decay = extract_param_with_defaults(line, "decay", &ctx.synth_defaults, 0.1);
            let sustain_level = extract_param_with_defaults(line, "sustain_level", &ctx.synth_defaults, 0.7);
            let release = extract_param_with_defaults(line, "release", &ctx.synth_defaults, 0.3);
            Some(ParsedCommand::PlayNote {
                synth_type,
                frequency: note,
                amplitude,
                duration,
                pan,
                envelope: Envelope {
                    attack,
                    decay,
                    sustain: sustain_level,
                    release,
                },
                params: extract_synth_params(line),
            })
        }
        "stop" => Some(ParsedCommand::Stop),
        "puts" | "print" | "log" => {
            let msg = parts[1..].join(" ").trim_matches('"').to_string();
            Some(ParsedCommand::Log(msg))
        }
        "cue" | "sync" => {
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "at" => {
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "use_random_seed" | "use_random_source" => {
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "use_synth_defaults" => {
            // use_synth_defaults attack: 0.1, release: 0.5, amp: 0.8
            parse_defaults_line(line, "use_synth_defaults", &mut ctx.synth_defaults);
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "use_sample_defaults" => {
            parse_defaults_line(line, "use_sample_defaults", &mut ctx.sample_defaults);
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "use_merged_synth_defaults" => {
            parse_defaults_line(line, "use_merged_synth_defaults", &mut ctx.synth_defaults);
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "use_merged_sample_defaults" => {
            parse_defaults_line(line, "use_merged_sample_defaults", &mut ctx.sample_defaults);
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "tick" => {
            // Standalone tick — advance global tick counter
            ctx.global_tick += 1;
            Some(ParsedCommand::Comment(format!("# tick = {}", ctx.global_tick)))
        }
        "look" => {
            // Standalone look — just reads counter, no side effect at parse time
            Some(ParsedCommand::Comment(format!("# look = {}", ctx.global_tick)))
        }
        "set" | "get" => {
            // set/get for shared state — treat like variables
            if parts[0] == "set" {
                if parts.len() >= 3 {
                    let key = parts[1].trim_start_matches(':').trim_end_matches(',').to_string();
                    let val = parts[2..].join(" ");
                    ctx.variables.insert(key, val);
                }
            }
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "control" => {
            // control — modifying running synths, not directly supported but don't error
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "midi" | "midi_note_on" | "midi_note_off" | "midi_cc" | "midi_raw" | "midi_pitch_bend"
        | "midi_channel_pressure" | "midi_poly_pressure" | "midi_clock_tick"
        | "midi_start" | "midi_stop" | "midi_reset" | "midi_local_control_off"
        | "midi_local_control_on" | "midi_mode" | "midi_all_notes_off" => {
            // MIDI commands — not applicable to audio engine but don't error
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "sample_duration" => {
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "use_timing_guarantees" | "use_arg_checks" | "use_debug"
        | "use_cue_logging" | "use_external_synths" | "use_arg_bpm_scaling" => {
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "time_warp" | "with_swing" => {
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "with_fx" | "with_synth" | "with_bpm" | "with_bpm_mul" => {
            None // Handled in block parser
        }
        _ => None,
    }
}

/// Parse synth name to type – maps every Sonic Pi synth name
fn parse_synth_name(name: &str) -> OscillatorType {
    match name {
        // ── Basic oscillators ──
        "sine" | "beep" => OscillatorType::Sine,
        "saw" => OscillatorType::Saw,
        "square" => OscillatorType::Square,
        "tri" | "triangle" => OscillatorType::Triangle,
        "noise" => OscillatorType::Noise,
        "pulse" => OscillatorType::Pulse,
        "supersaw" | "super_saw" => OscillatorType::SuperSaw,

        // ── Detuned oscillators ──
        "dsaw" => OscillatorType::DSaw,
        "dpulse" => OscillatorType::DPulse,
        "dtri" => OscillatorType::DTri,

        // ── FM synthesis ──
        "fm" => OscillatorType::FM,
        "mod_fm" => OscillatorType::ModFM,

        // ── Modulated oscillators ──
        "mod_sine" => OscillatorType::ModSine,
        "mod_saw" => OscillatorType::ModSaw,
        "mod_dsaw" => OscillatorType::ModDSaw,
        "mod_tri" => OscillatorType::ModTri,
        "mod_pulse" => OscillatorType::ModPulse,

        // ── Classic synths ──
        "tb303" => OscillatorType::TB303,
        "prophet" => OscillatorType::Prophet,
        "zawa" => OscillatorType::Zawa,

        // ── Filtered / layered ──
        "blade" => OscillatorType::Blade,
        "tech_saws" => OscillatorType::TechSaws,
        "hoover" => OscillatorType::Hoover,

        // ── Plucked / percussive ──
        "pluck" => OscillatorType::Pluck,
        "piano" => OscillatorType::Piano,
        "pretty_bell" => OscillatorType::PrettyBell,
        "dull_bell" => OscillatorType::DullBell,

        // ── Pads / ambient ──
        "hollow" => OscillatorType::Hollow,
        "dark_ambience" => OscillatorType::DarkAmbience,
        "growl" => OscillatorType::Growl,

        // ── Chiptune ──
        "chiplead" | "chip_lead" => OscillatorType::ChipLead,
        "chipbass" | "chip_bass" => OscillatorType::ChipBass,
        "chipnoise" | "chip_noise" => OscillatorType::ChipNoise,

        // ── Colored noise ──
        "bnoise" | "brown_noise" => OscillatorType::BNoise,
        "pnoise" | "pink_noise" => OscillatorType::PNoise,
        "gnoise" | "grey_noise" => OscillatorType::GNoise,
        "cnoise" | "clip_noise" => OscillatorType::CNoise,

        // ── Sub ──
        "subpulse" | "sub_pulse" => OscillatorType::SubPulse,

        // ── Aliases / fallbacks ──
        "bass" => OscillatorType::TB303,
        "lead" => OscillatorType::SuperSaw,
        "pad" => OscillatorType::Hollow,
        "winwood_lead" => OscillatorType::SuperSaw,

        _ => OscillatorType::Sine,
    }
}

/// Parse "play chord(:e3, :minor7), release: 1, amp: 1"
fn parse_play_chord(line: &str, ctx: &ParseContext) -> Option<ParsedCommand> {
    let amplitude = extract_param(line, "amp").unwrap_or(0.5);
    let release = extract_param(line, "release").unwrap_or(0.3);
    let attack = extract_param(line, "attack").unwrap_or(0.01);

    // Extract chord(...) content
    let chord_start = line.find("chord(")?;
    let chord_inner_start = chord_start + 6;
    let chord_end = line[chord_inner_start..].find(')')? + chord_inner_start;
    let chord_args = &line[chord_inner_start..chord_end];

    // Parse chord args: :e3, :minor7 or :e3, :m7 etc.
    let args: Vec<&str> = chord_args.split(',').map(|s| s.trim()).collect();
    let root_str = args.first()?.trim_start_matches(':');
    let chord_type = args.get(1).map(|s| s.trim_start_matches(':')).unwrap_or("major");

    // Get root note frequency
    let root_midi = note_name_to_midi(&root_str.to_uppercase())?;

    // Generate chord intervals
    let _intervals = chord_intervals(chord_type);

    // Return first note as the main note (we'll generate all chord notes as separate PlayNote commands
    // but for simplicity, return the root - the run_code handler will handle the full chord)
    // Actually, let's return multiple notes - we need a way. For now return root.
    let freq = midi_to_freq(root_midi);

    // We'll just play the root note for now with chord context.
    // A better approach: generate all notes. But ParsedCommand is a single command.
    // So we return root and will handle chord expansion below.
    Some(ParsedCommand::PlayNote {
        synth_type: ctx.current_synth,
        frequency: freq,
        amplitude,
        duration: 0.5,
        pan: 0.0,
        envelope: Envelope {
            attack,
            decay: 0.1,
            sustain: 0.7,
            release,
        },
        params: extract_synth_params(line),
    })
}

/// Get chord intervals in semitones from root
fn chord_intervals(chord_type: &str) -> Vec<i32> {
    match chord_type {
        "major" | "M" => vec![0, 4, 7],
        "minor" | "m" => vec![0, 3, 7],
        "major7" | "M7" | "maj7" => vec![0, 4, 7, 11],
        "minor7" | "m7" | "min7" => vec![0, 3, 7, 10],
        "dom7" | "7" => vec![0, 4, 7, 10],
        "dim" | "diminished" => vec![0, 3, 6],
        "dim7" | "diminished7" => vec![0, 3, 6, 9],
        "aug" | "augmented" => vec![0, 4, 8],
        "sus2" => vec![0, 2, 7],
        "sus4" => vec![0, 5, 7],
        "add9" => vec![0, 4, 7, 14],
        "m9" | "minor9" => vec![0, 3, 7, 10, 14],
        "9" | "dom9" => vec![0, 4, 7, 10, 14],
        "11" => vec![0, 4, 7, 10, 14, 17],
        "13" => vec![0, 4, 7, 10, 14, 17, 21],
        "power" | "5" => vec![0, 7],
        "i" => vec![0, 4, 7],
        "ii" => vec![0, 3, 7],
        _ => vec![0, 4, 7], // Default to major
    }
}

/// Parse play_pattern_timed: play_pattern_timed [:e2, :g2, :b2, :d3], [0.5, 0.5, 1, 0.5], release: 0.3
fn parse_play_pattern_timed(line: &str, ctx: &ParseContext) -> Option<ParsedCommand> {
    let amplitude = extract_param(line, "amp").unwrap_or(0.5);
    let release = extract_param(line, "release").unwrap_or(0.3);
    let attack = extract_param(line, "attack").unwrap_or(0.01);
    let synth_params = extract_synth_params(line);

    // Extract the notes array and timing array
    let notes = extract_array(line, 0)?;
    let timings = extract_array(line, 1).unwrap_or_else(|| vec!["0.5".to_string()]);

    // Parse notes to frequencies
    let frequencies: Vec<f32> = notes
        .iter()
        .filter_map(|n| parse_note_value(n))
        .collect();

    // Parse timings
    let timing_vals: Vec<f32> = timings
        .iter()
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();

    if frequencies.is_empty() {
        return None;
    }

    // Generate a sequence of PlayNote + Sleep commands
    // Since we can only return one ParsedCommand, we'll create a TimesLoop with the sequence
    let mut sub_commands = Vec::new();
    for (idx, freq) in frequencies.iter().enumerate() {
        if *freq > 0.0 {
            sub_commands.push(ParsedCommand::PlayNote {
                synth_type: ctx.current_synth,
                frequency: *freq,
                amplitude,
                duration: release,
                pan: 0.0,
                envelope: Envelope {
                    attack,
                    decay: 0.05,
                    sustain: 0.7,
                    release,
                },
                params: synth_params.clone(),
            });
        }
        let sleep_dur = timing_vals
            .get(idx % timing_vals.len().max(1))
            .copied()
            .unwrap_or(0.5);
        sub_commands.push(ParsedCommand::Sleep(sleep_dur));
    }

    Some(ParsedCommand::TimesLoop {
        count: 1,
        commands: sub_commands,
    })
}

/// Parse play_pattern: play_pattern [:c4, :e4, :g4]
fn parse_play_pattern(line: &str, ctx: &ParseContext) -> Option<ParsedCommand> {
    let amplitude = extract_param(line, "amp").unwrap_or(0.5);
    let release = extract_param(line, "release").unwrap_or(0.3);
    let synth_params = extract_synth_params(line);

    let notes = extract_array(line, 0)?;
    let frequencies: Vec<f32> = notes
        .iter()
        .filter_map(|n| parse_note_value(n))
        .collect();

    if frequencies.is_empty() {
        return None;
    }

    let mut sub_commands = Vec::new();
    for freq in &frequencies {
        if *freq > 0.0 {
            sub_commands.push(ParsedCommand::PlayNote {
                synth_type: ctx.current_synth,
                frequency: *freq,
                amplitude,
                duration: release,
                pan: 0.0,
                envelope: Envelope::default(),
                params: synth_params.clone(),
            });
        }
        sub_commands.push(ParsedCommand::Sleep(1.0));
    }

    Some(ParsedCommand::TimesLoop {
        count: 1,
        commands: sub_commands,
    })
}

/// Extract the Nth bracketed array from a line
/// e.g., for "play_pattern_timed [:c4, :e4], [0.5, 0.5], amp: 1" with n=0 returns [":c4", ":e4"]
fn extract_array(line: &str, nth: usize) -> Option<Vec<String>> {
    let mut arrays_found = 0;
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();

    while i < chars.len() {
        if chars[i] == '[' {
            let start = i + 1;
            let mut depth = 1;
            i += 1;
            while i < chars.len() && depth > 0 {
                if chars[i] == '[' {
                    depth += 1;
                } else if chars[i] == ']' {
                    depth -= 1;
                }
                i += 1;
            }
            if arrays_found == nth {
                let content: String = chars[start..i - 1].iter().collect();
                let items: Vec<String> = content
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                return Some(items);
            }
            arrays_found += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Split sample expression from params: ":bd_haus, amp: 2" -> (":bd_haus", "amp: 2")
fn split_sample_and_params(rest: &str) -> (&str, &str) {
    // The sample name could be: :symbol, "path", variable + "path"
    let trimmed = rest.trim();

    // If starts with :, find end of symbol
    if trimmed.starts_with(':') {
        if let Some(comma_pos) = trimmed.find(',') {
            let name = trimmed[..comma_pos].trim();
            let params = trimmed[comma_pos + 1..].trim();
            return (name, params);
        }
        return (trimmed, "");
    }

    // If it contains string concatenation (+), find the end of the expression
    if trimmed.contains('+') || trimmed.starts_with('"') {
        // Find the first comma that's not inside quotes or string concat
        let mut in_string = false;
        let chars: Vec<char> = trimmed.chars().collect();
        for i in 0..chars.len() {
            if chars[i] == '"' {
                in_string = !in_string;
            } else if chars[i] == ',' && !in_string {
                return (trimmed[..i].trim(), trimmed[i + 1..].trim());
            }
        }
        return (trimmed, "");
    }

    // Simple identifier
    if let Some(comma_pos) = trimmed.find(',') {
        (trimmed[..comma_pos].trim(), trimmed[comma_pos + 1..].trim())
    } else {
        (trimmed, "")
    }
}

/// Resolve a sample name expression to a final path/name
fn resolve_sample_name(expr: &str, ctx: &ParseContext) -> String {
    let trimmed = expr.trim();

    // Symbol like :bd_haus
    if trimmed.starts_with(':') {
        return trimmed[1..].trim_end_matches(',').to_string();
    }

    // String concatenation or variable
    ctx.resolve_string(trimmed)
}

fn parse_note_value(value: &str) -> Option<f32> {
    let v = value.trim().trim_end_matches(',').trim_start_matches(':');

    // Rest / silence
    if v == "r" || v == "rest" || v == "R" {
        return Some(0.0);
    }

    // Direct frequency (large number)
    if let Ok(f) = v.parse::<f32>() {
        if f > 20.0 {
            return Some(f);
        } else if f >= 0.0 {
            // Treat as MIDI note
            return Some(midi_to_freq(f as u8));
        }
    }

    // MIDI note number as integer
    if let Ok(midi) = v.parse::<u8>() {
        return Some(midi_to_freq(midi));
    }

    // Note name like c4, fs3, eb5
    let name = v.to_uppercase();
    if let Some(midi) = note_name_to_midi(&name) {
        return Some(midi_to_freq(midi));
    }

    None
}

fn extract_param(line: &str, param: &str) -> Option<f32> {
    let patterns = [
        format!("{}: ", param),
        format!("{}:", param),
        format!("{} => ", param),
    ];
    for pat in &patterns {
        if let Some(pos) = line.find(pat.as_str()) {
            let after = &line[pos + pat.len()..];
            let after_trimmed = after.trim();

            // Check if the value is a function call like rrand(), rand(), etc.
            for func_name in &["rrand", "rrand_i", "rand", "rand_i", "dice"] {
                if after_trimmed.starts_with(&format!("{}(", func_name)) {
                    // Extract the full function call including parens
                    if let Some(inner) = extract_func_args(after_trimmed, func_name) {
                        let func_call = &after_trimmed[..func_name.len() + 1 + inner.len() + 1];
                        let ctx = ParseContext::new();
                        if let Some(val) = ctx.resolve_numeric(func_call) {
                            return Some(val);
                        }
                    }
                }
            }

            // Check for arithmetic with rrand: "1 + rrand(0, 0.5)"
            if after_trimmed.contains("rrand") || after_trimmed.contains("rand(") || after_trimmed.contains("dice(") {
                // Find the extent of the expression (up to next comma or end)
                let expr_end = after_trimmed.find(|c: char| c == ',' && !after_trimmed[..after_trimmed.find(c).unwrap_or(0)].contains('('))
                    .unwrap_or(after_trimmed.len());
                let expr = &after_trimmed[..expr_end];
                let ctx = ParseContext::new();
                if let Some(val) = ctx.resolve_numeric(expr) {
                    return Some(val);
                }
            }

            let val_str: String = after
                .trim()
                .chars()
                .take_while(|c| c.is_numeric() || *c == '.' || *c == '-')
                .collect();
            if let Ok(v) = val_str.parse::<f32>() {
                return Some(v);
            }
        }
    }
    None
}

/// Extract a note value from a named param like "note: :c4"
fn extract_note_param(line: &str, param: &str) -> Option<f32> {
    let patterns = [
        format!("{}: ", param),
        format!("{}:", param),
    ];
    for pat in &patterns {
        if let Some(pos) = line.find(pat.as_str()) {
            let after = &line[pos + pat.len()..].trim();
            let val_str: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == ':' || *c == '#' || *c == '_')
                .collect();
            let clean = val_str.trim_start_matches(':');
            if let Some(freq) = parse_note_value(clean) {
                return Some(freq);
            }
        }
    }
    None
}

fn extract_symbol(line: &str) -> Option<String> {
    if let Some(pos) = line.find(':') {
        let after = &line[pos + 1..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn extract_fx_params(line: &str) -> Vec<(String, f32)> {
    let mut params = Vec::new();
    let param_names = [
        "mix", "room", "time", "feedback", "phase", "decay", "cutoff", "res",
        "rate", "depth", "amp", "pre_amp", "distort", "damp", "spread",
        "release", "attack", "sustain", "reps",
    ];
    for name in &param_names {
        if let Some(val) = extract_param(line, name) {
            params.push((name.to_string(), val));
        }
    }
    params
}

/// Extract synth-specific parameters from a play/synth line.
/// These are forwarded to SuperCollider as named OSC args so the
/// SynthDef can use them (cutoff, res, detune, wave, depth, divisor, etc.)
fn extract_synth_params(line: &str) -> Vec<(String, f32)> {
    let mut params = Vec::new();
    let synth_param_names = [
        "cutoff", "res", "detune", "depth", "divisor", "wave",
        "pulse_width", "width", "sub_amp", "noise", "coef",
        "mod_phase", "mod_range", "mod_pulse_width", "mod_phase_offset",
        "mod_wave", "mod_invert_wave", "vel",
    ];
    for name in &synth_param_names {
        if let Some(val) = extract_param(line, name) {
            params.push((name.to_string(), val));
        }
    }
    params
}

/// Convert parsed commands to audio commands with timing
pub fn commands_to_audio(
    parsed: &[ParsedCommand],
    bpm: f32,
) -> Vec<(f32, AudioCommand)> {
    let mut result = Vec::new();
    let mut time_offset = 0.0f32;
    let mut current_bpm = bpm;
    let mut beat_duration = 60.0 / current_bpm;
    let mut current_reverb = 0.0f32;
    let mut current_delay_time = 0.0f32;
    let mut current_delay_feedback = 0.0f32;
    let mut current_distortion = 0.0f32;
    let mut current_lpf = 20000.0f32;
    let mut current_hpf = 20.0f32;

    for cmd in parsed {
        match cmd {
            ParsedCommand::PlayNote {
                synth_type,
                frequency,
                amplitude,
                duration,
                pan,
                envelope,
                params,
            } => {
                if *frequency > 0.0 {
                    let total_dur = duration + envelope.attack + envelope.decay + envelope.release;
                    result.push((
                        time_offset,
                        AudioCommand::PlayNote {
                            synth_type: *synth_type,
                            frequency: *frequency,
                            amplitude: *amplitude,
                            duration_secs: total_dur,
                            envelope: *envelope,
                            pan: *pan,
                            params: params.clone(),
                        },
                    ));
                }
            }
            ParsedCommand::PlaySample {
                name: _name,
                rate,
                amplitude,
                pan,
            } => {
                result.push((
                    time_offset,
                    AudioCommand::PlaySample {
                        samples: Vec::new(), // Will be filled by the caller
                        sample_rate: 44100,
                        amplitude: *amplitude,
                        rate: *rate,
                        pan: *pan,
                    },
                ));
            }
            ParsedCommand::Sleep(beats) => {
                time_offset += beats * beat_duration;
            }
            ParsedCommand::SetBpm(bpm_val) => {
                current_bpm = *bpm_val;
                beat_duration = 60.0 / current_bpm;
                result.push((time_offset, AudioCommand::SetBpm(*bpm_val)));
            }
            ParsedCommand::SetVolume(vol) => {
                result.push((time_offset, AudioCommand::SetMasterVolume(*vol)));
            }
            ParsedCommand::WithFx {
                fx_type,
                params,
                commands,
            } => {
                // Emit FxStart — the SC engine will allocate a private audio bus,
                // create the FX synth on it, and route subsequent synths through it.
                // The cpal engine falls back to global SetEffect.
                result.push((
                    time_offset,
                    AudioCommand::FxStart {
                        fx_type: fx_type.clone(),
                        params: params.clone(),
                    },
                ));

                // Also emit SetEffect for the cpal engine path (fallback)
                // Save FX state before block so we can restore it after (scoped FX)
                let saved_reverb = current_reverb;
                let saved_delay_time = current_delay_time;
                let saved_delay_feedback = current_delay_feedback;
                let saved_distortion = current_distortion;
                let saved_lpf = current_lpf;
                let saved_hpf = current_hpf;

                match fx_type.as_str() {
                    "reverb" | "gverb" | "krush" => {
                        current_reverb = params.iter().find(|(n, _)| n == "mix").map(|(_, v)| *v).unwrap_or(0.5);
                    }
                    "echo" | "delay" => {
                        current_delay_time = params.iter().find(|(n, _)| n == "phase" || n == "time").map(|(_, v)| *v).unwrap_or(0.25);
                        current_delay_feedback = params.iter().find(|(n, _)| n == "feedback" || n == "decay").map(|(_, v)| *v).unwrap_or(0.5);
                    }
                    "distortion" | "tanh" => {
                        current_distortion = params.iter().find(|(n, _)| n == "distort" || n == "mix").map(|(_, v)| *v).unwrap_or(0.5);
                    }
                    "lpf" | "rlpf" | "nrlpf" => {
                        current_lpf = params.iter().find(|(n, _)| n == "cutoff").map(|(_, v)| *v).unwrap_or(1000.0);
                    }
                    "hpf" | "rhpf" | "nrhpf" => {
                        current_hpf = params.iter().find(|(n, _)| n == "cutoff").map(|(_, v)| *v).unwrap_or(500.0);
                    }
                    _ => {}
                }

                result.push((
                    time_offset,
                    AudioCommand::SetEffect {
                        reverb_mix: current_reverb,
                        delay_time: current_delay_time,
                        delay_feedback: current_delay_feedback,
                        distortion: current_distortion,
                        lpf_cutoff: current_lpf,
                        hpf_cutoff: current_hpf,
                    },
                ));

                // Process inner commands
                let inner = commands_to_audio(commands, current_bpm);
                for (t, c) in inner {
                    result.push((time_offset + t, c));
                }

                // Update time offset from inner commands
                let inner_duration = commands_to_duration(commands, current_bpm);
                time_offset += inner_duration;

                // Emit FxEnd — SC engine will free the FX synth and restore bus
                result.push((time_offset, AudioCommand::FxEnd));

                // Restore FX state after block (with_fx is scoped in Sonic Pi)
                current_reverb = saved_reverb;
                current_delay_time = saved_delay_time;
                current_delay_feedback = saved_delay_feedback;
                current_distortion = saved_distortion;
                current_lpf = saved_lpf;
                current_hpf = saved_hpf;

                // Emit a SetEffect to restore the previous FX state (cpal fallback)
                result.push((
                    time_offset,
                    AudioCommand::SetEffect {
                        reverb_mix: current_reverb,
                        delay_time: current_delay_time,
                        delay_feedback: current_delay_feedback,
                        distortion: current_distortion,
                        lpf_cutoff: current_lpf,
                        hpf_cutoff: current_hpf,
                    },
                ));
            }
            ParsedCommand::Loop { commands, name, parallel } => {
                // Check if the loop body contains a Stop command at the top level
                let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                // If the body has 'stop', it's a one-shot section — run just once
                // Otherwise repeat up to 500 times for indefinite loops
                let loop_iterations = if has_stop { 1 } else { 500 };
                eprintln!("[parser] live_loop :{} → {} iteration(s), stop={}, parallel={}", name, loop_iterations, has_stop, parallel);
                
                let loop_start_offset = time_offset;
                let mut loop_time = loop_start_offset;
                for iter in 0..loop_iterations {
                    let inner = commands_to_audio(commands, current_bpm);
                    let inner_duration = commands_to_duration(commands, current_bpm);
                    for (t, c) in inner {
                        result.push((loop_time + t, c));
                    }
                    loop_time += inner_duration;
                    // Safety: cap at 100k commands to prevent blocking
                    if result.len() > 100_000 {
                        eprintln!("[parser] WARNING: command limit reached in live_loop :{} at iteration {}", name, iter);
                        break;
                    }
                }
                
                if *parallel {
                    // Parallel loops (live_loop, in_thread) do NOT advance the
                    // parent time offset — they run concurrently with subsequent code.
                    // time_offset stays unchanged.
                } else {
                    // Sequential loops advance time normally
                    time_offset = loop_time;
                }
            }
            ParsedCommand::TimesLoop { count, commands } => {
                // Repeat commands N times
                for iter in 0..*count {
                    let inner = commands_to_audio(commands, current_bpm);
                    let inner_duration = commands_to_duration(commands, current_bpm);
                    for (t, c) in inner {
                        result.push((time_offset + t, c));
                    }
                    time_offset += inner_duration;
                    // Safety: cap at 100k commands
                    if result.len() > 100_000 {
                        eprintln!("[parser] WARNING: command limit reached in {}.times at iteration {}", count, iter);
                        break;
                    }
                }
            }
            ParsedCommand::Stop => {
                // Stop this sequence - break out
                break;
            }
            ParsedCommand::SetSynth(_) | ParsedCommand::Comment(_) | ParsedCommand::Log(_) => {}
        }
    }

    result
}

/// Calculate the total duration of a sequence of parsed commands in seconds
fn commands_to_duration(parsed: &[ParsedCommand], bpm: f32) -> f32 {
    let mut current_bpm = bpm;
    let mut beat_duration = 60.0 / current_bpm;
    let mut dur = 0.0f32;
    for cmd in parsed {
        match cmd {
            ParsedCommand::Sleep(beats) => {
                dur += beats * beat_duration;
            }
            ParsedCommand::SetBpm(bpm_val) => {
                current_bpm = *bpm_val;
                beat_duration = 60.0 / current_bpm;
            }
            ParsedCommand::TimesLoop { count, commands } => {
                dur += *count as f32 * commands_to_duration(commands, current_bpm);
            }
            ParsedCommand::Loop { commands, parallel, .. } => {
                if *parallel {
                    // Parallel loops don't contribute to sequential duration
                    // (they run concurrently)
                } else {
                    let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                    let iters = if has_stop { 1.0 } else { 500.0 };
                    dur += iters * commands_to_duration(commands, current_bpm);
                }
            }
            ParsedCommand::WithFx { commands, .. } => {
                dur += commands_to_duration(commands, current_bpm);
            }
            ParsedCommand::Stop => break,
            _ => {}
        }
    }
    dur
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_path_variable_resolution() {
        let code = r#"
sample_path = "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/"
live_loop :verse1_vocals do
  6.times do
    sample sample_path + "african-vocals-gubulah-high.wav", amp: 1.5
    sleep 5.6
  end
  stop
end
"#;
        let parsed = parse_code(code).unwrap();
        
        fn find_samples(cmds: &[ParsedCommand]) -> Vec<String> {
            let mut result = Vec::new();
            for cmd in cmds {
                match cmd {
                    ParsedCommand::PlaySample { name, .. } => {
                        result.push(name.clone());
                    }
                    ParsedCommand::Loop { commands, .. }
                    | ParsedCommand::WithFx { commands, .. }
                    | ParsedCommand::TimesLoop { commands, .. } => {
                        result.extend(find_samples(commands));
                    }
                    _ => {}
                }
            }
            result
        }
        let samples = find_samples(&parsed);
        eprintln!("Found sample names: {:?}", samples);
        assert!(!samples.is_empty(), "Should have found sample names");
        assert_eq!(
            samples[0],
            "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/african-vocals-gubulah-high.wav"
        );
    }

    #[test]
    fn test_builtin_sample_parsing() {
        let code = r#"
sample :bd_haus, amp: 2
sleep 1
sample :perc_snap, rate: 2, amp: 0.7
"#;
        let parsed = parse_code(code).unwrap();
        let mut sample_names = Vec::new();
        for cmd in &parsed {
            if let ParsedCommand::PlaySample { name, .. } = cmd {
                sample_names.push(name.clone());
            }
        }
        assert_eq!(sample_names, vec!["bd_haus", "perc_snap"]);
    }

    #[test]
    fn test_commands_to_audio_sample_count() {
        let code = r#"
sample_path = "C:/test/"
live_loop :test do
  sample sample_path + "file.wav", amp: 1.0
  sleep 1
  sample :bd_haus, amp: 1.0
  sleep 1
  stop
end
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let sample_cmds: Vec<_> = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .collect();
        eprintln!("Timed sample commands: {:?}", sample_cmds.len());
        assert_eq!(sample_cmds.len(), 2, "Should have 2 sample commands");
    }

    /// Mirrored from lib.rs to test index matching
    fn collect_sample_names_test(parsed: &[ParsedCommand]) -> Vec<String> {
        let mut names = Vec::new();
        collect_names_recursive(parsed, &mut names);
        names
    }
    fn collect_names_recursive(parsed: &[ParsedCommand], names: &mut Vec<String>) {
        for cmd in parsed {
            match cmd {
                ParsedCommand::PlaySample { name, .. } => {
                    names.push(name.clone());
                }
                ParsedCommand::Loop { commands, .. } => {
                    let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                    let iters = if has_stop { 1 } else { 500 };
                    for _ in 0..iters {
                        collect_names_recursive(commands, names);
                        if names.len() > 100_000 { return; }
                    }
                }
                ParsedCommand::TimesLoop { count, commands } => {
                    for _ in 0..*count {
                        collect_names_recursive(commands, names);
                    }
                }
                ParsedCommand::WithFx { commands, .. } => {
                    collect_names_recursive(commands, names);
                }
                ParsedCommand::Stop => { return; }
                _ => {}
            }
        }
    }

    #[test]
    fn test_sample_index_matching_full() {
        let code = r#"
use_bpm 123
use_synth :fm
sample_path = "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/"

live_loop :intro_perc do
  8.times do
    sample :perc_snap, amp: 0.5
    sleep 0.5
  end
  stop
end

sleep 8

live_loop :verse1_drums do
  16.times do
    sample :bd_haus, amp: 2
    sleep 1
    sample :sn_dub, amp: 1.5
    sleep 1
  end
  stop
end

live_loop :verse1_vocals do
  sleep 2
  6.times do
    sample sample_path + "african-vocals-gubulah-high.wav", amp: 1.5
    sleep 5.6
  end
  sleep 15
  6.times do
    sample sample_path + "african-vocals-gubulah-high.wav", amp: 1.5
    sleep 5.6
  end
  sleep 12
  stop
end

live_loop :breakdown do
  sample sample_path + "african-vocals-weeh-oh-mid.wav", amp: 1.5
  sleep 2.73
  sample sample_path + "zap-mama-style-3.wav", amp: 1.2
  sleep 3
  stop
end
"#;
        let parsed = parse_code(code).unwrap();
        
        // Count PlaySample commands in timed_commands
        let timed = commands_to_audio(&parsed, 123.0);
        let play_sample_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();
        
        // Count sample names from collect_sample_names
        let sample_names = collect_sample_names_test(&parsed);
        
        eprintln!("PlaySample commands in timed_commands: {}", play_sample_count);
        eprintln!("Sample names collected: {}", sample_names.len());
        for (i, name) in sample_names.iter().enumerate() {
            eprintln!("  [{}] {}", i, name);
        }
        
        assert_eq!(
            play_sample_count, sample_names.len(),
            "PlaySample count in commands_to_audio ({}) must match collect_sample_names count ({})",
            play_sample_count, sample_names.len()
        );
    }

    #[test]
    fn test_parallel_live_loops_timing() {
        // Two live_loops separated by sleep should start at different offsets.
        // Two consecutive live_loops without sleep should start at the SAME offset.
        let code = r#"
use_bpm 120

live_loop :a do
  sample :bd_haus
  sleep 1
  stop
end

live_loop :b do
  sample :sn_dub
  sleep 1
  stop
end

sleep 4

live_loop :c do
  sample :perc_snap
  sleep 1
  stop
end
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        
        let sample_times: Vec<f32> = timed.iter()
            .filter_map(|(t, c)| {
                if matches!(c, AudioCommand::PlaySample { .. }) { Some(*t) } else { None }
            })
            .collect();
        
        eprintln!("Sample times: {:?}", sample_times);
        assert_eq!(sample_times.len(), 3, "Should have 3 samples");
        // Loop :a and :b are consecutive live_loops → both at t=0
        assert!((sample_times[0] - 0.0).abs() < 0.01, "Loop :a should start at t=0");
        assert!((sample_times[1] - 0.0).abs() < 0.01, "Loop :b should start at t=0 (parallel)");
        // sleep 4 with BPM 120 → 4 * 0.5s = 2.0s
        assert!((sample_times[2] - 2.0).abs() < 0.01, "Loop :c should start at t=2.0 (after sleep 4)");
    }

    #[test]
    fn test_define_blocks_and_function_calls() {
        let code = r#"
use_bpm 120
use_synth :dsaw

define :guitar_riff do
  with_fx :distortion, distort: 0.8 do
    play_pattern_timed [:E2, :G2, :A2], [0.5, 0.5, 0.25], release: 0.3
  end
end

define :dark_drums do
  live_loop :drums do
    sample :bd_haus, amp: 3
    sleep 0.5
    sample :sn_dolf, amp: 2
    sleep 0.5
    stop
  end
end

# Call the function
guitar_riff

# Start drums via function
dark_drums

# Call in times loop
2.times do
  guitar_riff
end
"#;
        let parsed = parse_code(code).unwrap();

        // Check that we got PlayNote commands from guitar_riff expansion
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Note commands from define expansion: {}", note_count);
        // guitar_riff called once directly + 2 times in loop = 3 calls
        // Each call has 3 notes = 9 total
        assert_eq!(note_count, 9, "Should have 9 notes (3 calls x 3 notes)");

        // Check that dark_drums produced sample commands (live_loop inside define)
        let sample_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();
        eprintln!("Sample commands from define expansion: {}", sample_count);
        assert!(sample_count >= 2, "Should have at least 2 samples from dark_drums");
    }

    #[test]
    fn test_trailing_if_one_in() {
        // Test that trailing "if one_in(1)" always includes the command
        // (one_in(1) = always true)
        let code = r#"
sample :bd_haus, amp: 2 if one_in(1)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        let has_sample = parsed.iter().any(|c| matches!(c, ParsedCommand::PlaySample { .. }));
        assert!(has_sample, "one_in(1) should always include the sample");
    }

    #[test]
    fn test_if_block() {
        // Test if block with always-true condition
        let code = r#"
if true do
  sample :bd_haus, amp: 2
  sleep 1
end
"#;
        let parsed = parse_code(code).unwrap();
        let has_sample = parsed.iter().any(|c| {
            match c {
                ParsedCommand::TimesLoop { commands, .. } => {
                    commands.iter().any(|c| matches!(c, ParsedCommand::PlaySample { .. }))
                }
                _ => false,
            }
        });
        assert!(has_sample, "if true should include the sample");
    }

    #[test]
    fn test_ring_and_spread() {
        let code = r#"
kick_pat = ring(1, 0, 0, 0, 0, 1, 0, 0)
snare_pat = spread(3, 8)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        // Should parse without errors
        assert!(!parsed.is_empty(), "Should have parsed commands");
    }

    #[test]
    fn test_rrand_in_params() {
        let code = r#"
play :c4, amp: rrand(0.5, 1.0)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        let has_note = parsed.iter().any(|c| {
            if let ParsedCommand::PlayNote { amplitude, .. } = c {
                *amplitude >= 0.5 && *amplitude <= 1.0
            } else {
                false
            }
        });
        assert!(has_note, "Should have a note with amplitude in rrand range");
    }

    #[test]
    fn test_scale_function() {
        let code = r#"
notes = scale(:c4, :minor_pentatonic)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        assert!(!parsed.is_empty(), "Should parse scale assignment");
    }

    #[test]
    fn test_chord_standalone() {
        let code = r#"
notes = chord(:e3, :minor7)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        assert!(!parsed.is_empty(), "Should parse chord assignment");
    }

    #[test]
    fn test_knit_function() {
        let code = r#"
pattern = knit(:e3, 3, :c3, 1)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        assert!(!parsed.is_empty(), "Should parse knit assignment");
    }

    #[test]
    fn test_range_function() {
        let code = r#"
values = range(0, 10, 2)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        assert!(!parsed.is_empty(), "Should parse range assignment");
    }

    #[test]
    fn test_inline_array_assignment() {
        let code = r#"
notes = [:c4, :e4, :g4]
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        assert!(!parsed.is_empty(), "Should parse inline array assignment");
    }

    #[test]
    fn test_elsif_else_blocks() {
        let code = r#"
if true do
  play :c4
  sleep 1
end
"#;
        let parsed = parse_code(code).unwrap();
        let has_note = parsed.iter().any(|c| {
            match c {
                ParsedCommand::TimesLoop { commands, .. } => {
                    commands.iter().any(|c| matches!(c, ParsedCommand::PlayNote { .. }))
                }
                _ => false,
            }
        });
        assert!(has_note, "if true should include the note");

        let code2 = r#"
if false do
  play :c4
  sleep 1
else
  play :e4
  sleep 1
end
"#;
        let parsed2 = parse_code(code2).unwrap();
        let has_note2 = parsed2.iter().any(|c| {
            match c {
                ParsedCommand::TimesLoop { commands, .. } => {
                    commands.iter().any(|c| matches!(c, ParsedCommand::PlayNote { .. }))
                }
                _ => false,
            }
        });
        assert!(has_note2, "if false with else should include the else branch note");
    }

    #[test]
    fn test_unless_block() {
        let code = r#"
unless false do
  sample :bd_haus, amp: 2
  sleep 1
end
"#;
        let parsed = parse_code(code).unwrap();
        let has_sample = parsed.iter().any(|c| {
            match c {
                ParsedCommand::TimesLoop { commands, .. } => {
                    commands.iter().any(|c| matches!(c, ParsedCommand::PlaySample { .. }))
                }
                _ => false,
            }
        });
        assert!(has_sample, "unless false should include the sample");
    }

    #[test]
    fn test_trailing_unless() {
        let code = r#"
sample :bd_haus, amp: 2 unless false
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        let has_sample = parsed.iter().any(|c| matches!(c, ParsedCommand::PlaySample { .. }));
        assert!(has_sample, "trailing unless false should include the sample");
    }

    #[test]
    fn test_use_synth_defaults() {
        let code = r#"
use_synth_defaults amp: 0.3, release: 2.0
play :c4
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        let note = parsed.iter().find_map(|c| {
            if let ParsedCommand::PlayNote { amplitude, envelope, .. } = c {
                Some((*amplitude, envelope.release))
            } else {
                None
            }
        });
        assert!(note.is_some(), "Should have a note");
        let (amp, release) = note.unwrap();
        assert!((amp - 0.3).abs() < 0.01, "Amplitude should be 0.3 from defaults, got {}", amp);
        assert!((release - 2.0).abs() < 0.01, "Release should be 2.0 from defaults, got {}", release);
    }

    #[test]
    fn test_with_synth_block() {
        let code = r#"
use_synth :sine
with_synth :saw do
  play :c4
  sleep 1
end
play :e4
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        // The first note (inside with_synth) should use Saw
        // The second note (outside) should use Sine
        fn find_synth_types(cmds: &[ParsedCommand]) -> Vec<OscillatorType> {
            let mut result = Vec::new();
            for cmd in cmds {
                match cmd {
                    ParsedCommand::PlayNote { synth_type, .. } => {
                        result.push(*synth_type);
                    }
                    ParsedCommand::TimesLoop { commands, .. } => {
                        result.extend(find_synth_types(commands));
                    }
                    _ => {}
                }
            }
            result
        }
        let synths = find_synth_types(&parsed);
        assert_eq!(synths.len(), 2, "Should have 2 notes");
        assert!(matches!(synths[0], OscillatorType::Saw), "First note should be Saw");
        assert!(matches!(synths[1], OscillatorType::Sine), "Second note should be Sine");
    }

    #[test]
    fn test_with_bpm_block() {
        let code = r#"
with_bpm 90 do
  play :c4
  sleep 1
end
"#;
        let parsed = parse_code(code).unwrap();
        // Should contain a SetBpm command
        fn has_set_bpm(cmds: &[ParsedCommand]) -> bool {
            cmds.iter().any(|c| match c {
                ParsedCommand::SetBpm(_) => true,
                ParsedCommand::TimesLoop { commands, .. } => has_set_bpm(commands),
                _ => false,
            })
        }
        assert!(has_set_bpm(&parsed), "Should have a SetBpm command from with_bpm");
    }

    #[test]
    fn test_sample_rpitch() {
        let code = r#"
sample :bd_haus, rpitch: 12, amp: 1.0
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        let rate = parsed.iter().find_map(|c| {
            if let ParsedCommand::PlaySample { rate, .. } = c {
                Some(*rate)
            } else {
                None
            }
        });
        assert!(rate.is_some(), "Should have a sample");
        // rpitch: 12 means up one octave → rate should be ~2.0
        assert!((rate.unwrap() - 2.0).abs() < 0.1, "rpitch 12 should set rate to ~2.0, got {}", rate.unwrap());
    }

    #[test]
    fn test_scale_intervals() {
        // Verify scale generation creates correct number of notes
        let intervals = scale_intervals("minor_pentatonic");
        assert_eq!(intervals.len(), 5, "Minor pentatonic should have 5 intervals");

        let intervals = scale_intervals("chromatic");
        assert_eq!(intervals.len(), 12, "Chromatic should have 12 intervals");
    }

    #[test]
    fn test_euclidean_knit_range() {
        let knitted = eval_knit(":e3, 3, :c3, 1");
        assert_eq!(knitted, vec![":e3", ":e3", ":e3", ":c3"]);

        let ranged = eval_range("0, 10, 2");
        assert_eq!(ranged.len(), 5); // 0, 2, 4, 6, 8

        let lined = eval_line("0, 1, steps: 5");
        assert_eq!(lined.len(), 5);
    }

    #[test]
    fn test_comprehensive_sonic_pi_code() {
        // Test a comprehensive Sonic Pi code sample using many features
        let code = r#"
use_bpm 120
use_synth :dsaw
use_synth_defaults release: 0.3, amp: 0.8

notes = scale(:c4, :minor_pentatonic)
chords = chord(:e3, :minor7)
pattern = knit(:e3, 3, :c3, 1)
beats = [:c4, :e4, :g4]
kick_pat = ring(1, 0, 0, 0, 0, 1, 0, 0)
snare_pat = spread(3, 8)

define :main_riff do
  with_fx :reverb, mix: 0.3 do
    play :c4, release: 0.2
    sleep 0.5
    play :e4, release: 0.2
    sleep 0.5
  end
end

live_loop :drums do
  sample :bd_haus, amp: 2
  sleep 0.5
  sample :sn_dub, amp: 1.5 if one_in(3)
  sleep 0.5
  stop
end

in_thread do
  main_riff
end

4.times do
  main_riff
end

if true do
  sample :perc_snap
  sleep 0.25
end

unless false do
  sample :elec_blip
  sleep 0.25
end

with_synth :fm do
  play :g4, release: 0.1
  sleep 0.5
end
"#;
        let parsed = parse_code(code).unwrap();
        assert!(!parsed.is_empty(), "Should parse comprehensive code without errors");

        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        let sample_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();
        eprintln!("Comprehensive test: {} notes, {} samples", note_count, sample_count);
        assert!(note_count > 0, "Should have notes");
        assert!(sample_count > 0, "Should have samples");
    }

    #[test]
    fn test_def_blocks_and_line_continuation() {
        // Test Ruby-style def blocks, Time.now.to_f, function calls with ?,
        // and multi-line continuation
        let code = r#"
use_bpm 135

stop_time = 220
start_time = Time.now.to_f

def should_stop?(start_time, stop_time)
  Time.now.to_f - start_time > stop_time
end

live_loop :intro_riff do
  stop if should_stop?(start_time, stop_time)
  use_synth :dark_ambience
  with_fx :reverb, room: 0.7 do
    with_fx :slicer, phase: 0.25 do
      play_pattern_timed [:c3, :e3, :g3, :b3, :g3, :e3, :c3], [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 1],
        release: 0.5, cutoff: 90, amp: 3
    end
  end
end
"#;
        let parsed = parse_code(code).unwrap();
        eprintln!("Parsed commands: {:#?}", parsed);
        assert!(!parsed.is_empty(), "Should parse the code without errors");

        let timed = commands_to_audio(&parsed, 135.0);
        let note_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Def block test: {} notes from timed commands", note_count);
        assert!(note_count > 0, "Should produce notes from play_pattern_timed inside live_loop");

        // Verify that the amplitude is 3.0 (from amp: 3)
        let first_note = timed.iter()
            .find_map(|(_, c)| if let AudioCommand::PlayNote { amplitude, .. } = c { Some(*amplitude) } else { None });
        assert!(first_note.is_some(), "Should have a note");
        assert!((first_note.unwrap() - 3.0).abs() < 0.01, "Amplitude should be 3.0, got {}", first_note.unwrap());
    }

    #[test]
    fn test_line_continuation_comma() {
        // Verify that lines ending with comma are joined
        let code = "play_pattern_timed [:c4, :e4, :g4], [0.5, 0.5, 0.5],\n  release: 0.5, amp: 2\nsleep 1\n";
        let preprocessed = join_continuation_lines(code);
        eprintln!("Preprocessed:\n{}", preprocessed);
        assert!(!preprocessed.contains("\n  release:"), "Continuation line should be joined");

        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        assert_eq!(note_count, 3, "Should have 3 notes from play_pattern_timed");

        // Verify amplitude
        let amp = timed.iter()
            .find_map(|(_, c)| if let AudioCommand::PlayNote { amplitude, .. } = c { Some(*amplitude) } else { None });
        assert!((amp.unwrap() - 2.0).abs() < 0.01, "Amplitude should be 2.0 from joined line");
    }

    #[test]
    fn test_def_function_stored_and_called() {
        let code = r#"
def my_riff()
  play :c4
  sleep 0.5
  play :e4
  sleep 0.5
end

my_riff
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed.iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        assert_eq!(note_count, 2, "Should have 2 notes from def function call");
    }
}
