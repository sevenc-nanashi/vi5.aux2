#[allow(dead_code)]
pub mod generated {
    pub mod common {
        tonic::include_proto!("common");
    }
    pub mod serverjs {
        tonic::include_proto!("serverjs");
    }
    pub mod libserver {
        tonic::include_proto!("libserver");
    }
}

pub use generated::serverjs::*;
pub use generated::libserver::*;
pub use generated::common::*;
