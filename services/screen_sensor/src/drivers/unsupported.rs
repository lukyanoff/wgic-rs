//! Stand-in for a driver that does not exist on this OS (e.g. `linux-x11` on Windows).

use crate::config::ScreenSelector;
use crate::domain::{CaptureDriver, CaptureError, RawFrame};

pub struct UnsupportedDriver {
    pub name: &'static str,
}

impl CaptureDriver for UnsupportedDriver {
    fn name(&self) -> &'static str {
        self.name
    }

    fn capture(&self, _screen: &ScreenSelector) -> Result<RawFrame, CaptureError> {
        Err(CaptureError::DriverNotSupported { driver: self.name })
    }
}
