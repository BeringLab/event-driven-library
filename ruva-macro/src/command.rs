use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{parse_macro_input, punctuated::Punctuated, Attribute, Data, DataStruct, DeriveInput, Fields};

use crate::{
	helpers::{derive_helpers::add_derive_macros, generic_helpers::add_sync_trait_bounds},
	utils::{get_attributes, get_type_name, skip_given_attribute, skip_over_attributes, strip_generic_constraints},
};

const COMMAND_CONSTRAINT: [&str; 4] = ["Send", "Sync", "'static", "std::fmt::Debug"];

fn craete_into_statement_from_struct_command(original_name: &syn::Ident, body_derive: &mut DeriveInput, data_struct: DataStruct) -> proc_macro2::TokenStream {
	let body_name = syn::Ident::new(&(original_name.to_string() + "Body"), original_name.span());

	let DataStruct { fields: Fields::Named(syn::FieldsNamed { named, brace_token }), struct_token, semi_token } = &data_struct else {
		panic!("Only Struct Allowed!");
	};

	let input_required_values = named.iter().filter(|f| get_attributes(f).into_iter().any(|ident| ident == *"required_input")).cloned().collect::<Punctuated<syn::Field, syn::token::Comma>>();

	let mut idents_in_vec: Vec<String> = vec![];
	let mut types_in_vec: Vec<String> = vec![];
	let mut input_not_required_ident_type_vec: Vec<String> = vec![];

	body_derive.data = Data::Struct(DataStruct {
		struct_token: *struct_token,
		fields: Fields::Named(syn::FieldsNamed {
			named: named
				.into_iter()
				.cloned()
				.map(|f| {
					// Get type name and identifier for the type
					idents_in_vec.push(f.ident.clone().unwrap().to_string());
					types_in_vec.push(get_type_name(&f.ty));
					f
				})
				.filter(|f| !input_required_values.iter().any(|required_f| required_f.ident == f.ident))
				.map(|mut f| {
					input_not_required_ident_type_vec.push(f.ident.clone().unwrap().to_string());
					skip_over_attributes(&mut f, "required_input");
					f
				})
				.collect::<Punctuated<syn::Field, syn::token::Comma>>(),
			brace_token: *brace_token,
		}),
		semi_token: *semi_token,
	});

	body_derive.ident = body_name.clone();

	let mut input_keys_in_vec: Vec<String> = vec![];
	let input_parameters = idents_in_vec
		.iter()
		.zip(types_in_vec.iter())
		.filter(|(key, _value)| !input_not_required_ident_type_vec.contains(key))
		.map(|(key, value)| {
			input_keys_in_vec.push(key.clone());
			format!("{}:{}", key, value)
		})
		.collect::<Vec<_>>()
		.join(",");

	// In case there is no input keys
	let mut input_keys = input_keys_in_vec.join(",");
	if !input_keys.is_empty() {
		input_keys += ",";
	}

	let self_parameters = idents_in_vec
		.iter()
		.zip(types_in_vec.iter())
		.filter(|(key, _value)| input_not_required_ident_type_vec.contains(key))
		.map(|(key, _)| format!("{}:self.{}", key, key))
		.collect::<Vec<_>>()
		.join(",");

	// Convert the generics to a string
	add_sync_trait_bounds(&mut body_derive.generics, &COMMAND_CONSTRAINT);
	let generics = if body_derive.generics.params.is_empty() { String::new() } else { format!("{}", body_derive.generics.to_token_stream()) };

	// Convert the where clause to a string (if it exists)
	let where_clause = match &body_derive.generics.where_clause {
		Some(where_clause) => format!("{}", where_clause.to_token_stream()),
		None => String::new(),
	};

	// based on ':', split them and take only the left portion
	// for example, <T:Serialize, U:Deserialize> -> <T, U>
	let generics_with_out_contraints = strip_generic_constraints(&generics);

	let into_statement: proc_macro2::TokenStream = format!(
		"     
	impl {generics} {body_name}{generics_with_out_contraints} {where_clause} {{
		pub fn into_command(self,{input_parameters}) -> {original_name}{generics_with_out_contraints}  {{
			{original_name}{{
				{input_keys}
				{self_parameters}
			}}
		}}
	}}
	"
	)
	.parse()
	.unwrap();

	into_statement
}

fn into_command_body(derive_input: &DeriveInput) -> (Option<DeriveInput>, proc_macro2::TokenStream) {
	let original_name = &derive_input.ident;

	let mut body_derive = derive_input.clone();

	match body_derive.data.clone() {
		// Data::Struct(data_struct) => {
		// 	let into_statement = craete_into_statement_from_struct_command(original_name, &mut body_derive, data_struct);
		// 	(Some(body_derive), into_statement)
		// }
		Data::Struct(data_struct @ DataStruct { fields: Fields::Named(_), .. }) => {
			let into_statement = craete_into_statement_from_struct_command(original_name, &mut body_derive, data_struct);
			(Some(body_derive), into_statement)
		}
		Data::Struct(DataStruct { fields: Fields::Unit, .. }) => (None, quote!()),

		_ => {
			panic!("Such type not supported!");
		}
	}
}

pub fn declare_command(ast: &mut DeriveInput) -> TokenStream {
	let name = ast.ident.clone();

	// add `Send`, `Sync`, `'static` and `std::fmt::Debug` to TypeGenerics if it doesn't have it
	add_sync_trait_bounds(&mut ast.generics, &COMMAND_CONSTRAINT);

	let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

	quote!(
		impl #impl_generics ruva::TCommand for #name #ty_generics #where_clause {}
	)
}

fn parse_attributes(attrs: &proc_macro::TokenStream) -> (Vec<String>, Vec<String>) {
	let mut macros_to_inject_to_body = vec!["Debug".to_string(), "ruva::Deserialize".to_string()];
	let normalized_body_macro = macros_to_inject_to_body.iter().map(|x| x.split("::").last().unwrap().to_string()).collect::<Vec<String>>();

	let mut macros_to_inject_to_original = vec!["Debug".to_string(), "ruva::Serialize".to_string()];
	let normalized_command_macro = macros_to_inject_to_original.iter().map(|x| x.split("::").last().unwrap().to_string()).collect::<Vec<String>>();

	let attr = attrs.to_string();

	if attr.is_empty() {
		return (macros_to_inject_to_body, macros_to_inject_to_original);
	}

	let re = regex::Regex::new(r"(command|body)\(([^)]+)\)").unwrap();

	for cap in re.captures_iter(&attr) {
		// Capture the type (either command or body)
		let where_to_place = &cap[1];
		// Capture the content inside the parentheses
		let content = &cap[2].trim_end_matches(',');

		for macro_to_add in content.split(',') {
			let mcr = macro_to_add.trim();
			let trimmed = mcr.split("::").last().unwrap();

			if where_to_place == "body" && !normalized_body_macro.contains(&trimmed.to_string()) {
				macros_to_inject_to_body.push(mcr.to_string());
			} else if where_to_place == "command" && !normalized_command_macro.contains(&trimmed.to_string()) {
				macros_to_inject_to_original.push(mcr.to_string());
			}
		}
	}

	(macros_to_inject_to_body, macros_to_inject_to_original)
}

fn reorder_attributes(input: &mut DeriveInput) {
	const LINT_GROUPS: [&str; 3] = ["allow", "warn", "deny"];

	// Temporary storage for the attributes we care about
	let mut derive_attr: Option<Attribute> = None;
	let internally_notifiable_attr: Option<Attribute> = None;

	// Collect the other attributes
	let mut other_attrs = vec![];

	for attr in input.attrs.drain(..) {
		if let syn::Meta::List(meta_list) = &attr.meta {
			if meta_list.path.is_ident("derive") {
				derive_attr = Some(attr);
			}
			// If LINT-related attribute, skip it
			else if LINT_GROUPS.contains(&meta_list.path.get_ident().unwrap().to_string().as_str()) {
			} else {
				other_attrs.push(attr);
			}
		} else {
			other_attrs.push(attr);
		}
	}

	// Add the `#[derive(...)]` attribute first, if it exists
	if let Some(attr) = derive_attr {
		input.attrs.push(attr);
	}

	// Then add the `#[internally_notifiable]` attribute, if it exists
	if let Some(attr) = internally_notifiable_attr {
		input.attrs.push(attr);
	}

	// Finally, add any other attributes
	input.attrs.extend(other_attrs);
}

pub fn render_into_command(input: proc_macro::TokenStream, attrs: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let (macros_to_inject_to_body, macros_to_inject_to_original) = parse_attributes(&attrs);

	let mut ast = parse_macro_input!(input as DeriveInput);

	let mut quotes = vec![];

	let (body_ast, into_statement) = into_command_body(&ast);
	if let Some(mut body_ast) = body_ast {
		add_derive_macros(&mut body_ast, &macros_to_inject_to_body);

		if macros_to_inject_to_original.contains(&"ruva::TEvent".to_string()) {
			body_ast.attrs.retain(|attr| !attr.path().is_ident("externally_notifiable"));
			skip_given_attribute(&mut body_ast, "identifier");
			body_ast.attrs.retain(|attr| !attr.path().is_ident("internally_notifiable"));
		}

		quotes.push(quote!(#body_ast));
		quotes.push(quote!(#into_statement));
	}

	add_derive_macros(&mut ast, &macros_to_inject_to_original);
	skip_given_attribute(&mut ast, "required_input");
	add_sync_trait_bounds(&mut ast.generics, &COMMAND_CONSTRAINT);

	let t_command = declare_command(&mut ast);
	quotes.push(quote!(#t_command));

	if macros_to_inject_to_original.contains(&"ruva::TEvent".to_string()) {
		reorder_attributes(&mut ast);
	}

	quotes.push(quote!(
		#ast
	));

	quote!(
		#(#quotes)*
	)
	.into()
}
