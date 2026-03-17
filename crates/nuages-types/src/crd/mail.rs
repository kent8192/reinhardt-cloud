use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Mail (SMTP) configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MailSpec {
	/// SMTP host
	pub smtp_host: Option<String>,
	/// SMTP port (1-65535)
	pub smtp_port: Option<i32>,
	/// Secret name containing SMTP credentials
	pub credentials_secret: Option<String>,
}

impl MailSpec {
	/// Validates the mail specification
	pub fn validate(&self) -> Result<(), String> {
		if let Some(port) = self.smtp_port
			&& !(1..=65535).contains(&port)
		{
			return Err("mail.smtp_port must be between 1 and 65535".to_string());
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_mail_spec_valid_port() {
		// Arrange
		let spec = MailSpec {
			smtp_host: Some("smtp.example.com".to_string()),
			smtp_port: Some(587),
			credentials_secret: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_mail_spec_rejects_zero_port() {
		// Arrange
		let spec = MailSpec {
			smtp_host: None,
			smtp_port: Some(0),
			credentials_secret: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_mail_spec_rejects_negative_port() {
		// Arrange
		let spec = MailSpec {
			smtp_host: None,
			smtp_port: Some(-1),
			credentials_secret: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_mail_spec_rejects_port_above_65535() {
		// Arrange
		let spec = MailSpec {
			smtp_host: None,
			smtp_port: Some(65536),
			credentials_secret: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_mail_spec_allows_none_port() {
		// Arrange
		let spec = MailSpec {
			smtp_host: Some("smtp.example.com".to_string()),
			smtp_port: None,
			credentials_secret: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_mail_spec_accepts_boundary_ports() {
		// Arrange
		let spec_min = MailSpec {
			smtp_host: None,
			smtp_port: Some(1),
			credentials_secret: None,
		};
		let spec_max = MailSpec {
			smtp_host: None,
			smtp_port: Some(65535),
			credentials_secret: None,
		};
		// Act
		let result_min = spec_min.validate();
		let result_max = spec_max.validate();
		// Assert
		assert!(result_min.is_ok());
		assert!(result_max.is_ok());
	}
}
