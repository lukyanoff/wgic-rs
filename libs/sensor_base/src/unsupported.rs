//! Stand-in for a driver that does not exist on this OS (e.g. `linux-x11` on Windows).
//! The service still starts; every call answers `failed_precondition`.

use crate::config::DeviceSelector;
use crate::domain::{CaptureError, DeviceInfo, FrameSource, RawFrame};

pub struct UnsupportedSource {
    pub name: &'static str,
    pub kind: &'static str,
}

impl FrameSource for UnsupportedSource {
    fn name(&self) -> &'static str {
        self.name
    }

    fn kind(&self) -> &'static str {
        self.kind
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, CaptureError> {
        Err(CaptureError::DriverNotSupported { driver: self.name })
    }

    fn grab(&self, _device: &DeviceSelector) -> Result<RawFrame, CaptureError> {
        Err(CaptureError::DriverNotSupported { driver: self.name })
    }
}
