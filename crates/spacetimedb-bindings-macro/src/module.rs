extern crate core;
extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{DeriveInput, ItemStruct};

/// This returns a function which will return the schema (TypeDef) for a struct. The signature
/// for this function is as follows:
/// pub fn get_struct_schema() -> spacetimedb::spacetimedb_lib::TypeDef {
///   ...
/// }
pub(crate) fn autogen_module_struct_to_schema(
    original_struct: &ItemStruct,
    tuple_name: &str,
) -> syn::Result<TokenStream> {
    let fields = original_struct.fields.iter().map(move |field| {
        let field_name = match &field.ident {
            Some(name) => {
                let name = name.to_string();
                quote!(Some(#name.to_owned()))
            }
            None => quote!(None),
        };
        let ty = &field.ty;
        quote!(spacetimedb::sats::ProductTypeElement {
            name: #field_name,
            algebraic_type: <#ty as spacetimedb::SchemaType>::get_schema(),
        })
    });

    let init_type_symbol = format!("__preinit__20_init_type_{}", tuple_name);

    let name = &original_struct.ident;
    let (impl_generics, ty_generics, where_clause) = original_struct.generics.split_for_impl();
    Ok(quote! {
        const _: () = {
            static __TYPEREF: spacetimedb::rt::Lazy<spacetimedb::sats::AlgebraicTypeRef> =
                spacetimedb::rt::Lazy::new(spacetimedb::rt::alloc_typespace_slot);
            #[export_name = #init_type_symbol]
            extern "C" fn __init_type() {
                let __typeref = *__TYPEREF;
                let __typ = spacetimedb::sats::ProductType {
                    elements: vec![#(#fields),*],
                };
                spacetimedb::rt::set_typespace_slot(__typeref, spacetimedb::sats::AlgebraicType::Product(__typ))
            }
            impl #impl_generics spacetimedb::RefType for #name #ty_generics #where_clause {
                fn typeref() -> spacetimedb::sats::AlgebraicTypeRef {
                    *__TYPEREF
                }
            }
        };
    })
}

pub(crate) fn derive_deserialize_struct(
    original_struct: &ItemStruct,
    spacetimedb_lib: &TokenStream,
) -> syn::Result<TokenStream> {
    derive_deserialize(&original_struct.clone().into(), spacetimedb_lib)
}

pub(crate) fn derive_deserialize(original: &DeriveInput, spacetimedb_lib: &TokenStream) -> syn::Result<TokenStream> {
    let tuple_name = original.ident.to_string();

    let name = &original.ident;
    let (_, ty_generics, where_clause) = original.generics.split_for_impl();

    let mut de_generics = original.generics.clone();
    let de_lifetime = syn::Lifetime::new("'de", Span::call_site());
    de_generics
        .params
        .insert(0, syn::LifetimeDef::new(de_lifetime.clone()).into());
    let (de_impl_generics, _, _) = de_generics.split_for_impl();

    let (iter_n, iter_n2, iter_n3) = (0usize.., 0usize.., 0usize..);

    match &original.data {
        syn::Data::Struct(struc) => {
            let n_fields = struc.fields.len();

            let field_names = struc
                .fields
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect::<Vec<_>>();
            let field_strings = field_names.iter().map(|f| f.to_string()).collect::<Vec<_>>();
            let field_types = struc.fields.iter().map(|f| &f.ty);
            Ok(quote! {
                #[allow(non_camel_case_types)]
                const _: () = {
                    impl #de_impl_generics #spacetimedb_lib::de::Deserialize<#de_lifetime> for #name #ty_generics #where_clause {
                        fn deserialize<D: #spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            deserializer.deserialize_product(__ProductVisitor)
                        }
                    }

                    struct __ProductVisitor;

                    impl<'de> #spacetimedb_lib::de::ProductVisitor<'de> for __ProductVisitor {
                        type Output = #name;

                        fn product_name(&self) -> Option<&str> {
                            Some(#tuple_name)
                        }
                        fn product_len(&self) -> usize {
                            #n_fields
                        }

                        fn visit_seq_product<A: #spacetimedb_lib::de::SeqProductAccess<'de>>(self, mut tup: A) -> Result<Self::Output, A::Error> {
                            Ok(#name {
                                #(#field_names:
                                    tup.next_element::<#field_types>()?
                                        .ok_or_else(|| #spacetimedb_lib::de::Error::invalid_product_length(#iter_n, &self))?,)*
                            })
                        }
                        fn visit_named_product<A: #spacetimedb_lib::de::NamedProductAccess<'de>>(self, mut __prod: A) -> Result<Self::Output, A::Error> {
                            #(let mut #field_names = None;)*
                            while let Some(__field) = #spacetimedb_lib::de::NamedProductAccess::get_field_ident(&mut __prod, __ProductVisitor)? {
                                match __field {
                                    #(__ProductFieldIdent::#field_names => {
                                        if #field_names.is_some() {
                                            return Err(#spacetimedb_lib::de::Error::duplicate_field(#iter_n2, Some(#field_strings), &self))
                                        }
                                        #field_names = Some(#spacetimedb_lib::de::NamedProductAccess::get_field_value(&mut __prod)?)
                                    })*
                                }
                            }
                            Ok(#name {
                                #(#field_names:
                                    #field_names.ok_or_else(|| #spacetimedb_lib::de::Error::missing_field(#iter_n3, Some(#field_strings), &self))?,)*
                            })
                        }
                    }

                    impl<'de> #spacetimedb_lib::de::FieldNameVisitor<'de> for __ProductVisitor {
                        type Output = __ProductFieldIdent;

                        fn field_names(&self, names: &mut dyn #spacetimedb_lib::de::ValidNames) {
                            names.extend([#(#field_strings),*])
                        }

                        fn visit<__E: #spacetimedb_lib::de::Error>(self, name: &str) -> Result<Self::Output, __E> {
                            match name {
                                #(#field_strings => Ok(__ProductFieldIdent::#field_names),)*
                                _ => Err(#spacetimedb_lib::de::Error::unknown_field_name(name, &self)),
                            }
                        }
                    }

                    enum __ProductFieldIdent {
                        #(#field_names,)*
                    }
                };
            })
        }
        syn::Data::Enum(enu) => {
            let variant_names = enu.variants.iter().map(|var| var.ident.to_string()).collect::<Vec<_>>();
            let variant_idents = enu.variants.iter().map(|var| &var.ident).collect::<Vec<_>>();
            let tags = 0u8..;
            let arms = enu.variants.iter().map(|var| {
                let data = variant_data(var)?;
                let ident = &var.ident;
                Ok(if let Some((member, ty)) = data {
                    quote! {
                        __Variant::#ident => Ok(#name::#ident { #member: #spacetimedb_lib::de::VariantAccess::deserialize::<#ty>(__access)? }),
                    }
                } else {
                    quote! {
                        __Variant::#ident => {
                            let () = #spacetimedb_lib::de::VariantAccess::deserialize(__access)?;
                            Ok(#name::#ident)
                        }
                    }
                })
            });
            let arms = arms.collect::<syn::Result<Vec<_>>>()?;
            Ok(quote! {
                const _: () = {
                    impl #de_impl_generics #spacetimedb_lib::de::Deserialize<#de_lifetime> for #name #ty_generics #where_clause {
                        fn deserialize<D: #spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            deserializer.deserialize_sum(__SumVisitor)
                        }
                    }

                    struct __SumVisitor;

                    impl<'de> #spacetimedb_lib::de::SumVisitor<'de> for __SumVisitor {
                        type Output = #name;

                        fn sum_name(&self) -> Option<&str> {
                            Some(#tuple_name)
                        }

                        fn visit_sum<A: #spacetimedb_lib::de::SumAccess<'de>>(self, __data: A) -> Result<Self::Output, A::Error> {
                            let (__variant, __access) = __data.variant(self)?;
                            match __variant {
                                #(#arms)*
                            }
                        }
                    }

                    enum __Variant {
                        #(#variant_idents,)*
                    }

                    impl #spacetimedb_lib::de::VariantVisitor for __SumVisitor {
                        type Output = __Variant;

                        fn variant_names(&self, names: &mut dyn #spacetimedb_lib::de::ValidNames) {
                            names.extend([#(#variant_names,)*])
                        }

                        fn visit_tag<E: #spacetimedb_lib::de::Error>(self, __tag: u8) -> Result<Self::Output, E> {
                            match __tag {
                                #(#tags => Ok(__Variant::#variant_idents),)*
                                _ => Err(#spacetimedb_lib::de::Error::unknown_variant_tag(__tag, &self)),
                            }
                        }
                        fn visit_name<E: #spacetimedb_lib::de::Error>(self, __name: &str) -> Result<Self::Output, E> {
                            match __name {
                                #(#variant_names => Ok(__Variant::#variant_idents),)*
                                _ => Err(#spacetimedb_lib::de::Error::unknown_variant_name(__name, &self)),
                            }
                        }
                    }
                };
            })
        }
        syn::Data::Union(u) => return Err(syn::Error::new(u.union_token.span, "unions not supported")),
    }
}

pub(crate) fn derive_serialize_struct(
    original_struct: &ItemStruct,
    spacetimedb_lib: &TokenStream,
) -> syn::Result<TokenStream> {
    derive_serialize(&original_struct.clone().into(), spacetimedb_lib)
}

pub(crate) fn derive_serialize(original: &DeriveInput, spacetimedb_lib: &TokenStream) -> syn::Result<TokenStream> {
    let name = &original.ident;
    let (impl_generics, ty_generics, where_clause) = original.generics.split_for_impl();
    let body = match &original.data {
        syn::Data::Struct(struc) => {
            let fieldnames = struc.fields.iter().map(|field| field.ident.as_ref().unwrap());
            let tys = struc.fields.iter().map(|f| &f.ty);
            let fieldnamestrings = fieldnames.clone().map(|f| f.to_string());
            let nfields = struc.fields.len();
            quote! {
                let mut __prod = __serializer.serialize_named_product(#nfields)?;
                #(#spacetimedb_lib::ser::SerializeNamedProduct::serialize_element::<#tys>(&mut __prod, Some(#fieldnamestrings), &self.#fieldnames)?;)*
                #spacetimedb_lib::ser::SerializeNamedProduct::end(__prod)
            }
        }
        syn::Data::Enum(enu) => {
            let arms = enu.variants.iter().enumerate().map(|(i, var)| {
                let data = variant_data(var)?;
                let name = &var.ident;
                let name_str = name.to_string();
                let tag = i as u8;
                Ok(if let Some((member, ty)) = data {
                    quote_spanned! {ty.span()=>
                        Self::#name { #member: __variant } => __serializer.serialize_variant::<#ty>(#tag, Some(#name_str), __variant),
                    }
                } else {
                    quote! {
                        Self::#name => __serializer.serialize_variant(#tag, Some(#name_str), &()),
                    }
                })
            });
            let arms = arms.collect::<syn::Result<Vec<_>>>()?;
            quote!(match self { #(#arms)* })
        }
        syn::Data::Union(u) => return Err(syn::Error::new(u.union_token.span, "unions not supported")),
    };
    Ok(quote! {
        impl #impl_generics #spacetimedb_lib::ser::Serialize for #name #ty_generics #where_clause {
            fn serialize<S: #spacetimedb_lib::ser::Serializer>(&self, __serializer: S) -> Result<S::Ok, S::Error> {
                #body
            }
        }
    })
}

fn variant_data(variant: &syn::Variant) -> syn::Result<Option<(syn::Member, &syn::Type)>> {
    let field = match &variant.fields {
        syn::Fields::Named(f) if f.named.len() == 1 => &f.named[0],
        syn::Fields::Named(_) => {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "must be a unit variant or a newtype variant",
            ))
        }
        syn::Fields::Unnamed(f) if f.unnamed.len() != 1 => {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "must be a unit variant or a newtype variant",
            ))
        }
        syn::Fields::Unnamed(f) => &f.unnamed[0],
        syn::Fields::Unit => return Ok(None),
    };
    let member = field
        .ident
        .clone()
        .map(Into::into)
        .unwrap_or_else(|| syn::Member::from(0));
    Ok(Some((member, &field.ty)))
}
