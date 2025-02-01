use super::executor::TConnection;
use crate::{make_smart_pointer, prelude::TEvent};
use std::{collections::VecDeque, sync::Arc};

/// Request Context Manager
/// it lives as long as the request lives

pub struct ContextManager {
	pub event_queue: VecDeque<Arc<dyn TEvent>>,
	pub conn: &'static dyn TConnection,
}

pub type AtomicContextManager = Arc<ContextManager>;

impl ContextManager {
	/// Creation of context manager returns context manager AND event receiver
	pub fn new(conn: &'static dyn TConnection) -> Self {
		Self { event_queue: VecDeque::new(), conn }
	}

	/// SAFETY: This is safe because we are sure this method is used only in the context of command and event handling
	pub(crate) fn get_mut<'a>(self: &Arc<Self>) -> &'a mut ContextManager {
		unsafe { &mut *(Arc::as_ptr(self) as *mut ContextManager) }
	}
}

make_smart_pointer!(ContextManager, VecDeque<Arc<dyn TEvent>>, event_queue);

/// Local context
/// it lasts only until logical unit of operation is done
pub struct Context {
	pub(crate) curr_events: VecDeque<std::sync::Arc<dyn TEvent>>,
	pub(crate) super_ctx: AtomicContextManager,

	#[cfg(feature = "sqlx-postgres")]
	pub(crate) pg_transaction: Option<sqlx::Transaction<'static, sqlx::Postgres>>,
}

impl Context {
	pub fn new(super_ctx: AtomicContextManager) -> Self {
		Self {
			curr_events: Default::default(),
			super_ctx,
			#[cfg(feature = "sqlx-postgres")]
			pg_transaction: None,
		}
	}

	pub fn event_hook(&mut self, aggregate: &mut impl crate::prelude::TAggregate) {
		self.set_current_events(aggregate.take_events());
	}

	pub async fn send_internally_notifiable_messages(&mut self) {
		// SAFETY: This is safe because we are sure that the context manager is not dropped

		self.curr_events.iter().filter(|e| e.internally_notifiable()).for_each(|e| self.super_ctx.get_mut().push_back(e.clone()));
	}
}

pub trait TSetCurrentEvents: Send + Sync {
	fn set_current_events(&mut self, events: VecDeque<std::sync::Arc<dyn TEvent>>);
}

impl TSetCurrentEvents for Context {
	fn set_current_events(&mut self, events: VecDeque<std::sync::Arc<dyn TEvent>>) {
		self.curr_events.extend(events)
	}
}

#[tokio::test]
async fn test_context_managers() {
	struct CustomConnection;
	#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
	struct CustomEvent(usize);
	impl TEvent for CustomEvent {
		fn externally_notifiable(&self) -> bool {
			true
		}
		fn internally_notifiable(&self) -> bool {
			true
		}
		fn state(&self) -> String {
			"state".to_string()
		}
	}
	impl TConnection for CustomConnection {}

	async fn add_event_to_queue(context_manager: Arc<ContextManager>, order: usize) {
		context_manager.get_mut().push_back(std::sync::Arc::new(CustomEvent(order)));
	}

	let context_manager = Arc::new(ContextManager::new(&CustomConnection));

	let count = 10000000;
	let futures = (0..count).map(|order| add_event_to_queue(Arc::clone(&context_manager), order));
	futures::future::join_all(futures).await;

	assert_eq!(context_manager.len(), count);
	let events = context_manager.iter().map(|e| e.downcast_ref::<CustomEvent>().unwrap().0).collect::<Vec<_>>();
	assert_eq!(events, (0..count).collect::<Vec<_>>());
}
