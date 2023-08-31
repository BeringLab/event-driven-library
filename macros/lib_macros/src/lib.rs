use proc_macro::TokenStream;
use syn::{DeriveInput, Meta, Path};
#[macro_use]
extern crate quote;

#[proc_macro_derive(Message, attributes(internally_notifiable, externally_notifiable))]
pub fn message_derive(attr: TokenStream) -> TokenStream {
	let ast: DeriveInput = syn::parse(attr.clone()).unwrap();
	let propagatability = extract_propagatability(&ast);

	impl_new(&ast, propagatability)
}

fn impl_new(ast: &DeriveInput, propagatability: Vec<&'static str>) -> TokenStream {
	let name = &ast.ident;

	let joined: proc_macro2::TokenStream = propagatability.join(" ").parse().unwrap();

	quote! {
		impl Message for #name {
			fn metadata(&self) -> MessageMetadata {
				MessageMetadata {
					aggregate_id: self.id.to_string(),
					topic: stringify!(#name).into(),
				}
			}
			fn message_clone(&self) -> Box<dyn Message> {
				Box::new(self.clone())
			}
			fn state(&self) -> String {
				serde_json::to_string(&self).expect("Failed to serialize")
			}
			fn to_message(self)-> Box<dyn Message+'static>{
				Box::new(self)
			}
			fn outbox(&self) -> Box<dyn OutBox>
			{
				let metadata = self.metadata();
				Box::new(Outbox::new(metadata.aggregate_id, metadata.topic, self.state()))
			}

			#joined
		}
		impl MailSendable for #name {
			fn template_name(&self) -> String {
				// * subject to change
				stringify!($name).into()
			}
		}
	}
	.into()
}

fn extract_propagatability(ast: &DeriveInput) -> Vec<&'static str> {
	let propagatability = ast
		.attrs
		.iter()
		.flat_map(|attr| {
			if let Meta::Path(Path { segments, .. }) = &attr.parse_meta().unwrap() {
				segments
					.iter()
					.map(|s| {
						if s.ident.to_string().as_str() == "internally_notifiable" {
							"fn internally_notifiable(&self)->bool{true}"
						} else if s.ident.to_string().as_str() == "externally_notifiable" {
							"fn externally_notifiable(&self)->bool{true}"
						} else {
							panic!("Error!")
						}
					})
					.collect::<Vec<_>>()
			} else {
				panic!("Error!")
			}
		})
		.collect::<Vec<_>>();
	propagatability
}
