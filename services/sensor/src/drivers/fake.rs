//! `FakeSource`: a stand-in driver for tests, so they need no screen or camera.

use connectrpc::ConnectError;
use contracts::proto::sensor::v1::DeviceInfo;

use super::{FrameSource, RawFrame, device, pick_device};

/// Two fake 4x2 devices, `LEFT` (0) and `RIGHT` (1, the default). With `error` set, every call fails with it.
pub struct FakeSource {
    pub error: Option<ConnectError>,
}

impl FrameSource for FakeSource {
    fn kind(&self) -> &'static str {
        "screen"
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, ConnectError> {
        match &self.error {
            Some(error) => Err(error.clone()),
            None => Ok(vec![device(0, "LEFT".into(), false), device(1, "RIGHT".into(), true)]),
        }
    }

    fn grab(&self, wanted: Option<u32>) -> Result<RawFrame, ConnectError> {
        let devices = self.list_devices()?;
        let position = pick_device(&devices, wanted)?;
        Ok(RawFrame { rgba: vec![0; 4 * 2 * 4], width: 4, height: 2, device: devices[position].clone() })
    }
}
