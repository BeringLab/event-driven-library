use std::{collections::HashMap, sync::LazyLock};

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::ToTokens;
use syn::{parse_macro_input, parse_quote, punctuated::Punctuated, token::Comma, FnArg, ImplItem, ItemFn, ItemImpl, PatParen};

static TRAIT_IMPL_COUNTER: LazyLock<std::sync::Arc<std::sync::RwLock<HashMap<String, i8>>>> = LazyLock::new(Default::default);
fn raise_impl_counter(key: &str) {
	let mut guard = TRAIT_IMPL_COUNTER.write().unwrap();
	*guard.entry(key.to_string()).or_insert(0) += 1;
}

fn impl_generator(trait_info: &syn::Path, redeined_methods: &[String]) -> TokenStream2 {
	(2..6)
		.map(|order| {
			let redefiend_methods = redeined_methods.iter().map(|method| syn::parse_str::<syn::ItemFn>(method).expect("Error occurred while generating impl!")).collect::<Vec<_>>();

			let idents: Vec<_> = (2..order + 1).map(|i| syn::Ident::new(&format!("D{}", i), proc_macro2::Span::call_site())).collect();

			quote! {
				impl<D1: #trait_info  #(,#idents : Send+Sync )*> #trait_info for (D1 #(,#idents)*) {
					#(#redefiend_methods)*
				}
			}
		})
		.collect()
}

pub(crate) fn render_inject(input: TokenStream, _attrs: TokenStream) -> TokenStream {
	// Parse the input as a trait definition
	let input = parse_macro_input!(input as ItemImpl);

	// Collect the signatures of all methods in the trait
	let trait_info = input.trait_.clone().unwrap().1;
	let key = trait_info.to_token_stream().to_string();
	raise_impl_counter(key.as_str());

	let mut redefined_methods = Vec::new();

	// Iterate through methods and find where generic parameters are used
	for item in &input.items {
		if let ImplItem::Fn(method) = item {
			let asyncness = match method.sig.asyncness {
				Some(_) => ".await",
				None => "",
			};

			let method_name = method.sig.ident.to_string();
			let signature = method.sig.to_token_stream().to_string();

			let args: String = method
				.sig
				.inputs
				.iter()
				.flat_map(|arg| match arg {
					FnArg::Typed(pat_type) => {
						let pat = pat_type.pat.to_token_stream().to_string();
						if pat.contains("self") {
							None
						} else {
							Some(pat)
						}
					}
					_ => None,
				})
				.collect::<Vec<String>>()
				.join(", ");

			// ! SPECIAL_CHAR is used to indicate the order of the generic parameter
			let formatted_method = format!("{signature} {{  self.0.{method_name}({args}){asyncness}}}");

			redefined_methods.push(formatted_method);
		}
	}

	let count = *TRAIT_IMPL_COUNTER.read().unwrap().get(&key).unwrap();
	let impls = if count > 1 { quote!() } else { impl_generator(&trait_info, &redefined_methods) };

	quote!(
		#input

		#impls

	)
	.into()
}

fn render_tuplified_dependencies(input: &ItemFn) -> FnArg {
	let dependencies = input.sig.inputs.iter().skip(1).flat_map(|arg| if let FnArg::Typed(pat_type) = arg { Some((pat_type.pat.clone(), pat_type.ty.clone())) } else { None }).collect::<Vec<_>>();
	let params = dependencies.iter().map(|(pat, _)| pat).collect::<Vec<_>>();
	let types = dependencies.iter().map(|(_, ty)| ty).collect::<Vec<_>>();

	let unified_deps = quote!( (#(#params),*): (#(#types),*));
	let tuple_dep: FnArg = parse_quote! { #unified_deps};
	tuple_dep
}

fn render_proxy_handler(input: &ItemFn, tuple_dep: &FnArg) -> ItemFn {
	let first_arg: syn::FnArg = input.sig.inputs.first().cloned().unwrap();

	let args: Punctuated<FnArg, Comma> = [first_arg.clone(), tuple_dep.clone()].into_iter().collect();
	let mut dedupled_args: Vec<syn::Ident> = vec![];

	args.iter().for_each(|arg| match arg {
		FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
			syn::Pat::Ident(pat) => dedupled_args.push(pat.ident.clone()),
			syn::Pat::Tuple(tuple) => dedupled_args.extend(tuple.elems.iter().map(|elem| syn::parse_quote! { #elem }).collect::<Vec<_>>()),

			syn::Pat::Paren(PatParen { pat, .. }) => {
				if let syn::Pat::Ident(pat) = pat.as_ref() {
					dedupled_args.push(pat.ident.clone());
				} else {
					eprintln!("probably not supported yet");
					panic!("unsupported variant of syn::Pat::PatParen given!")
				}
			}
			_ => {
				eprintln!("probably not supported yet");
				panic!("unsupported variant of syn::Pat given!")
			}
		},

		_ => {
			panic!("Only FnArg::Typed variant is allowed!")
		}
	});

	// render message_handler
	// change the name of message_handler to __original_name
	let mut message_handler = input.clone();
	let asyncness = input.sig.asyncness;

	message_handler.sig.ident = syn::Ident::new(&format!("__{}", message_handler.sig.ident), proc_macro2::Span::call_site());
	message_handler.sig.inputs = args;

	// ! Optimization - change body so it internally calls original method
	let original_name = &input.sig.ident;
	let token = format!("{}({}){}", original_name, dedupled_args.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(","), if asyncness.is_some() { ".await" } else { "" });

	let expr = syn::parse_str::<syn::Expr>(&token).expect("Expression parsing failed!");

	message_handler.block = parse_quote!( { #expr });
	message_handler
}

pub(crate) fn render_message_handler(input: TokenStream) -> TokenStream {
	// Parse the input as a function definition and make argument after the first one as tuple
	let input = parse_macro_input!(input as ItemFn);
	let tuple_dep = render_tuplified_dependencies(&input);

	let proxy_handler = render_proxy_handler(&input, &tuple_dep);

	let quote = quote!(
		#input
		#proxy_handler
	);

	quote.into()
}
