extern crate core;
extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::punctuated::Pair;
use syn::spanned::Spanned;
use syn::{LitStr, Token};

use crate::{check_duplicate_meta, sym};

pub(crate) struct SatsType<'a> {
    pub ident: &'a syn::Ident,
    pub generics: &'a syn::Generics,
    pub name: String,
    pub krate: TokenStream,
    pub original_attrs: &'a [syn::Attribute],
    pub data: SatsTypeData<'a>,
}

pub(crate) enum SatsTypeData<'a> {
    Product(Vec<SatsField<'a>>),
    Sum(Vec<SatsVariant<'a>>),
}

pub(crate) struct SatsField<'a> {
    pub ident: Option<&'a syn::Ident>,
    pub vis: &'a syn::Visibility,
    pub name: Option<String>,
    pub ty: &'a syn::Type,
    pub original_attrs: &'a [syn::Attribute],
    pub span: Span,
}

pub(crate) struct SatsVariant<'a> {
    pub ident: &'a syn::Ident,
    pub name: String,
    pub ty: Option<&'a syn::Type>,
    pub member: Option<syn::Member>,
    #[allow(unused)]
    pub original_attrs: &'a [syn::Attribute],
}

pub(crate) fn sats_type_from_derive(
    input: &syn::DeriveInput,
    crate_fallback: TokenStream,
) -> syn::Result<SatsType<'_>> {
    let data = match &input.data {
        syn::Data::Struct(struc) => {
            let fields = struc.fields.iter().map(|field| SatsField {
                ident: field.ident.as_ref(),
                vis: &field.vis,
                name: field.ident.as_ref().map(syn::Ident::to_string),
                ty: &field.ty,
                original_attrs: &field.attrs,
                span: field.span(),
            });
            SatsTypeData::Product(fields.collect())
        }
        syn::Data::Enum(enu) => {
            let variants = enu.variants.iter().map(|var| {
                let (member, ty) = variant_data(var)?.unzip();
                Ok(SatsVariant {
                    ident: &var.ident,
                    name: var.ident.to_string(),
                    ty,
                    member,
                    original_attrs: &var.attrs,
                })
            });
            SatsTypeData::Sum(variants.collect::<syn::Result<Vec<_>>>()?)
        }
        syn::Data::Union(u) => return Err(syn::Error::new(u.union_token.span, "unions not supported")),
    };
    extract_sats_type(&input.ident, &input.generics, &input.attrs, data, crate_fallback)
}

pub(crate) fn extract_sats_type<'a>(
    ident: &'a syn::Ident,
    generics: &'a syn::Generics,
    attrs: &'a [syn::Attribute],
    data: SatsTypeData<'a>,
    crate_fallback: TokenStream,
) -> syn::Result<SatsType<'a>> {
    let mut name = None;
    let mut krate = None;
    for attr in attrs {
        if attr.path() != sym::SATS {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path == sym::CRATE {
                check_duplicate_meta(&krate, &meta)?;
                let value = meta.value()?;
                let v = value.call(syn::Path::parse_mod_style)?;
                krate = Some(v.into_token_stream());
            } else if meta.path == sym::NAME {
                check_duplicate_meta(&name, &meta)?;
                let value = meta.value()?;
                let v = value.parse::<LitStr>()?;
                name = Some(v.value());
            } else {
                return Err(meta.error("unknown sats attribute"));
            }
            Ok(())
        })?;
    }
    let krate = krate.unwrap_or(crate_fallback);
    let name = name.unwrap_or_else(|| ident.to_string());

    Ok(SatsType {
        ident,
        generics,
        name,
        krate,
        original_attrs: attrs,
        data,
    })
}

pub(crate) fn derive_satstype(ty: &SatsType<'_>, gen_type_alias: bool) -> TokenStream {
    let ty_name = &ty.name;
    let name = &ty.ident;

    let typ = match &ty.data {
        SatsTypeData::Product(fields) => {
            let fields = fields.iter().map(|field| {
                let field_name = match &field.name {
                    Some(name) => quote!(Some(#name)),
                    None => quote!(None),
                };
                let ty = field.ty;
                quote!((
                    #field_name,
                    <#ty as spacetimedb::SpacetimeType>::make_type(__typespace),
                ))
            });
            let len = fields.len();
            quote!(
                spacetimedb::sats::AlgebraicType::product::<
                    [(Option<&str>, spacetimedb::sats::AlgebraicType); #len]
                >(
                    [#(#fields),*]
                )
            )
        }
        SatsTypeData::Sum(variants) => {
            let unit = syn::Type::Tuple(syn::TypeTuple {
                paren_token: Default::default(),
                elems: Default::default(),
            });
            let variants = variants.iter().map(|var| {
                let variant_name = &var.name;
                let ty = var.ty.unwrap_or(&unit);
                quote!((
                    #variant_name,
                    <#ty as spacetimedb::SpacetimeType>::make_type(__typespace),
                ))
            });
            let len = variants.len();
            quote!(
                spacetimedb::sats::AlgebraicType::sum::<
                    [(&str, spacetimedb::sats::AlgebraicType); #len]
                >(
                    [#(#variants),*]
                )
            )
            // todo!()
        } // syn::Data::Union(u) => return Err(syn::Error::new(u.union_token.span, "unions not supported")),
    };

    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();
    let ty_name = if gen_type_alias {
        quote!(Some(#ty_name))
    } else {
        quote!(None)
    };
    quote! {
        #[allow(clippy::all)]
        const _: () = {
            impl #impl_generics spacetimedb::SpacetimeType for #name #ty_generics #where_clause {
                fn make_type<S: spacetimedb::sats::typespace::TypespaceBuilder>(__typespace: &mut S) -> spacetimedb::sats::AlgebraicType {
                    spacetimedb::sats::typespace::TypespaceBuilder::add(
                        __typespace,
                        // is this correct? ignoring generics and stuff?
                        { struct __Marker; core::any::TypeId::of::<__Marker>() },
                        #ty_name,
                        |__typespace| #typ,
                    )
                }
            }
        };
    }
}

pub(crate) fn derive_deserialize(ty: &SatsType<'_>) -> TokenStream {
    let (name, tuple_name) = (&ty.ident, &ty.name);
    let spacetimedb_lib = &ty.krate;
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

    let mut de_generics = ty.generics.clone();
    let de_lifetime = syn::Lifetime::new("'de", Span::call_site());
    for lp in de_generics.lifetimes_mut() {
        lp.bounds.push(de_lifetime.clone());
    }

    let mut de_lt_param = syn::LifetimeParam::new(de_lifetime);
    de_lt_param.bounds = de_generics
        .lifetimes()
        .map(|lp| Pair::Punctuated(lp.lifetime.clone(), Token![+](Span::call_site())))
        .collect();

    de_generics.params.insert(0, de_lt_param.into());
    let (de_impl_generics, _, _) = de_generics.split_for_impl();

    let (iter_n, iter_n2, iter_n3) = (0usize.., 0usize.., 0usize..);

    match &ty.data {
        SatsTypeData::Product(fields) => {
            let n_fields = fields.len();

            let field_names = fields.iter().map(|f| f.ident.unwrap()).collect::<Vec<_>>();
            let field_strings = fields.iter().map(|f| f.name.as_deref().unwrap()).collect::<Vec<_>>();
            let field_types = fields.iter().map(|f| &f.ty);
            quote! {
                #[allow(non_camel_case_types)]
                #[allow(clippy::all)]
                const _: () = {
                    impl #de_impl_generics #spacetimedb_lib::de::Deserialize<'de> for #name #ty_generics #where_clause {
                        fn deserialize<D: #spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            deserializer.deserialize_product(__ProductVisitor {
                                _marker: std::marker::PhantomData::<fn() -> #name #ty_generics>,
                            })
                        }
                    }

                    struct __ProductVisitor #impl_generics #where_clause {
                        _marker: std::marker::PhantomData<fn() -> #name #ty_generics>,
                    }

                    impl #de_impl_generics #spacetimedb_lib::de::ProductVisitor<'de> for __ProductVisitor #ty_generics #where_clause {
                        type Output = #name #ty_generics;

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
                            while let Some(__field) = #spacetimedb_lib::de::NamedProductAccess::get_field_ident(&mut __prod, Self {
                                _marker: std::marker::PhantomData,
                            })? {
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

                    impl #de_impl_generics #spacetimedb_lib::de::FieldNameVisitor<'de> for __ProductVisitor #ty_generics #where_clause {
                        type Output = __ProductFieldIdent;

                        fn field_names(&self, names: &mut dyn #spacetimedb_lib::de::ValidNames) {
                            names.extend::<&[&str]>(&[#(#field_strings),*])
                        }

                        fn visit<__E: #spacetimedb_lib::de::Error>(self, name: &str) -> Result<Self::Output, __E> {
                            match name {
                                #(#field_strings => Ok(__ProductFieldIdent::#field_names),)*
                                _ => Err(#spacetimedb_lib::de::Error::unknown_field_name(name, &self)),
                            }
                        }
                    }

                    #[allow(non_camel_case_types)]
                    enum __ProductFieldIdent {
                        #(#field_names,)*
                    }
                };
            }
        }
        SatsTypeData::Sum(variants) => {
            let variant_names = variants.iter().map(|var| &*var.name).collect::<Vec<_>>();
            let variant_idents = variants.iter().map(|var| var.ident).collect::<Vec<_>>();
            let tags = 0u8..;
            let arms = variants.iter().map(|var| {
                let ident = var.ident;
                if let (Some(member), Some(ty)) = (&var.member, var.ty) {
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
                }
            });
            quote! {
                #[allow(clippy::all)]
                const _: () = {
                    impl #de_impl_generics #spacetimedb_lib::de::Deserialize<'de> for #name #ty_generics #where_clause {
                        fn deserialize<D: #spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            deserializer.deserialize_sum(__SumVisitor {
                                _marker: std::marker::PhantomData::<fn() -> #name #ty_generics>,
                            })
                        }
                    }

                    struct __SumVisitor #impl_generics #where_clause {
                        _marker: std::marker::PhantomData<fn() -> #name #ty_generics>,
                    }

                    impl #de_impl_generics #spacetimedb_lib::de::SumVisitor<'de> for __SumVisitor #ty_generics #where_clause {
                        type Output = #name #ty_generics;

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

                    #[allow(non_camel_case_types)]
                    enum __Variant {
                        #(#variant_idents,)*
                    }

                    impl #impl_generics #spacetimedb_lib::de::VariantVisitor for __SumVisitor #ty_generics #where_clause {
                        type Output = __Variant;

                        fn variant_names(&self, names: &mut dyn #spacetimedb_lib::de::ValidNames) {
                            names.extend::<&[&str]>(&[#(#variant_names,)*])
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
            }
        }
    }
}

pub(crate) fn derive_serialize(ty: &SatsType) -> TokenStream {
    let spacetimedb_lib = &ty.krate;
    let name = &ty.ident;
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();
    let body = match &ty.data {
        SatsTypeData::Product(fields) => {
            let fieldnames = fields.iter().map(|field| field.ident.as_ref().unwrap());
            let tys = fields.iter().map(|f| &f.ty);
            let fieldnamestrings = fields.iter().map(|field| field.name.as_ref().unwrap());
            let nfields = fields.len();
            quote! {
                let mut __prod = __serializer.serialize_named_product(#nfields)?;
                #(#spacetimedb_lib::ser::SerializeNamedProduct::serialize_element::<#tys>(&mut __prod, Some(#fieldnamestrings), &self.#fieldnames)?;)*
                #spacetimedb_lib::ser::SerializeNamedProduct::end(__prod)
            }
        }
        SatsTypeData::Sum(variants) => {
            let arms = variants.iter().enumerate().map(|(i, var)| {
                let (name,name_str) = (var.ident, &var.name);
                let tag = i as u8;
                if let (Some(member), Some(ty)) = (&var.member, var.ty) {
                    quote_spanned! {ty.span()=>
                        Self::#name { #member: __variant } => __serializer.serialize_variant::<#ty>(#tag, Some(#name_str), __variant),
                    }
                } else {
                    quote! {
                        Self::#name => __serializer.serialize_variant(#tag, Some(#name_str), &()),
                    }
                }
            });
            quote!(match self {
                #(#arms)*
                _ => unreachable!(),
            })
        }
    };
    quote! {
        impl #impl_generics #spacetimedb_lib::ser::Serialize for #name #ty_generics #where_clause {
            fn serialize<S: #spacetimedb_lib::ser::Serializer>(&self, __serializer: S) -> Result<S::Ok, S::Error> {
                #body
            }
        }
    }
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
