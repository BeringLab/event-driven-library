use crate::prelude::{Command, Message};
use crate::responses::{ApplicationError, ApplicationResponse, BaseError};
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::{
	any::{Any, TypeId},
	collections::HashMap,
	pin::Pin,
	sync::Arc,
};
use tokio::sync::RwLock;

pub type Future<T, E> = Pin<Box<dyn futures::Future<Output = Result<T, E>> + Send>>;
pub type AtomicContextManager = Arc<RwLock<ContextManager>>;

pub type TEventHandler<R, E> = HashMap<String, Vec<Box<dyn Fn(Box<dyn Message>, AtomicContextManager) -> Future<R, E> + Send + Sync>>>;

/// Task Local Context Manager
/// This is called for every time Messagebus.handle is invoked within which it manages events raised in service.
/// It spawns out Executor that manages transaction.
pub struct ContextManager {
	pub event_queue: VecDeque<Box<dyn Message>>,
}

impl ContextManager {
	/// Creation of context manager returns context manager AND event receiver
	pub fn new() -> AtomicContextManager {
		Arc::new(RwLock::new(Self { event_queue: VecDeque::new() }))
	}
}

impl Deref for ContextManager {
	type Target = VecDeque<Box<dyn Message>>;
	fn deref(&self) -> &Self::Target {
		&self.event_queue
	}
}
impl DerefMut for ContextManager {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.event_queue
	}
}

pub struct MessageBus<R: ApplicationResponse, E: ApplicationError> {
	command_handler: &'static TCommandHandler<R, E>,
	event_handler: &'static TEventHandler<R, E>,
}

impl<R, E> MessageBus<R, E>
where
	R: ApplicationResponse,
	E: ApplicationError + std::convert::From<crate::responses::BaseError> + std::convert::Into<crate::responses::BaseError>,
{
	pub fn new(command_handler: &'static TCommandHandler<R, E>, event_handler: &'static TEventHandler<R, E>) -> Arc<Self> {
		Self { command_handler, event_handler }.into()
	}

	pub async fn handle<C>(&self, message: C) -> Result<R, E>
	where
		C: Command,
	{
		println!("Handle Command {:?}", message);
		let context_manager = ContextManager::new();

		let res = self.command_handler.get(&message.type_id()).ok_or_else(|| {
			eprintln!("Unprocessable Command Given!");
			BaseError::CommandNotFound
		})?(Box::new(message), context_manager.clone())
		.await?;

		'event_handling_loop: loop {
			let event = context_manager.write().await.event_queue.pop_front();

			if let Some(msg) = event {
				if let Err(err) = self.handle_event(msg, context_manager.clone()).await {
					// ! Safety:: BaseError Must Be Enforced To Be Accepted As Variant On ServiceError
					eprintln!("{:?}", err);
				}
			} else {
				break 'event_handling_loop;
			}
		}
		Ok(res)
	}

	async fn handle_event(&self, msg: Box<dyn Message>, context_manager: AtomicContextManager) -> Result<(), E> {
		// ! msg.topic() returns the name of event. It is crucial that it corresponds to the key registered on Event Handler.

		let handlers = self.event_handler.get(&msg.metadata().topic).ok_or_else(|| {
			eprintln!("Unprocessable Event Given! {:?}", msg);
			BaseError::EventNotFound
		})?;

		println!("Handle Event : {:?}", msg);
		for handler in handlers.iter() {
			match handler(msg.message_clone(), context_manager.clone()).await {
				Ok(_val) => {
					eprintln!("Event Handling Succeeded!");
				}

				// ! Safety:: BaseError Must Be Enforced To Be Accepted As Variant On ServiceError
				Err(err) => match err.into() {
					BaseError::StopSentinel => {
						eprintln!("Stop Sentinel Arrived!");

						break;
					}
					BaseError::StopSentinelWithEvent(event) => {
						eprintln!("Stop Sentinel With Event Arrived!");
						context_manager.write().await.push_back(event);
						break;
					}
					err => {
						eprintln!("Error Occurred While Handling Event! Error:{:?}", err);
					}
				},
			};
		}
		drop(context_manager);
		Ok(())
	}
}

/// Dependency Initializer
/// You must initialize this in crate::dependencies with
/// Whatever dependency container you want.
#[macro_export]
macro_rules! create_dependency {
	() => {
		pub struct Dependency;
		pub fn dependency() -> &'static Dependency {
			static DEPENDENCY: ::std::sync::OnceLock<Dependency> = ::std::sync::OnceLock::new();
			DEPENDENCY.get_or_init(|| Dependency)
		}
	};
}

/// init_command_handler creating macro
/// Not that crate must have `Dependency` struct with its own implementation
pub type TCommandHandler<R, E> = HashMap<TypeId, fn(Box<dyn Any + Send + Sync>, AtomicContextManager) -> Future<R, E>>;

#[macro_export]
macro_rules! init_command_handler {
    (
        {$($command:ty:$handler:expr $(=>($($injectable:ident),*))? ),* $(,)?}
    )
        => {

		pub fn command_handler() -> &'static TCommandHandler<ServiceResponse, ServiceError> {
			extern crate self as current_crate;
			static COMMAND_HANDLER: ::std::sync::OnceLock<TCommandHandler<ServiceResponse, ServiceError>> = OnceLock::new();

			COMMAND_HANDLER.get_or_init(||{
				let dependency= current_crate::dependencies::dependency();
				let mut _map: TCommandHandler<ServiceResponse,ServiceError>= HashMap::new();
				$(
					_map.insert(
						// ! Only one command per one handler is acceptable, so the later insertion override preceding one.
						TypeId::of::<$command>(),

							|c:Box<dyn Any+Send+Sync>, context_manager: event_driven_library::prelude::AtomicContextManager|->Future<ServiceResponse,ServiceError> {
								// * Convert event so event handler accepts not Box<dyn Message> but `event_happend` type of message.
								// ! Logically, as it's from TypId of command, it doesn't make to cause an error.
								Box::pin($handler(
									*c.downcast::<$command>().unwrap(),
									context_manager,
								$(
									// * Injectable functions are added here.
									$(dependency.$injectable(),)*
								)?
							))
							},
					);
				)*
				_map
			})

		}
    };
}

/// init_event_handler creating macro
/// Not that crate must have `Dependency` struct with its own implementation
#[macro_export]
macro_rules! init_event_handler {
    (
        {$($event:ty: [$($handler:expr $(=>($($injectable:ident),*))? ),* $(,)? ]),* $(,)?}
    ) =>{
		pub fn event_handler() -> &'static TEventHandler<ServiceResponse, ServiceError>  {
			extern crate self as current_crate;
			static EVENT_HANDLER: ::std::sync::OnceLock<TEventHandler<ServiceResponse, ServiceError>> = OnceLock::new();

			EVENT_HANDLER.get_or_init(||{
            let dependency= current_crate::dependencies::dependency();

            let mut _map : TEventHandler<ServiceResponse, ServiceError> = HashMap::new();
            $(
                _map.insert(
                    stringify!($event).into(),
                    vec![
                        $(
                            Box::new(
                                |e:Box<dyn Message>, context_manager:event_driven_library::prelude::AtomicContextManager| -> Future<ServiceResponse,ServiceError>{
                                    Box::pin($handler(
                                        // * Convert event so event handler accepts not Box<dyn Message> but `event_happend` type of message.
                                        // Safety:: client should access this vector of handlers by providing the corresponding event name
                                        // So, when it is followed, it logically doesn't make sense to cause an error.
                                        *e.downcast::<$event>().expect("Not Convertible!"), context_manager,
                                    $(
                                        // * Injectable functions are added here.
                                        $(dependency.$injectable(),)*
                                    )?
                                    ))
                                }
                                ),
                        )*
                    ]
                );
            )*
            _map
        })
    }
}}
