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

        match rust_to_spacetimedb_ident(field_type) {
            Some(spacetimedb_type) => {
                fields.push(quote! {
                    spacetimedb_bindings::ElementDef {
                        tag: #col_num,
                        element_type: spacetimedb_bindings::TypeDef::#spacetimedb_type,
                    }
                });
            }
            None => {
                if let syn::Type::Path(syn::TypePath { ref path, .. }) = field.ty {
                    if path.segments.len() > 0 {
                        match path.segments[0].ident.to_token_stream().to_string().as_str() {
                            "Hash" => {
                                fields.push(quote! {
                                    spacetimedb_bindings::ElementDef {
                                        tag: #col_num,
                                        element_type: spacetimedb_bindings::TypeDef::Bytes,
                                    }
                                });
                            }
                            "Vec" => {
                                let vec_param = parse_generic_arg(path.segments[0].arguments.to_token_stream());

                                match vec_param {
                                    Ok(param) => match rust_to_spacetimedb_ident(param.to_string().as_str()) {
                                        Some(spacetimedb_type) => {
                                            fields.push(quote! {
                                                    spacetimedb_bindings::ElementDef {
                                                        tag: #col_num,
                                                        element_type: spacetimedb_bindings::TypeDef::Vec{ element_type: spacetimedb_bindings::TypeDef::#spacetimedb_type.into() },
                                                    }
                                                });
                                        }
                                        None => match param.to_string().as_str() {
                                            "Hash" => {
                                                fields.push(quote! {
                                                            spacetimedb_bindings::ElementDef {
                                                                tag: #col_num,
                                                                element_type: spacetimedb_bindings::TypeDef::Vec{ element_type: spacetimedb_bindings::TypeDef::Bytes },
                                                            }
                                                        });
                                            }
                                            other_type => {
                                                let get_schema_func: proc_macro2::TokenStream =
                                                    format!("__get_struct_schema__{}", other_type).parse().unwrap();
                                                fields.push(quote! {
                                                            spacetimedb_bindings::ElementDef {
                                                                tag: #col_num,
                                                                element_type: spacetimedb_bindings::TypeDef::Vec{ element_type: #get_schema_func().into() },
                                                            }
                                                        });
                                            }
                                        },
                                    },
                                    Err(e) => {
                                        return quote! {
                                            compile_err(#e)
                                        }
                                    }
                                }
                            }
                            other_type => {
                                let get_func = format_ident!("__get_struct_schema__{}", other_type);
                                fields.push(quote! {
                                    spacetimedb_bindings::ElementDef {
                                        tag: #col_num,
                                        element_type: #get_func(),
                                    }
                                });
                            }
                        }
                    }
                }
            }
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
            None => {
                if let syn::Type::Path(syn::TypePath { ref path, .. }) = field.ty {
                    if path.segments.len() > 0 {
                        match path.segments[0].ident.to_token_stream().to_string().as_str() {
                            "Hash" => {
                                match_paren2.push(quote! {
                                    spacetimedb_bindings::TypeValue::Bytes(#tmp_name)
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
                                            spacetimedb_bindings::TypeValue::Vec(#tmp_name)
                                        });

                                        match rust_to_spacetimedb_ident(param.to_string().as_str()) {
                                            Some(spacetimedb_type) => {
                                                let err_message = format!(
                                                    "Vec contains wrong type, expected TypeValue::{}",
                                                    spacetimedb_type
                                                );
                                                extra_assignments.push(quote!{
                                                    let mut #tmp_name_vec: Vec<#param> = Vec::<#param>::new();
                                                    for tuple_val in #tmp_name {
                                                        match tuple_val {
                                                            spacetimedb_bindings::TypeValue::#spacetimedb_type(entry) => {
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
                                                    extra_assignments.push(quote!{
                                                            let mut #tmp_name_vec: Vec<#param> = Vec::<#param>::new();
                                                            for tuple_val in #tmp_name {
                                                                match tuple_val {
                                                                    spacetimedb_bindings::TypeValue::Bytes(entry) => {
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
                                                    let conversion_func: proc_macro2::TokenStream =
                                                        format!("__tuple_to_struct__{}", other_type).parse().unwrap();
                                                    extra_assignments.push(quote!{
                                                            let mut #tmp_name_vec: Vec<#param> = Vec::<#param>::new();
                                                            for tuple_val in #tmp_name {
                                                                match tuple_val {
                                                                    spacetimedb_bindings::TypeValue::Tuple(entry) => {
                                                                        match #conversion_func(entry) {
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
                                        return quote! {
                                            compile_error!(#e)
                                        }
                                    }
                                }
                            }
                            other_type => {
                                let get_func = format_ident!("__tuple_to_struct__{}", other_type);
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

    let tuple_value_to_struct_func_name = format_ident!("__tuple_to_struct__{}", original_struct_ident);
    if tuple_num > 0 {
        let table_func = quote! {
            #[allow(non_snake_case)]
            fn #tuple_value_to_struct_func_name(value: spacetimedb_bindings::TupleValue) -> Option<#original_struct_ident> {
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
    let mut vec_conversion: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: usize = 0;

    for field in &original_struct.fields {
        let field_ident = field.clone().ident.unwrap();
        let field_type_str = field.ty.clone().to_token_stream().to_string();
        match rust_to_spacetimedb_ident(field_type_str.as_str()) {
            Some(spacetimedb_type) => {
                type_values.push(quote! {
                    spacetimedb_bindings::TypeValue::#spacetimedb_type(value.#field_ident)
                });
            }
            _ => {
                if let syn::Type::Path(syn::TypePath { ref path, .. }) = field.ty {
                    if path.segments.len() > 0 {
                        match path.segments[0].ident.to_token_stream().to_string().as_str() {
                            "Hash" => {
                                type_values.push(quote! {
                                    spacetimedb_bindings::TypeValue::Bytes(value.#field_ident.to_vec())
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
                                                    let mut #tuple_vec_name: Vec<spacetimedb_bindings::TypeValue> = Vec::<spacetimedb_bindings::TypeValue>::new();
                                                    for entry in value.#field_ident {
                                                        #tuple_vec_name.push(spacetimedb_bindings::TypeValue::#spacetimedb_type(entry));
                                                    }
                                                });

                                                type_values.push(quote! {
                                                    spacetimedb_bindings::TypeValue::Vec(#tuple_vec_name)
                                                });
                                            }
                                            None => match arg.to_token_stream().to_string().as_str() {
                                                "Hash" => {
                                                    vec_conversion.push(quote! {
                                                            let mut #tuple_vec_name: Vec<spacetimedb_bindings::TypeValue> = Vec::<spacetimedb_bindings::TypeValue>::new();
                                                            for entry in value.#field_ident {
                                                                #tuple_vec_name.push(entry.data);
                                                            }
                                                        });

                                                    type_values.push(quote! {
                                                        spacetimedb_bindings::TypeValue::Vec(#tuple_vec_name)
                                                    });
                                                }
                                                other_type => {
                                                    let conversion_func: proc_macro2::TokenStream =
                                                        format!("__struct_to_tuple__{}", other_type).parse().unwrap();
                                                    vec_conversion.push(quote! {
                                                            let mut #tuple_vec_name: Vec<spacetimedb_bindings::TypeValue> = Vec::<spacetimedb_bindings::TypeValue>::new();
                                                            for entry in value.#field_ident {
                                                                #tuple_vec_name.push(#conversion_func(entry));
                                                            }
                                                        });

                                                    type_values.push(quote! {
                                                        spacetimedb_bindings::TypeValue::Vec(#tuple_vec_name)
                                                    });
                                                }
                                            },
                                        }
                                    }
                                    Err(e) => {
                                        return quote! {
                                            compile_err(#e)
                                        }
                                    }
                                }
                            }
                            other_type => {
                                let struct_to_tuple_value_func_name =
                                    format_ident!("__struct_to_tuple__{}", other_type);
                                type_values.push(quote! {
                                    #struct_to_tuple_value_func_name(value.#field_ident)
                                });
                            }
                        }
                    }
                }
            }
        }

        col_num = col_num + 1;
    }

    let struct_to_tuple_func_name = format_ident!("__struct_to_tuple__{}", original_struct_ident);
    let table_func = quote! {
        #[allow(non_snake_case)]
        fn #struct_to_tuple_func_name(value: #original_struct_ident) -> spacetimedb_bindings::TypeValue {
            #(#vec_conversion)*
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
