extern crate core;
extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::punctuated::Pair;
use syn::spanned::Spanned;
use syn::{LitStr, Token};

use crate::sym;
use crate::util::{check_duplicate, match_meta};

pub(crate) struct SatsType<'a> {
    pub ident: &'a syn::Ident,
    pub generics: &'a syn::Generics,
    pub name: LitStr,
    pub krate: TokenStream,
    // may want to use in the future
    #[allow(unused)]
    pub original_attrs: &'a [syn::Attribute],
    pub data: SatsTypeData<'a>,
    /// Was the type marked as `#[repr(C)]`?
    pub is_repr_c: bool,
}

pub(crate) enum SatsTypeData<'a> {
    Product(Vec<SatsField<'a>>),
    Sum(Vec<SatsVariant<'a>>),
}

#[derive(Clone)]
pub(crate) struct SatsField<'a> {
    pub ident: Option<&'a syn::Ident>,
    pub vis: &'a syn::Visibility,
    pub name: Option<String>,
    pub ty: &'a syn::Type,
    pub original_attrs: &'a [syn::Attribute],
}

pub(crate) struct SatsVariant<'a> {
    pub ident: &'a syn::Ident,
    pub name: String,
    pub ty: Option<&'a syn::Type>,
    pub member: Option<syn::Member>,
    // may want to use in the future
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

fn is_repr_c(attrs: &[syn::Attribute]) -> bool {
    let mut is_repr_c = false;
    for attr in attrs.iter().filter(|a| a.path() == sym::repr) {
        let _ = attr.parse_nested_meta(|meta| {
            is_repr_c |= meta.path.is_ident("C");
            Ok(())
        });
    }
    is_repr_c
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
        if attr.path() != sym::sats {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::crate_ => {
                    check_duplicate(&krate, &meta)?;
                    let value = meta.value()?;
                    let v = value.call(syn::Path::parse_mod_style)?;
                    krate = Some(v.into_token_stream());
                }
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    let value = meta.value()?;
                    let v = value.parse::<LitStr>()?;
                    name = Some(v);
                }
            });
            Ok(())
        })?;
    }
    let krate = krate.unwrap_or(crate_fallback);
    let name = name.unwrap_or_else(|| crate::util::ident_to_litstr(ident));

    let is_repr_c = is_repr_c(attrs);

    Ok(SatsType {
        ident,
        generics,
        name,
        krate,
        original_attrs: attrs,
        data,
        is_repr_c,
    })
}

pub(crate) fn derive_satstype(ty: &SatsType<'_>) -> TokenStream {
    let ty_name = &ty.name;
    let name = &ty.ident;
    let krate = &ty.krate;

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
                    <#ty as #krate::SpacetimeType>::make_type(__typespace)
                ))
            });
            let len = fields.len();
            quote!(
                #krate::sats::AlgebraicType::product::<
                    [(Option<&str>, #krate::sats::AlgebraicType); #len]
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
                    <#ty as #krate::SpacetimeType>::make_type(__typespace)
                ))
            });
            let len = variants.len();
            quote!(
                #krate::sats::AlgebraicType::sum::<
                    [(&str, #krate::sats::AlgebraicType); #len]
                >(
                    [#(#variants),*]
                )
            )
            // todo!()
        } // syn::Data::Union(u) => return Err(syn::Error::new(u.union_token.span, "unions not supported")),
    };

    let mut sats_generics = ty.generics.clone();
    // the 'static here is an unfortunate restriction from TypeId :(
    add_type_bounds(&mut sats_generics, &quote!(#krate::SpacetimeType + 'static));
    let (impl_generics, ty_generics, where_clause) = sats_generics.split_for_impl();

    // TypeId::of() requires all the lifetimes to be 'static
    let mut typeid_generics = sats_generics.clone();
    let static_lt = syn::Lifetime::new("'static", Span::call_site());
    for param in &mut typeid_generics.params {
        if let syn::GenericParam::Lifetime(param) = param {
            param.lifetime = static_lt.clone();
        }
    }
    let (_, typeid_ty_generics, _) = typeid_generics.split_for_impl();

    quote! {
        #[automatically_derived]
        impl #impl_generics #krate::SpacetimeType for #name #ty_generics #where_clause {
            fn make_type<S: #krate::sats::typespace::TypespaceBuilder>(__typespace: &mut S) -> #krate::sats::AlgebraicType {
                #krate::sats::typespace::TypespaceBuilder::add(
                    __typespace,
                    core::any::TypeId::of::<#name #typeid_ty_generics>(),
                    Some(#ty_name),
                    |__typespace| #typ,
                )
            }
        }
    }
}

fn add_type_bounds(generics: &mut syn::Generics, trait_bound: &TokenStream) {
    for param in &generics.params {
        let syn::GenericParam::Type(param) = param else {
            continue;
        };
        let param_name = &param.ident;
        let where_clause = generics.where_clause.get_or_insert_with(|| syn::WhereClause {
            where_token: Default::default(),
            predicates: Default::default(),
        });
        where_clause
            .predicates
            .push(syn::parse_quote!(#param_name: #trait_bound));
    }
}

/// Returns the list of types if syntactically we see that the `ty`
/// is `#[repr(C)]` of only primitives.
///
/// We later assert semantically in generated code that the list of types
/// actually are primitives.
/// We'll also check that `ty` is paddingless.
fn extract_repr_c_primitive<'a>(ty: &'a SatsType) -> Option<Vec<&'a syn::Ident>> {
    // Ensure we have a `#[repr(C)]` struct.
    if !ty.is_repr_c {
        return None;
    }
    let SatsTypeData::Product(fields) = &ty.data else {
        return None;
    };

    // Ensure every field is a primitive and collect the idents.
    const PRIM_TY: &[sym::Symbol] = &[
        sym::u8,
        sym::i8,
        sym::u16,
        sym::i16,
        sym::u32,
        sym::i32,
        sym::u64,
        sym::i64,
        sym::u128,
        sym::i128,
        sym::f32,
        sym::f64,
    ];
    let mut field_tys = Vec::with_capacity(fields.len());
    for field in fields {
        if let syn::Type::Path(ty) = &field.ty {
            let ident = ty.path.get_ident().filter(|ident| PRIM_TY.iter().any(|p| ident == p))?;
            field_tys.push(ident);
        } else {
            return None;
        }
    }
    Some(field_tys)
}

pub(crate) fn derive_deserialize(ty: &SatsType<'_>) -> TokenStream {
    let (name, tuple_name) = (&ty.ident, &ty.name);
    let spacetimedb_lib = &ty.krate;
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

    let de_lifetime = syn::Lifetime::new("'de", Span::call_site());
    let deserialize_t = quote!(#spacetimedb_lib::de::Deserialize<#de_lifetime>);

    let mut de_generics = ty.generics.clone();

    add_type_bounds(&mut de_generics, &deserialize_t);

    for lp in de_generics.lifetimes_mut() {
        lp.bounds.push(de_lifetime.clone());
    }

    let mut de_lt_param = syn::LifetimeParam::new(de_lifetime);
    de_lt_param.bounds = de_generics
        .lifetimes()
        .map(|lp| Pair::Punctuated(lp.lifetime.clone(), Token![+](Span::call_site())))
        .collect();

    de_generics.params.insert(0, de_lt_param.into());
    let (de_impl_generics, _, de_where_clause) = de_generics.split_for_impl();

    let (iter_n, iter_n2, iter_n3) = (0usize.., 0usize.., 0usize..);

    match &ty.data {
        SatsTypeData::Product(fields) => {
            let mut fast_body = None;
            if let Some(fields) = extract_repr_c_primitive(ty) {
                fast_body = Some(quote! {
                    #[inline(always)]
                    fn deserialize_from_bsatn<R: #spacetimedb_lib::buffer::BufReader<'de>>(
                        mut deserializer: #spacetimedb_lib::bsatn::Deserializer<'de, R>
                    ) -> Result<Self, #spacetimedb_lib::bsatn::DecodeError> {
                        const _: () = {
                            #(#spacetimedb_lib::bsatn::assert_is_primitive_type::<#fields>();)*
                        };
                        // This guarantees that `Self` has no padding.
                        if const { core::mem::size_of::<Self>() == #(core::mem::size_of::<#fields>())+* } {
                            let bytes = deserializer.get_slice(core::mem::size_of::<Self>())?;
                            let ptr = bytes as *const [u8] as *const u8 as *const Self;
                            // SAFETY:
                            // - `ptr` is valid for reads, `size_of::<T>()`.
                            // - `ptr` is trivially properly aligned (alignment = 1).
                            // - `ptr` points to a properly initialized `Foo`
                            //   as we've guaranteed that there is no padding.
                            Ok(unsafe { core::ptr::read(ptr) })
                        } else {
                            Self::deserialize(deserializer)
                        }
                    }
                });
            }

            let n_fields = fields.len();

            let field_names = fields.iter().map(|f| f.ident.unwrap()).collect::<Vec<_>>();
            let field_strings = fields.iter().map(|f| f.name.as_deref().unwrap()).collect::<Vec<_>>();
            let field_types = fields.iter().map(|f| &f.ty);
            let field_types2 = field_types.clone();
            quote! {
                #[allow(non_camel_case_types)]
                #[allow(clippy::all)]
                const _: () = {
                    impl #de_impl_generics #spacetimedb_lib::de::Deserialize<'de> for #name #ty_generics #de_where_clause {
                        #fast_body

                        fn deserialize<D: #spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            deserializer.deserialize_product(__ProductVisitor {
                                _marker: std::marker::PhantomData::<fn() -> #name #ty_generics>,
                            })
                        }
                    }

                    struct __ProductVisitor #impl_generics #where_clause {
                        _marker: std::marker::PhantomData<fn() -> #name #ty_generics>,
                    }

                    impl #de_impl_generics #spacetimedb_lib::de::ProductVisitor<'de> for __ProductVisitor #ty_generics #de_where_clause {
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
                                        #field_names = Some(#spacetimedb_lib::de::NamedProductAccess::get_field_value::<#field_types2>(&mut __prod)?)
                                    })*
                                }
                            }
                            Ok(#name {
                                #(#field_names:
                                    #field_names.ok_or_else(|| #spacetimedb_lib::de::Error::missing_field(#iter_n3, Some(#field_strings), &self))?,)*
                            })
                        }
                    }

                    impl #de_impl_generics #spacetimedb_lib::de::FieldNameVisitor<'de> for __ProductVisitor #ty_generics #de_where_clause {
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
                    impl #de_impl_generics #spacetimedb_lib::de::Deserialize<'de> for #name #ty_generics #de_where_clause {
                        fn deserialize<D: #spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            deserializer.deserialize_sum(__SumVisitor {
                                _marker: std::marker::PhantomData::<fn() -> #name #ty_generics>,
                            })
                        }
                    }

                    struct __SumVisitor #impl_generics #where_clause {
                        _marker: std::marker::PhantomData<fn() -> #name #ty_generics>,
                    }

                    impl #de_impl_generics #spacetimedb_lib::de::SumVisitor<'de> for __SumVisitor #ty_generics #de_where_clause {
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

                    impl #de_impl_generics #spacetimedb_lib::de::VariantVisitor for __SumVisitor #ty_generics #de_where_clause {
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

    let mut generics = ty.generics.clone();

    let serialize_t = quote!(#spacetimedb_lib::ser::Serialize);
    add_type_bounds(&mut generics, &serialize_t);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut fast_body = None;
    let body = match &ty.data {
        SatsTypeData::Product(fields) => {
            if let Some(fields) = extract_repr_c_primitive(ty) {
                fast_body = Some(quote! {
                    #[inline(always)]
                    fn serialize_into_bsatn<W: #spacetimedb_lib::buffer::BufWriter>(
                            &self,
                            serializer: #spacetimedb_lib::bsatn::Serializer<'_, W>
                    ) -> Result<(), #spacetimedb_lib::bsatn::EncodeError> {
                        const _: () = {
                            #(#spacetimedb_lib::bsatn::assert_is_primitive_type::<#fields>();)*
                        };
                        // This guarantees that `Self` has no padding.
                        if const { core::mem::size_of::<Self>() == #(core::mem::size_of::<#fields>())+* } {
                            // SAFETY:
                            // - We know `self` is non-null as it's a shared reference
                            //   and we know it's valid for reads for `core::mem::size_of::<Self>()` bytes.
                            //   Alignment of `u8` is 1, so it's trivially satisfied.
                            //   - The slice is all within `self`, so in the same allocated object.
                            // - `self` does point to `core::mem::size_of::<Self>()` consecutive `u8`s,
                            //    as per `assert_is_primitive_type` above,
                            //    we know none of the fields of `Self` have any padding.
                            // - We're not going to mutate the memory within `bytes`.
                            // - We know `core::mem::size_of::<Self>() < isize::MAX`.
                            let bytes = unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, core::mem::size_of::<Self>()) };
                            serializer.raw_write_bytes(bytes);
                            Ok(())
                        } else {
                            self.serialize(serializer)
                        }
                    }
                });
            }

            let fieldnames = fields.iter().map(|field| field.ident.unwrap());
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
            #fast_body
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
