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
        pub static FILE_DESCRIPTOR_SET: &[u8] =
            tonic::include_file_descriptor_set!("libserver_descriptor");
    }
}

pub use generated::*;
