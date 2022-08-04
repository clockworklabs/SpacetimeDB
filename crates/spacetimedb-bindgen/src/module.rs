extern crate core;
extern crate proc_macro;

use crate::rust_to_spacetimedb_ident;
use quote::{format_ident, quote, ToTokens};
use syn::ItemStruct;

/// This returns a function which will return the schema (TypeDef) for a struct. The signature
/// for this function is as follows:
/// fn __get_struct_schema__<struct_type_ident>() -> spacetimedb_bindings::TypeDef {
///   ...
/// }
pub(crate) fn autogen_module_struct_to_schema(original_struct: ItemStruct) -> proc_macro2::TokenStream {
    let original_struct_ident = &original_struct.clone().ident;
    let mut fields: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: u8 = 0;

    for field in &original_struct.fields {
        let field_type = field.ty.clone().to_token_stream().to_string();
        let field_type = field_type.as_str();

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(spacetimedb_type) => {
                fields.push(quote! {
                    spacetimedb_bindings::ElementDef {
                        tag: #col_num,
                        element_type: spacetimedb_bindings::TypeDef::#spacetimedb_type,
                    }
                });
            }
            None => match field_type {
                "Hash" => {
                    fields.push(quote! {
                        spacetimedb_bindings::ElementDef {
                            tag: #col_num,
                            element_type: spacetimedb_bindings::TypeDef::Bytes,
                        }
                    });
                }
                _ => {
                    let get_func = format_ident!("__get_struct_schema__{}", field_type);
                    fields.push(quote! {
                        spacetimedb_bindings::ElementDef {
                            tag: #col_num,
                            element_type: #get_func(),
                        }
                    });
                }
            },
        }

        col_num = col_num + 1;
    }

    let return_schema_func_name = format_ident!("__get_struct_schema__{}", original_struct_ident);
    let table_func = quote! {
        #[allow(non_snake_case)]
        fn #return_schema_func_name() -> spacetimedb_bindings::TypeDef {
            return spacetimedb_bindings::TypeDef::Tuple {
                0: spacetimedb_bindings::TupleDef { elements: vec![
                    #(#fields),*
                ] },
            };
        }
    };

    // Output all macro data
    quote! {
        #table_func
    }
}

/// Returns a generated function that will return a struct value from a TupleValue. The signature
/// for this function is as follows:
///
/// fn __tuple_to_struct__<struct_type_ident>(value: TupleValue) -> <struct_type_ident> {
///   ...
/// }
///
/// If the TupleValue's structure does not match the expected fields of the struct, we panic.
pub(crate) fn autogen_module_tuple_to_struct(original_struct: ItemStruct) -> proc_macro2::TokenStream {
    let original_struct_ident = &original_struct.clone().ident;
    let mut match_paren1: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut match_paren2: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut match_body: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut tuple_match1: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut tuple_match2: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut extra_assignments: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: usize = 0;
    let mut tuple_num: u8 = 0;

    for field in &original_struct.fields {
        let field_type = field.ty.clone().to_token_stream().to_string();
        let field_type = field_type.as_str();
        let field_ident = field.clone().ident;
        let tmp_name = format_ident!("field_{}", col_num);
        match_paren1.push(quote! {
            elements_arr[#col_num].clone()
        });

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(spacetimedb_type) => {
                match_paren2.push(quote! {
                    spacetimedb_bindings::TypeValue::#spacetimedb_type(#tmp_name)
                });
            }
            None => match field.ty.clone().to_token_stream().to_string().as_str() {
                "Hash" => {
                    match_paren2.push(quote! {
                        spacetimedb_bindings::TypeValue::Bytes(#tmp_name)
                    });
                    extra_assignments.push(quote! {
                           let #tmp_name : spacetimedb_bindings::hash::Hash = spacetimedb_bindings::hash::Hash::from_slice(#tmp_name.as_slice());
                        });
                }
                _ => {
                    let get_func = format_ident!("__tuple_to_struct__{}", field_type);
                    match_paren2.push(quote! {
                        spacetimedb_bindings::TypeValue::Tuple(#tmp_name)
                    });

                    tuple_match1.push(quote! {
                        #get_func(#tmp_name)
                    });

                    tuple_match2.push(quote! {
                        Some(#tmp_name)
                    });

                    tuple_num += 1;
                }
            },
        }

        match_body.push(quote! {
            #field_ident: #tmp_name
        });

        col_num = col_num + 1;
    }

    let tuple_value_to_struct_func_name = format_ident!("__tuple_to_struct__{}", original_struct_ident);
    if tuple_num > 0 {
        let table_func = quote! {
            #[allow(non_snake_case)]
            fn #tuple_value_to_struct_func_name(value: spacetimedb_bindings::TupleValue) -> Option<#original_struct_ident> {
                let elements_arr = value.elements;
                match (#(#match_paren1),*) {
                    (#(#match_paren2),*) =>
                    {
                        match(#(#tuple_match1),*) {
                            ((#(#tuple_match2),*)) => {
                                #(#extra_assignments)*

                                return Some(#original_struct_ident {
                                    #(#match_body),*
                                });
                            },
                            _ => {}
                        }
                    }
                    _ => {}
                }

                return None;
            }
        };

        // Output all macro data
        return quote! {
            #table_func
        };
    } else {
        let table_func = quote! {
            #[allow(non_snake_case)]
            fn #tuple_value_to_struct_func_name(value: spacetimedb_bindings::TupleValue) -> Option<#original_struct_ident> {
                let elements_arr = value.elements;
                return match (#(#match_paren1),*) {
                    (#(#match_paren2),*) => {
                        #(#extra_assignments)*
                        Some(#original_struct_ident {
                            #(#match_body),*
                        })
                    },
                    _ => None
                }
            }
        };

        // Output all macro data
        return quote! {
            #table_func
        };
    }
}

/// Returns a generated function that will return a tuple from a struct. The signature for this
/// function is as follows:
///
/// fn __struct_to_tuple__<struct_type_ident>(value: <struct_type_ident>>) -> TypeValue::Tuple {
///   ...
/// }
///
/// If the TupleValue's structure does not match the expected fields of the struct, we panic.
pub(crate) fn autogen_module_struct_to_tuple(original_struct: ItemStruct) -> proc_macro2::TokenStream {
    let original_struct_ident = &original_struct.clone().ident;
    let mut type_values: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: usize = 0;

    for field in &original_struct.fields {
        let field_ident = field.clone().ident.unwrap();
        let field_type_str = field.ty.clone().to_token_stream().to_string();
        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(spacetimedb_type) => {
                type_values.push(quote! {
                    spacetimedb_bindings::TypeValue::#spacetimedb_type(value.#field_ident)
                });
            }
            _ => match field_type_str.as_str() {
                "Hash" => {
                    type_values.push(quote! {
                        spacetimedb_bindings::TypeValue::Bytes(value.#field_ident.to_vec())
                    });
                }
                _ => {
                    let struct_to_tuple_value_func_name = format_ident!("__struct_to_tuple__{}", field_type_str);
                    type_values.push(quote! {
                        #struct_to_tuple_value_func_name(value.#field_ident)
                    });
                }
            },
        }

        col_num = col_num + 1;
    }

    let struct_to_tuple_func_name = format_ident!("__struct_to_tuple__{}", original_struct_ident);
    let table_func = quote! {
        #[allow(non_snake_case)]
        fn #struct_to_tuple_func_name(value: #original_struct_ident) -> spacetimedb_bindings::TypeValue {
            return spacetimedb_bindings::TypeValue::Tuple(spacetimedb_bindings::TupleValue {
                elements: vec![
                    #(#type_values),*
                ]
            });
        }
    };

    // Output all macro data
    return quote! {
        #table_func
    };
}
