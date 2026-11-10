//! NATS micro adapter for connectrpc.
//!
//! NATS carries Connect-protocol **unary** messages, so every service registered in a
//! [`connectrpc::Router`] can be hosted on NATS ([`serve_nats`]) and every generated
//! client can call over NATS ([`NatsTransport`]), with no per-service code.
//!
//! Wire format:
//! - subject: `<prefix>.<package.Service>.<Method>`, e.g. `rpc.greet.v1.GreetService.Greet`
//! - micro service name: the module name (e.g. `greet`); endpoint name `<Service>-<Method>`
//! - request: payload = message, header `Content-Type` = `application/json` (default) or
//!   `application/proto`; other headers are passed through as RPC metadata
//! - success reply: payload = message, header `Content-Type`
//! - error reply: empty payload, headers `Nats-Service-Error` (message),
//!   `Nats-Service-Error-Code` (HTTP status) and `Connect-Code` (e.g. `invalid_argument`)

mod client;
mod server;
mod wire;

pub use client::NatsTransport;
pub use server::serve_nats;
pub use wire::{CONNECT_CODE, NatsConfig, code_for_status, endpoint_name, subject_for};
