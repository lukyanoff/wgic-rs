//! Proto ⇄ shared sensor logic. All real work is in `sensor_base::SensorRpc` (≈ the base class);
//! this file only wraps its results into `camera_sensor.v1` messages.

use connectrpc::{RequestContext, Response, ServiceRequest, ServiceResult};
use contracts::proto::camera_sensor::v1::{
    CameraSensorService, CaptureRequest, CaptureResponse, ListDevicesRequest, ListDevicesResponse,
};
use sensor_base::SensorRpc;

/// ≈ `class CameraSensorHandler : SensorBase` — but by holding the base, not inheriting it.
pub struct CameraSensorHandler {
    rpc: SensorRpc,
}

impl CameraSensorHandler {
    pub fn new(rpc: SensorRpc) -> Self {
        Self { rpc }
    }
}

#[allow(refining_impl_trait)]
impl CameraSensorService for CameraSensorHandler {
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
