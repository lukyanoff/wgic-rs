pub mod proto {
    pub mod greet {
        pub mod v1 {
            include!("gen/buffa/greet.v1.rs");
            include!("gen/connect/greet.v1.rs");
        }
    }
}
