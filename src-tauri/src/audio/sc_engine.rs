/// SuperCollider engine integration via OSC.
///
/// This module manages a scsynth subprocess and communicates with it
/// via the Open Sound Control (OSC) protocol over UDP. It provides the
/// same interface as the cpal-based AudioEngine so it can be used as
/// an alternative audio backend for professional-quality sound.
///
/// Supports two modes:
/// 1. **Embedded mode**: scsynth is bundled with the app in sc-bundle/
///    along with UGen plugins and pre-compiled SynthDefs. No external
///    install needed.
/// 2. **System mode**: Falls back to a system-installed SuperCollider
///    if the bundle is not found.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::path::{PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};

use super::engine::AudioCommand;
use super::sc_synthdefs;
use super::synth::OscillatorType;

/// Default scsynth port
const SC_PORT: u16 = 57110;
/// Our client port for receiving OSC replies
const CLIENT_PORT: u16 = 57120;

/// SuperCollider node add actions
const ADD_TO_HEAD: i32 = 0;
const ADD_TO_TAIL: i32 = 1;

/// Group IDs
const ROOT_GROUP: i32 = 0;
const SOURCE_GROUP: i32 = 1000;
const FX_GROUP: i32 = 1001;
const MONITOR_GROUP: i32 = 1002;

/// SuperCollider engine state
pub struct ScEngineState {
    pub waveform_buffer: Vec<f32>,
    pub is_playing: bool,
    pub master_volume: f32,
    pub bpm: f32,
    pub sample_rate: u32,
}

impl Default for ScEngineState {
    fn default() -> Self {
        Self {
            waveform_buffer: vec![0.0; 2048],
            is_playing: false,
            master_volume: 1.0,
            bpm: 120.0,
            sample_rate: 44100,
        }
    }
}

/// SuperCollider engine — manages scsynth process and OSC communication
pub struct ScEngine {
    /// UDP socket for sending/receiving OSC messages
    socket: UdpSocket,
    /// scsynth subprocess handle
    scsynth_process: Mutex<Option<Child>>,
    /// scsynth listen port
    sc_port: u16,
    /// Next node ID (monotonically increasing)
    next_node_id: AtomicI32,
    /// Next buffer ID (monotonically increasing)
    next_buffer_id: AtomicI32,
    /// Map from sample file path to SC buffer number
    pub loaded_buffers: Mutex<HashMap<String, i32>>,
    /// Currently active FX node IDs
    active_fx_nodes: Mutex<Vec<i32>>,
    /// Whether scsynth has booted and is ready
    is_booted: AtomicBool,
    /// Path to scsynth executable
    scsynth_path: PathBuf,
    /// Path to sclang executable (for SynthDef compilation — only needed in system mode)
    sclang_path: Option<PathBuf>,
    /// Directory for compiled SynthDef files
    synthdefs_dir: PathBuf,
    /// Directory containing UGen plugins (.scx files) — only for embedded mode
    plugins_dir: Option<PathBuf>,
    /// Whether we're running from a bundled sc-bundle
    use_bundled: bool,
    /// Buffer for waveform scope (SC buffer ID)
    scope_buffer_id: i32,
    /// Shared engine state
    pub state: Mutex<ScEngineState>,
}

impl ScEngine {
    /// Create a new SC engine. Does NOT start scsynth yet — call `boot()` for that.
    /// 
    /// If `sc_bundle_dir` is Some, looks for bundled scsynth in that directory first.
    /// Falls back to searching for a system-installed SuperCollider.
    pub fn new(sc_bundle_dir: Option<PathBuf>) -> Result<Self, String> {
        // Try bundled scsynth first, then fall back to system install
        let (scsynth_path, sclang_path, plugins_dir, synthdefs_dir, use_bundled) =
            if let Some(ref bundle_dir) = sc_bundle_dir {
                match find_bundled_scsynth(bundle_dir) {
                    Some((synth_path, plugins, synthdefs)) => {
                        eprintln!("[SC] Using bundled scsynth from: {}", bundle_dir.display());
                        (synth_path, None, Some(plugins), synthdefs, true)
                    }
                    None => {
                        eprintln!("[SC] Bundle dir exists but scsynth not found, trying system install...");
                        let (synth, lang) = find_supercollider()?;
                        let sd_dir = get_synthdefs_dir();
                        (synth, lang, None, sd_dir, false)
                    }
                }
            } else {
                // No bundle dir provided, try system install
                let (synth, lang) = find_supercollider()?;
                let sd_dir = get_synthdefs_dir();
                (synth, lang, None, sd_dir, false)
            };

        eprintln!(
            "[SC] Found scsynth: {} (bundled={})",
            scsynth_path.display(),
            use_bundled
        );
        if let Some(ref sclang) = sclang_path {
            eprintln!("[SC] Found sclang: {}", sclang.display());
        }
        if let Some(ref plugins) = plugins_dir {
            eprintln!("[SC] UGen plugins dir: {}", plugins.display());
        }

        // Set up synthdefs directory
        std::fs::create_dir_all(&synthdefs_dir)
            .map_err(|e| format!("Cannot create synthdefs dir: {}", e))?;

        // Bind UDP socket for OSC communication
        // Try a range of ports in case CLIENT_PORT is taken
        let socket = bind_udp_socket(CLIENT_PORT, CLIENT_PORT + 100)?;
        socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .ok();
        socket
            .set_nonblocking(false)
            .map_err(|e| format!("Socket config error: {}", e))?;

        Ok(Self {
            socket,
            scsynth_process: Mutex::new(None),
            sc_port: SC_PORT,
            next_node_id: AtomicI32::new(2000), // Start above our group IDs
            next_buffer_id: AtomicI32::new(1),   // Buffer 0 reserved for scope
            loaded_buffers: Mutex::new(HashMap::new()),
            active_fx_nodes: Mutex::new(Vec::new()),
            is_booted: AtomicBool::new(false),
            scsynth_path,
            sclang_path,
            synthdefs_dir,
            plugins_dir,
            use_bundled,
            scope_buffer_id: 0,
            state: Mutex::new(ScEngineState::default()),
        })
    }

    /// Boot the SuperCollider server: start scsynth, load SynthDefs
    pub fn boot(&self) -> Result<(), String> {
        if self.is_booted.load(Ordering::Relaxed) {
            return Ok(());
        }

        eprintln!("[SC] Booting SuperCollider server (bundled={})...", self.use_bundled);

        // Step 1: Start scsynth subprocess
        self.start_scsynth()?;

        // Step 2: Wait for server to be ready
        self.wait_for_boot(Duration::from_secs(10))?;

        // Step 3: Ensure SynthDefs are available
        if !sc_synthdefs::synthdefs_exist(&self.synthdefs_dir) {
            if self.use_bundled {
                // In bundled mode, SynthDefs should already be pre-compiled
                // If they're missing, that's a build/setup error
                return Err(
                    "Pre-compiled SynthDefs not found in bundle. Run setup_sc.ps1 to set up the SC bundle."
                        .to_string(),
                );
            } else {
                // System mode: compile SynthDefs using sclang
                eprintln!("[SC] Compiling SynthDefs via sclang...");
                self.compile_synthdefs()?;
            }
        } else {
            eprintln!("[SC] SynthDefs already compiled, skipping compilation");
        }

        // Step 4: Load SynthDefs into scsynth
        self.load_synthdefs()?;

        // Step 5: Set up node groups
        self.setup_groups()?;

        // Step 6: Set up scope buffer for waveform visualization
        self.setup_scope()?;

        self.is_booted.store(true, Ordering::Relaxed);
        eprintln!("[SC] SuperCollider server is ready!");
        Ok(())
    }

    /// Check if scsynth is booted and ready
    pub fn is_booted(&self) -> bool {
        self.is_booted.load(Ordering::Relaxed)
    }

    /// Shut down the SuperCollider server
    pub fn shutdown(&self) {
        eprintln!("[SC] Shutting down SuperCollider server...");

        // Send /quit to scsynth
        let _ = self.send_osc_msg("/quit", vec![]);

        // Kill the process if it didn't quit gracefully
        if let Some(ref mut child) = *self.scsynth_process.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *self.scsynth_process.lock() = None;
        self.is_booted.store(false, Ordering::Relaxed);
        eprintln!("[SC] Server shut down");
    }

    // ================================================================
    // PUBLIC API — matches AudioEngine interface
    // ================================================================

    /// Send an AudioCommand to SuperCollider (translate to OSC)
    pub fn send_command(&self, cmd: AudioCommand) -> Result<(), String> {
        if !self.is_booted.load(Ordering::Relaxed) {
            return Err("SuperCollider server not booted".to_string());
        }

        match cmd {
            AudioCommand::PlayNote {
                synth_type,
                frequency,
                amplitude,
                duration_secs,
                envelope,
                pan,
            } => {
                self.play_note(synth_type, frequency, amplitude, duration_secs, &envelope, pan)
            }
            AudioCommand::PlaySample {
                samples: _,
                sample_rate: _,
                amplitude: _,
                rate: _,
                pan: _,
            } => {
                // For raw sample data, we can't easily send to SC.
                // Samples should be loaded via load_sample_buffer() instead.
                // This is a fallback that plays nothing.
                eprintln!("[SC] Warning: PlaySample with raw data not supported, use load_sample_buffer()");
                Ok(())
            }
            AudioCommand::SetBpm(bpm) => {
                self.state.lock().bpm = bpm;
                Ok(())
            }
            AudioCommand::SetMasterVolume(vol) => {
                self.state.lock().master_volume = vol;
                // In SC, we could control a master volume node, but for simplicity
                // we apply volume when creating synths
                Ok(())
            }
            AudioCommand::Stop => self.stop_all(),
            AudioCommand::SetEffect {
                reverb_mix,
                delay_time,
                delay_feedback,
                distortion,
                lpf_cutoff,
                hpf_cutoff,
            } => self.set_global_effects(
                reverb_mix,
                delay_time,
                delay_feedback,
                distortion,
                lpf_cutoff,
                hpf_cutoff,
            ),
        }
    }

    /// Play a note using a SuperCollider synth
    pub fn play_note(
        &self,
        synth_type: OscillatorType,
        frequency: f32,
        amplitude: f32,
        duration_secs: f32,
        envelope: &super::synth::Envelope,
        pan: f32,
    ) -> Result<(), String> {
        let node_id = self.alloc_node_id();
        let def_name = sc_synthdefs::synthdef_name(&synth_type);
        let master_vol = self.state.lock().master_volume;

        let sustain_time = (duration_secs - envelope.attack - envelope.release).max(0.0);

        self.send_osc_msg(
            "/s_new",
            vec![
                OscType::String(def_name.to_string()),
                OscType::Int(node_id),
                OscType::Int(ADD_TO_HEAD),
                OscType::Int(SOURCE_GROUP),
                // Parameters
                OscType::String("freq".to_string()),
                OscType::Float(frequency),
                OscType::String("amp".to_string()),
                OscType::Float(amplitude * master_vol),
                OscType::String("pan".to_string()),
                OscType::Float(pan),
                OscType::String("attack".to_string()),
                OscType::Float(envelope.attack),
                OscType::String("sustain".to_string()),
                OscType::Float(sustain_time),
                OscType::String("release".to_string()),
                OscType::Float(envelope.release),
            ],
        )?;

        self.state.lock().is_playing = true;
        Ok(())
    }

    /// Play a sample that has been loaded into a SC buffer
    pub fn play_sample_buffer(
        &self,
        buffer_id: i32,
        amplitude: f32,
        rate: f32,
        pan: f32,
    ) -> Result<(), String> {
        let node_id = self.alloc_node_id();
        let master_vol = self.state.lock().master_volume;

        self.send_osc_msg(
            "/s_new",
            vec![
                OscType::String("sonic_playbuf".to_string()),
                OscType::Int(node_id),
                OscType::Int(ADD_TO_HEAD),
                OscType::Int(SOURCE_GROUP),
                OscType::String("buf".to_string()),
                OscType::Int(buffer_id),
                OscType::String("amp".to_string()),
                OscType::Float(amplitude * master_vol),
                OscType::String("rate".to_string()),
                OscType::Float(rate),
                OscType::String("pan".to_string()),
                OscType::Float(pan),
            ],
        )?;

        self.state.lock().is_playing = true;
        Ok(())
    }

    /// Load a sample file into a SuperCollider buffer and return the buffer ID.
    /// Caches loaded buffers so the same file isn't loaded twice.
    pub fn load_sample_buffer(&self, file_path: &str) -> Result<i32, String> {
        // Check cache
        {
            let loaded = self.loaded_buffers.lock();
            if let Some(&buf_id) = loaded.get(file_path) {
                return Ok(buf_id);
            }
        }

        let buf_id = self.alloc_buffer_id();
        eprintln!(
            "[SC] Loading sample '{}' into buffer {}",
            file_path, buf_id
        );

        // Convert path separators for scsynth (it prefers forward slashes)
        let sc_path = file_path.replace('\\', "/");

        // /b_allocRead: [buf_num, file_path, start_frame, num_frames]
        // 0 frames = read entire file
        self.send_osc_msg(
            "/b_allocRead",
            vec![
                OscType::Int(buf_id),
                OscType::String(sc_path),
                OscType::Int(0),
                OscType::Int(0), // 0 = entire file
            ],
        )?;

        // Wait for /done response
        self.wait_for_done("/b_allocRead", Duration::from_secs(5))?;

        // Cache the buffer ID
        self.loaded_buffers
            .lock()
            .insert(file_path.to_string(), buf_id);

        eprintln!("[SC] Sample loaded into buffer {}", buf_id);
        Ok(buf_id)
    }

    /// Stop all audio
    pub fn stop_all(&self) -> Result<(), String> {
        // Free all nodes in the source group
        self.send_osc_msg(
            "/g_freeAll",
            vec![OscType::Int(SOURCE_GROUP)],
        )?;

        // Also free FX group nodes and re-create the groups
        self.send_osc_msg(
            "/g_freeAll",
            vec![OscType::Int(FX_GROUP)],
        )?;

        self.active_fx_nodes.lock().clear();
        self.state.lock().is_playing = false;

        Ok(())
    }

    /// Get the waveform buffer for visualization
    pub fn get_waveform(&self) -> Vec<f32> {
        self.state.lock().waveform_buffer.clone()
    }

    /// Get state snapshot: (is_playing, master_volume, bpm)
    pub fn get_state_snapshot(&self) -> (bool, f32, f32) {
        let s = self.state.lock();
        (s.is_playing, s.master_volume, s.bpm)
    }

    /// Set global effects
    pub fn set_global_effects(
        &self,
        reverb_mix: f32,
        delay_time: f32,
        delay_feedback: f32,
        distortion: f32,
        lpf_cutoff: f32,
        hpf_cutoff: f32,
    ) -> Result<(), String> {
        // Free existing FX nodes
        let old_nodes = {
            let mut fx = self.active_fx_nodes.lock();
            let old = fx.clone();
            fx.clear();
            old
        };
        for node_id in &old_nodes {
            let _ = self.send_osc_msg("/n_free", vec![OscType::Int(*node_id)]);
        }

        let mut new_fx = Vec::new();

        // Add LPF if cutoff is below 20000
        if lpf_cutoff < 19000.0 {
            let node_id = self.alloc_node_id();
            self.send_osc_msg(
                "/s_new",
                vec![
                    OscType::String("sonic_fx_lpf".to_string()),
                    OscType::Int(node_id),
                    OscType::Int(ADD_TO_TAIL),
                    OscType::Int(FX_GROUP),
                    OscType::String("out".to_string()),
                    OscType::Int(0),
                    OscType::String("cutoff".to_string()),
                    OscType::Float(lpf_cutoff),
                ],
            )?;
            new_fx.push(node_id);
        }

        // Add HPF if cutoff is above 20
        if hpf_cutoff > 30.0 {
            let node_id = self.alloc_node_id();
            self.send_osc_msg(
                "/s_new",
                vec![
                    OscType::String("sonic_fx_hpf".to_string()),
                    OscType::Int(node_id),
                    OscType::Int(ADD_TO_TAIL),
                    OscType::Int(FX_GROUP),
                    OscType::String("out".to_string()),
                    OscType::Int(0),
                    OscType::String("cutoff".to_string()),
                    OscType::Float(hpf_cutoff),
                ],
            )?;
            new_fx.push(node_id);
        }

        // Add distortion if > 0
        if distortion > 0.01 {
            let node_id = self.alloc_node_id();
            self.send_osc_msg(
                "/s_new",
                vec![
                    OscType::String("sonic_fx_distortion".to_string()),
                    OscType::Int(node_id),
                    OscType::Int(ADD_TO_TAIL),
                    OscType::Int(FX_GROUP),
                    OscType::String("out".to_string()),
                    OscType::Int(0),
                    OscType::String("distort".to_string()),
                    OscType::Float(distortion),
                ],
            )?;
            new_fx.push(node_id);
        }

        // Add delay if delay_time > 0
        if delay_time > 0.01 {
            let node_id = self.alloc_node_id();
            self.send_osc_msg(
                "/s_new",
                vec![
                    OscType::String("sonic_fx_echo".to_string()),
                    OscType::Int(node_id),
                    OscType::Int(ADD_TO_TAIL),
                    OscType::Int(FX_GROUP),
                    OscType::String("out".to_string()),
                    OscType::Int(0),
                    OscType::String("phase".to_string()),
                    OscType::Float(delay_time),
                    OscType::String("decay".to_string()),
                    OscType::Float(delay_feedback * 4.0),
                ],
            )?;
            new_fx.push(node_id);
        }

        // Add reverb if mix > 0
        if reverb_mix > 0.01 {
            let node_id = self.alloc_node_id();
            self.send_osc_msg(
                "/s_new",
                vec![
                    OscType::String("sonic_fx_reverb".to_string()),
                    OscType::Int(node_id),
                    OscType::Int(ADD_TO_TAIL),
                    OscType::Int(FX_GROUP),
                    OscType::String("out".to_string()),
                    OscType::Int(0),
                    OscType::String("mix".to_string()),
                    OscType::Float(reverb_mix),
                    OscType::String("room".to_string()),
                    OscType::Float(0.7),
                ],
            )?;
            new_fx.push(node_id);
        }

        *self.active_fx_nodes.lock() = new_fx;
        Ok(())
    }

    /// Create an FX node for a with_fx block and return the node ID.
    /// The FX is placed at the tail of the source group so it processes
    /// all synths inside the same group.
    pub fn create_fx_node(
        &self,
        fx_type: &str,
        params: &[(String, f32)],
    ) -> Result<i32, String> {
        let node_id = self.alloc_node_id();
        let def_name = match fx_type {
            "reverb" | "gverb" => "sonic_fx_reverb",
            "echo" | "delay" => "sonic_fx_echo",
            "distortion" | "tanh" => "sonic_fx_distortion",
            "slicer" => "sonic_fx_slicer",
            "lpf" | "rlpf" | "nrlpf" => "sonic_fx_lpf",
            "hpf" | "rhpf" | "nrhpf" => "sonic_fx_hpf",
            "flanger" => "sonic_fx_flanger",
            "compressor" => "sonic_fx_compressor",
            _ => {
                eprintln!("[SC] Unknown FX type '{}', using reverb", fx_type);
                "sonic_fx_reverb"
            }
        };

        let mut args = vec![
            OscType::String(def_name.to_string()),
            OscType::Int(node_id),
            OscType::Int(ADD_TO_TAIL),
            OscType::Int(SOURCE_GROUP),
            OscType::String("out".to_string()),
            OscType::Int(0),
        ];

        for (name, val) in params {
            args.push(OscType::String(name.clone()));
            args.push(OscType::Float(*val));
        }

        self.send_osc_msg("/s_new", args)?;
        Ok(node_id)
    }

    /// Free a specific node (e.g., an FX node when a with_fx block ends)
    pub fn free_node(&self, node_id: i32) -> Result<(), String> {
        self.send_osc_msg("/n_free", vec![OscType::Int(node_id)])
    }

    // ================================================================
    // INTERNAL METHODS
    // ================================================================

    fn alloc_node_id(&self) -> i32 {
        self.next_node_id.fetch_add(1, Ordering::Relaxed)
    }

    fn alloc_buffer_id(&self) -> i32 {
        self.next_buffer_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Start the scsynth subprocess
    fn start_scsynth(&self) -> Result<(), String> {
        // Check if scsynth is already running (we might be connecting to an existing instance)
        if self.ping_server() {
            eprintln!("[SC] scsynth already running on port {}", self.sc_port);
            return Ok(());
        }

        let mut cmd = Command::new(&self.scsynth_path);
        cmd.args([
            "-u",
            &self.sc_port.to_string(),
            "-a",
            "1024",  // max number of synths
            "-i",
            "0",     // no audio inputs
            "-o",
            "2",     // stereo output
            "-b",
            "1026",  // number of buffers
            "-m",
            "131072", // memory size
            "-R",
            "0",     // no rendezvous
            "-l",
            "1",     // max logins
        ]);

        // In bundled mode, tell scsynth where to find UGen plugins
        if let Some(ref plugins_dir) = self.plugins_dir {
            let plugins_path = plugins_dir.to_string_lossy().to_string();
            eprintln!("[SC] Using bundled UGen plugins: {}", plugins_path);
            cmd.args(["-U", &plugins_path]);
        }

        // Set working directory to scsynth's parent dir so it can find its DLLs
        if let Some(parent) = self.scsynth_path.parent() {
            cmd.current_dir(parent);
        }

        let child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start scsynth: {}", e))?;

        eprintln!("[SC] scsynth started with PID {}", child.id());
        *self.scsynth_process.lock() = Some(child);

        Ok(())
    }

    /// Wait for scsynth to boot by polling /status
    fn wait_for_boot(&self, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(200);

        while start.elapsed() < timeout {
            if self.ping_server() {
                eprintln!("[SC] Server is alive (boot took {:.1}s)", start.elapsed().as_secs_f64());
                return Ok(());
            }
            std::thread::sleep(poll_interval);
        }

        Err("Timeout waiting for scsynth to boot".to_string())
    }

    /// Ping the server with /status and check for /status.reply
    fn ping_server(&self) -> bool {
        if self.send_osc_msg("/status", vec![]).is_err() {
            return false;
        }

        // Wait for reply
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(300) {
            if let Ok(packet) = self.recv_osc() {
                if let OscPacket::Message(msg) = &packet {
                    if msg.addr == "/status.reply" {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Compile SynthDefs by writing a .scd script and running sclang
    fn compile_synthdefs(&self) -> Result<(), String> {
        let sclang = self.sclang_path.as_ref()
            .ok_or("sclang not found — cannot compile SynthDefs. Please install SuperCollider.")?;

        // Write the SynthDef compilation script
        let script = sc_synthdefs::generate_synthdef_script(&self.synthdefs_dir);
        let script_path = self.synthdefs_dir.join("compile_synthdefs.scd");
        std::fs::write(&script_path, &script)
            .map_err(|e| format!("Failed to write SynthDef script: {}", e))?;

        eprintln!("[SC] Running sclang to compile SynthDefs...");
        eprintln!("[SC] Script: {}", script_path.display());

        // Run sclang to compile
        let output = Command::new(sclang)
            .arg(&script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to run sclang: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.is_empty() {
            eprintln!("[SC] sclang stdout: {}", stdout);
        }
        if !stderr.is_empty() {
            eprintln!("[SC] sclang stderr: {}", stderr);
        }

        if output.status.success() || stdout.contains("SynthDefs compiled successfully") {
            eprintln!("[SC] SynthDefs compiled successfully");
            Ok(())
        } else {
            Err(format!(
                "sclang failed (exit code {:?}):\nstdout: {}\nstderr: {}",
                output.status.code(),
                stdout,
                stderr
            ))
        }
    }

    /// Load compiled SynthDefs into scsynth
    fn load_synthdefs(&self) -> Result<(), String> {
        let dir_str = self.synthdefs_dir.to_string_lossy().replace('\\', "/");
        eprintln!("[SC] Loading SynthDefs from {}", dir_str);

        self.send_osc_msg(
            "/d_loadDir",
            vec![OscType::String(dir_str)],
        )?;

        // Wait for /done
        self.wait_for_done("/d_loadDir", Duration::from_secs(5))?;

        eprintln!("[SC] SynthDefs loaded");
        Ok(())
    }

    /// Set up the node group hierarchy
    fn setup_groups(&self) -> Result<(), String> {
        // Create source group (for synths and samples)
        self.send_osc_msg(
            "/g_new",
            vec![
                OscType::Int(SOURCE_GROUP),
                OscType::Int(ADD_TO_HEAD),
                OscType::Int(ROOT_GROUP),
            ],
        )?;

        // Create FX group (for global effects, after source group)
        self.send_osc_msg(
            "/g_new",
            vec![
                OscType::Int(FX_GROUP),
                OscType::Int(ADD_TO_TAIL),
                OscType::Int(ROOT_GROUP),
            ],
        )?;

        // Create monitor group (for scope, after FX group)
        self.send_osc_msg(
            "/g_new",
            vec![
                OscType::Int(MONITOR_GROUP),
                OscType::Int(ADD_TO_TAIL),
                OscType::Int(ROOT_GROUP),
            ],
        )?;

        eprintln!("[SC] Groups created (source={}, fx={}, monitor={})",
            SOURCE_GROUP, FX_GROUP, MONITOR_GROUP);
        Ok(())
    }

    /// Set up the scope buffer and meter node for waveform visualization
    fn setup_scope(&self) -> Result<(), String> {
        // Allocate a buffer for waveform data (2048 frames, 1 channel)
        self.send_osc_msg(
            "/b_alloc",
            vec![
                OscType::Int(self.scope_buffer_id),
                OscType::Int(2048),
                OscType::Int(1),
            ],
        )?;

        // Wait for buffer allocation
        std::thread::sleep(Duration::from_millis(100));

        // Create scope synth (writes output to buffer)
        let scope_node = self.alloc_node_id();
        self.send_osc_msg(
            "/s_new",
            vec![
                OscType::String("sonic_scope".to_string()),
                OscType::Int(scope_node),
                OscType::Int(ADD_TO_TAIL),
                OscType::Int(MONITOR_GROUP),
                OscType::String("buf".to_string()),
                OscType::Int(self.scope_buffer_id),
            ],
        )?;

        // Create meter synth (sends amplitude via SendReply)
        let meter_node = self.alloc_node_id();
        self.send_osc_msg(
            "/s_new",
            vec![
                OscType::String("sonic_meter".to_string()),
                OscType::Int(meter_node),
                OscType::Int(ADD_TO_TAIL),
                OscType::Int(MONITOR_GROUP),
            ],
        )?;

        eprintln!("[SC] Scope and meter nodes created");
        Ok(())
    }

    /// Poll the scope buffer to update the waveform display
    pub fn poll_waveform(&self) {
        // Request buffer data from scsynth
        // /b_getn [buf_num, start_index, num_samples]
        if self.send_osc_msg(
            "/b_getn",
            vec![
                OscType::Int(self.scope_buffer_id),
                OscType::Int(0),
                OscType::Int(2048),
            ],
        ).is_err() {
            return;
        }

        // Try to receive the response
        if let Ok(packet) = self.recv_osc() {
            if let OscPacket::Message(msg) = packet {
                if msg.addr == "/b_setn" {
                    // Extract float values from the response
                    let mut waveform = Vec::with_capacity(2048);
                    // Skip first 3 args (buf_num, start, count)
                    for arg in msg.args.iter().skip(3) {
                        if let OscType::Float(v) = arg {
                            waveform.push(*v);
                        }
                    }
                    if !waveform.is_empty() {
                        let mut state = self.state.lock();
                        state.waveform_buffer = waveform;
                    }
                }
                // Check for meter data (is_playing indicator)
                if msg.addr == "/sonic/meter" {
                    let mut state = self.state.lock();
                    if let Some(OscType::Float(amp)) = msg.args.get(2) {
                        state.is_playing = *amp > 0.001;
                    }
                }
            }
        }
    }

    /// Process any pending OSC messages from scsynth (e.g., meter updates)
    pub fn process_incoming(&self) {
        // Non-blocking receive of any pending messages
        let _ = self.socket.set_nonblocking(true);
        loop {
            match self.recv_osc() {
                Ok(packet) => {
                    if let OscPacket::Message(msg) = &packet {
                        if msg.addr == "/sonic/meter" {
                            // Update is_playing based on amplitude
                            if msg.args.len() >= 4 {
                                if let (Some(OscType::Float(l)), Some(OscType::Float(r))) =
                                    (msg.args.get(2), msg.args.get(3))
                                {
                                    let amp = (*l + *r) * 0.5;
                                    self.state.lock().is_playing = amp > 0.001;
                                }
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = self.socket.set_nonblocking(false);
        let _ = self.socket.set_read_timeout(Some(Duration::from_millis(500)));
    }

    // ================================================================
    // OSC COMMUNICATION
    // ================================================================

    /// Send an OSC message to scsynth
    fn send_osc_msg(&self, addr: &str, args: Vec<OscType>) -> Result<(), String> {
        let msg = OscMessage {
            addr: addr.to_string(),
            args,
        };
        let packet = OscPacket::Message(msg);
        let buf = encoder::encode(&packet)
            .map_err(|e| format!("OSC encode error: {}", e))?;

        self.socket
            .send_to(&buf, format!("127.0.0.1:{}", self.sc_port))
            .map_err(|e| format!("OSC send error: {}", e))?;

        Ok(())
    }

    /// Receive an OSC packet from scsynth (blocking with timeout)
    fn recv_osc(&self) -> Result<OscPacket, String> {
        let mut buf = [0u8; 65536];
        let (size, _addr) = self
            .socket
            .recv_from(&mut buf)
            .map_err(|e| format!("OSC recv error: {}", e))?;

        let (_, packet) = decoder::decode_udp(&buf[..size])
            .map_err(|e| format!("OSC decode error: {:?}", e))?;

        Ok(packet)
    }

    /// Wait for a /done response for a specific command
    fn wait_for_done(&self, cmd_name: &str, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            match self.recv_osc() {
                Ok(OscPacket::Message(msg)) => {
                    if msg.addr == "/done" {
                        if let Some(OscType::String(ref done_cmd)) = msg.args.first() {
                            if done_cmd == cmd_name {
                                return Ok(());
                            }
                        }
                        // /done for a different command — acceptable for simpler ops
                        return Ok(());
                    }
                    if msg.addr == "/fail" {
                        let error = msg
                            .args
                            .iter()
                            .filter_map(|a| {
                                if let OscType::String(s) = a {
                                    Some(s.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        return Err(format!("SC server error: {}", error));
                    }
                    // Not the message we're waiting for, continue
                }
                Ok(_) => {} // Bundle or other
                Err(_) => {
                    // Timeout on individual recv, keep trying
                }
            }
        }
        // Don't fail hard on timeout — the operation may have succeeded without reply
        eprintln!(
            "[SC] Warning: timeout waiting for /done {} (may be OK)",
            cmd_name
        );
        Ok(())
    }
}

impl Drop for ScEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}


// ================================================================
// HELPER FUNCTIONS
// ================================================================

/// Find scsynth in a bundled sc-bundle directory.
/// Returns (scsynth_path, plugins_dir, synthdefs_dir) if found.
fn find_bundled_scsynth(bundle_dir: &std::path::Path) -> Option<(PathBuf, PathBuf, PathBuf)> {
    #[cfg(target_os = "windows")]
    let scsynth_name = "scsynth.exe";
    #[cfg(not(target_os = "windows"))]
    let scsynth_name = "scsynth";

    let scsynth_path = bundle_dir.join(scsynth_name);
    if !scsynth_path.exists() {
        eprintln!("[SC] Bundled scsynth not found at: {}", scsynth_path.display());
        return None;
    }

    let plugins_dir = bundle_dir.join("plugins");
    if !plugins_dir.exists() {
        eprintln!("[SC] Warning: UGen plugins directory not found at: {}", plugins_dir.display());
        // Don't fail — scsynth might work with default plugins path
    }

    let synthdefs_dir = bundle_dir.join("synthdefs");
    if !synthdefs_dir.exists() {
        eprintln!("[SC] Warning: SynthDefs directory not found at: {}", synthdefs_dir.display());
        // Create it — SynthDefs might be compiled later
        let _ = std::fs::create_dir_all(&synthdefs_dir);
    }

    Some((scsynth_path, plugins_dir, synthdefs_dir))
}

/// Locate the sc-bundle directory by checking multiple possible locations.
/// Used to find the bundled SuperCollider without needing a Tauri app handle.
pub fn find_sc_bundle_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let scsynth_name = "scsynth.exe";
    #[cfg(not(target_os = "windows"))]
    let scsynth_name = "scsynth";

    // Helper: check if a bundle dir looks complete (has scsynth + synthdefs + plugins)
    let bundle_score = |dir: &std::path::Path| -> u8 {
        let mut score = 0u8;
        if dir.join(scsynth_name).exists() { score += 1; }
        if dir.join("plugins").exists() { score += 1; }
        // Check for at least one compiled SynthDef
        if dir.join("synthdefs").join("sonic_beep.scsyndef").exists() { score += 1; }
        score
    };

    let mut best: Option<(PathBuf, u8)> = None;
    let mut consider = |candidate: PathBuf, label: &str| {
        if !candidate.join(scsynth_name).exists() {
            return;
        }
        let score = bundle_score(&candidate);
        eprintln!("[SC] Candidate sc-bundle ({}, score={}): {}", label, score, candidate.display());
        if best.as_ref().map_or(true, |(_, bs)| score > *bs) {
            best = Some((candidate, score));
        }
    };

    // 1. Check relative to current directory (development mode — most likely to be complete)
    if let Ok(cwd) = std::env::current_dir() {
        eprintln!("[SC] CWD: {}", cwd.display());
    }
    let dev_paths = [
        PathBuf::from("src-tauri").join("sc-bundle"),
        PathBuf::from("sc-bundle"),
    ];
    for candidate in dev_paths {
        let abs = candidate.canonicalize().unwrap_or(candidate);
        consider(abs, "dev");
    }

    // 2. Check relative to executable (production builds)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidates = [
                exe_dir.join("sc-bundle"),
                exe_dir.join("resources").join("sc-bundle"),
                exe_dir.join("_up_").join("resources").join("sc-bundle"),
            ];
            for candidate in candidates {
                consider(candidate, "exe-relative");
            }
        }
    }

    // 3. Check via environment variable (allows user override)
    if let Ok(sc_bundle) = std::env::var("SONIC_DAW_SC_BUNDLE") {
        let path = PathBuf::from(&sc_bundle);
        consider(path, "env-var");
    }

    if let Some((path, score)) = best {
        eprintln!("[SC] Selected sc-bundle (score={}): {}", score, path.display());
        Some(path)
    } else {
        None
    }
}

/// Find SuperCollider installation (scsynth and sclang)
fn find_supercollider() -> Result<(PathBuf, Option<PathBuf>), String> {
    let mut scsynth: Option<PathBuf> = None;
    let mut sclang: Option<PathBuf> = None;

    // Common installation paths by OS
    #[cfg(target_os = "windows")]
    let search_paths: Vec<PathBuf> = {
        let mut paths = Vec::new();
        // Standard install locations
        if let Ok(pf) = std::env::var("ProgramFiles") {
            paths.push(PathBuf::from(&pf).join("SuperCollider"));
            // Check versioned directories
            if let Ok(entries) = std::fs::read_dir(&pf) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("SuperCollider") {
                        paths.push(entry.path());
                    }
                }
            }
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            paths.push(PathBuf::from(&pf86).join("SuperCollider"));
        }
        // User-local install
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            paths.push(PathBuf::from(&local).join("Programs").join("SuperCollider"));
        }
        // Also check PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in path_var.split(';') {
                let p = PathBuf::from(dir);
                if p.join("scsynth.exe").exists() {
                    paths.push(p);
                }
            }
        }
        paths
    };

    #[cfg(target_os = "macos")]
    let search_paths: Vec<PathBuf> = vec![
        PathBuf::from("/Applications/SuperCollider.app/Contents/MacOS"),
        PathBuf::from("/Applications/SuperCollider.app/Contents/Resources"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
    ];

    #[cfg(target_os = "linux")]
    let search_paths: Vec<PathBuf> = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/share/SuperCollider"),
    ];

    for dir in &search_paths {
        #[cfg(target_os = "windows")]
        {
            let synth_path = dir.join("scsynth.exe");
            let lang_path = dir.join("sclang.exe");
            if synth_path.exists() {
                scsynth = Some(synth_path);
            }
            if lang_path.exists() {
                sclang = Some(lang_path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let synth_path = dir.join("scsynth");
            let lang_path = dir.join("sclang");
            if synth_path.exists() {
                scsynth = Some(synth_path);
            }
            if lang_path.exists() {
                sclang = Some(lang_path);
            }
        }

        if scsynth.is_some() {
            break;
        }
    }

    // Also try `which`/`where` as fallback
    if scsynth.is_none() {
        #[cfg(target_os = "windows")]
        if let Ok(output) = Command::new("where").arg("scsynth").output() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                scsynth = Some(PathBuf::from(path));
            }
        }
        #[cfg(not(target_os = "windows"))]
        if let Ok(output) = Command::new("which").arg("scsynth").output() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                scsynth = Some(PathBuf::from(path));
            }
        }
    }

    if sclang.is_none() {
        #[cfg(target_os = "windows")]
        if let Ok(output) = Command::new("where").arg("sclang").output() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                sclang = Some(PathBuf::from(path));
            }
        }
        #[cfg(not(target_os = "windows"))]
        if let Ok(output) = Command::new("which").arg("sclang").output() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                sclang = Some(PathBuf::from(path));
            }
        }
    }

    match scsynth {
        Some(path) => Ok((path, sclang)),
        None => Err(
            "SuperCollider not found. Please install SuperCollider from https://supercollider.github.io/downloads and ensure scsynth is on your PATH."
                .to_string(),
        ),
    }
}

/// Get the directory for storing compiled SynthDef files
fn get_synthdefs_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    let base = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
        .unwrap_or_else(|_| PathBuf::from("."));
    #[cfg(target_os = "linux")]
    let base = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".local").join("share"))
        .unwrap_or_else(|_| PathBuf::from("."));

    base.join("PiBeat").join("synthdefs")
}

/// Bind a UDP socket, trying ports in a range
fn bind_udp_socket(start_port: u16, end_port: u16) -> Result<UdpSocket, String> {
    for port in start_port..=end_port {
        match UdpSocket::bind(format!("127.0.0.1:{}", port)) {
            Ok(socket) => {
                eprintln!("[SC] UDP socket bound to port {}", port);
                return Ok(socket);
            }
            Err(_) => continue,
        }
    }
    Err(format!(
        "Could not bind UDP socket on ports {}-{}",
        start_port, end_port
    ))
}
