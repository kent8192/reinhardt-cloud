//! Real-time WebSocket module for deployment monitoring and notifications.

pub mod broadcaster;
pub mod consumer;

pub use broadcaster::{ConnectionHandle, WsBroadcaster};
pub use consumer::NotificationConsumer;
