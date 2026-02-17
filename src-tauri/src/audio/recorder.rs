use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct Recorder {
    buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<Mutex<bool>>,
    sample_rate: u32,
}

impl Recorder {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(Mutex::new(false)),
            sample_rate,
        }
    }

    pub fn start(&self) {
        let mut recording = self.is_recording.lock();
        let mut buffer = self.buffer.lock();
        buffer.clear();
        *recording = true;
    }

    pub fn stop(&self) {
        let mut recording = self.is_recording.lock();
        *recording = false;
    }

    pub fn is_recording(&self) -> bool {
        *self.is_recording.lock()
    }

    pub fn push_samples(&self, samples: &[f32]) {
        if *self.is_recording.lock() {
            let mut buffer = self.buffer.lock();
            buffer.extend_from_slice(samples);
        }
    }

    pub fn save_to_file(&self, path: &str) -> Result<String, String> {
        let buffer = self.buffer.lock();
        if buffer.is_empty() {
            return Err("No audio recorded".to_string());
        }

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create file: {}", e))?;

        for &sample in buffer.iter() {
            writer
                .write_sample(sample)
                .map_err(|e| format!("Failed to write: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize: {}", e))?;

        Ok(format!(
            "Saved {} samples ({:.1}s) to {}",
            buffer.len(),
            buffer.len() as f32 / self.sample_rate as f32,
            path
        ))
    }

    pub fn get_buffer(&self) -> Vec<f32> {
        self.buffer.lock().clone()
    }
}
