//! Test doubles for sensor services (≈ a shared C# `FakeFrameSource` in a test-helpers package).
//! Public so every sensor service's integration tests can use it without a real device.

use std::collections::BTreeMap;

use crate::config::DeviceSelector;
use crate::domain::{CaptureError, DeviceInfo, FrameSource, RawFrame, pick_device};

/// Two fake 4x2 devices, `LEFT` (index 0) and `RIGHT` (index 1, the default).
/// With `error` set, every call fails with it instead.
pub struct FakeSource {
    pub kind: &'static str,
    pub error: Option<CaptureError>,
}

impl FakeSource {
    pub fn new(kind: &'static str) -> Self {
        Self { kind, error: None }
    }

    pub fn failing(kind: &'static str, error: CaptureError) -> Self {
        Self { kind, error: Some(error) }
    }

    fn devices() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo { index: 0, name: "LEFT".into(), is_default: false },
            DeviceInfo { index: 1, name: "RIGHT".into(), is_default: true },
        ]
    }
}

impl FrameSource for FakeSource {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn kind(&self) -> &'static str {
        self.kind
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, CaptureError> {
        match &self.error {
            Some(error) => Err(error.clone()),
            None => Ok(Self::devices()),
        }
    }

    fn grab(&self, device: &DeviceSelector) -> Result<RawFrame, CaptureError> {
        let devices = self.list_devices()?;
        let index = pick_device(&devices, device)?;
        let mut attributes = BTreeMap::new();
        attributes.insert("fake".to_string(), "yes".to_string());
        Ok(RawFrame {
            rgba: vec![0; 4 * 2 * 4],
            width: 4,
            height: 2,
            device_name: devices[index].name.clone(),
            device_index: devices[index].index,
            attributes,
        })
    }
}
