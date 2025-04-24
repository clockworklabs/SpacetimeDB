extern crate core;
extern crate proc_macro;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::LitStr;

use super::sym;
use super::util::{check_duplicate, match_meta};

pub struct SatsType<'a> {
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

pub enum SatsTypeData<'a> {
    Product(Vec<SatsField<'a>>),
    Sum(Vec<SatsVariant<'a>>),
}

#[derive(Clone)]
pub struct SatsField<'a> {
    pub ident: Option<&'a syn::Ident>,
    pub vis: &'a syn::Visibility,
    pub name: Option<String>,
    pub ty: &'a syn::Type,
    pub original_attrs: &'a [syn::Attribute],
}

pub struct SatsVariant<'a> {
    pub ident: &'a syn::Ident,
    pub name: String,
    pub ty: Option<&'a syn::Type>,
    pub member: Option<syn::Member>,
    // may want to use in the future
    #[allow(unused)]
    pub original_attrs: &'a [syn::Attribute],
}

pub fn sats_type_from_derive(
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

fn extract_sats_type<'a>(
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
