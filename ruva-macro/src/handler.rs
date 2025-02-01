use proc_macro::TokenStream;

use syn::{punctuated::Punctuated, token::Comma, FnArg, Ident, ImplItemFn, ItemFn, Pat, PatIdent, PatType, ReturnType, Signature};

#[allow(unused)]
pub fn parse_handler(ast: ItemFn) -> TokenStream {
	const OUTPUT_TYPE_NOT_VALID: &str = "#[handler] fn must have valid output type";
	let ItemFn { sig: Signature { ident, output: ReturnType::Type(_, var), inputs, generics, asyncness, .. }, block, .. } = ast else { panic!("{}", OUTPUT_TYPE_NOT_VALID) };

	if inputs.is_empty() {
		panic!("There must be message argument!");
	}

	let message = inputs.first().unwrap();

	//TODO Restrict message to type that implements either TCommand or TEvent OR impl Trait
	let message_type = match message.clone() {
		FnArg::Typed(PatType { ty, .. }) => *ty,
		_ => panic!(""),
	};

	let mut args = inputs.iter().skip(1).cloned().collect::<Punctuated<FnArg, Comma>>();
	let flagged_args: Vec<(FnArg, bool)> = flag_context(&mut args);

	// Get idents. This will be passed to inner(or redefined) function
	let idents = get_puntuated_idents(inputs.clone());

	// injectables
	let injectables = take_injectables(flagged_args);

	let generic_where = &generics.where_clause;

	quote!(
		// * Check if the first argument is of either TCommand or TEvent


		// ::ruva::static_assertions::assert_impl_any!(
		// 	#message_type:
		// 	::ruva::TEvent,
		// 	::ruva::TCommand
		// );
		pub #asyncness fn #ident (#message,context: ::ruva::AtomicContextManager)-> #var {

			#asyncness fn inner #generics (#message,#args)->#var #generic_where{

				#block
			};
			// let dependency= crate::dependencies::dependency();
			#(
				#injectables
			)*

			inner(#idents).await
			}
	)
	.into()
}

#[allow(unused)]
fn get_puntuated_idents(inputs: Punctuated<FnArg, Comma>) -> Punctuated<Ident, Comma> {
	inputs
		.into_iter()
		.filter_map(|i| {
			if let FnArg::Typed(PatType { pat, .. }) = i {
				match *pat {
					Pat::Ident(PatIdent { ref ident, .. }) => Some(syn::Ident::new(&ident.to_string(), proc_macro2::Span::call_site())),
					_ => panic!("Not Allowed!"),
				}
			} else {
				None
			}
		})
		.collect()
}

#[allow(unused)]
fn flag_context(args: &mut Punctuated<FnArg, Comma>) -> Vec<(FnArg, bool)> {
	let mut container = vec![];

	for arg in args.into_iter() {
		let mut context_injection_required = false;

		if let FnArg::Typed(PatType { ref mut attrs, .. }) = arg {
			// * Take initial length
			let init_len = attrs.len();
			attrs.retain(|x| !x.path().is_ident("context"));

			// TODO if attribute other than `context` is passed, cause panic

			// * If initial length is not the same after ratain method is applied, it means 'context' attribute has been idenfitied
			if init_len != attrs.len() {
				context_injection_required = true;
			}
		}
		container.push((arg.clone(), context_injection_required))
	}
	container
}

fn take_injectables(inputs: Vec<(FnArg, bool)>) -> Vec<proc_macro2::TokenStream> {
	inputs
		.into_iter()
		.filter_map(|i| {
			// TODO get attributes on function parameter to sort out context, context-injectable and context-agnostic-injectables

			match i {
				(FnArg::Typed(PatType { pat, .. }), false) => match *pat {
					Pat::Ident(PatIdent { ident, .. }) => Some(quote!(

						let #ident = crate::dependencies::#ident();
					)),
					_ => panic!("Not Allowed!"),
				},
				(FnArg::Typed(PatType { pat, .. }), true) => match *pat {
					Pat::Ident(PatIdent { ident, .. }) => Some(quote!(


						let #ident = crate::dependencies::#ident(context.clone());
					)),
					_ => panic!("Not Allowed!"),
				},
				_ => None,
			}
		})
		.collect::<Vec<_>>()
}

#[allow(unused)]
pub fn parse_event_handler(ast: ImplItemFn) -> TokenStream {
	const PREFIX: &str = "__";
	let handler_name = syn::Ident::new((PREFIX.to_string() + ast.sig.ident.to_string().as_str()).as_str(), proc_macro2::Span::call_site());
	let idents = get_puntuated_idents(ast.sig.inputs.clone()).iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");

	let const_declaration = quote!(
		pub const #handler_name: &str = #idents;
	);

	quote!(
		#ast
		#const_declaration


	)
	.into()
}
