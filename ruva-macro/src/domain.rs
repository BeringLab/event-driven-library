use proc_macro::TokenStream;

use quote::ToTokens;
use syn::{parse::Parser, parse_macro_input, punctuated::Punctuated, token::Comma, Data, DataStruct, DeriveInput, Field, GenericParam, Generics, Ident, Type, WherePredicate};

use crate::{
	helpers::{derive_helpers::add_derive_macros, generic_helpers::add_aggregate_generic_defaults},
	utils::{check_if_field_has_attribute, extract_field_names, locate_crate_on_derive_macro, remove_fields_based_on_field_name, skip_over_attributes, sort_macros_to_inject},
};

pub(crate) fn render_aggregate(input: TokenStream, attrs: TokenStream) -> TokenStream {
	let mut macros_to_inject = vec!["ruva::Serialize".to_string(), "Debug".to_string(), "Default".to_string()];
	sort_macros_to_inject(&mut macros_to_inject, attrs);

	let mut ast = parse_macro_input!(input as DeriveInput);

	let name = ast.ident.clone();

	add_aggregate_generic_defaults(&mut ast.generics);
	add_derive_macros(&mut ast, &macros_to_inject);
	let generics: &Generics = &ast.generics;
	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

	let crates = locate_crate_on_derive_macro(&ast);

	let adapter_quote = create_struct_adapter_quote(&ast, true);

	let setters = set_entity_fields(&mut ast.data, true);

	quote!(
		#ast
		impl #impl_generics  #crates::TAggregate for #name #ty_generics #where_clause {
			// type Identifier = #aggregate_identifier_type;

			fn events(&self) -> &::std::collections::VecDeque<::std::sync::Arc<dyn #crates::TEvent>> {
				&self.events
			}
			fn take_events(&mut self) -> ::std::collections::VecDeque<::std::sync::Arc<dyn #crates::TEvent>> {
				::std::mem::take(&mut self.events)
			}
			fn raise_event(&mut self, event: ::std::sync::Arc<dyn #crates::TEvent>) {
				tracing::info!("event raised! {:?}", event.metadata());
				self.events.push_back(event)
			}
		}

		impl #impl_generics #name #ty_generics #where_clause{
			#setters
		}


		#adapter_quote
	)
	.into()
}

pub(crate) fn render_entity_token(input: TokenStream, attrs: TokenStream) -> TokenStream {
	let mut macros_to_inject = vec!["ruva::Serialize".to_string(), "Debug".to_string(), "Default".to_string()];
	sort_macros_to_inject(&mut macros_to_inject, attrs);

	let mut ast = parse_macro_input!(input as DeriveInput);
	add_derive_macros(&mut ast, &macros_to_inject);
	let name = &ast.ident;
	let generics = &ast.generics;
	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
	let adapter_quote = create_struct_adapter_quote(&ast, false);

	let setters = set_entity_fields(&mut ast.data, false);

	quote!(
		#ast

		impl #impl_generics #name #ty_generics #where_clause{
			#setters
		}

		#adapter_quote

	)
	.into()
}

pub(crate) fn set_entity_fields(input_data: &mut syn::Data, for_aggregate: bool) -> proc_macro2::TokenStream {
	if let syn::Data::Struct(DataStruct { fields: syn::Fields::Named(ref mut fields), .. }) = input_data {
		fields.named.iter_mut().for_each(|f| {
			skip_over_attributes(f, "adapter_ignore");
		});

		if fields.named.iter().any(|x| x.ident.as_ref().unwrap() == "is_existing") {
			panic!("is_existing field not injectable! Perhaps it's duplicated?");
		}
		if fields.named.iter().any(|x| x.ident.as_ref().unwrap() == "is_updated") {
			panic!("is_updated field not injectable! Perhaps it's duplicated?");
		}

		fields.named.extend([
			syn::Field::parse_named
				.parse2(quote! {
				   #[serde(skip_deserializing, skip_serializing)]
				   pub(crate) is_existing: bool
				})
				.unwrap(),
			syn::Field::parse_named
				.parse2(quote! {
				   #[serde(skip_deserializing, skip_serializing)]
				   pub(crate) is_updated: bool
				})
				.unwrap(),
		]);

		if for_aggregate {
			if fields.named.iter().any(|x| x.ident.as_ref().unwrap() == "events") {
				panic!("events field not injectable! Perhaps it's duplicated?");
			}

			fields.named.push(
				syn::Field::parse_named
					.parse2(quote! {
					   #[serde(skip_deserializing, skip_serializing)]
					   pub(crate) events: ::std::collections::VecDeque<::std::sync::Arc<dyn ruva::TEvent>>
					})
					.unwrap(),
			)
		}
	} else {
		if for_aggregate {
			panic!("[aggregate] can be attached only to struct")
		}
		panic!("[entity] can be attached only to struct")
	}

	get_setters(input_data)
}

fn get_setters(data: &Data) -> proc_macro2::TokenStream {
	let field_idents: Vec<Field> = match data {
		Data::Struct(data) => data.fields.clone().into_iter().filter_map(Some).collect(),
		_ => panic!("Only Struct Is supported"),
	};
	let mut quotes = vec![];
	for f in field_idents {
		let ident = f.ident.unwrap();
		let ty = f.ty.to_token_stream().to_string();
		let code = format!("pub fn set_{}(&mut self, {}:impl core::convert::Into<{}>){{self.{}={}.into();self.is_updated=true}}", ident, ident, ty, ident, ident);
		quotes.push(code);
	}
	let joined: proc_macro2::TokenStream = quotes.join(" ").parse().unwrap();
	joined
}

pub fn create_struct_adapter_quote(input: &DeriveInput, for_aggregate: bool) -> proc_macro2::TokenStream {
	let aggregate_name = input.ident.clone();
	let mut generics = input.generics.clone();
	add_aggregate_generic_defaults(&mut generics);

	let adapter_name = Ident::new(&(input.ident.to_string() + "Adapter"), proc_macro2::Span::call_site());

	let mut adapter_input = input.clone();
	adapter_input.ident = adapter_name.clone();

	let mut fields_to_ignore: Vec<String> = vec![];

	if let syn::Data::Struct(DataStruct { fields: syn::Fields::Named(ref mut fields), .. }) = &mut adapter_input.data {
		fields.named.iter_mut().for_each(|f: &mut Field| {
			if let Some(ignorable_field) = check_if_field_has_attribute(f, "adapter_ignore") {
				// if the field's type is generic, skip over

				fields_to_ignore.push(ignorable_field);
				skip_over_attributes(f, "adapter_ignore");

				try_remove_generic_type(&mut generics, f.ty.clone());
			}
		});
		remove_fields_based_on_field_name(fields, &fields_to_ignore);
	}

	let mut aggregates_fields: Vec<String> = vec![];
	let mut adapter_fields: Vec<String> = vec![];

	extract_field_names(input).into_iter().for_each(|field_name| {
		// ignorable field means that the field is not compatible with adapter
		if !fields_to_ignore.contains(&field_name) {
			adapter_fields.push(format!("{}: value.{}", field_name, field_name));
			aggregates_fields.push(format!("{}: value.{}", field_name, field_name));
		}
	});

	aggregates_fields.push("is_existing: true".to_string());
	aggregates_fields.push("is_updated: false".to_string());

	// ! Event field is only for aggregate
	if for_aggregate {
		aggregates_fields.push("events: ::std::collections::VecDeque::new()".to_string());
	}
	// aggregates_fields.push("version: 0".to_string());

	if !fields_to_ignore.is_empty() {
		aggregates_fields.push("..Default::default()".to_string());
	}
	let aggregates_fields = aggregates_fields.join(",");
	let adapter_fields = adapter_fields.join(",");

	let aggregates_fields: proc_macro2::TokenStream = aggregates_fields.parse().unwrap();

	let adapter_fields: proc_macro2::TokenStream = adapter_fields.parse().unwrap();

	adapter_input.generics = generics.clone();

	let (impl_adapter_generics, ty_adapter_generics, _where_adapter_clause) = generics.split_for_impl();

	let (impl_aggregate_generics, ty_aggregate_generics, where_aggregate_clause) = input.generics.split_for_impl();

	quote!(

		#adapter_input

		impl #impl_aggregate_generics From<#adapter_name #ty_adapter_generics> for #aggregate_name #impl_aggregate_generics #where_aggregate_clause{
			fn from(value: #adapter_name #ty_adapter_generics) -> #aggregate_name #impl_aggregate_generics {
				Self{
					#aggregates_fields
				}
			}
		}

		impl #impl_aggregate_generics From<#aggregate_name #ty_aggregate_generics> for #adapter_name #impl_adapter_generics #where_aggregate_clause{
			fn from(value: #aggregate_name #ty_aggregate_generics) -> #adapter_name #ty_adapter_generics{
				Self{
					#adapter_fields
				}
			}
		}
	)
}

fn try_remove_generic_type(generics: &mut Generics, ty: Type) {
	// find the generic type and remove it from the generics and from where clause
	let mut removed_generic = vec![];

	let generic_params: Punctuated<GenericParam, _> = generics
		.params
		.iter()
		.filter_map(|generic_param| {
			if let GenericParam::Type(type_param) = generic_param {
				let type_name = ty.to_token_stream().to_string();
				if type_param.ident != type_name {
					Some(generic_param.clone())
				} else {
					removed_generic.push(type_name);
					None
				}
			} else {
				Some(generic_param.clone())
			}
		})
		.collect();

	// find the generic type and remove it from the generics and from where clause using removed_generic
	if let Some(where_clauses) = generics.where_clause.as_mut() {
		let mut predicates: Punctuated<WherePredicate, Comma> = Punctuated::new();

		where_clauses.predicates.iter().for_each(|predicate| {
			if let syn::WherePredicate::Type(predicate_type) = predicate {
				if let syn::Type::Path(type_path) = &predicate_type.bounded_ty {
					if let Some(segment) = type_path.path.segments.first() {
						if removed_generic.contains(&segment.ident.to_string()) {
							return;
						}
					}
				}
				predicates.push(predicate.clone());
			}
		});
		where_clauses.predicates = predicates;
	}

	generics.params = generic_params;
}
