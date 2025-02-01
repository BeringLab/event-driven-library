use proc_macro::TokenStream;
use syn::DeriveInput;

use crate::utils::{find_enum_variant, locate_crate_on_derive_macro};

pub(crate) fn render_response_token(ast: &DeriveInput) -> TokenStream {
	let syn::Data::Enum(_data) = &ast.data else { panic!("Only Enum type is supported by #[derive(ApplicationError)].") };
	let name = &ast.ident;
	let crates = locate_crate_on_derive_macro(ast);

	quote! {
		impl #crates::ApplicationResponse for #name{}

	}
	.into()
}

pub(crate) fn render_error_token(ast: &DeriveInput) -> TokenStream {
	// Forcing target to be enum
	let data_enum = match &ast.data {
		syn::Data::Enum(data) => data,
		_ => {
			panic!("Only Enum type is supported by #[derive(ApplicationError)].")
		}
	};

	let name = &ast.ident;

	let find_variant = find_enum_variant(data_enum);

	/* \#\[crates(...)\] */
	let crates = locate_crate_on_derive_macro(ast);

	/* \#\[stop_sentinel\] */
	let stop_sentinel = find_variant("stop_sentinel");
	if let Some(stop_sentinel) = stop_sentinel {
		if let syn::Fields::Unit = stop_sentinel.fields {
		} else {
			panic!("#[stop_sentinel] expects unit.")
		}
	}
	let stop_sentinel = if let Some(stop_sentinel) = stop_sentinel { stop_sentinel.ident.clone() } else { syn::Ident::new("StopSentinel", proc_macro2::Span::call_site()) };

	/* \#\[stop_sentinel_with_event\] */
	let stop_sentinel_with_event = find_variant("stop_sentinel_with_event");
	if let Some(stop_sentinel_with_event) = stop_sentinel_with_event {
		if let syn::Fields::Unnamed(_) = stop_sentinel_with_event.fields {
		} else {
			panic!("#[stop_sentinel_with_event] expects Field(TEvent).")
		}
	}
	let stop_sentinel_with_event =
		if let Some(stop_sentinel_with_event) = stop_sentinel_with_event { stop_sentinel_with_event.ident.clone() } else { syn::Ident::new("StopSentinelWithEvent", proc_macro2::Span::call_site()) };
	let stop_sentinel_with_event_type = if let syn::Fields::Unnamed(field) =
		&data_enum.variants.iter().find(|x| x.ident == stop_sentinel_with_event).expect("#[stop_sentinel_with_event] and StopSentinelWithEvent field not found.").fields
	{
		if field.unnamed.len() == 1 {
			field.unnamed[0].ty.clone()
		} else {
			panic!("#[stop_sentinel_with_event] expects Field(TEvent).");
		}
	} else {
		panic!("StopSentinelWithEvent field expects Field(TEvent).")
	};

	/* \#\[database_error\] */
	let database_error = find_variant("database_error");
	if let Some(database_error) = database_error {
		if let syn::Fields::Unnamed(_) = database_error.fields {
		} else {
			panic!("#[database_error] expects Field(Box<AnyError>).")
		}
	}
	let database_error = if let Some(database_error) = database_error { database_error.ident.clone() } else { syn::Ident::new("DatabaseError", proc_macro2::Span::call_site()) };

	quote!(
		impl #crates::ApplicationError for #name {}

		impl ::std::convert::From<#crates::BaseError> for #name {
			fn from(value: #crates::BaseError) -> Self {
				match value {
					#crates::BaseError::StopSentinel => Self::#stop_sentinel,
					#crates::BaseError::StopSentinelWithEvent(event) => Self::#stop_sentinel_with_event(event),
					#crates::BaseError::DatabaseError(error) => Self::#database_error(error),
					err => Self::BaseError(err),
				}
			}
		}

		impl ::std::convert::From<#name> for #crates::BaseError {
			fn from(value: #name) -> Self {
				let data = match value {
					#name::#stop_sentinel => #crates::BaseError::StopSentinel,
					#name::#stop_sentinel_with_event(event) => #crates::BaseError::StopSentinelWithEvent(event),
					#name::#database_error(error) => #crates::BaseError::DatabaseError(error),
					// _ => #crates::BaseError::ServiceError(::std::boxed::Box::new(value)),
					_=> #crates::BaseError::ServiceError,
				};
				data
			}
		}
		// #crates::static_assertions::assert_impl_all!(#stop_sentinel_with_event_type: ::std::sync::Arc<dyn #crates::TEvent>);
		#crates::static_assertions::assert_type_eq_all!(#stop_sentinel_with_event_type, ::std::sync::Arc<dyn #crates::TEvent>);
	)
	.into()
}
