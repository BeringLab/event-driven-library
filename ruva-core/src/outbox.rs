use chrono::{DateTime, Utc};

use crate::prelude::SnowFlake;

#[derive(Debug, Clone)]
pub struct OutBox {
	pub id: i64,
	pub aggregate_id: String,
	pub aggregate_name: String,
	pub topic: String,
	pub state: String,
	pub processed: bool,
	pub create_dt: DateTime<Utc>,
}

impl OutBox {
	pub fn new(aggregate_id: String, aggregate_name: String, topic: String, state: String) -> Self {
		Self { id: *SnowFlake::generate(), aggregate_id, aggregate_name, topic, state, processed: false, create_dt: Default::default() }
	}
}
