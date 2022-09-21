extern crate core;
extern crate proc_macro;

use crate::{parse_generated_func, rust_to_spacetimedb_ident};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::punctuated::Iter;
use syn::{FnArg, ItemStruct};

/// Returns a function which returns the schema (TypeDef) for a given Type. The signature
/// for this function is as follows:
/// fn get_struct_schema() -> spacetimedb_lib::TypeDef {
///   ...
/// }
pub(crate) fn module_type_to_schema(path: &syn::Path) -> TokenStream {
    match path.segments[0].ident.to_token_stream().to_string().as_str() {
        "Hash" => {
            quote! {
               spacetimedb_lib::TypeDef::Bytes
            }
        }
        "Vec" => {
            let vec_param = parse_generic_arg(path.segments[0].arguments.to_token_stream());

            match vec_param {
                Ok(param) => match rust_to_spacetimedb_ident(param.to_string().as_str()) {
                    Some(spacetimedb_type) => {
                        quote! {
                            spacetimedb_lib::TypeDef::Vec { element_type: spacetimedb_lib::TypeDef::#spacetimedb_type.into() }
                        }
                    }
                    None => match param.to_string().as_str() {
                        "Hash" => {
                            quote! {
                                 spacetimedb_lib::TypeDef::Vec{ element_type: spacetimedb_lib::TypeDef::Bytes }
                            }
                        }
                        other_type => {
                            let other_type = format_ident!("{}", other_type);
                            quote! {
                                spacetimedb_lib::TypeDef::Vec { element_type: #other_type::get_struct_schema().into() },
                            }
                        }
                    },
                },
                Err(e) => {
                    quote! {compile_err(#e)}
                }
            }
        }
        other_type => {
            let other_type = format_ident!("{}", other_type);
            quote! { #other_type::get_struct_schema() }
        }
    }
}

fn type_to_tuple_schema(arg_name: Option<String>, col_num: u8, ty: &syn::Type) -> Option<TokenStream> {
    let arg_type = ty.clone().to_token_stream().to_string();
    let arg_type = arg_type.as_str();

    let arg_name_token = match arg_name {
        None => {
            quote! { None }
        }
        Some(n) => {
            quote! { Some(#n.to_string())}
        }
    };
    match rust_to_spacetimedb_ident(arg_type) {
        Some(spacetimedb_type) => {
            return Some(quote! {
                spacetimedb_lib::ElementDef {
                    tag: #col_num,
                    name: #arg_name_token,
                    element_type: spacetimedb_lib::TypeDef::#spacetimedb_type,
                }
            });
        }
        None => {
            if let syn::Type::Path(syn::TypePath { ref path, .. }) = ty {
                if !path.segments.is_empty() {
                    let schema = module_type_to_schema(path);
                    return Some(quote! {
                            spacetimedb_lib::ElementDef {
                                tag: #col_num,
                                name: #arg_name_token,
                                element_type: #schema
                            }
                    });
                }
            }
        }
    }
    None
}

pub(crate) fn args_to_tuple_schema(args: Iter<'_, FnArg>) -> Vec<TokenStream> {
    let mut elements = Vec::new();
    let mut col_num: u8 = 0;
    for arg in args {
        match arg {
            FnArg::Receiver(_) => {
                continue;
            }
            FnArg::Typed(arg) => {
                let argument = if let syn::Pat::Ident(pat_ident) = *arg.pat.clone() {
                    Some(pat_ident.ident.to_string())
                } else {
                    None
                };
                match type_to_tuple_schema(argument, col_num, &*arg.ty) {
                    None => {}
                    Some(e) => elements.push(e),
                }
                col_num = col_num + 1;
            }
        }
    }
    elements
}

/// This returns a function which will return the schema (TypeDef) for a struct. The signature
/// for this function is as follows:
/// pub fn get_struct_schema() -> spacetimedb_lib::TypeDef {
///   ...
/// }
pub(crate) fn autogen_module_struct_to_schema(
    original_struct: ItemStruct,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let mut fields: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: u8 = 0;

    for field in &original_struct.fields {
        let field_name = field.ident.clone().unwrap().to_token_stream().to_string();
        match type_to_tuple_schema(Some(field_name), col_num, &field.ty) {
            None => {}
            Some(e) => fields.push(e),
        }
        col_num = col_num + 1;
    }

    match parse_generated_func(quote! {
        pub fn get_struct_schema() -> spacetimedb_lib::TypeDef {
            return spacetimedb_lib::TypeDef::Tuple {
                0: spacetimedb_lib::TupleDef { elements: vec![
                    #(#fields),*
                ] },
            };
        }
    }) {
        Ok(func) => Ok(quote! {
            #[allow(non_snake_case)]
            #func
        }),
        Err(err) => Err(err),
    }
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
    original_struct: ItemStruct,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
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
        let field_ident = field.clone().ident;
        let tmp_name = format_ident!("field_{}", col_num);
        match_paren1.push(quote! {
            elements_arr[#col_num].clone()
        });

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(spacetimedb_type) => {
                match_paren2.push(quote! {
                    spacetimedb_lib::TypeValue::#spacetimedb_type(#tmp_name)
                });
            }
            None => {
                if let syn::Type::Path(syn::TypePath { ref path, .. }) = field.ty {
                    if path.segments.len() > 0 {
                        match path.segments[0].ident.to_token_stream().to_string().as_str() {
                            "Hash" => {
                                match_paren2.push(quote! {
                                    spacetimedb_lib::TypeValue::Bytes(#tmp_name)
                                });
                                extra_assignments.push(quote! {
                                   let #tmp_name : spacetimedb_bindings::hash::Hash = spacetimedb_bindings::hash::Hash::from_slice(#tmp_name.as_slice());
                                });
                            }
                            "Vec" => {
                                let vec_param = parse_generic_arg(path.segments[0].arguments.to_token_stream());
                                let tmp_name_vec: proc_macro2::TokenStream =
                                    format!("native_vec_{}", tmp_name).parse().unwrap();

                                match vec_param {
                                    Ok(param) => {
                                        match_paren2.push(quote! {
                                            spacetimedb_lib::TypeValue::Vec(#tmp_name)
                                        });

                                        match rust_to_spacetimedb_ident(param.to_string().as_str()) {
                                            Some(spacetimedb_type) => {
                                                let err_message = format!(
                                                    "Vec contains wrong type, expected TypeValue::{}",
                                                    spacetimedb_type
                                                );
                                                extra_assignments.push(quote! {
                                                    let mut #tmp_name_vec: Vec<#param> = Vec::<#param>::new();
                                                    for tuple_val in #tmp_name {
                                                        match tuple_val {
                                                            spacetimedb_lib::TypeValue::#spacetimedb_type(entry) => {
                                                                #tmp_name_vec.push(entry);
                                                            }, _ => {
                                                                spacetimedb_bindings::println!(#err_message);
                                                            }
                                                        }
                                                    }
                                                    let #tmp_name = #tmp_name_vec;
                                                });
                                            }
                                            None => match param.to_string().as_str() {
                                                "Hash" => {
                                                    let err_message =
                                                        format!("Vec contains wrong type, expected TypeValue::Tuple");
                                                    extra_assignments.push(quote! {
                                                            let mut #tmp_name_vec: Vec<#param> = Vec::<#param>::new();
                                                            for tuple_val in #tmp_name {
                                                                match tuple_val {
                                                                    spacetimedb_lib::TypeValue::Bytes(entry) => {
                                                                        #tmp_name_vec.push(spacetimedb_bindings::hash::Hash::from_slice(entry.as_slice()));
                                                                    }, _ => {
                                                                        spacetimedb_bindings::println!(#err_message);
                                                                    }
                                                                }
                                                            }
                                                            let #tmp_name = #tmp_name_vec;
                                                        });
                                                }
                                                other_type => {
                                                    let err_message =
                                                        format!("Vec contains wrong type, expected TypeValue::Tuple");
                                                    let other_type_ident = format_ident!("{}", other_type);
                                                    extra_assignments.push(quote! {
                                                            let mut #tmp_name_vec: Vec<#param> = Vec::<#param>::new();
                                                            for tuple_val in #tmp_name {
                                                                match tuple_val {
                                                                    spacetimedb_lib::TypeValue::Tuple(entry) => {
                                                                        match #other_type_ident::tuple_to_struct(entry) {
                                                                            Some(native_value) => {
                                                                                #tmp_name_vec.push(native_value);
                                                                            } None => {
                                                                                spacetimedb_bindings::println!("Failed to convert TypeValue::Tuple to native struct type: {}", #other_type);
                                                                            }
                                                                        }
                                                                    }, _ => {
                                                                        spacetimedb_bindings::println!(#err_message);
                                                                    }
                                                                }
                                                            }
                                                            let #tmp_name = #tmp_name_vec;
                                                        });
                                                }
                                            },
                                        }
                                    }
                                    Err(e) => {
                                        return Err(quote! {
                                            compile_error!(#e)
                                        });
                                    }
                                }
                            }
                            other_type => {
                                let other_type = format_ident!("{}", other_type);
                                match_paren2.push(quote! {
                                    spacetimedb_lib::TypeValue::Tuple(#tmp_name)
                                });

                                tuple_match1.push(quote! {
                                    #other_type::tuple_to_struct(#tmp_name)
                                });

                                tuple_match2.push(quote! {
                                    Some(#tmp_name)
                                });

                                tuple_num += 1;
                            }
                        }
                    }
                }
            }
        }

        match_body.push(quote! {
            #field_ident: #tmp_name
        });

        col_num = col_num + 1;
    }

    return if tuple_num > 0 {
        match parse_generated_func(quote! {
            pub fn tuple_to_struct(value: spacetimedb_lib::TupleValue) -> Option<#original_struct_ident> {
                let elements_arr = value.elements;
                // Here we are enumerating all individual elements in the tuple and matching on the types we're expecting
                match (#(#match_paren1),*) {
                    (#(#match_paren2),*) =>
                    {
                        // Here we are doing any nested tuple parsing
                        match(#(#tuple_match1),*) {
                            ((#(#tuple_match2),*)) => {

                                // Any extra conversion before the final construction happens here
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
        }) {
            Ok(func) => Ok(quote! {
                #[allow(non_snake_case)]
                #func
            }),
            Err(err) => Err(err),
        }
    } else {
        match parse_generated_func(quote! {
            pub fn tuple_to_struct(value: spacetimedb_lib::TupleValue) -> Option<#original_struct_ident> {
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
        }) {
            Ok(func) => Ok(quote! {
                #[allow(non_snake_case)]
                #func
            }),
            Err(err) => Err(err),
        }
    };
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
    original_struct: ItemStruct,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let original_struct_ident = &original_struct.clone().ident;
    let mut type_values: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut vec_conversion: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: usize = 0;

    for field in &original_struct.fields {
        let field_ident = field.clone().ident.unwrap();
        let field_type_str = field.ty.clone().to_token_stream().to_string();
        match rust_to_spacetimedb_ident(field_type_str.as_str()) {
            Some(spacetimedb_type) => {
                type_values.push(quote! {
                    spacetimedb_lib::TypeValue::#spacetimedb_type(value.#field_ident)
                });
            }
            _ => {
                if let syn::Type::Path(syn::TypePath { ref path, .. }) = field.ty {
                    if path.segments.len() > 0 {
                        match path.segments[0].ident.to_token_stream().to_string().as_str() {
                            "Hash" => {
                                type_values.push(quote! {
                                    spacetimedb_lib::TypeValue::Bytes(value.#field_ident.to_vec())
                                });
                            }
                            "Vec" => {
                                let tuple_vec_name: proc_macro2::TokenStream =
                                    format!("tuple_vec_{}", field_ident).parse().unwrap();
                                match parse_generic_arg(path.segments[0].arguments.to_token_stream()) {
                                    Ok(arg) => {
                                        match rust_to_spacetimedb_ident(arg.to_token_stream().to_string().as_str()) {
                                            Some(spacetimedb_type) => {
                                                vec_conversion.push(quote! {
                                                    let mut #tuple_vec_name: Vec<spacetimedb_lib::TypeValue> = Vec::<spacetimedb_lib::TypeValue>::new();
                                                    for entry in value.#field_ident {
                                                        #tuple_vec_name.push(spacetimedb_lib::TypeValue::#spacetimedb_type(entry));
                                                    }
                                                });

                                                type_values.push(quote! {
                                                    spacetimedb_lib::TypeValue::Vec(#tuple_vec_name)
                                                });
                                            }
                                            None => match arg.to_token_stream().to_string().as_str() {
                                                "Hash" => {
                                                    vec_conversion.push(quote! {
                                                            let mut #tuple_vec_name: Vec<spacetimedb_lib::TypeValue> = Vec::<spacetimedb_lib::TypeValue>::new();
                                                            for entry in value.#field_ident {
                                                                #tuple_vec_name.push(entry.data);
                                                            }
                                                        });

                                                    type_values.push(quote! {
                                                        spacetimedb_lib::TypeValue::Vec(#tuple_vec_name)
                                                    });
                                                }
                                                other_type => {
                                                    let other_type = format_ident!("{}", other_type);
                                                    vec_conversion.push(quote! {
                                                            let mut #tuple_vec_name: Vec<spacetimedb_lib::TypeValue> = Vec::<spacetimedb_lib::TypeValue>::new();
                                                            for entry in value.#field_ident {
                                                                #tuple_vec_name.push(#other_type::struct_to_tuple(entry));
                                                            }
                                                        });

                                                    type_values.push(quote! {
                                                        spacetimedb_lib::TypeValue::Vec(#tuple_vec_name)
                                                    });
                                                }
                                            },
                                        }
                                    }
                                    Err(e) => {
                                        return Err(quote! {
                                            compile_err(#e)
                                        });
                                    }
                                }
                            }
                            other_type => {
                                let other_type = format_ident!("{}", other_type);
                                type_values.push(quote! {
                                    #other_type::struct_to_tuple(value.#field_ident)
                                });
                            }
                        }
                    }
                }
            }
        }

        col_num = col_num + 1;
    }

    return match parse_generated_func(quote! {
        pub fn struct_to_tuple(value: #original_struct_ident) -> spacetimedb_lib::TypeValue {
            #(#vec_conversion)*
            return spacetimedb_lib::TypeValue::Tuple(spacetimedb_lib::TupleValue {
                elements: vec![
                    #(#type_values),*
                ]
            });
        }
    }) {
        Ok(func) => Ok(quote! {
            #[allow(non_snake_case)]
            #func
        }),
        Err(err) => Err(err),
    };
}

/// Converts a token stream that is in the form "< MyType >" to just "MyType". This also does
/// input validation to make sure there are no other generic parameters. An example is this
/// type: < Vec < MyVecMember > >
pub(crate) fn parse_generic_arg(stream: proc_macro2::TokenStream) -> Result<proc_macro2::TokenStream, String> {
    let mut x = 0;
    let err_string = format!("Generic argument malformed: {}", stream.to_string());
    let mut tok_stream: Option<proc_macro2::TokenStream> = None;
    for tok_tree in stream {
        let tok_str = tok_tree.to_string();
        match x {
            0 => {
                if !tok_str.eq("<") {
                    return Err(err_string);
                }
            }
            1 => {
                tok_stream = Some(tok_tree.to_token_stream());
            }
            2 => {
                if !tok_str.eq(">") {
                    return Err(err_string);
                }
            }
            _ => {
                // Too many tokens!
                return Err(err_string);
            }
        }

        x += 1;
    }

    return match tok_stream {
        Some(a) => Ok(a),
        None => Err(err_string),
    };
}
