//! ### TEvent
//! [TEvent] is a trait to manage application-specific events.
//! Using ruva framework, you can simply annotate struct as follows:
//! ```rust,no_run
//! #[derive(Serialize, Deserialize, Clone, TEvent)]
//! #[aggregate(CustomAggregate)]
//! #[internally_notifiable]
//! #[externally_notifiable]
//! pub struct CustomEvent {
//!     
//!     pub id: i64,
//!     pub custom_field: String,
//! }
//! ```
//! Here, `internally_notifiable` indicates that the event will be handled internally by `MessageBus`
//! And the `externally_notifiable` means that the event will be stored in the form of `OutBox` and
//! will be handled in the separate process (or thread)
use crate::prelude::OutBox;
use downcast_rs::{impl_downcast, Downcast};
use std::fmt::Debug;

pub trait TEvent: Sync + Send + Downcast {
	fn externally_notifiable(&self) -> bool {
		false
	}
	fn internally_notifiable(&self) -> bool {
		false
	}

	fn metadata(&self) -> EventMetadata {
		let event_name = std::any::type_name::<Self>().split("::").last().unwrap();
		EventMetadata { aggregate_id: Default::default(), aggregate_name: Default::default(), topic: event_name.to_string() }
	}
	fn outbox(&self) -> OutBox {
		let metadata = self.metadata();
		OutBox::new(metadata.aggregate_id, metadata.aggregate_name, metadata.topic, self.state())
	}

	fn state(&self) -> String;
}

impl_downcast!(TEvent);
impl Debug for dyn TEvent {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.metadata().topic)
	}
}

#[derive(Debug)]
pub struct EventMetadata {
	pub aggregate_id: String,
	pub aggregate_name: String,
	pub topic: String,
}

pub trait TCommand: 'static + Send + Sync + Debug {}
