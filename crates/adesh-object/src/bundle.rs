//! AdobBundle: Fat binary and multi-architecture / heterogeneous container.

use crate::format::AdobObject;
use crate::target::{Architecture, ComputeDevice, TargetDescriptor};

pub const BUNDLE_MAGIC: &[u8; 4] = b"ADBB";
pub const BUNDLE_VERSION: u16 = 1;

/// Entry in an ADOB fat bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    pub target: TargetDescriptor,
    pub object: AdobObject,
}

/// Heterogeneous / Multi-architecture Fat Binary bundle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdobBundle {
    pub entries: Vec<BundleEntry>,
}

impl AdobBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_object(&mut self, object: AdobObject) {
        let target = object.target.clone();
        self.entries.push(BundleEntry { target, object });
    }

    /// Find an object that matches the requested target architecture and device.
    pub fn find_matching(&self, arch: &Architecture, device: ComputeDevice) -> Option<&AdobObject> {
        self.entries
            .iter()
            .find(|e| &e.target.architecture == arch && e.target.device == device)
            .map(|e| &e.object)
    }

    /// Find all accelerator modules (GPU, NPU, TPU) in the bundle.
    pub fn find_accelerators(&self) -> Vec<&AdobObject> {
        self.entries
            .iter()
            .filter(|e| e.target.device != ComputeDevice::Cpu)
            .map(|e| &e.object)
            .collect()
    }
}
