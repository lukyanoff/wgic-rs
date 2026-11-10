//! Box 3a: client factories. A service that calls another one by instance id (from its config) uses
//! the factory here instead of building channels itself:
//!
//! ```ignore
//! let screens = ScreenSensorClients::new(&deps);
//! let client = screens.get("screen-1", Duration::from_secs(15));   // ScreenSensorServiceClient<Channel>
//! let shot = client.capture(CaptureRequest::default()).await?;
//! ```
//!
//! One file per service, all the same shape. New service → copy one file, change the names.

mod camera_sensor;
mod greet;
mod llm_caller;
mod screen_sensor;
mod visioner;

pub use camera_sensor::CameraSensorClients;
pub use greet::GreetClients;
pub use llm_caller::LlmCallerClients;
pub use screen_sensor::ScreenSensorClients;
pub use visioner::VisionerClients;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
