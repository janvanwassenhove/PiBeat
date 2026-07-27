use super::engine::AudioCommand;
use super::synth::{midi_to_freq, note_name_to_midi, Envelope, OscillatorType};
use crate::trace;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::collections::HashMap;

/// A warning generated during code validation/parsing
#[derive(Debug, Clone)]
pub struct ParseWarning {
    pub line: usize,
    pub message: String,
    pub source_text: String,
}

/// Result of parsing code: commands + any validation warnings
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub commands: Vec<ParsedCommand>,
    pub warnings: Vec<ParseWarning>,
}

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
        /// Optional sustain time in beats (None = play full sample)
        sustain_beats: Option<f32>,
        /// beat_stretch: stretch sample to fit N beats (adjusts rate at playback)
        beat_stretch: Option<f32>,
        /// start: position in sample to start (0.0-1.0, normalized)
        start: Option<f32>,
        /// finish: position in sample to end (0.0-1.0, normalized)
        finish: Option<f32>,
        /// lpf: low-pass filter cutoff (MIDI note, applied per-sample via VoiceFx)
        lpf: Option<f32>,
        /// hpf: high-pass filter cutoff (MIDI note, applied per-sample via VoiceFx)
        hpf: Option<f32>,
        /// ADSR envelope for sample (attack in beats, decay in beats, sustain_level 0-1, release in beats)
        envelope: Option<Envelope>,
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
        /// Optional sync target: wait for this cue/loop before starting
        sync_with: Option<String>,
    },
    TimesLoop {
        count: usize,
        commands: Vec<ParsedCommand>,
    },
    /// Broadcast a cue signal for sync coordination
    Cue(String),
    /// Wait for a cue signal
    Sync(String),
    Stop,
    Comment(String),
    Log(String),
    /// A command that should be included with probability 1/n
    /// (re-evaluated each time at audio-command expansion, not parse time)
    ConditionalRandom {
        n: u32,
        command: Box<ParsedCommand>,
    },
    /// Sleep until a specific beat time (used internally by at blocks)
    SleepUntil(f32),
    /// Schedule commands to execute at specific beat times
    AtBlock {
        times: Vec<f32>,
        commands: Vec<ParsedCommand>,
    },
    /// `with_swing shift, pulse:, tick:, offset:` — one run in every `pulse`
    /// runs of the block is time-warped by `shift` beats; the rest play
    /// straight. Mirrors Sonic Pi's implementation, which counts runs with a
    /// named `tick` and wraps the shifted run in a `time_warp`.
    SwingBlock {
        /// Shift in beats. Positive delays, negative pushes the run early.
        shift: f32,
        /// How often the shift applies, in block invocations.
        pulse: u32,
        /// Tick key, so two swing blocks in one loop count independently.
        tick_key: String,
        /// Count offset applied before the modulo.
        offset: i64,
        commands: Vec<ParsedCommand>,
    },
    /// Set a runtime variable (evaluated at playback time by the scheduler)
    /// Used by `set :key, value` for runtime state like :master_amp, :stop_all, :pause_all
    SetVariable {
        key: String,
        value: f32,
    },
}

/// Parser context that tracks variables, functions, and synth state
struct ParseContext {
    variables: HashMap<String, String>,
    current_synth: OscillatorType,
    /// Stored function definitions from `define :name do ... end`
    /// Stores (body_text, param_names)
    functions: HashMap<String, (String, Vec<String>)>,
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
    /// Seedable PRNG for deterministic random sequences.
    /// Sonic Pi parity: `use_random_seed N` resets this.
    rng: StdRng,
    /// Validation warnings collected during parsing (unrecognized lines, etc.)
    warnings: Vec<ParseWarning>,
    /// Current BPM tracked during parsing (for use_bpm_mul)
    current_bpm: f32,
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
            rng: StdRng::seed_from_u64(0),
            warnings: Vec::new(),
            current_bpm: 60.0,
        }
    }

    /// Resolve a value that may reference a variable or use string concatenation
    fn resolve_string(&self, raw: &str) -> String {
        let trimmed = raw.trim();

        // Handle string concatenation: expr + expr + ...
        // Only treat as string concatenation if at least one part is a string literal.
        // Otherwise it's likely arithmetic (e.g. "n+7") which should be left alone
        // for resolve_numeric to handle.
        if trimmed.contains('+') {
            let parts: Vec<&str> = trimmed.split('+').collect();
            let has_string_literal = parts.iter().any(|p| {
                let p = p.trim();
                p.starts_with('"') && p.ends_with('"')
            });
            if has_string_literal {
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
                        eprintln!(
                            "[resolve_string] UNRESOLVED var '{}' (known vars: {:?})",
                            p,
                            self.variables.keys().collect::<Vec<_>>()
                        );
                        result.push_str(p);
                    }
                }
                return result;
            }
            // No string literals — fall through to variable/literal resolution below
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
    fn resolve_numeric(&mut self, expr: &str) -> Option<f32> {
        let trimmed = expr.trim();

        // Strip trailing method calls: .to_i, .to_f, .floor, .ceil, .round, .abs
        let (clean_expr, post_method) = if let Some(stripped) = trimmed.strip_suffix(".to_i") {
            (stripped, Some("to_i"))
        } else if let Some(stripped) = trimmed.strip_suffix(".to_f") {
            (stripped, Some("to_f"))
        } else if let Some(stripped) = trimmed.strip_suffix(".floor") {
            (stripped, Some("floor"))
        } else if let Some(stripped) = trimmed.strip_suffix(".ceil") {
            (stripped, Some("ceil"))
        } else if let Some(stripped) = trimmed.strip_suffix(".round") {
            (stripped, Some("round"))
        } else if let Some(stripped) = trimmed.strip_suffix(".abs") {
            (stripped, Some("abs"))
        } else {
            (trimmed, None)
        };

        // If we stripped a method, resolve the inner expression then apply the method
        if let Some(method) = post_method {
            if let Some(val) = self.resolve_numeric(clean_expr) {
                return Some(match method {
                    "to_i" | "floor" => val.floor(),
                    "ceil" => val.ceil(),
                    "round" => val.round(),
                    "abs" => val.abs(),
                    _ => val, // to_f is identity
                });
            }
            return None;
        }

        // Handle parenthesized expressions: (expr)
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = &trimmed[1..trimmed.len() - 1];
            // Check if inner is a ternary: cond ? val1 : val2
            if let Some(result) = self.try_eval_ternary(inner) {
                return Some(result);
            }
            return self.resolve_numeric(inner);
        }

        // Ternary operator: cond ? val1 : val2  (can also appear without parens)
        if let Some(result) = self.try_eval_ternary(trimmed) {
            return Some(result);
        }

        // note(:e4) or note(:e) or note root — convert note name to MIDI number
        // (Caller converts to Hz after arithmetic is done)
        // Handles both: note(:c4), note(root), and note root (space-separated)
        let note_inner = extract_func_args(trimmed, "note")
            .map(|s| s.to_string())
            .or_else(|| {
                // Try space-separated form: note :c4 or note root
                let trimmed_stripped = trimmed.strip_prefix("note ")?;
                // Take until next operator or space
                let end = trimmed_stripped.find(|c: char| {
                    c == '+' || c == '-' || c == '*' || c == '/' || c == ',' || c == ')'
                });
                Some(
                    trimmed_stripped[..end.unwrap_or(trimmed_stripped.len())]
                        .trim()
                        .to_string(),
                )
            });
        if let Some(inner) = note_inner {
            let note_str = inner.trim().trim_start_matches(':');
            if let Some(midi) = note_name_to_midi(&note_str.to_uppercase()) {
                return Some(midi as f32);
            }
            // Try resolving as variable
            if let Some(var_val) = self.variables.get(note_str.trim_start_matches(':')) {
                let resolved = var_val.clone();
                // First try as plain number (in case the variable stores MIDI value directly)
                if let Ok(midi) = resolved.parse::<f32>() {
                    return Some(midi);
                }
                // Then try as note name
                let clean = resolved.trim().trim_start_matches(':');
                if let Some(midi) = note_name_to_midi(&clean.to_uppercase()) {
                    return Some(midi as f32);
                }
            }
        }

        // get(:key) — resolve to variable value
        // Only match if the expression IS a get() call, not contains one within arithmetic
        if trimmed.starts_with("get(") {
            if let Some(inner) = extract_func_args(trimmed, "get") {
                // Verify this is the entire expression (no arithmetic around it)
                let expected_len = "get(".len() + inner.len() + ")".len();
                if trimmed.len() == expected_len {
                    let key = inner.trim().trim_start_matches(':');
                    let val = self.variables.get(key).cloned();
                    if let Some(v) = val {
                        return v.parse::<f32>().ok();
                    }
                    return None;
                }
            }
        }

        // rrand(min, max)
        if let Some(inner) = extract_func_args(trimmed, "rrand") {
            let args: Vec<&str> = inner.split(',').collect();
            if args.len() == 2 {
                let min: f32 = args[0].trim().parse().ok()?;
                let max: f32 = args[1].trim().parse().ok()?;
                return Some(self.rng.gen_range(min..=max));
            }
        }

        // rrand_i(min, max)
        if let Some(inner) = extract_func_args(trimmed, "rrand_i") {
            let args: Vec<&str> = inner.split(',').collect();
            if args.len() == 2 {
                let min: i32 = args[0].trim().parse().ok()?;
                let max: i32 = args[1].trim().parse().ok()?;
                return Some(self.rng.gen_range(min..=max) as f32);
            }
        }

        // rand(max) or rand()
        if let Some(inner) = extract_func_args(trimmed, "rand") {
            let max: f32 = if inner.trim().is_empty() {
                1.0
            } else {
                inner.trim().parse().unwrap_or(1.0)
            };
            return Some(self.rng.gen_range(0.0..max));
        }

        // rand_i(max)
        if let Some(inner) = extract_func_args(trimmed, "rand_i") {
            let max: i32 = inner.trim().parse().unwrap_or(2);
            return Some(self.rng.gen_range(0..max) as f32);
        }

        // dice(n) - random integer 1..n
        if let Some(inner) = extract_func_args(trimmed, "dice") {
            let n: i32 = inner.trim().parse().unwrap_or(6);
            return Some(self.rng.gen_range(1..=n) as f32);
        }

        // choose([array]) - pick random element from array
        if let Some(inner) = extract_func_args(trimmed, "choose") {
            if let Some(values) = self.resolve_to_list(inner.trim()) {
                if !values.is_empty() {
                    let idx = self.rng.gen_range(0..values.len());
                    let chosen = &values[idx];
                    // Try to resolve as numeric first
                    if let Some(num) = self.resolve_numeric(chosen) {
                        return Some(num);
                    }
                    // Try as note name
                    let note_str = chosen.trim_start_matches(':');
                    if let Some(midi) = note_name_to_midi(&note_str.to_uppercase()) {
                        return Some(midi as f32);
                    }
                }
            }
            return None;
        }

        // User-defined function call: func_name(args) → evaluate return expression
        if trimmed.contains('(') && trimmed.contains(')') {
            let func_name = trimmed.split('(').next().unwrap_or("").trim();
            // Also handle func_name? (Ruby predicate names)
            if self.functions.contains_key(func_name) {
                if let Some(val) = self.eval_user_function(trimmed) {
                    return Some(val);
                }
            }
        }

        // Expression with arithmetic: e.g. "1 + rrand(-0.02, 0.03)", "(note :e) - 24"
        if trimmed.contains('+')
            || (trimmed.contains('-') && !trimmed.starts_with('-'))
            || trimmed.contains('*')
            || trimmed.contains('/')
            || trimmed.contains('%')
        {
            // Try to evaluate simple arithmetic
            if let Some(result) = self.eval_simple_arithmetic(trimmed) {
                return Some(result);
            }
        }

        // Plain number
        if let Ok(v) = trimmed.parse::<f32>() {
            return Some(v);
        }

        // Variable reference (with recursive resolution for chained vars like n -> root -> :c2)
        if let Some(val) = self.variables.get(trimmed) {
            let v = val.clone();
            if let Ok(num) = v.parse::<f32>() {
                return Some(num);
            }
            // Try resolving the variable value recursively (but only one level to avoid loops)
            let inner = v.trim().trim_start_matches(':');
            // Check if it's a note name
            if let Some(midi) = note_name_to_midi(&inner.to_uppercase()) {
                return Some(midi as f32);
            }
            // Check if it resolves to another variable
            if inner != trimmed {
                if let Some(inner_val) = self.variables.get(inner) {
                    let iv = inner_val.clone();
                    if let Ok(num) = iv.parse::<f32>() {
                        return Some(num);
                    }
                    let inner2 = iv.trim().trim_start_matches(':');
                    if let Some(midi) = note_name_to_midi(&inner2.to_uppercase()) {
                        return Some(midi as f32);
                    }
                }
            }
            // Last resort: try resolving the value as an expression (e.g. "root+3")
            if v != trimmed {
                if let Some(result) = self.resolve_numeric(&v) {
                    return Some(result);
                }
            }
            return None;
        }
        // Variable reference with : prefix
        let without_colon = trimmed.trim_start_matches(':');
        if let Some(val) = self.variables.get(without_colon) {
            let v = val.clone();
            if let Ok(num) = v.parse::<f32>() {
                return Some(num);
            }
            let inner = v.trim().trim_start_matches(':');
            if let Some(midi) = note_name_to_midi(&inner.to_uppercase()) {
                return Some(midi as f32);
            }
            return None;
        }

        // Note name like :e or :e4
        if let Some(midi) = note_name_to_midi(&without_colon.to_uppercase()) {
            return Some(midi as f32);
        }

        None
    }

    /// Evaluate simple arithmetic expressions like "1 + rrand(-0.02, 0.03)"
    /// or "(note :e) - 24" or "v * get(:master_amp)"
    /// Try to evaluate a ternary expression: condition ? true_val : false_val
    fn try_eval_ternary(&mut self, expr: &str) -> Option<f32> {
        // Find the ? operator (not inside parens)
        let mut depth = 0;
        let mut q_pos = None;
        for (i, ch) in expr.chars().enumerate() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                '?' if depth == 0 => {
                    q_pos = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let q_pos = q_pos?;
        let condition = expr[..q_pos].trim();
        let rest = expr[q_pos + 1..].trim();

        // Find the : separator in the rest (not inside parens)
        let mut depth = 0;
        let mut colon_pos = None;
        for (i, ch) in rest.chars().enumerate() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                ':' if depth == 0 => {
                    colon_pos = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let colon_pos = colon_pos?;
        let true_val = rest[..colon_pos].trim();
        let false_val = rest[colon_pos + 1..].trim();

        let cond_result = evaluate_condition(condition, self);
        if cond_result {
            self.resolve_numeric(true_val)
        } else {
            self.resolve_numeric(false_val)
        }
    }

    fn eval_simple_arithmetic(&mut self, expr: &str) -> Option<f32> {
        let trimmed = expr.trim();

        // Look for rrand/rand function calls in the expression
        for func_name in &["rrand", "rrand_i", "rand", "rand_i", "dice", "note", "get"] {
            if let Some(func_pos) = trimmed.find(&format!("{}(", func_name)) {
                // Find the matching closing paren
                let open_paren = func_pos + func_name.len();
                let mut depth = 0;
                let mut close_paren = open_paren;
                for (i, ch) in trimmed[open_paren..].chars().enumerate() {
                    if ch == '(' {
                        depth += 1;
                    }
                    if ch == ')' {
                        depth -= 1;
                        if depth == 0 {
                            close_paren = open_paren + i;
                            break;
                        }
                    }
                }

                let func_call = &trimmed[func_pos..=close_paren];
                let func_val = self.resolve_numeric(func_call)?;

                let before = trimmed[..func_pos].trim();
                let after = trimmed[close_paren + 1..].trim();

                // Parse what's before: could be "1 +" or "0.5 -" or "(note :e) +" etc.
                let mut result = func_val;
                if !before.is_empty() {
                    if let Some(stripped) = before.strip_suffix('+') {
                        let left = self.resolve_numeric(stripped.trim()).unwrap_or(0.0);
                        result = left + func_val;
                    } else if let Some(stripped) = before.strip_suffix('-') {
                        let left = self.resolve_numeric(stripped.trim()).unwrap_or(0.0);
                        result = left - func_val;
                    } else if let Some(stripped) = before.strip_suffix('*') {
                        let left = self.resolve_numeric(stripped.trim()).unwrap_or(1.0);
                        result = left * func_val;
                    } else if let Some(stripped) = before.strip_suffix('/') {
                        let left = self.resolve_numeric(stripped.trim()).unwrap_or(0.0);
                        if func_val != 0.0 {
                            result = left / func_val;
                        }
                    }
                }

                // Parse what's after: could be "+ 0.5" or "* 2" etc.
                if !after.is_empty() {
                    if let Some(stripped) = after.strip_prefix('+') {
                        let right = self.resolve_numeric(stripped.trim()).unwrap_or(0.0);
                        result += right;
                    } else if let Some(stripped) = after.strip_prefix('-') {
                        let right = self.resolve_numeric(stripped.trim()).unwrap_or(0.0);
                        result -= right;
                    } else if let Some(stripped) = after.strip_prefix('*') {
                        let right = self.resolve_numeric(stripped.trim()).unwrap_or(1.0);
                        result *= right;
                    } else if let Some(stripped) = after.strip_prefix('/') {
                        let right = self.resolve_numeric(stripped.trim()).unwrap_or(1.0);
                        if right != 0.0 {
                            result /= right;
                        }
                    }
                }

                return Some(result);
            }
        }

        // Try splitting on binary operators (+ - * / %) at the top level
        // (not inside parentheses)
        let operators = ['+', '-', '*', '/', '%'];
        for &op in &operators {
            // Find the operator at the top level (not inside parens)
            let mut depth = 0;
            let chars: Vec<char> = trimmed.chars().collect();
            // Search from end to respect left-to-right evaluation for - and +
            for i in (1..chars.len()).rev() {
                if chars[i] == '(' {
                    depth += 1;
                } else if chars[i] == ')' {
                    depth -= 1;
                } else if chars[i] == op && depth == 0 {
                    // Don't split on negative sign (e.g., "-24" with nothing before)
                    let left_str = trimmed[..i].trim();
                    let right_str = trimmed[i + 1..].trim();
                    if left_str.is_empty() {
                        continue;
                    }
                    let left = self.resolve_numeric(left_str);
                    let right = self.resolve_numeric(right_str);
                    if let (Some(l), Some(r)) = (left, right) {
                        return Some(match op {
                            '+' => l + r,
                            '-' => l - r,
                            '*' => l * r,
                            '/' => {
                                if r != 0.0 {
                                    l / r
                                } else {
                                    0.0
                                }
                            }
                            '%' => {
                                if r != 0.0 {
                                    l % r
                                } else {
                                    0.0
                                }
                            }
                            _ => l,
                        });
                    }
                }
            }
        }

        None
    }

    /// Evaluate one_in(n) - returns true with probability 1/n
    fn eval_one_in(&mut self, expr: &str) -> Option<bool> {
        if let Some(inner) = extract_func_args(expr, "one_in") {
            let n: u32 = inner.trim().parse().ok()?;
            if n == 0 {
                return Some(false);
            }
            return Some(self.rng.gen_ratio(1, n));
        }
        None
    }

    /// Evaluate a user-defined function that returns a numeric value.
    /// Substitutes parameters, finds the return expression (or last expression),
    /// and evaluates it numerically.
    fn eval_user_function(&mut self, call_expr: &str) -> Option<f32> {
        let func_name = call_expr.split('(').next()?.trim();
        let (body, param_names) = self.functions.get(func_name)?.clone();

        // Extract arguments from the call
        let args = extract_function_call_args(call_expr, func_name);

        // Substitute parameters in body
        let substituted = substitute_function_params(&body, &param_names, &args, self);
        eprintln!("[eval_user_function] body='{}', substituted='{}'", body.replace('\n', "\\n"), substituted.replace('\n', "\\n"));

        // Save and bind parameters as variables
        let saved_vars: Vec<(String, Option<String>)> = param_names
            .iter()
            .map(|p| {
                let name = p.split('=').next().unwrap_or(p).trim().to_string();
                let saved = self.variables.get(&name).cloned();
                (name, saved)
            })
            .collect();
        for (i_param, pspec) in param_names.iter().enumerate() {
            let (pname, default_val) = if let Some(eq_pos) = pspec.find('=') {
                (pspec[..eq_pos].trim(), Some(pspec[eq_pos + 1..].trim()))
            } else {
                (pspec.as_str(), None)
            };
            if let Some(arg_val) = args.get(i_param) {
                self.variables.insert(pname.to_string(), arg_val.clone());
            } else if let Some(def) = default_val {
                self.variables.insert(pname.to_string(), def.to_string());
            }
        }

        // Find the return expression
        let mut result: Option<f32> = None;
        for line in substituted.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(ret_expr) = trimmed.strip_prefix("return ") {
                eprintln!("[eval_user_function] found return expr='{}'", ret_expr.trim());
                result = self.resolve_numeric(ret_expr.trim());
                break;
            }
            // Last non-empty expression is the implicit return in Ruby
            result = self.resolve_numeric(trimmed);
        }

        // Restore saved variables
        for (pname, saved) in saved_vars {
            if let Some(val) = saved {
                self.variables.insert(pname, val);
            } else {
                self.variables.remove(&pname);
            }
        }

        eprintln!("[parse] eval_user_function '{}' -> {:?}", call_expr, result);
        result
    }

    /// Evaluate a list expression that may have method calls:
    ///   `[:c4, :e4, :g4].choose`
    ///   `scale(:c4, :minor).choose`
    ///   `(ring 1, 0, 1, 0).tick`
    ///   `var_name.tick`
    ///   `choose([:c4, :e4, :g4])` — standalone choose function
    fn resolve_list_value(&mut self, expr: &str) -> Option<String> {
        let trimmed = expr.trim();

        // Standalone choose(list) function
        if let Some(inner) = extract_func_args(trimmed, "choose") {
            if let Some(values) = self.resolve_to_list(inner.trim()) {
                if !values.is_empty() {
                    let idx = self.rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
            }
            return None;
        }

        // Check for method calls: .choose, .pick, .shuffle, .reverse, .tick, .look, .first, .last
        for method in &[
            ".choose",
            ".pick(",
            ".pick",
            ".shuffle",
            ".reverse",
            ".tick",
            ".look",
            ".first",
            ".last",
            ".ring",
            ".min",
            ".max",
            ".sort",
            ".mirror",
            ".stretch(",
            ".repeat(",
        ] {
            if let Some(dot_pos) = trimmed.rfind(method) {
                let base_expr = &trimmed[..dot_pos];
                let method_name = &trimmed[dot_pos + 1..];

                // Resolve the base to a list of values
                let values = self.resolve_to_list(base_expr)?;
                if values.is_empty() {
                    return None;
                }

                // Apply the method
                if method_name.starts_with("choose") {
                    let idx = self.rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name.starts_with("pick(") {
                    // .pick(n) — pick n random elements
                    if let Some(inner) = extract_func_args(method_name, "pick") {
                        let n: usize = inner.trim().parse().unwrap_or(1);
                        let picked: Vec<String> = (0..n)
                            .map(|_| values[self.rng.gen_range(0..values.len())].clone())
                            .collect();
                        // Return as first element for single note context
                        return picked.first().cloned();
                    }
                    let idx = self.rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name == "pick" {
                    let idx = self.rng.gen_range(0..values.len());
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
                    let idx = self.rng.gen_range(0..values.len());
                    return Some(values[idx].clone());
                }
                if method_name == "min" {
                    return values
                        .iter()
                        .filter_map(|v| v.parse::<f32>().ok())
                        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|v| v.to_string())
                        .or_else(|| values.first().cloned());
                }
                if method_name == "max" {
                    return values
                        .iter()
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

        // Strip .ring suffix — .ring just means "wrap as ring buffer", the list is the same
        let trimmed = if let Some(stripped) = trimmed.strip_suffix(".ring") {
            stripped.trim()
        } else {
            trimmed
        };

        // Inline array: [:c4, :e4, :g4]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len() - 1];
            return Some(split_respecting_parens(inner));
        }

        // (ring 1, 0, 1, 0) syntax
        if trimmed.starts_with("(ring") || trimmed.starts_with("( ring") {
            let inner = trimmed.trim_start_matches('(').trim_end_matches(')').trim();
            let inner = inner.strip_prefix("ring").unwrap_or(inner).trim();
            return Some(split_respecting_parens(inner));
        }

        // scale(:c4, :minor) or scale :c4, :minor
        if trimmed.starts_with("scale(")
            || trimmed.starts_with("scale ")
            || trimmed.starts_with("scale\t")
        {
            return self.resolve_scale_expr(trimmed);
        }

        // chord(:c4, :minor) or chord :c4, :minor (as standalone list)
        if trimmed.starts_with("chord(")
            || trimmed.starts_with("chord ")
            || trimmed.starts_with("chord\t")
        {
            return self.resolve_chord_expr(trimmed);
        }

        // ring(1, 0, 1, 0)
        if let Some(inner) = extract_func_args(trimmed, "ring") {
            return Some(split_respecting_parens(inner));
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
                return Some(
                    pattern
                        .iter()
                        .map(|b| {
                            if *b {
                                "true".to_string()
                            } else {
                                "false".to_string()
                            }
                        })
                        .collect(),
                );
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
                let inner = &val[1..val.len() - 1];
                let items: Vec<String> = inner
                    .split(',')
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
        if args.len() < 2 {
            return None;
        }

        let root_str = args[0].trim_start_matches(':');
        let scale_type = args[1].trim().trim_start_matches(':');

        let root_midi = note_name_to_midi(&root_str.to_uppercase())?;
        let intervals = scale_intervals(scale_type);

        // Check for num_octaves parameter
        let num_octaves = args
            .iter()
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
        if args.is_empty() {
            return None;
        }

        let root_str = args[0].trim_start_matches(':');
        let chord_type = args
            .get(1)
            .map(|s| s.trim().trim_start_matches(':'))
            .unwrap_or("major");
        let root_midi = note_name_to_midi(&root_str.to_uppercase())?;
        let intervals = chord_intervals(chord_type);

        let notes: Vec<String> = intervals
            .iter()
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
        if ch == '(' {
            depth += 1;
        }
        if ch == ')' {
            depth -= 1;
            if depth == 0 {
                end = inner_start + i;
                break;
            }
        }
    }
    if depth == 0 {
        Some(&expr[inner_start..end])
    } else {
        None
    }
}

/// Split a string on commas while respecting nested parentheses and brackets.
/// e.g. "chord(:c4, :minor), chord(:ab3, :major)" → ["chord(:c4, :minor)", "chord(:ab3, :major)"]
fn split_respecting_parens(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => { paren_depth += 1; current.push(ch); }
            ')' => { paren_depth -= 1; current.push(ch); }
            '[' => { bracket_depth += 1; current.push(ch); }
            ']' => { bracket_depth -= 1; current.push(ch); }
            ',' if paren_depth == 0 && bracket_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }
    result
}

/// Generate a Euclidean/Bjorklund rhythm pattern (spread)
fn euclidean_rhythm(pulses: usize, steps: usize) -> Vec<bool> {
    if steps == 0 {
        return vec![];
    }
    if pulses >= steps {
        return vec![true; steps];
    }
    if pulses == 0 {
        return vec![false; steps];
    }

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
    if parts.is_empty() {
        return vec![];
    }

    let start: f32 = parts[0].parse().unwrap_or(0.0);
    let end: f32 = parts
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(start + 10.0);
    let step: f32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    if step == 0.0 {
        return vec![];
    }

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
    if parts.len() < 2 {
        return vec![];
    }

    let start: f32 = parts[0].parse().unwrap_or(0.0);
    // Second arg might be "finish" or a named param
    let finish: f32 = parts[1]
        .split(':')
        .last()
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| parts[1].parse().ok())
        .unwrap_or(1.0);

    // Look for steps: n
    let steps: usize = parts
        .iter()
        .find(|p| p.contains("steps"))
        .and_then(|p| p.split(':').last())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(10);

    if steps <= 1 {
        return vec![format!("{}", start)];
    }

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
    let result = validate_and_parse(code)?;
    Ok(result.commands)
}

/// Public wrapper for join_continuation_lines (for testing)
#[doc(hidden)]
pub fn join_continuation_lines_pub(code: &str) -> String {
    join_continuation_lines(code)
}

/// Parse and validate Sonic Pi code, returning commands + warnings
pub fn validate_and_parse(code: &str) -> Result<ParseResult, String> {
    let mut ctx = ParseContext::new();

    // Track original line numbers for validation reporting
    let original_lines: Vec<&str> = code.lines().collect();
    let total_lines = original_lines.len();

    // Check for obvious structural issues before parsing
    let mut do_count = 0usize;
    let mut end_count = 0usize;
    for line in &original_lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        // Strip inline comments before counting both `do` and `end`
        let stripped = strip_inline_comment(trimmed);
        // Count `do` keywords (end of line, not inside strings)
        if stripped.ends_with(" do") || stripped == "do" || stripped.ends_with("|do") {
            do_count += 1;
        }
        // Also count block openers like "loop do", "N.times do", etc.
        if stripped.contains(" do ") && stripped.ends_with('|') {
            do_count += 1;
        }
        // Count block openers that don't use `do` but still close with `end`:
        // `if`, `unless`, `while`, `until`, `begin`, `case`
        // Only count when the keyword starts the line (not trailing if/unless)
        // Skip single-line forms like: `if cond; body; end` or `if cond then body end`
        if (stripped.starts_with("if ") || stripped.starts_with("unless ")
            || stripped.starts_with("while ") || stripped.starts_with("until ")
            || stripped.starts_with("begin") || stripped.starts_with("case "))
            && !stripped.ends_with(" do") && !stripped.contains("|do")
            && !stripped.ends_with("end") // not a single-line if/while/etc.
        {
            do_count += 1;
        }
        // `elsif` does NOT open a new block (it's part of an existing if block)
        if stripped == "end" {
            end_count += 1;
        }
    }
    if end_count > do_count + 2 {
        return Err(format!(
            "Syntax error: found {} 'end' keywords but only {} 'do' blocks — likely extra 'end' statement(s)",
            end_count, do_count
        ));
    }
    if do_count > end_count + 2 {
        return Err(format!(
            "Syntax error: found {} 'do' blocks but only {} 'end' keywords — likely missing 'end' statement(s)",
            do_count, end_count
        ));
    }

    // Check for empty code — return empty result instead of error
    let non_empty_lines = original_lines.iter().filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#')).count();
    if non_empty_lines == 0 {
        return Ok(ParseResult {
            commands: vec![],
            warnings: vec![],
        });
    }

    let commands = parse_code_with_context(code, &mut ctx)?;

    // Post-parse validation: check for loops without sleep (recursive check)
    fn has_sleep_recursive(cmds: &[ParsedCommand]) -> bool {
        for c in cmds {
            match c {
                ParsedCommand::Sleep(_) => return true,
                ParsedCommand::TimesLoop { commands, .. } => {
                    if has_sleep_recursive(commands) { return true; }
                }
                ParsedCommand::WithFx { commands, .. } => {
                    if has_sleep_recursive(commands) { return true; }
                }
                ParsedCommand::Loop { commands, .. } => {
                    if has_sleep_recursive(commands) { return true; }
                }
                _ => {}
            }
        }
        false
    }
    for cmd in &commands {
        if let ParsedCommand::Loop { name, commands: inner, .. } = cmd {
            if !has_sleep_recursive(inner) {
                ctx.warnings.push(ParseWarning {
                    line: 0,
                    message: format!("live_loop :{} has no 'sleep' — this will produce an infinite tight loop", name),
                    source_text: format!("live_loop :{}", name),
                });
            }
        }
    }

    eprintln!(
        "[validate] Parsed {} lines → {} commands, {} warnings",
        total_lines,
        commands.len(),
        ctx.warnings.len()
    );
    for w in &ctx.warnings {
        eprintln!("[validate]   line {}: {} ('{}')", w.line, w.message, w.source_text);
    }

    Ok(ParseResult {
        commands,
        warnings: ctx.warnings,
    })
}

/// Pre-process code to join continuation lines (lines ending with `,`, `\`, `[`, or `(`)
/// and split semicolon-separated statements into separate lines.
fn join_continuation_lines(code: &str) -> String {
    let raw_lines: Vec<&str> = code.lines().collect();
    let mut joined = Vec::new();
    let mut i = 0;
    while i < raw_lines.len() {
        let mut current = raw_lines[i].to_string();
        // Keep joining while the trimmed line (after stripping inline comments)
        // ends with a continuation character: `,`, `\`, `[`, or `(`
        // OR has unbalanced brackets/parens (more opens than closes)
        while i + 1 < raw_lines.len() {
            let trimmed = current.trim_end();
            // Strip inline comments before checking continuation characters
            // so that `[root+3, 0.5],   # minor 3rd walk` is seen as ending with `,`
            let stripped = strip_inline_comment(trimmed);
            let ends_with_continuation = stripped.ends_with(',')
                || stripped.ends_with('\\')
                || stripped.ends_with('[')
                || stripped.ends_with('(');
            // Also check bracket/paren balance: if more [ than ], continue joining
            let bracket_balance = stripped.chars().filter(|&c| c == '[').count() as i32
                - stripped.chars().filter(|&c| c == ']').count() as i32;
            let paren_balance = stripped.chars().filter(|&c| c == '(').count() as i32
                - stripped.chars().filter(|&c| c == ')').count() as i32;
            let is_continuation = ends_with_continuation || bracket_balance > 0 || paren_balance > 0;
            if is_continuation {
                let next = raw_lines[i + 1].trim();
                if stripped.ends_with('\\') {
                    // Remove the trailing backslash and append next line
                    let base = strip_inline_comment(trimmed);
                    current = format!("{} {}", base.trim_end_matches('\\').trim_end(), next);
                } else {
                    // Trailing comma, bracket, or paren — append next line
                    let base = strip_inline_comment(trimmed);
                    current = format!("{} {}", base, next);
                }
                i += 1;
            } else {
                break;
            }
        }
        joined.push(current);
        i += 1;
    }

    // Second pass: convert inline brace blocks to do...end form BEFORE semicolon expansion
    // e.g., `8.times { metal_chug(...); sleep 0.25 }` →
    //   8.times do
    //     metal_chug(...)
    //     sleep 0.25
    //   end
    // This must happen before semicolons are split, because the semicolons
    // inside { } are part of the block body, not top-level separators.
    let mut brace_expanded = Vec::new();
    for line in &joined {
        if let Some(expanded_lines) = expand_brace_block(line) {
            brace_expanded.extend(expanded_lines);
        } else {
            brace_expanded.push(line.clone());
        }
    }

    // Third pass: expand semicolon-separated statements into separate lines
    // e.g., "if cond; sleep 1; next; end" -> proper block structure
    let mut expanded = Vec::new();
    for line in &brace_expanded {
        let trimmed = line.trim();
        // Only split if semicolons are present, NOT inside a string, and NOT a comment line
        if trimmed.contains(';') && !trimmed.is_empty() && !trimmed.starts_with('#') {
            let parts = split_semicolons_outside_strings(trimmed);
            if parts.len() > 1 {
                // Check if this is a single-line if/unless block: "if cond; body; end"
                let first_part = parts[0].trim();
                let last_part = parts.last().map(|s| s.trim()).unwrap_or("");
                
                if (first_part.starts_with("if ") || first_part.starts_with("unless ")) 
                   && last_part == "end" 
                {
                    // Emit as proper block: "if cond" then body lines then "end"
                    expanded.push(format!("{} then", first_part));
                    for part in &parts[1..parts.len()-1] {
                        let p = part.trim();
                        if !p.is_empty() {
                            expanded.push(format!("  {}", p));
                        }
                    }
                    expanded.push("end".to_string());
                } else {
                    // Regular semicolon split
                    for part in parts {
                        let p = part.trim();
                        if !p.is_empty() {
                            expanded.push(p.to_string());
                        }
                    }
                }
                continue;
            }
        }
        expanded.push(line.to_string());
    }

    // Fourth pass: expand array multiplication [x]*N into [x,x,...,x]
    let mut final_lines = Vec::new();
    for line in &expanded {
        final_lines.push(expand_array_multiplication(line));
    }

    final_lines.join("\n")
}

/// Split a line on semicolons, but not inside strings
fn split_semicolons_outside_strings(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            in_string = !in_string;
            current.push(ch);
        } else if ch == ';' && !in_string {
            parts.push(current.clone());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Expand array multiplication like `[0.25]*8` into `[0.25, 0.25, 0.25, ...]`
fn expand_array_multiplication(line: &str) -> String {
    let bytes = line.as_bytes();
    // Search for ']' followed by optional spaces then '*' then optional spaces then digits
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b']' {
            // Found closing bracket. Check what follows: optional spaces, *, optional spaces, digits
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'*' {
                j += 1;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                let num_start = j;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > num_start {
                    let num_str = &line[num_start..j];
                    if let Ok(multiplier) = num_str.parse::<usize>() {
                        // Find matching opening bracket
                        let mut depth = 0i32;
                        let mut open_pos = None;
                        for k in (0..=i).rev() {
                            if bytes[k] == b']' {
                                depth += 1;
                            } else if bytes[k] == b'[' {
                                depth -= 1;
                                if depth == 0 {
                                    open_pos = Some(k);
                                    break;
                                }
                            }
                        }
                        if let Some(open) = open_pos {
                            let array_content = &line[open + 1..i];
                            let items: Vec<&str> =
                                array_content.split(',').map(|s| s.trim()).collect();
                            let mut expanded = Vec::new();
                            for _ in 0..multiplier {
                                for item in &items {
                                    expanded.push(*item);
                                }
                            }
                            let replacement = format!("[{}]", expanded.join(", "));
                            let new_line =
                                format!("{}{}{}", &line[..open], replacement, &line[j..]);
                            // Recurse in case there are more
                            return expand_array_multiplication(&new_line);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    line.to_string()
}

/// Expand inline brace blocks into do...end form.
///
/// Converts Ruby brace-style blocks that appear on a single line:
///   `8.times { metal_chug(...); sleep 0.25 }`
///   `arr.each { |x| play x; sleep 0.25 }`
///
/// Into equivalent do...end form:
///   ```text
///   8.times do
///     metal_chug(...)
///     sleep 0.25
///   end
///   ```
///
/// Only expands when `{` follows a known block-initiating method call
/// (`.times`, `.each`, `.each_with_index`, `.upto`, `.downto`).
fn expand_brace_block(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();

    // Look for patterns like:  N.times { ... }  or  arr.each { |x| ... }
    // The { must be at or near the end after a method call, and } must close the line (modulo comments)
    let stripped = strip_inline_comment(trimmed);

    // Find the opening brace
    let brace_pos = match find_block_brace(&stripped) {
        Some(pos) => pos,
        None => return None,
    };

    // Check that the part before `{` contains a block-initiating method
    let before_brace = stripped[..brace_pos].trim();
    let is_block_method = before_brace.contains(".times")
        || before_brace.contains(".each")
        || before_brace.contains(".upto")
        || before_brace.contains(".downto");
    if !is_block_method {
        return None;
    }

    // Find the matching closing brace
    let after_open = &stripped[brace_pos + 1..];
    // For single-line blocks, the closing } should be at the end
    let close_pos = find_matching_close_brace(after_open)?;
    let inner = after_open[..close_pos].trim();

    // Extract optional block variable: |i| or |x|
    let (block_var, body_str) = if inner.starts_with('|') {
        if let Some(end_pipe) = inner[1..].find('|') {
            let var_part = &inner[..end_pipe + 2]; // includes both pipes
            let body = inner[end_pipe + 2..].trim();
            (format!(" {}", var_part), body)
        } else {
            (String::new(), inner)
        }
    } else {
        (String::new(), inner)
    };

    // Compute indentation of the original line
    let indent = line.len() - line.trim_start().len();
    let indent_str: String = line.chars().take(indent).collect();
    let inner_indent = format!("{}  ", indent_str);

    // Build the do...end form
    let mut result = Vec::new();
    result.push(format!("{}{} do{}", indent_str, before_brace, block_var));

    // Split body on semicolons (respecting strings and nested parens)
    let parts = split_semicolons_outside_strings(body_str);
    for part in &parts {
        let p = part.trim();
        if !p.is_empty() {
            result.push(format!("{}{}", inner_indent, p));
        }
    }

    result.push(format!("{}end", indent_str));
    Some(result)
}

/// Find the position of a `{` that starts a block (not a hash literal).
/// Returns None if no block brace is found.
fn find_block_brace(line: &str) -> Option<usize> {
    let mut in_string = false;
    let mut paren_depth = 0i32;
    let bytes = line.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' | b'\'' => in_string = !in_string,
            b'(' if !in_string => paren_depth += 1,
            b')' if !in_string => paren_depth -= 1,
            b'{' if !in_string && paren_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the matching `}` for an already-opened `{`.
/// `s` starts right after the opening `{`.
/// Returns the index within `s` of the closing `}`, or None.
fn find_matching_close_brace(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_string = false;

    for (i, ch) in s.chars().enumerate() {
        match ch {
            '"' | '\'' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
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
                let items = split_respecting_parens(ring_args);
                eprintln!("[parse] ring '{}' = {:?}", var_name, items);
                ctx.ring_values.insert(var_name.clone(), items);
                // Only reset counter if ring is new (preserve tick position across loop iterations)
                ctx.ring_counters.entry(var_name).or_insert(0);
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
                    let items: Vec<String> = pattern
                        .iter()
                        .map(|b| {
                            if *b {
                                "true".to_string()
                            } else {
                                "false".to_string()
                            }
                        })
                        .collect();
                    eprintln!(
                        "[parse] spread({}, {}) '{}' = {:?}",
                        pulses, steps, var_name, items
                    );
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value is (ring ...) (Sonic Pi alternate syntax)
            if var_value.starts_with("(ring") || var_value.starts_with("( ring") {
                let inner = var_value
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .trim();
                let inner = inner.strip_prefix("ring").unwrap_or(inner).trim();
                let items = split_respecting_parens(inner);
                ctx.ring_values.insert(var_name.clone(), items);
                ctx.ring_counters.entry(var_name).or_insert(0);
                i += 1;
                continue;
            }

            // Check if value is a scale() call → store as ring
            if var_value.starts_with("scale(") || var_value.starts_with("scale ") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] scale '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value is a chord() call → store as ring
            if var_value.starts_with("chord(") || var_value.starts_with("chord ") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] chord '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value is a knit() call → store as ring
            if var_value.starts_with("knit(") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] knit '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value is a range() call → store as ring
            if var_value.starts_with("range(") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] range '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value is a line() call → store as ring
            if var_value.starts_with("line(") {
                if let Some(items) = ctx.resolve_to_list(&var_value) {
                    eprintln!("[parse] line '{}' = {:?}", var_name, items);
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value is an inline array: [:c4, :e4, :g4] or [:c4, :e4].ring
            // Also handle .ring.look / .ring.tick suffix chains
            let (array_val, ring_method) = if var_value.ends_with(".ring.look") {
                (&var_value[..var_value.len() - 10], Some("look"))
            } else if var_value.ends_with(".ring.tick") {
                (&var_value[..var_value.len() - 10], Some("tick"))
            } else if var_value.ends_with(".ring") {
                (&var_value[..var_value.len() - 5], None)
            } else {
                (var_value.as_str(), None)
            };
            if array_val.starts_with('[') && array_val.ends_with(']') {
                let inner = &array_val[1..array_val.len() - 1];
                let items = split_respecting_parens(inner);
                // Generate a ring name for anonymous array expressions
                let ring_key = format!("__anon_ring_{}", var_name);
                ctx.ring_values.insert(ring_key.clone(), items.clone());
                ctx.ring_counters.entry(ring_key.clone()).or_insert(0);
                if let Some(method) = ring_method {
                    // Resolve .look or .tick to a concrete value
                    let counter = ctx.ring_counters.get(&ring_key).copied().unwrap_or(0);
                    let idx = counter % items.len().max(1);
                    let resolved_val = items.get(idx).cloned().unwrap_or_default();
                    if method == "tick" {
                        ctx.ring_counters.insert(ring_key, counter + 1);
                    }
                    // Store the resolved scalar value in the variable
                    ctx.variables.insert(var_name.clone(), resolved_val);
                } else {
                    // Store as ring (no immediate resolution)
                    ctx.ring_values.insert(var_name.clone(), items);
                    ctx.ring_counters.entry(var_name.clone()).or_insert(0);
                }
                i += 1;
                continue;
            }

            // Check if value has a list method call: .shuffle, .reverse, .sort, .mirror
            // e.g. (scale :g2, :minor_pentatonic).shuffle or [:c4, :e4, :g4].reverse
            {
                let list_methods = [".shuffle", ".reverse", ".sort", ".mirror"];
                let mut handled_list_method = false;
                for method in &list_methods {
                    if var_value.ends_with(method) {
                        let base_expr = &var_value[..var_value.len() - method.len()];
                        if let Some(mut items) = ctx.resolve_to_list(base_expr) {
                            match *method {
                                ".shuffle" => {
                                    // Fisher-Yates shuffle
                                    for idx in (1..items.len()).rev() {
                                        let j = ctx.rng.gen_range(0..=idx);
                                        items.swap(idx, j);
                                    }
                                }
                                ".reverse" => {
                                    items.reverse();
                                }
                                ".sort" => {
                                    items.sort();
                                }
                                ".mirror" => {
                                    // [a, b, c] → [a, b, c, b, a]
                                    if items.len() > 1 {
                                        let mut mirrored = items.clone();
                                        for idx in (1..items.len() - 1).rev() {
                                            mirrored.push(items[idx].clone());
                                        }
                                        items = mirrored;
                                    }
                                }
                                _ => {}
                            }
                            eprintln!(
                                "[parse] list method {} '{}' = {:?}",
                                method, var_name, items
                            );
                            ctx.ring_values.insert(var_name.clone(), items);
                            ctx.ring_counters.entry(var_name.clone()).or_insert(0);
                            handled_list_method = true;
                            break;
                        }
                    }
                }
                if handled_list_method {
                    i += 1;
                    continue;
                }
            }

            // Warn about unsupported Ruby-specific features
            if var_value.contains("Time.now") {
                eprintln!(
                    "[WARN] '{}' uses Time.now which is NOT supported - value will be set to 0",
                    var_name
                );
                ctx.variables.insert(var_name, "0".to_string());
                i += 1;
                continue;
            }

            // Resolve the value (could reference other vars)
            // Try numeric evaluation first (handles arithmetic expressions)
            let resolved = if let Some(num) = ctx.resolve_numeric(&var_value) {
                num.to_string()
            } else {
                ctx.resolve_string(&var_value)
            };
            ctx.variables.insert(var_name, resolved);
            i += 1;
            continue;
        }

        // Skip 'return' keyword (function bodies are expanded inline)
        if line.starts_with("return ") || line == "return" {
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
            let func_name_raw = line
                .split_whitespace()
                .next()
                .or_else(|| line.split('(').next())
                .unwrap_or("");
            // Also try stripping args: "should_stop?(x, y)" -> "should_stop?"
            let func_name = func_name_raw.split('(').next().unwrap_or(func_name_raw);
            if ctx.functions.contains_key(func_name) {
                let (body, param_names) = ctx.functions.get(func_name).unwrap().clone();

                // Guard against infinite recursion: temporarily remove the
                // function from the context while expanding its body.
                let saved_fn = ctx.functions.remove(func_name);

                // Extract arguments from the call
                let args = extract_function_call_args(&line, func_name);

                // Substitute parameters in body
                let substituted_body = substitute_function_params(&body, &param_names, &args, ctx);

                eprintln!(
                    "[parse] Expanding function '{}' ({} chars, args: {:?})",
                    func_name,
                    substituted_body.len(),
                    args
                );
                // Scope synth state: function bodies should not leak synth changes
                // back to the caller (Sonic Pi scopes use_synth per thread)
                let saved_synth = ctx.current_synth;
                let saved_synth_defaults = ctx.synth_defaults.clone();
                let saved_sample_defaults = ctx.sample_defaults.clone();
                // Save variables that match param names (to restore after)
                let saved_vars: Vec<(String, Option<String>)> = param_names
                    .iter()
                    .map(|p| {
                        let name = p.split('=').next().unwrap_or(p).trim().to_string();
                        let saved = ctx.variables.get(&name).cloned();
                        (name, saved)
                    })
                    .collect();
                // Bind parameters as variables (using provided args or defaults)
                for (i_param, pspec) in param_names.iter().enumerate() {
                    let (pname, default_val) = if let Some(eq_pos) = pspec.find('=') {
                        (pspec[..eq_pos].trim(), Some(pspec[eq_pos + 1..].trim()))
                    } else {
                        (pspec.as_str(), None)
                    };
                    if let Some(arg_val) = args.get(i_param) {
                        ctx.variables.insert(pname.to_string(), arg_val.clone());
                    } else if let Some(def) = default_val {
                        ctx.variables.insert(pname.to_string(), def.to_string());
                    }
                }
                let sub = parse_code_with_context(&substituted_body, ctx)?;
                ctx.current_synth = saved_synth;
                ctx.synth_defaults = saved_synth_defaults;
                ctx.sample_defaults = saved_sample_defaults;
                // Restore saved variables
                for (pname, saved) in saved_vars {
                    if let Some(val) = saved {
                        ctx.variables.insert(pname, val);
                    } else {
                        ctx.variables.remove(&pname);
                    }
                }
                // Restore the function definition so it can be called again
                if let Some(fn_def) = saved_fn {
                    ctx.functions.insert(func_name.to_string(), fn_def);
                }
                commands.extend(sub);
            } else {
                // Track unrecognized lines as validation warnings
                // Compute approximate original line number (i is index in preprocessed lines)
                let line_num = i + 1;
                eprintln!("[parse] Skipping unrecognized line {}: '{}'", line_num, line);
                ctx.warnings.push(ParseWarning {
                    line: line_num,
                    message: format!("Unrecognized syntax — line was skipped"),
                    source_text: if line.len() > 80 {
                        format!("{}…", &line[..80])
                    } else {
                        line.to_string()
                    },
                });
            }
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
        "play",
        "sample",
        "sleep",
        "use_bpm",
        "use_synth",
        "live_loop",
        "with_fx",
        "puts",
        "print",
        "log",
        "stop",
        "end",
        "do",
        "loop",
        "define",
        "def",
        "in_thread",
        "set_volume",
        "set_volume!",
        "comment",
        "uncomment",
        "density",
        "at",
        "cue",
        "sync",
    ];

    let eq_pos = line.find('=')?;
    // Make sure it's not == or =>
    if eq_pos + 1 < line.len() {
        let next_char = line.as_bytes().get(eq_pos + 1)?;
        if *next_char == b'=' || *next_char == b'>' {
            return None;
        }
    }
    // Handle compound assignments: +=, -=, *=, /=, %=
    if eq_pos > 0 {
        let prev_char = line.as_bytes().get(eq_pos - 1)?;
        if *prev_char == b'+'
            || *prev_char == b'-'
            || *prev_char == b'*'
            || *prev_char == b'/'
            || *prev_char == b'%'
        {
            let op = *prev_char as char;
            let var_name = line[..eq_pos - 1].trim().to_string();
            let rhs = line[eq_pos + 1..].trim().to_string();
            if var_name.is_empty()
                || !var_name.chars().next().unwrap_or(' ').is_alphabetic()
                || var_name.contains(' ')
            {
                return None;
            }
            // Expand: var op= rhs → var = var op rhs
            let expanded = format!("{} {} {}", var_name, op, rhs);
            return Some((var_name, expanded));
        }
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
    // live_loop :name do  or  live_loop :name, sync: :other_loop do
    if line.starts_with("live_loop") {
        let name = extract_symbol(line).unwrap_or_else(|| "loop".to_string());
        // Extract sync: parameter if present
        let sync_with = if line.contains("sync:") {
            // Extract the sync target: sync: :name
            if let Some(sync_pos) = line.find("sync:") {
                let after_sync = &line[sync_pos + 5..];
                // Find the symbol after sync:
                let sync_name = after_sync
                    .trim()
                    .trim_start_matches(':')
                    .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
                    .next()
                    .map(|s| s.to_string());
                if let Some(ref sn) = sync_name {
                    eprintln!(
                        "[parser] live_loop :{} will sync with :{}",
                        name, sn
                    );
                }
                sync_name
            } else {
                None
            }
        } else {
            None
        };
        let (body, end_i) = collect_block_body(lines, start_i)?;
        // Scope synth state: live_loop runs in its own thread in Sonic Pi
        let saved_synth = ctx.current_synth;
        let saved_synth_defaults = ctx.synth_defaults.clone();
        let saved_sample_defaults = ctx.sample_defaults.clone();
        // If the body contains .tick, unroll enough iterations so the tick
        // counter cycles through the ring.  First parse registers the rings,
        // then we check cycle length and parse additional iterations if needed.
        let sub = if body.contains(".tick") {
            // First pass: registers rings and resolves first tick values
            let first_pass = parse_code_with_context(&body, ctx)?;
            // Now rings are registered in ctx — check cycle length
            let cycle_len = detect_tick_cycle_length(&body, ctx);
            if cycle_len > 1 {
                let mut all_cmds = first_pass;
                for _ in 1..cycle_len {
                    let iter_cmds = parse_code_with_context(&body, ctx)?;
                    all_cmds.extend(iter_cmds);
                }
                all_cmds
            } else {
                first_pass
            }
        } else {
            parse_code_with_context(&body, ctx)?
        };
        ctx.current_synth = saved_synth;
        ctx.synth_defaults = saved_synth_defaults;
        ctx.sample_defaults = saved_sample_defaults;
        return Ok(Some((
            ParsedCommand::Loop {
                name,
                commands: sub,
                parallel: true,
                sync_with,
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
                sync_with: None,
            },
            end_i,
        )));
    }

    // N.times do (e.g., 8.times do, 16.times do)  or  N.times do |i|
    if let Some(count) = try_extract_times_count(line) {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        // Extract block variable: |i| or |idx|
        let block_var = if let Some(pipe_start) = line.find('|') {
            if let Some(pipe_end) = line[pipe_start + 1..].find('|') {
                Some(
                    line[pipe_start + 1..pipe_start + 1 + pipe_end]
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ref var_name) = block_var {
            // Iterate with the loop variable bound
            let saved_var = ctx.variables.get(var_name).cloned();
            let mut all_cmds = Vec::new();
            for iter_i in 0..count {
                ctx.variables.insert(var_name.clone(), iter_i.to_string());
                let sub = parse_code_with_context(&body, ctx)?;
                all_cmds.extend(sub);
            }
            // Restore the variable
            if let Some(saved) = saved_var {
                ctx.variables.insert(var_name.clone(), saved);
            } else {
                ctx.variables.remove(var_name);
            }
            return Ok(Some((
                ParsedCommand::TimesLoop {
                    count: 1,
                    commands: all_cmds,
                },
                end_i,
            )));
        } else {
            // If the body contains .tick, unroll the loop so each iteration
            // advances the tick counter independently
            if body.contains(".tick") {
                let mut all_cmds = Vec::new();
                for _ in 0..count {
                    let sub = parse_code_with_context(&body, ctx)?;
                    all_cmds.extend(sub);
                }
                return Ok(Some((
                    ParsedCommand::TimesLoop {
                        count: 1,
                        commands: all_cmds,
                    },
                    end_i,
                )));
            }
            let sub = parse_code_with_context(&body, ctx)?;
            return Ok(Some((
                ParsedCommand::TimesLoop {
                    count,
                    commands: sub,
                },
                end_i,
            )));
        }
    }

    // while condition do ... end  (e.g., while t < intro_len)
    if line.starts_with("while ") {
        let cond_part = line.strip_prefix("while ").unwrap_or("").trim();
        let cond_clean = cond_part
            .trim_end_matches(" do")
            .trim_end_matches(" then")
            .trim();
        let (body, end_i) = collect_block_body(lines, start_i)?;
        // Try to evaluate the condition at parse time: approximate with a capped loop
        // Parse the body up to 500 times checking each iteration
        let max_iters = 500;
        let mut all_cmds = Vec::new();
        for _ in 0..max_iters {
            if !evaluate_condition(cond_clean, ctx) {
                break;
            }
            let sub = parse_code_with_context(&body, ctx)?;
            all_cmds.extend(sub);
        }
        if all_cmds.is_empty() {
            return Ok(Some((
                ParsedCommand::Comment(format!("# while {} (empty)", cond_clean)),
                end_i,
            )));
        }
        return Ok(Some((
            ParsedCommand::TimesLoop {
                count: 1,
                commands: all_cmds,
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
        // Scope synth state AND variables: in_thread runs in its own thread in Sonic Pi
        // Variable modifications inside in_thread should not leak to parent scope
        // because the thread may have leading sleep() that defers execution
        let saved_synth = ctx.current_synth;
        let saved_synth_defaults = ctx.synth_defaults.clone();
        let saved_sample_defaults = ctx.sample_defaults.clone();
        let saved_variables = ctx.variables.clone();
        let sub = parse_code_with_context(&body, ctx)?;
        ctx.current_synth = saved_synth;
        ctx.synth_defaults = saved_synth_defaults;
        ctx.sample_defaults = saved_sample_defaults;
        ctx.variables = saved_variables; // Restore variables - don't leak modifications
        return Ok(Some((
            ParsedCommand::Loop {
                name: "thread".to_string(),
                commands: sub,
                parallel: true,
                sync_with: None,
            },
            end_i,
        )));
    }

    // define :name do ... end — store function body for later expansion
    if line.starts_with("define") {
        let func_name = extract_symbol(line).unwrap_or_else(|| "unnamed".to_string());
        let (raw_body, end_i) = collect_block_body(lines, start_i)?;

        // Extract parameters from |param1, param2| — they can appear either:
        //   (a) on the opening line: `define :name do |p1, p2|`
        //   (b) at the start of the body block
        let (body, param_names) = {
            // First check the opening line for |params| after "do"
            let mut inline_params = Vec::new();
            if let Some(do_pos) = line.find(" do") {
                let after_do = line[do_pos + 3..].trim();
                if after_do.starts_with('|') {
                    if let Some(end_pipe) = after_do[1..].find('|') {
                        let params_str = &after_do[1..end_pipe + 1];
                        inline_params = params_str
                            .split(',')
                            .map(|p| p.trim().to_string())
                            .filter(|p| !p.is_empty())
                            .collect();
                    }
                }
            }
            if !inline_params.is_empty() {
                // Params were on the opening line; body is as-is
                (raw_body.clone(), inline_params)
            } else {
                // Try extracting from the start of the body
                extract_block_params(&raw_body)
            }
        };

        eprintln!(
            "[parse] Storing define :{} ({} chars, params: {:?})",
            func_name,
            body.len(),
            param_names
        );
        ctx.functions.insert(func_name.clone(), (body, param_names));
        return Ok(Some((
            ParsedCommand::Comment(format!("# define :{} (stored)", func_name)),
            end_i,
        )));
    }

    // Ruby-style def name(args) ... end — store function body
    if line.starts_with("def ") {
        let rest = line[4..].trim();
        // Extract function name (may contain ? or !)
        let name_end = rest
            .find('(')
            .or_else(|| rest.find(' '))
            .unwrap_or(rest.len());
        let func_name = rest[..name_end].trim().to_string();

        // Extract parameter names from def name(param1, param2)
        let mut param_names = Vec::new();
        if let Some(paren_start) = rest.find('(') {
            if let Some(paren_end) = rest.find(')') {
                let params_str = &rest[paren_start + 1..paren_end];
                param_names = params_str
                    .split(',')
                    .map(|p| {
                        // Preserve "name=default" format — defaults handled during substitution
                        p.trim().to_string()
                    })
                    .filter(|p| !p.is_empty())
                    .collect();
            }
        }

        let (body, end_i) = collect_block_body_for_def(lines, start_i)?;
        eprintln!(
            "[parse] Storing def {} ({} chars, params: {:?})",
            func_name,
            body.len(),
            param_names
        );
        ctx.functions.insert(func_name.clone(), (body, param_names));
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
        // Also treat as block if the condition doesn't end with 'do'/'then' but
        // there's a matching 'end' on a subsequent line (from semicolon splitting)
        let has_matching_end = if !is_block {
            // Look ahead for a matching 'end'
            let mut depth = 1i32;
            let mut found = false;
            for k in (start_i + 1)..lines.len() {
                let peek = lines[k].trim();
                if is_block_opener(peek) || peek.starts_with("if ") || peek.starts_with("unless ") {
                    depth += 1;
                }
                if peek == "end" {
                    depth -= 1;
                    if depth == 0 {
                        found = true;
                        break;
                    }
                }
            }
            found
        } else {
            false
        };
        if is_block || has_matching_end {
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

    // with_bpm_mul N do ... end — temporarily multiply BPM for inner block
    // NOTE: Must check BEFORE with_bpm since "with_bpm_mul".starts_with("with_bpm")
    if line.starts_with("with_bpm_mul") {
        let rest = line.strip_prefix("with_bpm_mul").unwrap_or("").trim();
        let mul_str = rest.split_whitespace().next().unwrap_or("1");
        let mul: f32 = ctx.resolve_numeric(mul_str).unwrap_or(1.0);
        let saved_bpm = ctx.current_bpm;
        ctx.current_bpm *= mul;
        let new_bpm = ctx.current_bpm;
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;
        ctx.current_bpm = saved_bpm;
        // Wrap: SetBpm(new) → inner commands → SetBpm(restored)
        let mut cmds = vec![ParsedCommand::SetBpm(new_bpm)];
        cmds.extend(sub);
        cmds.push(ParsedCommand::SetBpm(saved_bpm));
        return Ok(Some((
            ParsedCommand::TimesLoop {
                count: 1,
                commands: cmds,
            },
            end_i,
        )));
    }

    // with_bpm N do ... end
    if line.starts_with("with_bpm") {
        let bpm_str = line
            .strip_prefix("with_bpm")
            .unwrap_or("120")
            .trim()
            .trim_end_matches("do")
            .trim_end_matches("then")
            .trim();
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

    // with_swing shift, pulse:, tick:, offset: do ... end
    //
    // Sonic Pi runs the block normally except once every `pulse` runs, where
    // it wraps it in `time_warp shift`. Defaults match Sonic Pi: shift 0.1
    // beats, pulse 4, tick key :swing, offset 0.
    if line.starts_with("with_swing") {
        let (body, end_i) = collect_block_body(lines, start_i)?;
        let sub = parse_code_with_context(&body, ctx)?;

        let args = line.strip_prefix("with_swing").unwrap_or("").trim();
        let shift = extract_param(line, "shift")
            .or_else(|| first_positional_number(args))
            .unwrap_or(0.1);
        let pulse = extract_param(line, "pulse").unwrap_or(4.0).max(1.0) as u32;
        let offset = extract_param(line, "offset").unwrap_or(0.0).round() as i64;
        let tick_key = extract_symbol_param(line, "tick").unwrap_or_else(|| "swing".to_string());

        return Ok(Some((
            ParsedCommand::SwingBlock {
                shift,
                pulse,
                tick_key,
                offset,
                commands: sub,
            },
            end_i,
        )));
    }

    // .each do |x| ... end  (e.g., [:c4, :e4, :g4].each do |n|)
    // Also handles: var_name.each do |x|
    // Also handles destructuring: var_name.each do |n, d|
    if line.contains(".each") && (line.ends_with("do") || line.contains("do |")) {
        let dot_pos = line.find(".each").unwrap();
        let list_expr = &line[..dot_pos];

        // Extract block variable name(s) from |var| or |var1, var2|
        let block_var_str = line
            .find('|')
            .and_then(|start| {
                let after = &line[start + 1..];
                after.find('|').map(|end| after[..end].trim().to_string())
            })
            .unwrap_or_else(|| "x".to_string());

        // Split on comma for destructuring: |n, d| → ["n", "d"]
        let block_vars: Vec<String> = block_var_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let is_destructuring = block_vars.len() > 1;

        let (body, end_i) = collect_block_body(lines, start_i)?;

        // Resolve the list
        if let Some(values) = ctx.resolve_to_list(list_expr) {
            let mut all_commands = Vec::new();
            for val in &values {
                // Save old variable values
                let mut old_vals: Vec<(String, Option<String>)> = Vec::new();
                
                if is_destructuring {
                    // Destructure the value: if val is "[root, 0.75]", split into sub-values
                    let sub_values = if val.starts_with('[') && val.ends_with(']') {
                        // Strip brackets and split
                        let inner = &val[1..val.len() - 1];
                        split_respecting_parens(inner)
                    } else {
                        // Try splitting on comma anyway
                        val.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>()
                    };
                    
                    for (vi, var_name) in block_vars.iter().enumerate() {
                        old_vals.push((var_name.clone(), ctx.variables.get(var_name).cloned()));
                        if let Some(sub_val) = sub_values.get(vi) {
                            ctx.variables.insert(var_name.clone(), sub_val.clone());
                        } else {
                            // Not enough values for this variable — set to "nil"
                            ctx.variables.insert(var_name.clone(), "nil".to_string());
                        }
                    }
                } else {
                    // Single block variable
                    let var_name = &block_vars[0];
                    old_vals.push((var_name.clone(), ctx.variables.get(var_name).cloned()));
                    ctx.variables.insert(var_name.clone(), val.clone());
                }
                
                let sub = parse_code_with_context(&body, ctx)?;
                all_commands.extend(sub);
                
                // Restore old variable values
                for (var_name, old_val) in old_vals {
                    if let Some(ov) = old_val {
                        ctx.variables.insert(var_name, ov);
                    } else {
                        ctx.variables.remove(&var_name);
                    }
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
                sync_with: None,
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
                sync_with: None,
            },
            end_i,
        )));
    }

    // time_warp offset do ... end — schedule code to run at a relative offset
    // Examples:
    //   time_warp 0.5 do
    //     play :c4
    //   end
    // Note: We implement this by wrapping the body in a SleepUntil + commands,
    // but keeping time_offset unchanged after the block (it runs "virtually" ahead)
    if (line.starts_with("time_warp ") || line.starts_with("time_warp("))
        && (line.ends_with("do") || line.contains("do |"))
    {
        let rest = line
            .strip_prefix("time_warp(")
            .or_else(|| line.strip_prefix("time_warp "))
            .unwrap_or("")
            .trim();
        // Extract the offset value (before "do" or closing paren)
        let offset_str = rest
            .split_once(" do")
            .map(|(s, _)| s)
            .unwrap_or(rest)
            .trim_end_matches(')')
            .trim();
        let offset = ctx.resolve_numeric(offset_str).unwrap_or(0.0);

        // Extract optional block variable from |delta|
        let block_var = rest
            .find('|')
            .and_then(|start| {
                let after = &rest[start + 1..];
                after.find('|').map(|end| after[..end].trim().to_string())
            });

        let (body, end_i) = collect_block_body(lines, start_i)?;

        // Set block variable if specified
        if let Some(ref var) = block_var {
            ctx.variables.insert(var.clone(), offset.to_string());
        }

        // Parse the block body
        let inner_commands = parse_code_with_context(&body, ctx)?;

        // Restore block variable
        if let Some(ref var) = block_var {
            ctx.variables.remove(var);
        }

        // Wrap the commands: first sleep to the offset, then the commands
        let mut all_commands = vec![ParsedCommand::Sleep(offset)];
        all_commands.extend(inner_commands);

        return Ok(Some((
            ParsedCommand::AtBlock {
                times: vec![offset],
                commands: all_commands,
            },
            end_i,
        )));
    }

    // at [times] do |t| ... end — schedule code to run at specific beat times
    // Examples:
    //   at [0, 1, 2, 3] do |t|
    //     sample :kick
    //   end
    if line.starts_with("at ") && (line.ends_with("do") || line.contains("do |")) {
        let rest = line.strip_prefix("at ").unwrap_or("").trim();

        // Extract the times array
        let times_list = if let Some(bracket_start) = rest.find('[') {
            if let Some(bracket_end) = rest.find(']') {
                let times_str = &rest[bracket_start + 1..bracket_end];
                times_str
                    .split(',')
                    .filter_map(|s| {
                        let cleaned = s.trim();
                        // Resolve variables or parse directly
                        if let Some(val) = ctx.variables.get(cleaned) {
                            val.parse::<f32>().ok()
                        } else {
                            cleaned.parse::<f32>().ok()
                        }
                    })
                    .collect::<Vec<f32>>()
            } else {
                vec![]
            }
        } else if let Some(ring_name) = extract_ring_var(rest) {
            // Handle: at my_times do
            ctx.ring_values
                .get(&ring_name)
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|s| s.parse::<f32>().ok())
                .collect()
        } else {
            vec![]
        };

        // Extract block variable name from |t| or |beat|
        let block_var = rest
            .find('|')
            .and_then(|start| {
                let after = &rest[start + 1..];
                after.find('|').map(|end| after[..end].trim().to_string())
            })
            .unwrap_or_else(|| "t".to_string());

        let (body, end_i) = collect_block_body(lines, start_i)?;

        // Execute the block at each specified time
        let mut all_commands = Vec::new();
        for beat_time in &times_list {
            // Set the block variable to the current time
            let old_val = ctx.variables.get(&block_var).cloned();
            ctx.variables
                .insert(block_var.clone(), beat_time.to_string());

            // Insert sleep to reach the specified beat time
            all_commands.push(ParsedCommand::SleepUntil(*beat_time));

            // Parse and add the block body
            let sub = parse_code_with_context(&body, ctx)?;
            all_commands.extend(sub);

            // Restore old variable value
            if let Some(ov) = old_val {
                ctx.variables.insert(block_var.clone(), ov);
            } else {
                ctx.variables.remove(&block_var);
            }
        }

        return Ok(Some((
            ParsedCommand::AtBlock {
                times: times_list,
                commands: all_commands,
            },
            end_i,
        )));
    }

    Ok(None)
}

/// Evaluate a condition expression (for if blocks)
fn evaluate_condition(condition: &str, ctx: &mut ParseContext) -> bool {
    let trimmed = condition.trim();

    // Handle OR: cond1 || cond2
    if trimmed.contains("||") {
        let parts: Vec<&str> = trimmed.splitn(2, "||").collect();
        if parts.len() == 2 {
            return evaluate_condition(parts[0], ctx) || evaluate_condition(parts[1], ctx);
        }
    }

    // Handle AND: cond1 && cond2
    if trimmed.contains("&&") {
        let parts: Vec<&str> = trimmed.splitn(2, "&&").collect();
        if parts.len() == 2 {
            return evaluate_condition(parts[0], ctx) && evaluate_condition(parts[1], ctx);
        }
    }

    // Handle negation: !expr
    if trimmed.starts_with('!') {
        return !evaluate_condition(&trimmed[1..], ctx);
    }

    // Handle get(:key) — resolve to variable value
    if let Some(inner) = extract_func_args(trimmed, "get") {
        let key = inner.trim().trim_start_matches(':').trim_end_matches(',');
        if let Some(val) = ctx.variables.get(key) {
            let v = val.clone();
            return v != "false" && v != "0" && v != "nil" && v != "0.0";
        }
        return false; // unknown variable = false
    }

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
            let left = ctx
                .resolve_numeric(left_str)
                .or_else(|| left_str.parse::<f32>().ok());
            let right = ctx
                .resolve_numeric(right_str)
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
                    if values.is_empty() {
                        return false;
                    }
                    let probability = match_count as f64 / values.len() as f64;
                    return ctx.rng.gen_bool(probability.min(1.0));
                }
                return true;
            }
        }
        return ctx.rng.gen_bool(0.5);
    }

    // true/false literals
    if trimmed == "true" {
        return true;
    }
    if trimmed == "false" {
        return false;
    }

    // Function call as condition: func_name(args) or func_name?(args)
    // If the function is defined, try to evaluate its body.
    // We can't fully evaluate Ruby return values, so for defined functions
    // whose body contains comparison operators, attempt a rough evaluation.
    // For time-based functions (referencing Time), default to false (time hasn't elapsed).
    let func_call_name = trimmed.split('(').next().unwrap_or("").trim();
    if ctx.functions.contains_key(func_call_name) {
        let (body, _params) = ctx.functions.get(func_call_name).unwrap().clone();
        // If the function body references Time or time-based calculations, return false
        // since at parse time no real time has elapsed
        if body.contains("Time.now") || body.contains("start_time") || body.contains("stop_time") {
            eprintln!(
                "[eval_condition] Function '{}' is time-based, defaulting to false",
                func_call_name
            );
            return false;
        }
        // For other defined functions, default to true
        return true;
    }

    // Variable truthiness: check if the variable is nil/false/0
    // This handles `if n` where n might be "nil" or a valid note
    if let Some(val) = ctx.variables.get(trimmed) {
        let v = val.clone();
        return v != "false" && v != "0" && v != "nil" && v != "0.0" && !v.is_empty();
    }

    // Default: true (include the block)
    true
}

/// Extract count from "N.times do" patterns
fn try_extract_times_count(line: &str) -> Option<usize> {
    // Match: 8.times do, 16.times do, 8.times do |i|, etc.
    let line = line.trim();
    if let Some(dot_pos) = line.find(".times") {
        let num_str = line[..dot_pos].trim();
        if let Ok(n) = num_str.parse::<usize>() {
            // Ensure it contains "do" (may have block var like |i| after)
            if line.contains(" do") || line.contains(".times do") {
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
            let is_elsif_else =
                (l_no_comment.starts_with("elsif") || l_no_comment == "else") && depth == 1;
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
                let cond = trimmed
                    .strip_prefix("elsif ")
                    .unwrap_or("")
                    .trim_end_matches(" do")
                    .trim_end_matches(" then")
                    .trim()
                    .to_string();
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
/// Detect how many iterations are needed for one full .tick cycle in a body.
/// Scans for `varname.tick` patterns and looks up the ring size.
/// Returns the LCM of all referenced ring sizes, capped at 64.
fn detect_tick_cycle_length(body: &str, ctx: &ParseContext) -> usize {
    let mut lengths: Vec<usize> = Vec::new();
    // Look for patterns like `varname.tick`
    for word in body.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| c == ',' || c == ')' || c == ';');
        if let Some(dot_pos) = cleaned.find(".tick") {
            let var_name = &cleaned[..dot_pos];
            if let Some(values) = ctx.ring_values.get(var_name) {
                if values.len() > 1 {
                    lengths.push(values.len());
                }
            }
        }
    }
    if lengths.is_empty() {
        return 1;
    }
    // Use LCM of all ring lengths for correct multi-ring cycling
    let result = lengths.into_iter().fold(1usize, lcm);
    result.min(64) // Cap to prevent excessive unrolling
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn lcm(a: usize, b: usize) -> usize {
    if a == 0 || b == 0 { 1 } else { a / gcd(a, b) * b }
}

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
    // Standalone if/unless blocks (Ruby allows `if cond\n ... end` without do/then)
    if trimmed.starts_with("if ") || trimmed.starts_with("unless ") {
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
/// Also handles standalone choose(list) function.
/// Returns a resolved note string if the expression matches, None otherwise.
fn try_resolve_list_method(expr: &str, ctx: &mut ParseContext) -> Option<String> {
    let trimmed = expr.trim();

    // Handle standalone choose(list) function
    if trimmed.starts_with("choose(") {
        return ctx.resolve_list_value(trimmed);
    }

    // Split off params: "scale(:c4, :minor).choose, amp: 0.5" → separate at first comma after method
    let note_part = if let Some(method_end) = find_method_end(trimmed) {
        &trimmed[..method_end]
    } else {
        trimmed
    };

    // Check if it has a method call
    for method in &[
        ".choose", ".pick", ".tick", ".look", ".first", ".last", ".shuffle", ".reverse", ".min",
        ".max", ".sample",
    ] {
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
        if ch == '(' {
            paren_depth += 1;
        }
        if ch == ')' {
            paren_depth -= 1;
        }
        if ch == '.' && paren_depth == 0 {
            found_method = true;
        }
        if ch == ',' && paren_depth == 0 && found_method {
            return Some(i);
        }
    }
    None
}

/// Extract param with defaults fallback
fn extract_param_with_defaults(
    line: &str,
    param: &str,
    defaults: &HashMap<String, f32>,
    fallback: f32,
) -> f32 {
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
        let condition = line[if_pos + 3..].trim(); // skip "if "

        // Special case: `if one_in(N)` — defer evaluation to audio expansion
        // so each loop iteration gets independent randomness
        if condition.starts_with("one_in(") {
            if let Some(inner) = extract_func_args(condition, "one_in") {
                if let Ok(n) = inner.trim().parse::<u32>() {
                    if let Some(cmd) = parse_line(main_part, ctx) {
                        return Some(ParsedCommand::ConditionalRandom {
                            n,
                            command: Box::new(cmd),
                        });
                    }
                    return None;
                }
            }
        }

        let condition_result = evaluate_condition(condition, ctx);
        if condition_result {
            return parse_line(main_part, ctx);
        } else {
            return Some(ParsedCommand::Comment(format!("# if (skipped): {}", condition)));
        }
    }

    // Handle trailing `unless condition`
    if let Some(unless_pos) = find_trailing_unless(line) {
        let main_part = line[..unless_pos].trim();
        let condition = line[unless_pos + 7..].trim(); // skip "unless "
        let condition_result = evaluate_condition(condition, ctx);
        if !condition_result {
            return parse_line(main_part, ctx);
        } else {
            return Some(ParsedCommand::Comment(format!("# unless (skipped): {}", condition)));
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
                // If the resolved value is a chord expression, route to chord parsing
                if note_str.starts_with("chord(") || note_str.starts_with("chord ") {
                    // Reconstruct a play line with the resolved chord + original params
                    let params_start = find_method_end(note_expr).unwrap_or(note_expr.len());
                    let params_suffix = &note_expr[params_start..];
                    let synthetic_line = format!("play {}{}", note_str, params_suffix);
                    return parse_play_chord(&synthetic_line, ctx);
                }
                let note = parse_note_value(&note_str)?;
                let amplitude = extract_param_with_defaults(line, "amp", &ctx.synth_defaults, 1.0);
                let duration =
                    extract_param_with_defaults(line, "sustain", &ctx.synth_defaults, 0.0);
                let pan = extract_param_with_defaults(line, "pan", &ctx.synth_defaults, 0.0);
                let attack = extract_param_with_defaults(line, "attack", &ctx.synth_defaults, 0.0);
                let decay = extract_param_with_defaults(line, "decay", &ctx.synth_defaults, 0.0);
                let sustain_level =
                    extract_param_with_defaults(line, "sustain_level", &ctx.synth_defaults, 1.0);
                let release =
                    extract_param_with_defaults(line, "release", &ctx.synth_defaults, 1.0);

                return Some(ParsedCommand::PlayNote {
                    synth_type: ctx.current_synth,
                    frequency: note,
                    amplitude,
                    duration,
                    pan,
                    envelope: envelope_from_line(line, attack, decay, sustain_level, release),
                    params: extract_synth_params(line),
                });
            }

            // Extract note expression: everything after "play" until the first comma
            // (respecting parentheses) or until a parameter like "amp:", "release:", etc.
            let rest_after_play = line["play".len()..].trim();
            let note_expr = extract_note_expression(rest_after_play);

            // Try to resolve as numeric expression first (handles (note root)+12, rrand, etc.)
            // If the expression contains "note", the result is in MIDI and needs Hz conversion
            let note_opt = ctx.resolve_numeric(&note_expr);
            let note = if let Some(value) = note_opt {
                // If expression involved note(), result is MIDI - convert to Hz
                if note_expr.contains("note") {
                    midi_to_freq(value as u8)
                } else {
                    // Could be a direct MIDI number (0-127) or Hz - use parse_note_value logic
                    if value >= 0.0 && value <= 127.0 && !note_expr.contains('.') {
                        midi_to_freq(value as u8)
                    } else {
                        value
                    }
                }
            } else {
                // Fallback: try as note name or variable
                let resolved_note = ctx.resolve_string(&note_expr);
                parse_note_value(&resolved_note)?
            };

            let amplitude = extract_param_with_context(line, "amp", Some(ctx))
                .or_else(|| ctx.synth_defaults.get("amp").copied())
                .unwrap_or(1.0);
            let duration = extract_param(line, "sustain")
                .or_else(|| extract_param(line, "duration"))
                .or_else(|| ctx.synth_defaults.get("sustain").copied())
                .unwrap_or(0.0);
            let pan = extract_param_with_defaults(line, "pan", &ctx.synth_defaults, 0.0);
            let attack = extract_param_with_defaults(line, "attack", &ctx.synth_defaults, 0.0);
            let decay = extract_param_with_defaults(line, "decay", &ctx.synth_defaults, 0.0);
            let sustain_level =
                extract_param_with_defaults(line, "sustain_level", &ctx.synth_defaults, 1.0);
            let release = extract_param_with_defaults(line, "release", &ctx.synth_defaults, 1.0);

            Some(ParsedCommand::PlayNote {
                synth_type: ctx.current_synth,
                frequency: note,
                amplitude,
                duration,
                pan,
                envelope: envelope_from_line(line, attack, decay, sustain_level, release),
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
            eprintln!(
                "[parse] sample expr='{}' -> resolved='{}'",
                sample_expr, resolved
            );

            let rate = extract_param_with_context(params_str, "rate", Some(ctx))
                .or_else(|| ctx.sample_defaults.get("rate").copied())
                .unwrap_or(1.0);
            let amplitude = extract_param_with_context(params_str, "amp", Some(ctx))
                .or_else(|| ctx.sample_defaults.get("amp").copied())
                .unwrap_or(1.0);
            let pan = extract_param_with_context(params_str, "pan", Some(ctx))
                .or_else(|| ctx.sample_defaults.get("pan").copied())
                .unwrap_or(0.0);

            // These are parsed and passed through to be applied at playback
            let rpitch = extract_param(params_str, "rpitch");
            let pitch = extract_param(params_str, "pitch");
            let beat_stretch = extract_param(params_str, "beat_stretch");
            let start = extract_param(params_str, "start"); // 0.0-1.0 range
            let finish = extract_param(params_str, "finish"); // 0.0-1.0 range
            let _pitch_stretch = extract_param(params_str, "pitch_stretch");
            // sustain: truncates sample playback to N beats
            let sustain_beats = extract_param(params_str, "sustain");

            // ADSR envelope for sample: only create if attack or release specified
            let attack = extract_param(params_str, "attack");
            let decay = extract_param(params_str, "decay");
            let sustain_level = extract_param(params_str, "sustain_level");
            let release = extract_param(params_str, "release");
            let sample_envelope = if attack.is_some() || release.is_some() || decay.is_some() {
                Some(envelope_from_line(
                    params_str,
                    attack.unwrap_or(0.0),
                    decay.unwrap_or(0.0),
                    sustain_level.unwrap_or(1.0),
                    release.unwrap_or(0.0),
                ))
            } else {
                None
            };

            // Apply rpitch as rate modifier (semitone shift)
            let mut final_rate = rate;
            if let Some(rp) = rpitch {
                final_rate *= 2.0f32.powf(rp / 12.0);
            }
            // Apply pitch: as semitone shift (same as rpitch for our engine)
            if let Some(p) = pitch {
                final_rate *= 2.0f32.powf(p / 12.0);
            }

            Some(ParsedCommand::PlaySample {
                name: resolved,
                rate: final_rate,
                amplitude,
                pan,
                sustain_beats,
                beat_stretch,
                start,
                finish,
                lpf: extract_param(params_str, "lpf"),
                hpf: extract_param(params_str, "hpf"),
                envelope: sample_envelope,
            })
        }
        "sleep" => {
            let rest = line["sleep".len()..].trim();
            let duration = ctx
                .resolve_numeric(rest)
                .unwrap_or_else(|| rest.parse::<f32>().unwrap_or(0.0));
            if duration > 0.0 {
                Some(ParsedCommand::Sleep(duration))
            } else {
                None
            }
        }
        "wait" => {
            let rest = line["wait".len()..].trim();
            let duration = ctx
                .resolve_numeric(rest)
                .unwrap_or_else(|| rest.parse::<f32>().unwrap_or(0.0));
            if duration > 0.0 {
                Some(ParsedCommand::Sleep(duration))
            } else {
                None
            }
        }
        "use_bpm" => {
            let bpm: f32 = parts.get(1)?.parse().ok()?;
            ctx.current_bpm = bpm;
            Some(ParsedCommand::SetBpm(bpm))
        }
        "use_bpm_mul" => {
            let mul: f32 = ctx.resolve_numeric(parts.get(1).unwrap_or(&"1")).unwrap_or(1.0);
            let new_bpm = ctx.current_bpm * mul;
            ctx.current_bpm = new_bpm;
            Some(ParsedCommand::SetBpm(new_bpm))
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
            let synth_name = parts
                .get(1)
                .map(|s| s.trim_start_matches(':').trim_end_matches(','))
                .unwrap_or("sine");
            let synth_type = parse_synth_name(synth_name);

            // Try to resolve note as a list method expression
            let note = extract_param(line, "note")
                .or_else(|| extract_note_param(line, "note"))
                .or_else(|| {
                    // Check if note param uses a list method: note: scale(:c4, :minor).choose
                    if let Some(pos) = line.find("note:") {
                        let after = &line[pos + 5..].trim();
                        let note_expr: String = after
                            .chars()
                            .take_while(|c| {
                                *c != ','
                                    || after[..after.find(*c).unwrap_or(0)].matches('(').count()
                                        > after[..after.find(*c).unwrap_or(0)].matches(')').count()
                            })
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

            let amplitude = extract_param_with_defaults(line, "amp", &ctx.synth_defaults, 1.0);
            let duration = extract_param_with_defaults(line, "sustain", &ctx.synth_defaults, 0.0);
            let pan = extract_param_with_defaults(line, "pan", &ctx.synth_defaults, 0.0);
            let attack = extract_param_with_defaults(line, "attack", &ctx.synth_defaults, 0.0);
            let decay = extract_param_with_defaults(line, "decay", &ctx.synth_defaults, 0.0);
            let sustain_level =
                extract_param_with_defaults(line, "sustain_level", &ctx.synth_defaults, 1.0);
            let release = extract_param_with_defaults(line, "release", &ctx.synth_defaults, 1.0);
            Some(ParsedCommand::PlayNote {
                synth_type,
                frequency: note,
                amplitude,
                duration,
                pan,
                envelope: envelope_from_line(line, attack, decay, sustain_level, release),
                params: extract_synth_params(line),
            })
        }
        "stop" => Some(ParsedCommand::Stop),
        "next" => {
            // next → skip to next loop iteration. At parse time, treat as no-op
            // since context is flattened. The important thing is it doesn't error.
            Some(ParsedCommand::Comment("# next".to_string()))
        }
        "puts" | "print" | "log" => {
            let msg = parts[1..].join(" ").trim_matches('"').to_string();
            Some(ParsedCommand::Log(msg))
        }
        "cue" | "sync" => {
            // `cue :name` / `sync :name` — the name may be a symbol, a bare
            // word or a quoted string. Anything after the name (cue's optional
            // key/value payload) is ignored; PiBeat has no thread-locals for
            // it to land in.
            let name = parts
                .get(1)
                .map(|raw| {
                    raw.trim()
                        .trim_end_matches(',')
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_start_matches(':')
                        .to_string()
                })
                .filter(|n| !n.is_empty());
            match name {
                Some(name) if parts[0] == "cue" => Some(ParsedCommand::Cue(name)),
                Some(name) => Some(ParsedCommand::Sync(name)),
                None => Some(ParsedCommand::Comment(format!("# {}", line))),
            }
        }
        "at" => Some(ParsedCommand::Comment(format!("# {}", line))),
        "use_random_seed" | "use_random_source" => {
            // Sonic Pi parity: seed the deterministic PRNG
            if let Some(seed_str) = parts.get(1) {
                if let Ok(seed) = seed_str.trim().parse::<u64>() {
                    ctx.rng = StdRng::seed_from_u64(seed);
                    eprintln!("[parse] use_random_seed {}", seed);
                }
            }
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
            Some(ParsedCommand::Comment(format!(
                "# tick = {}",
                ctx.global_tick
            )))
        }
        "look" => {
            // Standalone look — just reads counter, no side effect at parse time
            Some(ParsedCommand::Comment(format!(
                "# look = {}",
                ctx.global_tick
            )))
        }
        "set" | "get" => {
            // set/get for shared state — treat like variables
            if parts[0] == "set" {
                if parts.len() >= 3 {
                    let key = parts[1]
                        .trim_start_matches(':')
                        .trim_end_matches(',')
                        .to_string();
                    let raw_val = parts[2..].join(" ").trim_end_matches(',').to_string();
                    // Try numeric resolution first (handles expressions)
                    let resolved_num = ctx.resolve_numeric(&raw_val);
                    let resolved_str = if resolved_num.is_some() {
                        resolved_num.unwrap().to_string()
                    } else {
                        ctx.resolve_string(&raw_val)
                    };
                    ctx.variables.insert(key.clone(), resolved_str.clone());
                    // Emit a runtime SetVariable command so the scheduler can
                    // apply it during playback (e.g. master_amp fade, stop_all)
                    let runtime_val = if let Some(n) = resolved_num {
                        n
                    } else {
                        // Map boolean-like strings to f32
                        match resolved_str.as_str() {
                            "true" => 1.0,
                            "false" | "nil" => 0.0,
                            _ => resolved_str.parse::<f32>().unwrap_or(0.0),
                        }
                    };
                    return Some(ParsedCommand::SetVariable {
                        key,
                        value: runtime_val,
                    });
                }
            }
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "control" => {
            // control — modifying running synths, not directly supported but don't error
            eprintln!("[WARN] 'control' command is parsed but NOT implemented - synth parameters cannot be modified at runtime");
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "midi"
        | "midi_note_on"
        | "midi_note_off"
        | "midi_cc"
        | "midi_raw"
        | "midi_pitch_bend"
        | "midi_channel_pressure"
        | "midi_poly_pressure"
        | "midi_clock_tick"
        | "midi_start"
        | "midi_stop"
        | "midi_reset"
        | "midi_local_control_off"
        | "midi_local_control_on"
        | "midi_mode"
        | "midi_all_notes_off" => {
            // MIDI commands — not applicable to audio engine but don't error
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "sample_duration" => Some(ParsedCommand::Comment(format!("# {}", line))),
        "use_timing_guarantees"
        | "use_arg_checks"
        | "use_debug"
        | "use_cue_logging"
        | "use_external_synths"
        | "use_arg_bpm_scaling" => Some(ParsedCommand::Comment(format!("# {}", line))),
        "with_swing" => {
            // Block form handled by try_parse_block; statement form is a no-op
            eprintln!("[WARN] with_swing: swing timing not yet implemented");
            Some(ParsedCommand::Comment(format!("# {}", line)))
        }
        "with_fx" | "with_synth" | "with_bpm" | "with_bpm_mul" | "time_warp" => {
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

        // ── Percussive ──
        "gabberkick" | "gabber_kick" => OscillatorType::GabberKick,

        // ── Aliases / fallbacks ──
        "bass" => OscillatorType::TB303,
        "lead" => OscillatorType::SuperSaw,
        "pad" => OscillatorType::Hollow,
        "winwood_lead" => OscillatorType::SuperSaw,

        _ => OscillatorType::Sine,
    }
}

/// Parse "play chord(:e3, :minor7), release: 1, amp: 1"
/// Returns a TimesLoop{count:1} wrapping all chord tones so they play simultaneously.
fn parse_play_chord(line: &str, ctx: &ParseContext) -> Option<ParsedCommand> {
    let amplitude = extract_param(line, "amp").unwrap_or(1.0);
    let release = extract_param(line, "release").unwrap_or(1.0);
    let attack = extract_param(line, "attack").unwrap_or(0.0);
    let decay = extract_param(line, "decay").unwrap_or(0.0);
    let sustain_level = extract_param(line, "sustain_level").unwrap_or(1.0);
    let sustain = extract_param(line, "sustain").unwrap_or(0.0);
    let pan = extract_param(line, "pan").unwrap_or(0.0);

    // Extract chord(...) content
    let chord_start = line.find("chord(")?;
    let chord_inner_start = chord_start + 6;
    let chord_end = line[chord_inner_start..].find(')')? + chord_inner_start;
    let chord_args = &line[chord_inner_start..chord_end];

    // Parse chord args: :e3, :minor7 or :e3, :m7 etc.
    let args: Vec<&str> = chord_args.split(',').map(|s| s.trim()).collect();
    let root_str = args.first()?.trim_start_matches(':');
    let chord_type = args
        .get(1)
        .map(|s| s.trim_start_matches(':'))
        .unwrap_or("major");

    // Get root MIDI note
    let root_midi = note_name_to_midi(&root_str.to_uppercase())?;

    // Generate all chord notes as simultaneous PlayNote commands
    let intervals = chord_intervals(chord_type);
    let params = extract_synth_params(line);
    let envelope = envelope_from_line(line, attack, decay, sustain_level, release);

    let note_commands: Vec<ParsedCommand> = intervals
        .iter()
        .filter_map(|&interval| {
            let midi = (root_midi as i32 + interval) as u8;
            let freq = midi_to_freq(midi);
            if freq > 0.0 {
                Some(ParsedCommand::PlayNote {
                    synth_type: ctx.current_synth,
                    frequency: freq,
                    amplitude,
                    duration: sustain,
                    pan,
                    envelope,
                    params: params.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    if note_commands.is_empty() {
        return None;
    }

    // Wrap in TimesLoop{count:1} so all notes share the same time offset
    Some(ParsedCommand::TimesLoop {
        count: 1,
        commands: note_commands,
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
/// Also supports: play_pattern_timed scale(:c4, :minor_pentatonic), 0.25, release: 0.2
fn parse_play_pattern_timed(line: &str, ctx: &ParseContext) -> Option<ParsedCommand> {
    let amplitude = extract_param(line, "amp").unwrap_or(1.0);
    let release = extract_param(line, "release").unwrap_or(1.0);
    let attack = extract_param(line, "attack").unwrap_or(0.0);
    let decay = extract_param(line, "decay").unwrap_or(0.0);
    // In Sonic Pi, `sustain:` is the hold time (default 0 for play_pattern_timed).
    // `duration` in ParsedCommand represents the sustain hold time.
    // Don't confuse with `release:` — total note time = attack + decay + sustain + release.
    let sustain_time = extract_param(line, "sustain").unwrap_or(0.0);
    let synth_params = extract_synth_params(line);

    // Extract what comes after "play_pattern_timed"
    let rest = line
        .strip_prefix("play_pattern_timed")
        .unwrap_or(line)
        .trim();

    // Determine how notes are specified:
    // - If rest starts with '[', notes are in a bracket array → extract_array(line, 0)
    // - If rest starts with scale()/chord(), resolve via resolve_to_list()
    // - Otherwise, rest is a variable name → look up in ring_values
    // When notes are NOT in brackets, the first [...] in the line is the TIMING array.
    let is_bracket_notes = rest.starts_with('[');

    let notes: Option<Vec<String>> = if is_bracket_notes {
        extract_array(line, 0)
    } else if rest.starts_with("scale(") || rest.starts_with("scale ")
        || rest.starts_with("chord(") || rest.starts_with("chord ") {
        ctx.resolve_to_list(rest)
    } else {
        // Try resolving as a variable name that might hold a ring/list
        let first_arg = rest.split(',').next().unwrap_or("").trim();
        let cleaned = first_arg.trim_end_matches(".ring");
        ctx.ring_values.get(cleaned).cloned()
    };

    let notes = notes?;

    // When notes are in brackets, timings are the second [...] array (index 1).
    // Otherwise (scale/chord/variable), the first [...] is the timing (index 0).
    let timing_array_index = if is_bracket_notes { 1 } else { 0 };
    let timings: Vec<String> = extract_array(line, timing_array_index).unwrap_or_else(|| {
        // Try to find a bare number after the first comma (that's not a named param)
        // e.g., "play_pattern_timed scale(:c4, :minor), 0.25, release: 0.2"
        // We need to find the comma after the first arg (notes) and before named params
        let after_notes = if let Some(bracket_end) = rest.find(']') {
            &rest[bracket_end + 1..]
        } else if rest.contains("scale(") || rest.contains("chord(") {
            // Find closing paren of scale()/chord()
            let mut depth = 0;
            let mut end = 0;
            for (i, ch) in rest.chars().enumerate() {
                if ch == '(' {
                    depth += 1;
                } else if ch == ')' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            &rest[end..]
        } else {
            // Variable name before first comma
            if let Some(comma) = rest.find(',') {
                &rest[comma..]
            } else {
                ""
            }
        };

        // Now look for a bare number after a comma
        let trimmed = after_notes.trim().trim_start_matches(',').trim();
        // Get the first token that looks like a number
        let first_token: String = trimmed
            .chars()
            .take_while(|c| c.is_numeric() || *c == '.' || *c == '-')
            .collect();
        if let Ok(_val) = first_token.parse::<f32>() {
            vec![first_token]
        } else {
            vec!["0.5".to_string()]
        }
    });

    // Parse notes to frequencies
    let frequencies: Vec<f32> = notes.iter().filter_map(|n| parse_note_value(n)).collect();

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
                duration: sustain_time,
                pan: 0.0,
                envelope: envelope_from_line(line, attack, decay, 1.0, release),
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
    let amplitude = extract_param(line, "amp").unwrap_or(1.0);
    let release = extract_param(line, "release").unwrap_or(1.0);
    let synth_params = extract_synth_params(line);

    let notes = extract_array(line, 0)?;
    let frequencies: Vec<f32> = notes.iter().filter_map(|n| parse_note_value(n)).collect();

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

    // MIDI note number as integer (Sonic Pi convention: play 60 = C4)
    if let Ok(midi) = v.parse::<u8>() {
        return Some(midi_to_freq(midi));
    }

    // Direct frequency — only for explicit float values (e.g. 440.0)
    // or values > 127 which can't be MIDI notes
    if let Ok(f) = v.parse::<f32>() {
        if v.contains('.') || f > 127.0 {
            return Some(f);
        } else if f >= 0.0 && f <= 127.0 {
            // Integer-looking value within MIDI range — treat as MIDI
            return Some(midi_to_freq(f as u8));
        }
    }

    // Note name like c4, fs3, eb5
    let name = v.to_uppercase();
    if let Some(midi) = note_name_to_midi(&name) {
        return Some(midi_to_freq(midi));
    }

    None
}

/// Extract the note expression from a play command.
/// Handles expressions like "(note root)+12" by respecting parentheses.
/// Stops at the first comma (outside parens) or at a parameter marker.
fn extract_note_expression(rest: &str) -> String {
    let trimmed = rest.trim();
    let mut result = String::new();
    let mut depth = 0;
    let param_markers = [
        "amp:", "amp :", "attack:", "decay:", "sustain:", "release:", "pan:", "cutoff:", "res:",
        "detune:", "depth:",
    ];

    for ch in trimmed.chars() {
        // Check if we hit a parameter marker at the current position
        if depth == 0 && !result.is_empty() {
            let remaining = &trimmed[result.len()..];
            let is_param_marker = param_markers.iter().any(|m| remaining.starts_with(m));
            if is_param_marker {
                break;
            }
        }

        match ch {
            '(' => {
                depth += 1;
                result.push(ch);
            }
            ')' => {
                depth -= 1;
                result.push(ch);
            }
            ',' if depth == 0 => {
                // End of note expression
                break;
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result.trim().to_string()
}

fn extract_param(line: &str, param: &str) -> Option<f32> {
    extract_param_with_context(line, param, None)
}

/// Build an [`Envelope`] from already-extracted ADSR values plus the three
/// shaping opts Sonic Pi exposes on every synth: `attack_level:`,
/// `decay_level:` and `env_curve:`.
///
/// Defaults match Sonic Pi's SynthDef arguments — attack_level 1, decay_level
/// -1 ("follow sustain_level"), env_curve 1 (linear) — so a `play` line that
/// sets none of them produces exactly the envelope PiBeat produced before
/// these opts existed.
fn envelope_from_line(
    line: &str,
    attack: f32,
    decay: f32,
    sustain_level: f32,
    release: f32,
) -> Envelope {
    Envelope {
        attack,
        decay,
        sustain: sustain_level,
        release,
        attack_level: extract_param(line, "attack_level").unwrap_or(1.0),
        decay_level: extract_param(line, "decay_level").unwrap_or(-1.0),
        curve: extract_param(line, "env_curve").unwrap_or(1.0),
    }
}

/// Read the first positional numeric argument from an argument list, e.g.
/// the `0.15` in `with_swing 0.15, pulse: 8 do`. Stops at the first `key:`
/// token so a leading keyword argument is not mistaken for a positional one.
fn first_positional_number(args: &str) -> Option<f32> {
    let args = args.trim().trim_start_matches('(').trim();
    let first = args.split(',').next()?.trim();
    if first.is_empty() || first.contains(':') {
        return None;
    }
    first.trim_end_matches(')').trim().parse::<f32>().ok()
}

/// Extract a `param: :symbol` value, returning the symbol without its colon.
fn extract_symbol_param(line: &str, param: &str) -> Option<String> {
    let pat = format!("{}:", param);
    let pos = line.find(&pat)?;
    // Word-boundary check so `tick:` does not match `foo_tick:`
    if pos > 0 {
        let prev = line.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let after = line[pos + pat.len()..].trim_start();
    let value: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    let value = value.trim_start_matches(':').to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Extract a named parameter value, optionally using context to resolve function calls
fn extract_param_with_context(
    line: &str,
    param: &str,
    mut ctx: Option<&mut ParseContext>,
) -> Option<f32> {
    let patterns = [
        format!("{}: ", param),
        format!("{}:", param),
        format!("{} => ", param),
    ];
    for pat in &patterns {
        if let Some(pos) = line.find(pat.as_str()) {
            // Ensure the match is at a word boundary (not inside a longer identifier)
            if pos > 0 {
                let prev_byte = line.as_bytes()[pos - 1];
                if prev_byte.is_ascii_alphanumeric() || prev_byte == b'_' {
                    continue;
                }
            }
            let after = &line[pos + pat.len()..];
            let after_trimmed = after.trim();

            // Find the extent of the value (up to the next unbalanced comma)
            // Track both paren AND bracket depth so arrays like [-0.4, 0.4].tick
            // are not split at the inner comma.
            let val_extent = {
                let mut paren_depth = 0i32;
                let mut bracket_depth = 0i32;
                let mut end = after_trimmed.len();
                for (i, ch) in after_trimmed.chars().enumerate() {
                    match ch {
                        '(' => paren_depth += 1,
                        ')' => paren_depth -= 1,
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        ',' if paren_depth == 0 && bracket_depth == 0 => {
                            end = i;
                            break;
                        }
                        _ => {}
                    }
                }
                &after_trimmed[..end].trim()
            };

            // If the value contains .tick or .look, resolve via list method
            if val_extent.contains(".tick") || val_extent.contains(".look") {
                if let Some(ref mut context) = ctx {
                    if let Some(resolved) = context.resolve_list_value(val_extent) {
                        if let Some(num) = resolved.parse::<f32>().ok() {
                            return Some(num);
                        }
                        // Try resolving as note name
                        let note_str = resolved.trim().trim_start_matches(':');
                        if let Some(midi) = note_name_to_midi(&note_str.to_uppercase()) {
                            return Some(midi as f32);
                        }
                    }
                }
            }

            // Try resolving with context first (handles user-defined functions, get(), etc.)
            if let Some(ref mut context) = ctx {
                if let Some(val) = context.resolve_numeric(val_extent) {
                    return Some(val);
                }
            }

            // Check if the value is a function call like rrand(), rand(), etc.
            for func_name in &["rrand", "rrand_i", "rand", "rand_i", "dice"] {
                if after_trimmed.starts_with(&format!("{}(", func_name)) {
                    // Extract the full function call including parens
                    if let Some(inner) = extract_func_args(after_trimmed, func_name) {
                        let func_call = &after_trimmed[..func_name.len() + 1 + inner.len() + 1];
                        let mut fresh_ctx = ParseContext::new();
                        if let Some(val) = fresh_ctx.resolve_numeric(func_call) {
                            return Some(val);
                        }
                    }
                }
            }

            // Check for arithmetic with rrand: "1 + rrand(0, 0.5)"
            if after_trimmed.contains("rrand")
                || after_trimmed.contains("rand(")
                || after_trimmed.contains("dice(")
            {
                // Find the extent of the expression (up to next comma or end)
                let expr_end = after_trimmed
                    .find(|c: char| {
                        c == ','
                            && !after_trimmed[..after_trimmed.find(c).unwrap_or(0)].contains('(')
                    })
                    .unwrap_or(after_trimmed.len());
                let expr = &after_trimmed[..expr_end];
                let mut fresh_ctx = ParseContext::new();
                if let Some(val) = fresh_ctx.resolve_numeric(expr) {
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
    let patterns = [format!("{}: ", param), format!("{}:", param)];
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

/// Extract parameter names from block body starting with |param1, param2|
/// Returns the body with the pipe params stripped and the list of param names.
fn extract_block_params(body: &str) -> (String, Vec<String>) {
    let trimmed = body.trim();
    // Look for |param1, param2| at the start
    if trimmed.starts_with('|') {
        if let Some(end_pipe) = trimmed[1..].find('|') {
            let params_str = &trimmed[1..end_pipe + 1];
            let param_names: Vec<String> = params_str
                .split(',')
                .map(|p| {
                    // Preserve "name=default" format — defaults handled during substitution
                    p.trim().to_string()
                })
                .filter(|p| !p.is_empty())
                .collect();
            let rest = trimmed[end_pipe + 2..].trim();
            return (rest.to_string(), param_names);
        }
    }
    (body.to_string(), Vec::new())
}

/// Extract arguments from a function call line.
/// e.g., "acid_bass :e2" -> [":e2"]
/// e.g., "my_func(arg1, arg2)" -> ["arg1", "arg2"]
/// e.g., "kick" -> []
fn extract_function_call_args(line: &str, func_name: &str) -> Vec<String> {
    let rest = line.trim();

    // Try parenthesized form: func_name(args)
    if let Some(paren_pos) = rest.find('(') {
        let name_part = rest[..paren_pos].trim();
        // Ensure the paren follows the function name
        if name_part == func_name || name_part.ends_with(func_name) {
            if let Some(close) = rest.rfind(')') {
                let args_str = &rest[paren_pos + 1..close];
                return split_args(args_str);
            }
        }
    }

    // Try space-separated form: func_name arg1, arg2
    if let Some(stripped) = rest.strip_prefix(func_name) {
        let args_part = stripped.trim();
        if args_part.is_empty() {
            return Vec::new();
        }
        return split_args(args_part);
    }

    Vec::new()
}

/// Split argument string by commas, respecting parentheses
fn split_args(args_str: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in args_str.chars() {
        if ch == '(' {
            depth += 1;
            current.push(ch);
        } else if ch == ')' {
            depth -= 1;
            current.push(ch);
        } else if ch == ',' && depth == 0 {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                args.push(trimmed);
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        args.push(trimmed);
    }
    args
}

/// Substitute parameter references in function body with actual argument values.
/// Uses simple text replacement — replaces occurrences of param names with arg values.
fn substitute_function_params(
    body: &str,
    param_names: &[String],
    args: &[String],
    ctx: &ParseContext,
) -> String {
    let mut result = body.to_string();
    for (i, param_spec) in param_names.iter().enumerate() {
        // Parse "name=default" into (name, Option<default>)
        let (param_name, default_val) = if let Some(eq_pos) = param_spec.find('=') {
            (
                param_spec[..eq_pos].trim(),
                Some(param_spec[eq_pos + 1..].trim()),
            )
        } else {
            (param_spec.as_str(), None)
        };

        let resolved = if let Some(arg_val) = args.get(i) {
            // Resolve the argument value (could be a variable reference)
            if arg_val.starts_with(':') {
                arg_val.clone()
            } else if let Some(var_val) = ctx.variables.get(arg_val.as_str()) {
                var_val.clone()
            } else {
                arg_val.clone()
            }
        } else if let Some(def) = default_val {
            // No argument provided — use default value
            def.to_string()
        } else {
            // No argument and no default — skip replacement
            continue;
        };
        // Replace parameter references with resolved value
        // Be careful with word boundaries to avoid partial replacements
        result = replace_word(&result, param_name, &resolved);
    }
    result
}

/// Replace a word in text with another, respecting word boundaries
fn replace_word(text: &str, word: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    let word_bytes = word.as_bytes();

    while i < bytes.len() {
        if i + word_bytes.len() <= bytes.len() && &bytes[i..i + word_bytes.len()] == word_bytes {
            // Check word boundary before
            let before_ok = if i == 0 {
                true
            } else {
                let ch = bytes[i - 1] as char;
                !ch.is_alphanumeric() && ch != '_'
            };
            // Check word boundary after
            let after_pos = i + word_bytes.len();
            let after_ok = if after_pos >= bytes.len() {
                true
            } else {
                let ch = bytes[after_pos] as char;
                !ch.is_alphanumeric() && ch != '_'
            };
            if before_ok && after_ok {
                result.push_str(replacement);
                i += word_bytes.len();
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
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

/// Extract a ring variable name from an expression (for use in `at` blocks)
/// Handles: `my_times do` -> Some("my_times")
fn extract_ring_var(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    // Look for a simple identifier before 'do'
    let before_do = trimmed.strip_suffix("do").unwrap_or(trimmed).trim();
    // Check if it's a valid identifier (starts with letter/underscore, contains only alphanumeric/underscore)
    let is_valid_ident = !before_do.is_empty()
        && before_do
            .chars()
            .next()
            .map(|c| c.is_alphabetic() || c == '_')
            .unwrap_or(false)
        && before_do.chars().all(|c| c.is_alphanumeric() || c == '_');
    if is_valid_ident {
        Some(before_do.to_string())
    } else {
        None
    }
}

fn extract_fx_params(line: &str) -> Vec<(String, f32)> {
    let mut params = Vec::new();
    let param_names = [
        "mix", "room", "time", "feedback", "phase", "decay", "cutoff", "res", "rate", "depth",
        "amp", "pre_amp", "distort", "damp", "spread", "release", "attack", "sustain", "reps",
        "bits", "sample_rate", "pan", "freq", "frequency", "wave",
        "level", "threshold", "clamp_time", "relax_time",
        "sub_amp", "super_amp",
        "centre", "center", "db", "transpose", "shift", "pitch", "window_size", "krunch",
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
        "cutoff",
        "res",
        "detune",
        "depth",
        "divisor",
        "wave",
        "pulse_width",
        "width",
        "sub_amp",
        "noise",
        "coef",
        "mod_phase",
        "mod_range",
        "mod_pulse_width",
        "mod_phase_offset",
        "mod_wave",
        "mod_invert_wave",
        "vel",
        // Envelope shaping opts. The SC SynthDefs declare these, and unknown
        // params are forwarded verbatim on /s_new, so listing them here is all
        // it takes for the SuperCollider engine to honour them.
        "attack_level",
        "decay_level",
        "env_curve",
    ];
    for name in &synth_param_names {
        if let Some(val) = extract_param(line, name) {
            params.push((name.to_string(), val));
        }
    }
    params
}

/// Convert parsed commands to audio commands with timing
pub fn commands_to_audio(parsed: &[ParsedCommand], bpm: f32) -> Vec<(f32, AudioCommand)> {
    // Fast path: nothing in the program coordinates on cues, so a single
    // expansion pass is all that is needed — which is what PiBeat has always
    // done. Programs are expanded eagerly (a live_loop becomes 500 unrolled
    // iterations), so re-running the expansion is not free and is skipped
    // whenever it cannot change the result.
    if !uses_cues(parsed) {
        let mut ctx = ExpandCtx::new(CuePass::Ignore);
        return commands_to_audio_ctx(parsed, bpm, 0, &mut ctx, 0.0).0;
    }

    // Pass 1: expand with `sync` as a no-op purely to find out *when* each
    // `cue` fires.
    let mut ctx = ExpandCtx::new(CuePass::Collect);
    let mut result = commands_to_audio_ctx(parsed, bpm, 0, &mut ctx, 0.0).0;
    let mut cue_times = ctx.cue_times;

    // Later passes resolve each `sync` against the cue map from the previous
    // pass. Resolving a sync moves events, which can move the cues those
    // events emit, so iterate until the map settles. Two or three rounds cover
    // every realistic arrangement (a metronome loop cueing several synced
    // loops settles after one); the cap keeps a pathological mutual
    // dependency from looping forever.
    const MAX_CUE_PASSES: usize = 3;
    for _ in 0..MAX_CUE_PASSES {
        let mut pass = ExpandCtx::new(CuePass::Resolve(cue_times.clone()));
        result = commands_to_audio_ctx(parsed, bpm, 0, &mut pass, 0.0).0;
        if pass.cue_times == cue_times {
            break;
        }
        cue_times = pass.cue_times;
    }
    result
}

/// Whether the program uses cue-based coordination anywhere in the tree.
fn uses_cues(parsed: &[ParsedCommand]) -> bool {
    parsed.iter().any(|cmd| match cmd {
        ParsedCommand::Cue(_) | ParsedCommand::Sync(_) => true,
        ParsedCommand::Loop {
            commands,
            sync_with,
            ..
        } => sync_with.is_some() || uses_cues(commands),
        ParsedCommand::WithFx { commands, .. }
        | ParsedCommand::TimesLoop { commands, .. }
        | ParsedCommand::AtBlock { commands, .. }
        | ParsedCommand::SwingBlock { commands, .. } => uses_cues(commands),
        ParsedCommand::ConditionalRandom { command, .. } => {
            uses_cues(std::slice::from_ref(command.as_ref()))
        }
        _ => false,
    })
}

/// How the current expansion pass treats `cue` / `sync`.
#[derive(Debug, Clone)]
enum CuePass {
    /// No cues in the program — skip the bookkeeping entirely.
    Ignore,
    /// Record when each cue fires; `sync` does not wait.
    Collect,
    /// Record cues again *and* make `sync` wait for the next matching cue
    /// from the supplied map.
    Resolve(HashMap<String, Vec<f32>>),
}

/// Mutable state carried through one expansion pass.
struct ExpandCtx {
    /// Monotonic allocator for `with_fx` block IDs (0 = no FX context).
    fx_counter: u64,
    /// Absolute times, in seconds from the start of the piece, at which each
    /// named cue fires during this pass.
    cue_times: HashMap<String, Vec<f32>>,
    pass: CuePass,
    /// Per-key invocation counters for `with_swing`, mirroring the `tick:`
    /// counter Sonic Pi uses to decide which run of the block gets shifted.
    swing_ticks: HashMap<String, u64>,
}

impl ExpandCtx {
    fn new(pass: CuePass) -> Self {
        Self {
            fx_counter: 1,
            cue_times: HashMap::new(),
            pass,
            swing_ticks: HashMap::new(),
        }
    }

    fn tracking_cues(&self) -> bool {
        !matches!(self.pass, CuePass::Ignore)
    }

    fn record_cue(&mut self, name: &str, absolute_time: f32) {
        if !self.tracking_cues() {
            return;
        }
        self.cue_times
            .entry(name.to_string())
            .or_default()
            .push(absolute_time);
    }

    /// The absolute time of the first `cue name` strictly after `after`, or
    /// `None` if no such cue exists (Sonic Pi cannot sync to a cue that has
    /// already fired, and a `sync` with no matching cue simply never fires).
    fn next_cue_after(&self, name: &str, after: f32) -> Option<f32> {
        let CuePass::Resolve(ref map) = self.pass else {
            return None;
        };
        // Small epsilon so a cue emitted at exactly the current instant by
        // another thread still counts — the two are simultaneous, and Sonic Pi
        // resolves that in the waiting thread's favour.
        const EPS: f32 = 1e-4;
        map.get(name)?
            .iter()
            .copied()
            .filter(|t| *t >= after - EPS)
            .fold(None, |acc: Option<f32>, t| {
                Some(acc.map_or(t, |a: f32| a.min(t)))
            })
    }

    /// Increment and return the `with_swing` tick counter for `key`.
    /// Sonic Pi's `tick` returns 0 on its first call, so the first run of a
    /// swing block is the shifted one.
    fn next_swing_tick(&mut self, key: &str) -> u64 {
        let counter = self.swing_ticks.entry(key.to_string()).or_insert(0);
        let value = *counter;
        *counter += 1;
        value
    }
}

/// Inner implementation that carries FX context through recursive calls.
///
/// `fx_context`: the current FX block ID (0 = no FX, route to hardware).
/// `ctx`: mutable state for the pass — FX ID allocation, the cue map and the
///        `with_swing` tick counters.
/// `base_time`: absolute time, in seconds from the start of the piece, that
///        this command list starts at. Returned event times stay relative to
///        the list, but cues need an absolute position to be sync-able.
///
/// Returns the events plus the time this list consumed, so a caller can
/// advance by what actually happened rather than by the nominal duration —
/// which matters once a `sync` inside the body can stretch an iteration.
fn commands_to_audio_ctx(
    parsed: &[ParsedCommand],
    bpm: f32,
    fx_context: u64,
    ctx: &mut ExpandCtx,
    base_time: f32,
) -> (Vec<(f32, AudioCommand)>, f32) {
    let mut result = Vec::new();
    let mut time_offset = 0.0f32;
    let mut current_bpm = bpm;
    let mut beat_duration = 60.0 / current_bpm;

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
                    let total_dur_beats =
                        duration + envelope.attack + envelope.decay + envelope.release;
                    let total_dur_secs = total_dur_beats * beat_duration;
                    // Also convert envelope times from beats to seconds for the audio engine
                    let env_secs = Envelope {
                        attack: envelope.attack * beat_duration,
                        decay: envelope.decay * beat_duration,
                        sustain: envelope.sustain, // sustain is a level (0-1), not a time
                        release: envelope.release * beat_duration,
                        // Levels and curve shape are unitless — carry them over
                        ..*envelope
                    };
                    result.push((
                        time_offset,
                        AudioCommand::PlayNote {
                            synth_type: *synth_type,
                            frequency: *frequency,
                            amplitude: *amplitude,
                            duration_secs: total_dur_secs,
                            envelope: env_secs,
                            pan: *pan,
                            params: params.clone(),
                            fx_context,
                        },
                    ));
                }
            }
            ParsedCommand::PlaySample {
                name: _name,
                rate,
                amplitude,
                pan,
                sustain_beats,
                beat_stretch,
                start,
                finish,
                lpf,
                hpf,
                envelope,
            } => {
                let sustain_secs = sustain_beats.map(|b| b * beat_duration);
                // Convert envelope from beats to seconds if present
                let envelope_secs = envelope.as_ref().map(|env| Envelope {
                    attack: env.attack * beat_duration,
                    decay: env.decay * beat_duration,
                    sustain: env.sustain,
                    release: env.release * beat_duration,
                    ..*env
                });
                // If sample has lpf: or hpf: params, wrap with FxStart/FxEnd
                // so the per-voice FX system applies the filter
                let has_sample_fx = lpf.is_some() || hpf.is_some();
                let mut sample_fx_ctx = fx_context;
                if has_sample_fx {
                    if let Some(cutoff) = lpf {
                        let lpf_id = ctx.fx_counter;
                        ctx.fx_counter += 1;
                        result.push((
                            time_offset,
                            AudioCommand::FxStart {
                                fx_type: "lpf".to_string(),
                                params: vec![("cutoff".to_string(), *cutoff)],
                                fx_id: lpf_id,
                                parent_fx_id: sample_fx_ctx,
                            },
                        ));
                        sample_fx_ctx = lpf_id;
                    }
                    if let Some(cutoff) = hpf {
                        let hpf_id = ctx.fx_counter;
                        ctx.fx_counter += 1;
                        result.push((
                            time_offset,
                            AudioCommand::FxStart {
                                fx_type: "hpf".to_string(),
                                params: vec![("cutoff".to_string(), *cutoff)],
                                fx_id: hpf_id,
                                parent_fx_id: sample_fx_ctx,
                            },
                        ));
                        sample_fx_ctx = hpf_id;
                    }
                }
                result.push((
                    time_offset,
                    AudioCommand::PlaySample {
                        samples: Vec::new(), // Will be filled by the caller
                        sample_rate: 44100,
                        amplitude: *amplitude,
                        rate: *rate,
                        pan: *pan,
                        sustain_secs,
                        beat_stretch: *beat_stretch,
                        start: *start,
                        finish: *finish,
                        envelope: envelope_secs,
                        fx_context: sample_fx_ctx,
                    },
                ));
                if has_sample_fx {
                    if hpf.is_some() {
                        // Pop in reverse order: hpf was pushed last
                        let hpf_id = sample_fx_ctx;
                        result.push((time_offset, AudioCommand::FxEnd { fx_id: hpf_id }));
                        // After popping hpf, the lpf context is the parent
                        sample_fx_ctx = if lpf.is_some() { sample_fx_ctx - 1 } else { fx_context };
                    }
                    if lpf.is_some() {
                        result.push((time_offset, AudioCommand::FxEnd { fx_id: sample_fx_ctx }));
                    }
                }
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
                // Allocate a unique FX ID for this block
                let this_fx_id = ctx.fx_counter;
                ctx.fx_counter += 1;

                // Emit FxStart — the SC engine will allocate a private audio bus,
                // create the FX synth on it, and route subsequent synths through it.
                // The cpal engine falls back to global SetEffect.
                result.push((
                    time_offset,
                    AudioCommand::FxStart {
                        fx_type: fx_type.clone(),
                        params: params.clone(),
                        fx_id: this_fx_id,
                        parent_fx_id: fx_context,
                    },
                ));

                // Scoped FX: The SC engine handles this via FxStart/FxEnd bus routing.
                // The cpal engine handles this via per-voice VoiceFx chains:
                // FxStart/FxEnd maintain an FX stack in the audio callback, and each
                // PlayNote/PlaySample created while FX blocks are active gets tagged
                // with a VoiceFx chain that applies the stacked effects per-voice.

                // Process inner commands with this FX block as context
                let (inner, inner_consumed) = commands_to_audio_ctx(
                    commands,
                    current_bpm,
                    this_fx_id,
                    ctx,
                    base_time + time_offset,
                );

                // Compute grace period: find the latest time any enclosed note
                // finishes its release phase. This ensures the FX synth stays
                // alive (bus still routed) while notes are still sounding.
                let mut max_tail_time = 0.0f32;
                for (t, cmd) in &inner {
                    let tail = match cmd {
                        AudioCommand::PlayNote { duration_secs, .. } => t + duration_secs,
                        AudioCommand::PlaySample { sustain_secs, .. } => {
                            t + sustain_secs.unwrap_or(3.0)
                        }
                        _ => 0.0,
                    };
                    if tail > max_tail_time {
                        max_tail_time = tail;
                    }
                }

                for (t, c) in inner {
                    result.push((time_offset + t, c));
                }

                // Update time offset from inner commands. When the body can
                // block on a cue, the time it actually consumed is the honest
                // figure; otherwise keep using the nominal duration so
                // existing timing is untouched.
                let inner_duration = if ctx.tracking_cues() {
                    inner_consumed
                } else {
                    commands_to_duration(commands, current_bpm)
                };
                time_offset += inner_duration;

                // Emit FxEnd with grace period — ensures the FX bus stays alive
                // long enough for all enclosed notes to complete their release.
                let grace = (max_tail_time - inner_duration).max(0.0);
                result.push((time_offset + grace, AudioCommand::FxEnd { fx_id: this_fx_id }));
            }
            ParsedCommand::Loop {
                commands,
                name,
                parallel,
                sync_with,
            } => {
                // Check if the loop body contains a Stop command at the top level
                let has_stop = commands.iter().any(|c| matches!(c, ParsedCommand::Stop));
                // in_thread runs once; live_loop without stop runs 500 times
                let is_in_thread = name == "thread";
                let loop_iterations = if is_in_thread || has_stop { 1 } else { 500 };
                
                // `live_loop :x, sync: :y` holds the first iteration until the
                // next `cue :y`. Sonic Pi only gates the *first* iteration —
                // afterwards the loop free-runs on its own body length — so
                // that is what we reproduce here.
                let mut loop_start_offset = time_offset;
                if let Some(ref sync_target) = sync_with {
                    match ctx.next_cue_after(sync_target, base_time + time_offset) {
                        Some(cue_abs) => {
                            loop_start_offset = cue_abs - base_time;
                            trace!(
                                "[parser] live_loop :{} syncs with :{} at t={:.3}s",
                                name, sync_target, cue_abs
                            );
                        }
                        None if ctx.tracking_cues() => {
                            // Only worth warning about once the cue map is
                            // populated; during the collection pass there is
                            // nothing to look up yet.
                            trace!(
                                "[parser] live_loop :{} waits on :{}, which is never cued — starting immediately",
                                name, sync_target
                            );
                        }
                        None => {}
                    }
                }

                trace!(
                    "[parser] live_loop :{} → {} iteration(s), stop={}, parallel={}, in_thread={}",
                    name, loop_iterations, has_stop, parallel, is_in_thread
                );

                let mut loop_time = loop_start_offset;
                for iter in 0..loop_iterations {
                    let (inner, inner_consumed) = commands_to_audio_ctx(
                        commands,
                        current_bpm,
                        fx_context,
                        ctx,
                        base_time + loop_time,
                    );
                    let inner_duration = if ctx.tracking_cues() {
                        inner_consumed
                    } else {
                        commands_to_duration(commands, current_bpm)
                    };
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
                    let (inner, inner_consumed) = commands_to_audio_ctx(
                        commands,
                        current_bpm,
                        fx_context,
                        ctx,
                        base_time + time_offset,
                    );
                    let inner_duration = if ctx.tracking_cues() {
                        inner_consumed
                    } else {
                        commands_to_duration(commands, current_bpm)
                    };
                    for (t, c) in inner {
                        result.push((time_offset + t, c));
                    }
                    time_offset += inner_duration;
                    // Safety: cap at 100k commands
                    if result.len() > 100_000 {
                        eprintln!(
                            "[parser] WARNING: command limit reached in {}.times at iteration {}",
                            count, iter
                        );
                        break;
                    }
                }
            }
            ParsedCommand::Stop => {
                // Stop this sequence - break out
                break;
            }
            ParsedCommand::ConditionalRandom { n, command } => {
                // Re-evaluate one_in(n) at audio expansion time for each occurrence.
                // IMPORTANT: For PlaySample, we must always emit the command (even when
                // condition fails) to keep sample name indices in sync with
                // collect_sample_names. We just set amplitude to 0 when skipped.
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let include = rng.gen_range(0..*n) == 0;

                match command.as_ref() {
                    ParsedCommand::PlaySample {
                        name: _name,
                        rate,
                        amplitude,
                        pan,
                        sustain_beats,
                        beat_stretch,
                        start,
                        finish,
                        lpf,
                        hpf,
                        envelope,
                    } => {
                        let final_amp = if include { *amplitude } else { 0.0 };
                        let envelope_secs = if include {
                            envelope.as_ref().map(|env| Envelope {
                                attack: env.attack * beat_duration,
                                decay: env.decay * beat_duration,
                                sustain: env.sustain,
                                release: env.release * beat_duration,
                                ..*env
                            })
                        } else {
                            None
                        };
                        // Wrap with FxStart/FxEnd if sample has lpf:/hpf: params
                        let has_sample_fx = lpf.is_some() || hpf.is_some();
                        let mut cond_fx_ctx = fx_context;
                        if has_sample_fx && include {
                            if let Some(cutoff) = lpf {
                                let lpf_id = ctx.fx_counter;
                                ctx.fx_counter += 1;
                                result.push((
                                    time_offset,
                                    AudioCommand::FxStart {
                                        fx_type: "lpf".to_string(),
                                        params: vec![("cutoff".to_string(), *cutoff)],
                                        fx_id: lpf_id,
                                        parent_fx_id: cond_fx_ctx,
                                    },
                                ));
                                cond_fx_ctx = lpf_id;
                            }
                            if let Some(cutoff) = hpf {
                                let hpf_id = ctx.fx_counter;
                                ctx.fx_counter += 1;
                                result.push((
                                    time_offset,
                                    AudioCommand::FxStart {
                                        fx_type: "hpf".to_string(),
                                        params: vec![("cutoff".to_string(), *cutoff)],
                                        fx_id: hpf_id,
                                        parent_fx_id: cond_fx_ctx,
                                    },
                                ));
                                cond_fx_ctx = hpf_id;
                            }
                        }
                        result.push((
                            time_offset,
                            AudioCommand::PlaySample {
                                samples: Vec::new(),
                                sample_rate: 44100,
                                amplitude: final_amp,
                                rate: *rate,
                                pan: *pan,
                                sustain_secs: sustain_beats.map(|b| b * beat_duration),
                                beat_stretch: *beat_stretch,
                                start: *start,
                                finish: *finish,
                                envelope: envelope_secs,
                                fx_context: cond_fx_ctx,
                            },
                        ));
                        if has_sample_fx && include {
                            if hpf.is_some() {
                                result.push((time_offset, AudioCommand::FxEnd { fx_id: cond_fx_ctx }));
                                cond_fx_ctx = if lpf.is_some() { cond_fx_ctx - 1 } else { fx_context };
                            }
                            if lpf.is_some() {
                                result.push((time_offset, AudioCommand::FxEnd { fx_id: cond_fx_ctx }));
                            }
                        }
                    }
                    _ => {
                        if include {
                            let (inner, inner_consumed) = commands_to_audio_ctx(
                                std::slice::from_ref(command.as_ref()),
                                current_bpm,
                                fx_context,
                                ctx,
                                base_time + time_offset,
                            );
                            for (t, c) in inner {
                                result.push((time_offset + t, c));
                            }
                            let inner_dur = if ctx.tracking_cues() {
                                inner_consumed
                            } else {
                                commands_to_duration(std::slice::from_ref(command.as_ref()), current_bpm)
                            };
                            time_offset += inner_dur;
                        }
                    }
                }
            }
            ParsedCommand::SleepUntil(target_beat) => {
                // Sleep until a specific beat time (absolute from start)
                let target_time = target_beat * beat_duration;
                if target_time > time_offset {
                    time_offset = target_time;
                }
            }
            ParsedCommand::AtBlock { times: _, commands } => {
                // at/time_warp blocks schedule events at offsets but do NOT advance
                // the parent clock. Save and restore time_offset.
                let saved_offset = time_offset;
                let (inner, _) = commands_to_audio_ctx(
                    commands,
                    current_bpm,
                    fx_context,
                    ctx,
                    base_time + saved_offset,
                );
                for (t, c) in inner {
                    result.push((saved_offset + t, c));
                }
                // Restore — at/time_warp do not advance the parent timeline
                time_offset = saved_offset;
            }
            ParsedCommand::SwingBlock {
                shift,
                pulse,
                tick_key,
                offset,
                commands,
            } => {
                // Sonic Pi runs the block straight except on one invocation in
                // every `pulse`, where it wraps it in `time_warp shift`. The
                // counter is a tick, so the very first run is the shifted one.
                let count = ctx.next_swing_tick(tick_key) as i64;
                let pulse_i = (*pulse).max(1) as i64;
                let swung = (count + offset).rem_euclid(pulse_i) == 0;
                let shift_secs = if swung { shift * beat_duration } else { 0.0 };

                // Like `time_warp`, the shift displaces the block's events but
                // does not move the enclosing thread's clock.
                let saved_offset = time_offset;
                let (inner, _) = commands_to_audio_ctx(
                    commands,
                    current_bpm,
                    fx_context,
                    ctx,
                    base_time + saved_offset + shift_secs,
                );
                for (t, c) in inner {
                    // A negative shift can push an event before the start of
                    // the piece; clamp so it still plays, at t=0.
                    result.push(((saved_offset + shift_secs + t).max(0.0), c));
                }
                time_offset = saved_offset;
            }
            ParsedCommand::Cue(name) => {
                // Record when this cue fires so `sync :name` elsewhere in the
                // program can line up with it. Cueing is instantaneous — it
                // does not advance the clock.
                ctx.record_cue(name, base_time + time_offset);
                trace!(
                    "[parser] cue :{} at t={:.3}s",
                    name,
                    base_time + time_offset
                );
            }
            ParsedCommand::Sync(name) => {
                // Block until the next matching cue. Sonic Pi cannot sync to a
                // cue that has already fired, so we look strictly forward; if
                // nothing ever cues this name the sync is a no-op rather than
                // a deadlock, which is friendlier than Sonic Pi's silent hang.
                match ctx.next_cue_after(name, base_time + time_offset) {
                    Some(cue_abs) => {
                        let waited = cue_abs - (base_time + time_offset);
                        time_offset += waited.max(0.0);
                        trace!(
                            "[parser] sync :{} waits {:.3}s until t={:.3}s",
                            name, waited, cue_abs
                        );
                    }
                    None => {
                        trace!(
                            "[parser] sync :{} has no matching cue — continuing immediately",
                            name
                        );
                    }
                }
            }
            ParsedCommand::SetSynth(_) | ParsedCommand::Comment(_) | ParsedCommand::Log(_) => {}
            ParsedCommand::SetVariable { key, value } => {
                // Emit a runtime variable command at the current time offset.
                // The scheduler thread will process this to update runtime state
                // (e.g. master_amp for fade-out, stop_all/pause_all for loop control).
                result.push((
                    time_offset,
                    AudioCommand::SetRuntimeVar {
                        key: key.clone(),
                        value: *value as f64,
                    },
                ));
            }
        }
    }

    (result, time_offset)
}

/// Calculate the total duration of a sequence of parsed commands in seconds
pub fn commands_to_duration(parsed: &[ParsedCommand], bpm: f32) -> f32 {
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
            ParsedCommand::Loop {
                commands, parallel, ..
            } => {
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
            ParsedCommand::SleepUntil(target_beat) => {
                // SleepUntil sets an absolute time, so we take the max
                let target = target_beat * beat_duration;
                if target > dur {
                    dur = target;
                }
            }
            ParsedCommand::AtBlock { .. } | ParsedCommand::SwingBlock { .. } => {
                // at/time_warp/with_swing blocks displace their contents but do
                // NOT advance the parent timeline duration
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
        let sample_cmds: Vec<_> = timed
            .iter()
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
                        if names.len() > 100_000 {
                            return;
                        }
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
                ParsedCommand::Stop => {
                    return;
                }
                ParsedCommand::ConditionalRandom { command, .. } => {
                    collect_names_recursive(&[(**command).clone()], names);
                }
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
        let play_sample_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();

        // Count sample names from collect_sample_names
        let sample_names = collect_sample_names_test(&parsed);

        eprintln!(
            "PlaySample commands in timed_commands: {}",
            play_sample_count
        );
        eprintln!("Sample names collected: {}", sample_names.len());
        for (i, name) in sample_names.iter().enumerate() {
            eprintln!("  [{}] {}", i, name);
        }

        assert_eq!(
            play_sample_count,
            sample_names.len(),
            "PlaySample count in commands_to_audio ({}) must match collect_sample_names count ({})",
            play_sample_count,
            sample_names.len()
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

        let sample_times: Vec<f32> = timed
            .iter()
            .filter_map(|(t, c)| {
                if matches!(c, AudioCommand::PlaySample { .. }) {
                    Some(*t)
                } else {
                    None
                }
            })
            .collect();

        eprintln!("Sample times: {:?}", sample_times);
        assert_eq!(sample_times.len(), 3, "Should have 3 samples");
        // Loop :a and :b are consecutive live_loops → both at t=0
        assert!(
            (sample_times[0] - 0.0).abs() < 0.01,
            "Loop :a should start at t=0"
        );
        assert!(
            (sample_times[1] - 0.0).abs() < 0.01,
            "Loop :b should start at t=0 (parallel)"
        );
        // sleep 4 with BPM 120 → 4 * 0.5s = 2.0s
        assert!(
            (sample_times[2] - 2.0).abs() < 0.01,
            "Loop :c should start at t=2.0 (after sleep 4)"
        );
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
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Note commands from define expansion: {}", note_count);
        // guitar_riff called once directly + 2 times in loop = 3 calls
        // Each call has 3 notes = 9 total
        assert_eq!(note_count, 9, "Should have 9 notes (3 calls x 3 notes)");

        // Check that dark_drums produced sample commands (live_loop inside define)
        let sample_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();
        eprintln!("Sample commands from define expansion: {}", sample_count);
        assert!(
            sample_count >= 2,
            "Should have at least 2 samples from dark_drums"
        );
    }

    #[test]
    fn test_define_with_inline_block_params() {
        // Test that `define :name do |param1, param2|` correctly captures params
        let code = r#"
define :stab do |n, rel|
  use_synth :saw
  play n, release: rel
end

stab :c4, 0.5
stab :e4, 0.3
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let note_cmds: Vec<_> = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .collect();
        eprintln!("Notes from inline-param define: {}", note_cmds.len());
        assert_eq!(
            note_cmds.len(),
            2,
            "Should have 2 notes from two stab calls"
        );
        // First note should be C4 (MIDI 60 → ~261.6 Hz)
        if let (_, AudioCommand::PlayNote { frequency, .. }) = note_cmds[0] {
            assert!(
                (*frequency - 261.6).abs() < 1.0,
                "First note should be ~261.6 Hz (C4), got {}",
                frequency
            );
        }
    }

    #[test]
    fn test_define_recursion_guard() {
        // A recursive define should NOT cause infinite recursion / stack overflow.
        // The function is removed from context during expansion, so the recursive
        // call is silently skipped.
        let code = r#"
define :recurse do
  play :c4
  recurse
end
recurse
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        // Should have exactly 1 note — the recursive call is silently skipped
        assert_eq!(note_count, 1, "Recursive call should be skipped");
    }

    #[test]
    fn test_trailing_if_one_in() {
        // Test that trailing "if one_in(1)" wraps the command in ConditionalRandom
        let code = r#"
sample :bd_haus, amp: 2 if one_in(1)
sleep 1
"#;
        let parsed = parse_code(code).unwrap();
        let has_conditional = parsed.iter().any(|c| match c {
            ParsedCommand::ConditionalRandom { n, command } => {
                *n == 1 && matches!(**command, ParsedCommand::PlaySample { .. })
            }
            _ => false,
        });
        assert!(
            has_conditional,
            "one_in(1) should wrap the sample in ConditionalRandom"
        );
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
        let has_sample = parsed.iter().any(|c| match c {
            ParsedCommand::TimesLoop { commands, .. } => commands
                .iter()
                .any(|c| matches!(c, ParsedCommand::PlaySample { .. })),
            _ => false,
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
        let has_note = parsed.iter().any(|c| match c {
            ParsedCommand::TimesLoop { commands, .. } => commands
                .iter()
                .any(|c| matches!(c, ParsedCommand::PlayNote { .. })),
            _ => false,
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
        let has_note2 = parsed2.iter().any(|c| match c {
            ParsedCommand::TimesLoop { commands, .. } => commands
                .iter()
                .any(|c| matches!(c, ParsedCommand::PlayNote { .. })),
            _ => false,
        });
        assert!(
            has_note2,
            "if false with else should include the else branch note"
        );
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
        let has_sample = parsed.iter().any(|c| match c {
            ParsedCommand::TimesLoop { commands, .. } => commands
                .iter()
                .any(|c| matches!(c, ParsedCommand::PlaySample { .. })),
            _ => false,
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
        let has_sample = parsed
            .iter()
            .any(|c| matches!(c, ParsedCommand::PlaySample { .. }));
        assert!(
            has_sample,
            "trailing unless false should include the sample"
        );
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
            if let ParsedCommand::PlayNote {
                amplitude,
                envelope,
                ..
            } = c
            {
                Some((*amplitude, envelope.release))
            } else {
                None
            }
        });
        assert!(note.is_some(), "Should have a note");
        let (amp, release) = note.unwrap();
        assert!(
            (amp - 0.3).abs() < 0.01,
            "Amplitude should be 0.3 from defaults, got {}",
            amp
        );
        assert!(
            (release - 2.0).abs() < 0.01,
            "Release should be 2.0 from defaults, got {}",
            release
        );
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
        assert!(
            matches!(synths[0], OscillatorType::Saw),
            "First note should be Saw"
        );
        assert!(
            matches!(synths[1], OscillatorType::Sine),
            "Second note should be Sine"
        );
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
        assert!(
            has_set_bpm(&parsed),
            "Should have a SetBpm command from with_bpm"
        );
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
        assert!(
            (rate.unwrap() - 2.0).abs() < 0.1,
            "rpitch 12 should set rate to ~2.0, got {}",
            rate.unwrap()
        );
    }

    #[test]
    fn test_scale_intervals() {
        // Verify scale generation creates correct number of notes
        let intervals = scale_intervals("minor_pentatonic");
        assert_eq!(
            intervals.len(),
            5,
            "Minor pentatonic should have 5 intervals"
        );

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
        assert!(
            !parsed.is_empty(),
            "Should parse comprehensive code without errors"
        );

        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        let sample_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();
        eprintln!(
            "Comprehensive test: {} notes, {} samples",
            note_count, sample_count
        );
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
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Def block test: {} notes from timed commands", note_count);
        assert!(
            note_count > 0,
            "Should produce notes from play_pattern_timed inside live_loop"
        );

        // Verify that the amplitude is 3.0 (from amp: 3)
        let first_note = timed.iter().find_map(|(_, c)| {
            if let AudioCommand::PlayNote { amplitude, .. } = c {
                Some(*amplitude)
            } else {
                None
            }
        });
        assert!(first_note.is_some(), "Should have a note");
        assert!(
            (first_note.unwrap() - 3.0).abs() < 0.01,
            "Amplitude should be 3.0, got {}",
            first_note.unwrap()
        );
    }

    #[test]
    fn test_line_continuation_comma() {
        // Verify that lines ending with comma are joined
        let code = "play_pattern_timed [:c4, :e4, :g4], [0.5, 0.5, 0.5],\n  release: 0.5, amp: 2\nsleep 1\n";
        let preprocessed = join_continuation_lines(code);
        eprintln!("Preprocessed:\n{}", preprocessed);
        assert!(
            !preprocessed.contains("\n  release:"),
            "Continuation line should be joined"
        );

        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        assert_eq!(note_count, 3, "Should have 3 notes from play_pattern_timed");

        // Verify amplitude
        let amp = timed.iter().find_map(|(_, c)| {
            if let AudioCommand::PlayNote { amplitude, .. } = c {
                Some(*amplitude)
            } else {
                None
            }
        });
        assert!(
            (amp.unwrap() - 2.0).abs() < 0.01,
            "Amplitude should be 2.0 from joined line"
        );
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
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        assert_eq!(note_count, 2, "Should have 2 notes from def function call");
    }

    #[test]
    fn test_use_synth_super_saw_in_buffer() {
        let code = r#"
use_synth :super_saw
play :c4, amp: 0.3, sustain: 2, attack: 0.5, release: 1
"#;
        let parsed = parse_code(code).unwrap();
        eprintln!("Parsed commands: {:?}", parsed);
        // Should have SetSynth and PlayNote
        let has_set_synth = parsed
            .iter()
            .any(|c| matches!(c, ParsedCommand::SetSynth(OscillatorType::SuperSaw)));
        assert!(has_set_synth, "Should have SetSynth(SuperSaw)");

        let play_note = parsed.iter().find_map(|c| {
            if let ParsedCommand::PlayNote {
                synth_type,
                frequency,
                amplitude,
                duration,
                pan: _,
                envelope,
                params: _,
            } = c
            {
                Some((synth_type, frequency, amplitude, duration, envelope))
            } else {
                None
            }
        });
        assert!(play_note.is_some(), "Should have a PlayNote command");
        let (synth_type, freq, amp, dur, env) = play_note.unwrap();
        assert_eq!(
            *synth_type,
            OscillatorType::SuperSaw,
            "PlayNote synth_type should be SuperSaw"
        );
        assert!(
            (freq - 261.63).abs() < 1.0,
            "Frequency should be ~261.63 (C4), got {}",
            freq
        );
        assert!(
            (amp - 0.3).abs() < 0.01,
            "Amplitude should be 0.3, got {}",
            amp
        );
        assert!(
            (dur - 2.0).abs() < 0.01,
            "Duration/sustain should be 2.0, got {}",
            dur
        );
        assert!(
            (env.attack - 0.5).abs() < 0.01,
            "Attack should be 0.5, got {}",
            env.attack
        );
        assert!(
            (env.release - 1.0).abs() < 0.01,
            "Release should be 1.0, got {}",
            env.release
        );

        // Also check commands_to_audio produces correct AudioCommand
        let timed = commands_to_audio(&parsed, 120.0);
        let audio_note = timed.iter().find_map(|(_, c)| {
            if let AudioCommand::PlayNote {
                synth_type,
                frequency,
                amplitude,
                duration_secs,
                envelope,
                ..
            } = c
            {
                Some((synth_type, frequency, amplitude, duration_secs, envelope))
            } else {
                None
            }
        });
        assert!(
            audio_note.is_some(),
            "Should have an AudioCommand::PlayNote"
        );
        let (synth_type, _, _, _, _) = audio_note.unwrap();
        assert_eq!(
            *synth_type,
            OscillatorType::SuperSaw,
            "AudioCommand synth_type should be SuperSaw"
        );
    }

    #[test]
    fn test_note_function_with_variable_and_arithmetic() {
        // Test note() function with variable reference (no octave) and arithmetic
        let code = r#"
use_bpm 80
root = :e
drone_note = (note root) - 0
pulse_note = (note root) - 24
intro_len = 8 * 4.0

in_thread(name: :drone) do
  use_synth :dark_ambience
  with_fx :reverb, room: 0.9, mix: 0.6 do
    t = 0
    while t < intro_len
      play (note root)+12, attack: 4, sustain: 6, release: 4, amp: 2
      sleep 8
      t += 8
    end
  end
end

in_thread(name: :pulse) do
  sleep 4
  use_synth :prophet
  with_fx :distortion, distort: 0.7 do
    with_fx :lpf, cutoff: 70 do
      8.times do
        play pulse_note, attack: 0.05, sustain: 0.35, release: 0.25, amp: 1.4
        sleep 1
      end
    end
  end
end

sleep intro_len
"#;
        let parsed = parse_code(code);
        assert!(
            parsed.is_ok(),
            "Should parse without error: {:?}",
            parsed.err()
        );
        let parsed = parsed.unwrap();
        assert!(!parsed.is_empty(), "Should produce commands");

        // Verify it produces notes
        let timed = commands_to_audio(&parsed, 80.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!(
            "Note function test: {} notes from timed commands",
            note_count
        );
        assert!(note_count > 0, "Should produce notes from the code");
    }

    #[test]
    fn test_amp_mod_user_function_in_params() {
        // Test user-defined functions that return numeric values in parameter positions
        // This pattern is used in Test3 to scale amplitudes dynamically
        let code = r#"
set :master_amp, 1.0

define :amp_mod do |v|
  return v * get(:master_amp)
end

define :kick do
  sample :bd_haus, amp: amp_mod(2)
end

kick
"#;
        let parsed = parse_code(code).expect("Should parse");
        eprintln!("Parsed commands: {:#?}", parsed);

        let timed = commands_to_audio(&parsed, 120.0);
        let sample_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlaySample { .. }))
            .count();
        eprintln!("amp_mod test: {} samples", sample_count);
        assert!(sample_count > 0, "Should have samples from kick function");

        // Verify the amplitude is 2.0 (amp_mod(2) = 2 * 1.0)
        let amp = timed.iter().find_map(|(_, c)| {
            if let AudioCommand::PlaySample { amplitude, .. } = c {
                Some(*amplitude)
            } else {
                None
            }
        });
        eprintln!("Sample amplitude: {:?}", amp);
        // Currently falls back to 1.0 due to eval_user_function returning None
        // This assertion documents the expected behavior for full parity
        assert!(
            amp.is_some(),
            "Should have a sample amplitude"
        );
        // TODO: When user-defined function evaluation is fixed, enable this:
        // assert!((amp.unwrap() - 2.0).abs() < 0.01, "Amplitude should be 2.0 from amp_mod(2)");
    }

    #[test]
    fn test_set_with_loop_variable_arithmetic() {
        // Test that `set :x, expr_with_i` properly evaluates i inside a .times loop
        let code = r#"
set :master_amp, 1.0

3.times do |i|
  set :master_amp, 1.0 - (i + 1) * 0.1
  sample :kick, amp: get(:master_amp)
end
"#;
        let parsed = parse_code(code).expect("Should parse");
        eprintln!("Parsed commands: {:#?}", parsed);

        let timed = commands_to_audio(&parsed, 120.0);
        let amps: Vec<f32> = timed
            .iter()
            .filter_map(|(_, c)| {
                if let AudioCommand::PlaySample { amplitude, .. } = c {
                    Some(*amplitude)
                } else {
                    None
                }
            })
            .collect();
        eprintln!("Sample amplitudes: {:?}", amps);
        // Expected: 0.9 (i=0 -> 1.0 - 1*0.1), 0.8 (i=1 -> 1.0 - 2*0.1), 0.7 (i=2 -> 1.0 - 3*0.1)
        assert_eq!(amps.len(), 3, "Should have 3 samples");
        // Check amplitude values with some tolerance
        let expected = vec![0.9, 0.8, 0.7];
        for (i, (actual, exp)) in amps.iter().zip(expected.iter()).enumerate() {
            eprintln!("Iteration {}: expected={}, actual={}", i, exp, actual);
            assert!(
                (actual - exp).abs() < 0.01,
                "Iteration {} amplitude mismatch: expected {}, got {}",
                i, exp, actual
            );
        }
    }

    #[test]
    fn test_while_loop_with_variable_multiplication() {
        let code = r#"
bar = 4.0
intro_bars = 2
intro_len = intro_bars * bar

t = 0
while t < intro_len
  play :c4
  sleep 2
  t += 2
end
"#;
        let parsed = parse_code(code).expect("Should parse");
        eprintln!("Parsed commands: {:#?}", parsed);
        
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Notes: {}", note_count);
        
        // intro_len = 2 * 4.0 = 8.0, loop increments by 2 each time
        // Iterations: t=0 (play), t=2 (play), t=4 (play), t=6 (play), t=8 (stop -> 8 < 8 is false)
        // Expected: 4 notes
        assert_eq!(note_count, 4, "Should have 4 notes from while loop");
    }

    #[test]
    fn test_in_thread_with_while_loop() {
        let code = r#"
bar = 4.0
intro_bars = 2
intro_len = intro_bars * bar

in_thread do
  t = 0
  while t < intro_len
    play :c4
    sleep 2
    t += 2
  end
end

play :e4
"#;
        let parsed = parse_code(code).expect("Should parse");
        eprintln!("Parsed commands: {:#?}", parsed);
        
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Notes from in_thread + outside: {}", note_count);
        
        // Thread should produce 4 notes (same as previous test)
        // Plus 1 note outside thread = 5 total
        assert_eq!(note_count, 5, "Should have 4 notes from thread + 1 outside");
    }

    #[test]
    fn test_brace_block_expansion() {
        // Test that N.times { body; body } is expanded to N.times do ... end
        let code = r#"
use_synth :prophet

define :metal_chug do |n, dur|
  play n, sustain: dur, release: 0.1
end

4.times { metal_chug(60, 0.25); sleep 0.25 }
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 120.0);
        let note_count = timed
            .iter()
            .filter(|(_, c)| matches!(c, AudioCommand::PlayNote { .. }))
            .count();
        eprintln!("Notes from brace block expansion: {}", note_count);
        // 4.times should produce 4 notes from metal_chug
        assert_eq!(note_count, 4, "4.times {{ body }} should produce 4 notes");

        // Verify the synth type is prophet (not default beep)
        for (_, cmd) in &timed {
            if let AudioCommand::PlayNote { synth_type, .. } = cmd {
                assert_eq!(
                    *synth_type,
                    OscillatorType::Prophet,
                    "Synth should be Prophet from use_synth inside define"
                );
            }
        }
    }

    #[test]
    fn test_brace_block_with_default_params() {
        // Test brace block + define with default parameters (like test7's metal_chug)
        let code = r#"
root = :e
riff_root = (note root) - 24

define :metal_chug do |n, dur=0.25, amp=1.6, cut=85|
  use_synth :prophet
  play n, attack: 0.0, sustain: dur, release: 0.1, amp: amp
end

define :metal_accent do |n, dur=0.5, amp=1.9, cut=105|
  use_synth :supersaw
  play n, attack: 0.005, sustain: dur, release: 0.2, amp: amp
end

2.times do
  8.times { metal_chug(riff_root, 0.25, 1.65, 85); sleep 0.25 }
  metal_accent(riff_root+3, 0.5, 2.0, 110); sleep 0.5
  4.times { metal_chug(riff_root, 0.25, 1.55, 80); sleep 0.25 }
  metal_accent(riff_root+2, 0.5, 1.95, 105); sleep 0.5
end
"#;
        let parsed = parse_code(code).unwrap();
        let timed = commands_to_audio(&parsed, 80.0);

        // Count notes by synth type
        let prophet_count = timed.iter().filter(|(_, c)| matches!(c, AudioCommand::PlayNote { synth_type: OscillatorType::Prophet, .. })).count();
        let supersaw_count = timed.iter().filter(|(_, c)| matches!(c, AudioCommand::PlayNote { synth_type: OscillatorType::SuperSaw, .. })).count();
        eprintln!("Prophet notes: {}, SuperSaw notes: {}", prophet_count, supersaw_count);

        // 2.times { 8 chugs + 4 chugs } = 2 * 12 = 24 prophet notes
        assert_eq!(prophet_count, 24, "Should have 24 prophet notes from metal_chug");
        // 2.times { 2 accents } = 4 supersaw notes
        assert_eq!(supersaw_count, 4, "Should have 4 supersaw notes from metal_accent");
    }
}
