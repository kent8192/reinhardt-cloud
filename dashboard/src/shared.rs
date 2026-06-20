//! Shared types and errors for WASM client and server communication.

pub mod client;
pub mod errors;
pub mod types;
pub mod ws_messages;

pub use errors::{AppError, FieldError};
pub use types::{AuthResponse, UserInfo};
pub use ws_messages::{
	DeploymentState, DeploymentStatusPayload, NotificationLevel, SystemNotificationPayload,
	WsClientMessage, WsMessage,
};
