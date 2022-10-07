extern crate core;
extern crate proc_macro;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Iter;
use syn::{FnArg, ItemStruct};

fn type_to_tuple_schema(arg_name: Option<String>, col_num: u8, ty: &syn::Type) -> TokenStream {
    let arg_name_token = match arg_name {
        None => {
            quote! { None }
        }
        Some(n) => {
            quote! { Some(#n.to_string())}
        }
    };
    quote! {
        spacetimedb::spacetimedb_lib::ElementDef {
            tag: #col_num,
            name: #arg_name_token,
            element_type: <#ty as spacetimedb::SchemaType>::get_schema(),
        }
    }
}

pub(crate) fn args_to_tuple_schema(args: Iter<'_, FnArg>) -> Vec<TokenStream> {
    let mut elements = Vec::new();
    let mut col_num: u8 = 0;
    for arg in args {
        match arg {
            FnArg::Receiver(_) => {
                // FIXME: should we error here maybe?
                continue;
            }
            FnArg::Typed(arg) => {
                let argument = if let syn::Pat::Ident(pat_ident) = *arg.pat.clone() {
                    Some(pat_ident.ident.to_string())
                } else {
                    None
                };
                elements.push(type_to_tuple_schema(argument, col_num, &*arg.ty));
                col_num += 1;
            }
        }
    }
    elements
}

/// This returns a function which will return the schema (TypeDef) for a struct. The signature
/// for this function is as follows:
/// pub fn get_struct_schema() -> spacetimedb::spacetimedb_lib::TypeDef {
///   ...
/// }
pub(crate) fn autogen_module_struct_to_schema(
    original_struct: &ItemStruct,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let fields = original_struct.fields.iter().enumerate().map(move |(col_num, field)| {
        let field_name = field.ident.as_ref().map(ToString::to_string);
        let col_num: u8 = col_num.try_into().expect("too many columns");
        type_to_tuple_schema(field_name, col_num, &field.ty)
    });

    let name = &original_struct.ident;
    let tuple_name = name.to_string();
    let (impl_generics, ty_generics, where_clause) = original_struct.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics spacetimedb::TupleType for #name #ty_generics #where_clause {
            fn get_tupledef() -> spacetimedb::spacetimedb_lib::TupleDef {
                spacetimedb::spacetimedb_lib::TupleDef {
                    name: Some(#tuple_name.into()),
                    elements: vec![
                        #(#fields),*
                    ],
                }
            }
        }
    })
}

/// Returns a generated function that will return a struct value from a TupleValue. The signature
/// for this function is as follows:
///
/// pub fn tuple_to_struct(value: TupleValue) -> <struct_type_ident> {
///   ...
/// }
///
/// If the TupleValue's structure does not match the expected fields of the struct, we panic.
pub(crate) fn autogen_module_tuple_to_struct(
    original_struct: &ItemStruct,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let n_fields = original_struct.fields.len();
    let mut fields = Vec::with_capacity(n_fields);
    for (i, field) in original_struct.fields.iter().enumerate() {
        let name = field.ident.as_ref().unwrap();
        let value_varname = format_ident!("value_{}", i);
        fields.push(quote!(#name: spacetimedb::FromValue::from_value(#value_varname)?));
    }
    let name = &original_struct.ident;
    let (impl_generics, ty_generics, where_clause) = original_struct.generics.split_for_impl();
    let value_varnames = (0..n_fields).map(|i| format_ident!("value_{}", i));
    Ok(quote! {
        impl #impl_generics spacetimedb::FromTuple for #name #ty_generics #where_clause {
            fn from_tuple(value: spacetimedb::spacetimedb_lib::TupleValue) -> Option<Self> {
                let value: Box<[_; #n_fields]> = core::convert::TryFrom::try_from(value.elements).ok()?;
                let [#(#value_varnames),*] = *value;
                Some(Self {
                    #(#fields),*
                })
            }
        }
    })
}

/// Returns a generated function that will return a tuple from a struct. The signature for this
/// function is as follows:
///
/// pub fn struct_to_tuple(value: <struct_type_ident>>) -> TypeValue::Tuple {
///   ...
/// }
///
/// If the TupleValue's structure does not match the expected fields of the struct, we panic.
pub(crate) fn autogen_module_struct_to_tuple(
    original_struct: &ItemStruct,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let fieldnames = original_struct.fields.iter().map(|field| field.ident.as_ref().unwrap());
    let name = &original_struct.ident;
    let (impl_generics, ty_generics, where_clause) = original_struct.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics spacetimedb::IntoTuple for #name #ty_generics #where_clause {
            fn into_tuple(self) -> spacetimedb::spacetimedb_lib::TupleValue {
                spacetimedb::spacetimedb_lib::TupleValue {
                    elements: vec![
                        #(spacetimedb::IntoValue::into_value(self.#fieldnames)),*
                    ]
                    .into(),
                }
            }
        }
    })
}
