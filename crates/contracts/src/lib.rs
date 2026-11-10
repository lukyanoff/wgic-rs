//! Generated protobuf messages, service traits and clients. Regenerate with `npm run gen`.
//! One module per proto package; add a `pub mod` block when you add a package.
#![allow(clippy::all)]

pub mod proto {
    pub mod greet {
        pub mod v1 {
            include!("gen/buffa/greet.v1.rs");
            include!("gen/connect/greet.v1.rs");
        }
    }
}
