use std::borrow::Borrow;
use syn::{parse_quote, punctuated::Punctuated, token::Comma, DataEnum, DeriveInput, Field, FieldsNamed, Ident, Meta, Path, Stmt, Type, Variant};

pub(crate) fn locate_crate_on_derive_macro(ast: &DeriveInput) -> Ident {
	let crates = ast.attrs.iter().find(|x| x.path().is_ident("crates"));
	let crates = if let Some(crates) = crates { crates.parse_args::<syn::ExprPath>().unwrap().path.get_ident().expect("#[crates(...)] expects path.").to_string() } else { "ruva".to_owned() };
	syn::Ident::new(&crates, proc_macro2::Span::call_site())
}

pub(crate) fn find_enum_variant<'a>(data_enum: &'a DataEnum) -> impl Fn(&'a str) -> Option<&'a Variant> {
	|name: &str| data_enum.variants.iter().find(|x| x.attrs.iter().any(|x| x.path().is_ident(name)))
}

// tell if a field is annotated with specific attribute name and get its Types
#[allow(unused)]
pub(crate) fn find_attr_and_locate_its_type_from_field(field: &mut Field, attribute_name: &str) -> Vec<Type> {
	let mut types_found = vec![];
	for attr in field.attrs.iter_mut() {
		if attr.meta.path().segments.iter().any(|f| f.ident == *attribute_name) {
			types_found.push(field.ty.clone());
		}
	}
	types_found
}

pub(crate) fn check_if_field_has_attribute(field: &Field, attribute_name: &str) -> Option<String> {
	field.attrs.iter().find(|attr| attr.path().is_ident(attribute_name)).map(|_| field.ident.as_ref().unwrap().to_string())
}

pub(crate) fn extract_field_names(ast: &DeriveInput) -> Vec<String> {
	match &ast.data {
		syn::Data::Struct(syn::DataStruct { fields: syn::Fields::Named(fields), .. }) => fields.named.iter().filter_map(|f| f.ident.as_ref().map(|ident| ident.to_string())).collect(),
		_ => panic!("Only Struct is supported"),
	}
}

pub(crate) fn remove_fields_based_on_field_name(given_fields: &mut syn::FieldsNamed, fields_to_remove: impl Borrow<Vec<String>>) {
	let fields_to_remove = fields_to_remove.borrow();
	let new_fields: Punctuated<Field, Comma> = given_fields.named.iter().filter(|f| f.ident.as_ref().map_or(true, |ident| !fields_to_remove.contains(&ident.to_string()))).cloned().collect();

	*given_fields = FieldsNamed { brace_token: syn::token::Brace::default(), named: new_fields };
}

pub(crate) fn skip_over_attributes(field: &mut Field, attribute_name: &str) -> bool {
	let original_length = field.attrs.len();

	field.attrs.retain(|attr| !attr.path().is_ident(attribute_name));
	original_length != field.attrs.len()
}
pub(crate) fn skip_given_attribute(ast: &mut DeriveInput, attribute_name: &str) {
	match &mut ast.data {
		syn::Data::Struct(syn::DataStruct { fields: syn::Fields::Named(fields), .. }) => {
			fields.named.iter_mut().for_each(|f| {
				skip_over_attributes(f, attribute_name);
			});
		}
		syn::Data::Struct(syn::DataStruct { fields: syn::Fields::Unit, .. }) => {}
		_ => {
			panic!("Only Struct is supported");
		}
	}
}

// get attributes from field
pub(crate) fn get_attributes(field: &Field) -> Vec<Ident> {
	let Field { attrs, .. } = field;
	{
		let mut attributes = attrs
			.iter()
			.flat_map(|attr| match &attr.meta {
				Meta::Path(Path { segments, .. }) => segments.iter().map(|segment| segment.ident.clone()).collect::<Vec<Ident>>(),
				_ => {
					vec![]
				}
			})
			.collect::<Vec<_>>();
		attributes.sort();
		attributes
	}
}
pub(crate) fn get_type_name(ty: &Type) -> String {
	let syn::Type::Path(syn::TypePath { path, .. }) = ty else { panic!("Wrong type") };
	path.segments.first().unwrap().ident.to_string()
}
pub(crate) fn get_trait_checking_stmts(trait_path: &str) -> Vec<Stmt> {
	let path = syn::parse::<Path>(trait_path.parse().expect("Unqualified path")).expect("Parsing path for trait failed!");

	vec![
		// Blacket implementation for Type T
		parse_quote!(
			trait __IsTraitNotImplemented {
				const IS_TRAIT: bool = false;

				fn get_data<T>(_: impl std::any::Any) -> &'static mut T {
					unreachable!()
				}
			}
		),
		parse_quote!(
			impl<T> __IsTraitNotImplemented for T {}
		),
		// Blacket implementation for Type T that implements TAggregate
		parse_quote!(
			struct IsTrait<T>(::core::marker::PhantomData<T>);
		),
		parse_quote!(
			#[allow(unused)]
			impl<T: #path> IsTrait<T> {
				const IS_TRAIT: bool = true;

				fn get_data(data: &mut T) -> &mut T {
					data
				}
			}
		),
	]
}

pub fn sort_macros_to_inject(macros_to_inject: &mut Vec<String>, attrs: proc_macro::TokenStream) {
	let normalized = macros_to_inject.iter().map(|x| x.split("::").last().unwrap().to_string()).collect::<Vec<String>>();
	let macro_attrs = attrs.to_string();
	if !attrs.is_empty() {
		for macro_to_add in macro_attrs.split(",") {
			let trimmed = macro_to_add.trim().split("::").last().unwrap();
			if !normalized.contains(&trimmed.to_string()) {
				macros_to_inject.push(macro_to_add.into());
			}
		}
	}
}

pub(crate) fn strip_generic_constraints(generics: &str) -> String {
	// Remove the angle brackets and split by comma to get each generic parameter
	let params: Vec<&str> = generics
		.trim_matches(|c| c == '<' || c == '>')
		.split(',')
		.map(|param| param.trim()) // Trim whitespace around each param
		.collect();

	// Split each parameter on `:` and take the left side
	let stripped_params: Vec<&str> = params.iter().map(|param| param.split(':').next().unwrap().trim()).collect();

	// Join them back with commas and wrap in angle brackets
	format!("<{}>", stripped_params.join(", "))
}
