//! Proto ⇄ shared sensor logic. All real work is in `sensor_base::SensorRpc` (≈ the base class);
//! this file only wraps its results into `screen_sensor.v1` messages.

use connectrpc::{RequestContext, Response, ServiceRequest, ServiceResult};
use contracts::proto::screen_sensor::v1::{
    CaptureRequest, CaptureResponse, ListDevicesRequest, ListDevicesResponse, ScreenSensorService,
};
use sensor_base::SensorRpc;

/// ≈ `class ScreenSensorHandler : SensorBase` — but by holding the base, not inheriting it.
pub struct ScreenSensorHandler {
    rpc: SensorRpc,
}

impl ScreenSensorHandler {
    pub fn new(rpc: SensorRpc) -> Self {
        Self { rpc }
    }
}

#[allow(refining_impl_trait)]
impl ScreenSensorService for ScreenSensorHandler {
    async fn capture(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, CaptureRequest>,
    ) -> ServiceResult<CaptureResponse> {
        let reply = self.rpc.capture().await?;
        Response::ok(CaptureResponse { image: reply.image, metadata: reply.metadata.into(), ..Default::default() })
    }

    async fn list_devices(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, ListDevicesRequest>,
    ) -> ServiceResult<ListDevicesResponse> {
        let devices = self.rpc.list_devices().await?;
        Response::ok(ListDevicesResponse { devices, ..Default::default() })
    }
}
