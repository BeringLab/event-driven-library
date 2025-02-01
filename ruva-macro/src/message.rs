use proc_macro2::TokenStream;
use syn::{parse_quote, Data, DataStruct, DeriveInput, Fields, FieldsNamed, FnArg, ItemFn, Meta, MetaList, Pat, PatIdent, PatType, Path, Type};

use crate::utils::{get_attributes, get_trait_checking_stmts, locate_crate_on_derive_macro};

pub(crate) fn render_message_token(ast: &DeriveInput, visibilities: Vec<TokenStream>, externally_notifiable_event_req: Option<(TokenStream, TokenStream)>) -> TokenStream {
	let name = &ast.ident;
	let crates = locate_crate_on_derive_macro(ast);

	let (metadata_generator, impl_assertion) = externally_notifiable_event_req.unwrap_or_else(|| (TokenStream::new(), TokenStream::new()));

	quote! {
		impl #crates::TEvent for #name {

			#metadata_generator

			fn state(&self) -> ::std::string::String {
				serde_json::to_string(&self).expect("Failed to serialize")
			}

			#(#visibilities)*
		}
		impl #name{
			pub(crate) fn to_message(self)->  ::std::sync::Arc<dyn #crates::TEvent> {
				::std::sync::Arc::new(self)
			}
		}
		#impl_assertion
	}
}

pub(crate) fn render_event_visibility(ast: &DeriveInput) -> Vec<TokenStream> {
	let propagatability = ast
		.attrs
		.iter()
		.flat_map(|attr| match &attr.meta {
			Meta::Path(Path { segments, .. }) => segments
				.iter()
				.filter_map(|s| {
					let name = s.ident.to_string();
					if name == "internally_notifiable" {
						Some(quote!(
							fn internally_notifiable(&self) -> bool {
								true
							}
						))
					} else if name == "externally_notifiable" {
						panic!("Wrong use of externally_notifiable annotation\rExample: #[externally_notifiable(SomeAggregate)]")
					} else {
						None
					}
				})
				.collect::<Vec<_>>(),
			Meta::List(MetaList { path, .. }) => {
				if path.get_ident().unwrap().to_string().as_str() == "externally_notifiable" {
					vec![quote!(
						fn externally_notifiable(&self) -> bool {
							true
						}
					)]
				} else {
					vec![quote!()]
				}
			}
			_ => panic!("Notifiability was not specified!"),
		})
		.collect::<Vec<_>>();
	if propagatability.is_empty() {
		panic!("Either `internally_notifiable` or `externally_notifiable` must be given!")
	}
	propagatability
}

// first return token is for identifier, second return token is for aggregate assertion
pub(crate) fn extract_externally_notifiable_event_req(ast: &mut DeriveInput) -> Option<(TokenStream, TokenStream)> {
	let mut token: Option<(TokenStream, TokenStream)> = None;
	for attr in ast.attrs.iter_mut() {
		if let Meta::List(MetaList { path, tokens, .. }) = &mut attr.meta {
			let ident = path.get_ident();
			if ident.unwrap() != "externally_notifiable" {
				return None;
			}

			// * Asserting that the given type is TAggregate
			let quote = quote!(
				ruva::static_assertions::assert_impl_any!(#tokens: ruva::TAggregate);
			);

			let mut aggregate_name = String::new();
			tokens.clone().into_iter().for_each(|t| {
				if let proc_macro2::TokenTree::Ident(ident) = t {
					aggregate_name.push_str(ident.to_string().as_str());
				}
			});

			if aggregate_name.is_empty() {
				panic!("TAggregate name must be given for externally notifiable event!");
			}

			// ! Event Metadata is required only when it is externally notifiable
			token = Some((generate_event_metadata(ast, aggregate_name), quote));
			break;
		}
	}

	// let res = res.first().expect("aggregate must be specified! \rExample: #[aggregate(CustomAggregate)]\n").clone();
	// ast.attrs.remove(idx);

	token
}

pub(crate) fn generate_event_metadata(ast: &DeriveInput, aggregate_metadata: String) -> TokenStream {
	let name = &ast.ident;
	let crates = locate_crate_on_derive_macro(ast);

	match &ast.data {
		Data::Struct(DataStruct { fields: Fields::Named(FieldsNamed { named, .. }), .. }) => {
			let identifier = named.iter().filter(|f| get_attributes(f).into_iter().any(|ident| ident == *"identifier")).collect::<Vec<_>>();
			if identifier.len() != 1 {
				panic!("One identifier Must Be Given To TEvent!")
			}

			let ident = identifier.first().unwrap().ident.clone().unwrap().clone();

			quote!(
				fn metadata(&self) -> #crates::EventMetadata {
					#crates::EventMetadata{
					aggregate_id: self.#ident.to_string(),
					aggregate_name: #aggregate_metadata.into(),
					topic: stringify!(#name).into()
				}
			}
			)
		}
		_ => panic!("Only Struct Allowed!"),
	}
}

pub(crate) fn event_hook(mut ast: ItemFn) -> TokenStream {
	if ast.sig.inputs.is_empty() {
		panic!("There must be message argument!");
	};

	let mut stmts = get_trait_checking_stmts("::ruva::TAggregate");

	for aggregate in &ast.sig.inputs.iter().skip(1).collect::<Vec<_>>() {
		if let FnArg::Typed(PatType { pat, ty, .. }) = aggregate {
			let ty: Box<Type> = match *ty.clone() {
				Type::Reference(a) => a.elem,
				ty => Box::new(ty),
			};

			if let Pat::Ident(PatIdent { ident, .. }) = *pat.clone() {
				stmts.push(parse_quote!(
					if <IsTrait<#ty>>::IS_TRAIT {
						self.event_hook(<IsTrait<#ty>>::get_data(#ident));
					}
				));
			}
		}
	}
	stmts.extend(std::mem::take(&mut ast.block.stmts));
	ast.block.stmts = stmts;
	quote!(
		#ast
	)
}
