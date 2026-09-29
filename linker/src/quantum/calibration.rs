//! Pulse calibration metadata and microwave waveform models for physical QPU backend linking.

/// Microwave pulse waveform shape
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformType {
    Gaussian = 1,
    DragGaussian = 2,
    Square = 3,
    Cosine = 4,
    Hermite = 5,
}

/// Physical microwave pulse calibration record per drive channel.
#[derive(Debug, Clone)]
pub struct PulseRecord {
    pub channel_id: u32,
    pub waveform: WaveformType,
    pub frequency_ghz: f64,
    pub amplitude: f64,
    pub duration_ns: f64,
    pub phase_rad: f64,
    pub drag_beta: f64,
}

/// Pulse calibration parameters embedded in `.quantum.pulses` sections.
#[derive(Debug, Clone, Default)]
pub struct QpuCalibration {
    pub pulses: Vec<PulseRecord>,
    pub t1_relaxation_us: Vec<f64>,
    pub t2_dephasing_us: Vec<f64>,
    pub single_qubit_gate_fidelity: Vec<f64>,
    pub two_qubit_cz_fidelity: Vec<f64>,
    pub readout_fidelity: Vec<f64>,
}

impl QpuCalibration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_pulse(&mut self, pulse: PulseRecord) {
        self.pulses.push(pulse);
    }

    pub fn encode_binary(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self.pulses.len() as u32).to_le_bytes());
        for p in &self.pulses {
            buf.extend_from_slice(&p.channel_id.to_le_bytes());
            buf.push(p.waveform as u8);
            buf.extend_from_slice(&[0; 3]); // padding
            buf.extend_from_slice(&p.frequency_ghz.to_le_bytes());
            buf.extend_from_slice(&p.amplitude.to_le_bytes());
            buf.extend_from_slice(&p.duration_ns.to_le_bytes());
            buf.extend_from_slice(&p.phase_rad.to_le_bytes());
            buf.extend_from_slice(&p.drag_beta.to_le_bytes());
        }
        buf
    }
}
