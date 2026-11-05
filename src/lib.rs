// tonic::Status is large by design; allow result_large_err across the crate
#![allow(clippy::result_large_err)]
// The double_must_use lint is raised by tonic/prost-generated code (include_proto!),
// which we cannot edit; allow it at crate level.
#![allow(clippy::double_must_use)]

pub mod pb {
    pub mod service {
        pub mod entry {
            tonic::include_proto!("service.entry");
        }
        pub mod budget {
            tonic::include_proto!("service.budget");
        }
    }
    pub mod common {
        pub mod base {
            tonic::include_proto!("common.base");
        }
    }
    pub mod shared {
        pub mod organization {
            tonic::include_proto!("shared.organization");
        }
        pub mod user {
            tonic::include_proto!("shared.user");
        }
    }
}

pub mod converters;
pub mod handler;
pub mod manager;
