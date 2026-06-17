//! Real-time WebSocket module for deployment monitoring and notifications.

pub mod broadcaster;
pub mod consumer;

pub use broadcaster::{WsBroadcaster, WsBroadcasterKey};
pub use consumer::NotificationConsumer;
