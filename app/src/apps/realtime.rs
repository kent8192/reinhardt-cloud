//! Real-time WebSocket module for deployment monitoring and notifications.

pub mod broadcaster;

pub use broadcaster::{ConnectionHandle, WsBroadcaster};
