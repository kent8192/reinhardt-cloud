//! Bounded live-log presentation with explicit continuity state.

use std::collections::VecDeque;

pub struct LogWindow {
	rows: VecDeque<String>,
	bytes: usize,
	max_rows: usize,
	max_bytes: usize,
	pub continuity_lost: bool,
}

impl LogWindow {
	pub fn new(max_rows: usize, max_bytes: usize) -> Self {
		Self {
			rows: VecDeque::new(),
			bytes: 0,
			max_rows,
			max_bytes,
			continuity_lost: false,
		}
	}

	pub fn push(&mut self, row: String) {
		if row.len() > self.max_bytes || self.max_rows == 0 {
			self.continuity_lost = true;
			return;
		}
		self.bytes += row.len();
		self.rows.push_back(row);
		while self.rows.len() > self.max_rows || self.bytes > self.max_bytes {
			if let Some(removed) = self.rows.pop_front() {
				self.bytes -= removed.len();
				self.continuity_lost = true;
			}
		}
	}

	pub fn rows(&self) -> &VecDeque<String> {
		&self.rows
	}
	pub fn retained_bytes(&self) -> usize {
		self.bytes
	}
}
