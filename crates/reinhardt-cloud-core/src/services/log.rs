//! Log service module.

pub mod buffer;
pub mod local;

pub use buffer::LogBuffer;
pub use local::LocalLogService;
