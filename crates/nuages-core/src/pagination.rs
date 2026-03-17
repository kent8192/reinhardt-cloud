//! Pagination types for list API responses.

use serde::{Deserialize, Serialize};

/// Default page number (1-indexed).
const DEFAULT_PAGE: u64 = 1;

/// Default number of items per page.
const DEFAULT_PAGE_SIZE: u64 = 20;

/// Maximum allowed page size to prevent excessive resource usage.
const MAX_PAGE_SIZE: u64 = 100;

/// Query parameters for paginated list endpoints.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PaginationParams {
	/// Page number (1-indexed, defaults to 1).
	page: Option<u64>,
	/// Number of items per page (defaults to 20, max 100).
	page_size: Option<u64>,
}

impl PaginationParams {
	/// Create new pagination parameters with explicit values.
	pub fn new(page: Option<u64>, page_size: Option<u64>) -> Self {
		Self { page, page_size }
	}

	/// Return the effective page number (1-indexed).
	pub fn page(&self) -> u64 {
		self.page.unwrap_or(DEFAULT_PAGE).max(1)
	}

	/// Return the effective page size, clamped to [`MAX_PAGE_SIZE`].
	pub fn page_size(&self) -> u64 {
		self.page_size
			.unwrap_or(DEFAULT_PAGE_SIZE)
			.clamp(1, MAX_PAGE_SIZE)
	}

	/// Calculate the SQL OFFSET value for the current page.
	pub fn offset(&self) -> u64 {
		(self.page() - 1) * self.page_size()
	}
}

/// Paginated response wrapper for list endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
	/// Items on the current page.
	pub items: Vec<T>,
	/// Total number of items across all pages.
	pub total: u64,
	/// Current page number (1-indexed).
	pub page: u64,
	/// Number of items per page.
	pub page_size: u64,
	/// Total number of pages.
	pub total_pages: u64,
}

impl<T: Serialize> PaginatedResponse<T> {
	/// Create a new paginated response from items, total count, and pagination params.
	pub fn new(items: Vec<T>, total: u64, params: &PaginationParams) -> Self {
		let page_size = params.page_size();
		let total_pages = if total == 0 {
			0
		} else {
			total.div_ceil(page_size)
		};

		Self {
			items,
			total,
			page: params.page(),
			page_size,
			total_pages,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_pagination_params_defaults() {
		// Arrange
		let params = PaginationParams::default();

		// Act & Assert
		assert_eq!(params.page(), DEFAULT_PAGE);
		assert_eq!(params.page_size(), DEFAULT_PAGE_SIZE);
		assert_eq!(params.offset(), 0);
	}

	#[rstest]
	fn test_pagination_params_custom_values() {
		// Arrange
		let params = PaginationParams::new(Some(3), Some(10));

		// Act & Assert
		assert_eq!(params.page(), 3);
		assert_eq!(params.page_size(), 10);
		assert_eq!(params.offset(), 20);
	}

	#[rstest]
	fn test_pagination_params_page_zero_clamped_to_one() {
		// Arrange
		let params = PaginationParams::new(Some(0), Some(10));

		// Act & Assert
		assert_eq!(params.page(), 1);
		assert_eq!(params.offset(), 0);
	}

	#[rstest]
	fn test_pagination_params_page_size_clamped_to_max() {
		// Arrange
		let params = PaginationParams::new(Some(1), Some(500));

		// Act & Assert
		assert_eq!(params.page_size(), MAX_PAGE_SIZE);
	}

	#[rstest]
	fn test_pagination_params_page_size_zero_clamped_to_one() {
		// Arrange
		let params = PaginationParams::new(Some(1), Some(0));

		// Act & Assert
		assert_eq!(params.page_size(), 1);
	}

	#[rstest]
	fn test_pagination_params_offset_calculation() {
		// Arrange
		let params = PaginationParams::new(Some(5), Some(25));

		// Act
		let offset = params.offset();

		// Assert
		assert_eq!(offset, 100); // (5 - 1) * 25 = 100
	}

	#[rstest]
	fn test_paginated_response_creation() {
		// Arrange
		let params = PaginationParams::new(Some(1), Some(2));
		let items = vec!["a", "b"];

		// Act
		let response = PaginatedResponse::new(items, 5, &params);

		// Assert
		assert_eq!(response.items, vec!["a", "b"]);
		assert_eq!(response.total, 5);
		assert_eq!(response.page, 1);
		assert_eq!(response.page_size, 2);
		assert_eq!(response.total_pages, 3); // ceil(5 / 2)
	}

	#[rstest]
	fn test_paginated_response_empty() {
		// Arrange
		let params = PaginationParams::default();
		let items: Vec<String> = vec![];

		// Act
		let response = PaginatedResponse::new(items, 0, &params);

		// Assert
		assert_eq!(response.items.len(), 0);
		assert_eq!(response.total, 0);
		assert_eq!(response.total_pages, 0);
	}

	#[rstest]
	fn test_paginated_response_exact_division() {
		// Arrange
		let params = PaginationParams::new(Some(1), Some(5));
		let items = vec![1, 2, 3, 4, 5];

		// Act
		let response = PaginatedResponse::new(items, 10, &params);

		// Assert
		assert_eq!(response.total_pages, 2); // 10 / 5 = 2
	}
}
