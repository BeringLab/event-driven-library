use syn::{punctuated::Punctuated, GenericParam, Generics, PredicateType, Type, TypePath, WherePredicate};

// Add `Send`, `Sync`, `'static` and `std::fmt::Debug` to TypeGenerics if it doesn't have them
// If they have, then selectively add `Send`, `Sync`, `'static` and `std::fmt::Debug` to TypeGenerics
// Plus, move T type constraints to where clause
pub fn add_sync_trait_bounds(generics: &mut syn::Generics, contraints: &[&str]) {
	generics
		.params
		.iter_mut()
		.filter_map(|param| match param {
			syn::GenericParam::Type(ty) => Some(ty),
			_ => None,
		})
		.for_each(|ty| {
			// optimize the above code
			add_type_param_bounds(&mut ty.bounds, contraints);
		})
}

/// add `Send`, `Sync`, `'static`, `std::fmt::Debug` and `Default` to TypeGenerics if it doesn't have them
/// The following will move contraints from TypeGenerics to where clause
pub fn add_aggregate_generic_defaults(generics: &mut Generics) {
	let new_predicates: Vec<WherePredicate> = generics
		.params
		.iter()
		.filter_map(|param| {
			if let GenericParam::Type(ty) = param {
				let bounded_ty = Type::Path(TypePath { qself: None, path: ty.ident.clone().into() });
				Some(WherePredicate::Type(PredicateType { bounded_ty, colon_token: Default::default(), bounds: Punctuated::new(), lifetimes: None }))
			} else {
				None
			}
		})
		.collect();

	if !new_predicates.is_empty() {
		let where_clause = generics.make_where_clause();
		where_clause.predicates.extend(new_predicates);

		for predicate in &mut where_clause.predicates {
			if let WherePredicate::Type(predicate_type) = predicate {
				add_type_param_bounds(&mut predicate_type.bounds, &["Send", "Sync", "'static", "std::fmt::Debug", "Default"]);
			}
		}
	}
}

fn add_type_param_bounds(bounds: &mut Punctuated<syn::TypeParamBound, syn::token::Plus>, new_bounds: &[&str]) {
	for bound in new_bounds {
		bounds.push(syn::parse_str(bound).unwrap());
	}
}
