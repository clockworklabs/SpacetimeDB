#![crate_type = "proc-macro"]

mod csharp;
mod module;

extern crate core;
extern crate proc_macro;

use crate::csharp::{spacetimedb_csharp_reducer, spacetimedb_csharp_tuple};
use crate::module::{autogen_module_struct_to_schema, autogen_module_struct_to_tuple, autogen_module_tuple_to_struct};
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote, ToTokens};
use regex::Regex;
use std::time::Duration;
use substring::Substring;
use syn::Fields::{Named, Unit, Unnamed};
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, ItemStruct};

// When we add support for more than 1 language uncomment this. For now its just cumbersome.
// enum Lang {
//     CS
// }

#[proc_macro_attribute]
pub fn spacetimedb(macro_args: TokenStream, item: TokenStream) -> TokenStream {
    // When we add support for more than 1 language uncomment this. For now its just cumbersome.
    // let mut lang: Option<Lang> = None;
    // for var in std::env::vars() {
    //     if var.0 == "STDB_LANG" {
    //         match var.1.to_lowercase().as_str() {
    //             "cs" | "csharp" | "c#"  => {
    //                 println!("Language set to csharp.");
    //                 lang = Some(Lang::CS);
    //             }
    //             _ => {
    //                 let str = format!("Unsupported language: {}\nSupported languages: CS (csharp)", var.1);
    //                 return proc_macro::TokenStream::from(quote! {
    //                     compile_error!(#str);
    //                 });
    //             }
    //         }
    //     }
    // }
    //
    // if let None = lang {
    //     let str = format!("No client language set. Supported languages: CS (csharp)");
    //     return proc_macro::TokenStream::from(quote! {
    //         compile_error!(#str);
    //     });
    // }

    let attribute_args = parse_macro_input!(macro_args as AttributeArgs);
    let attribute_str = attribute_args[0].to_token_stream().to_string();
    let attribute_str = attribute_str.as_str();
    let create_table_regex = Regex::new(r"table\(\d+\)").unwrap();
    if create_table_regex.is_match(attribute_str) {
        let table_num_str = attribute_str.substring(6, attribute_str.len() - 1);
        let table_id_parsed = table_num_str.parse::<u32>();
        return match table_id_parsed {
            Ok(table_id) => spacetimedb_table(attribute_args, item, table_id),
            Err(_) => {
                let str = format!("Invalid table ID provided in macro: {}", table_num_str);
                proc_macro::TokenStream::from(quote! {
                    compile_error!(#str);
                })
            }
        };
    }

    match attribute_str {
        "reducer" => spacetimedb_reducer(attribute_args, item),
        "connect" => spacetimedb_connect_disconnect(attribute_args, item, true),
        "disconnect" => spacetimedb_connect_disconnect(attribute_args, item, false),
        "migrate" => spacetimedb_migrate(attribute_args, item),
        "tuple" => spacetimedb_tuple(attribute_args, item),
        "index(btree)" => spacetimedb_index(attribute_args, item),
        "index(hash)" => spacetimedb_index(attribute_args, item),
        _ => proc_macro::TokenStream::from(quote! {
            compile_error!("Please pass a valid attribute to the spacetimedb macro: reducer, table, connect, disconnect, migrate, tuple, index, ...");
        }),
    }
}

fn spacetimedb_reducer(args: AttributeArgs, item: TokenStream) -> TokenStream {
    if *(&args.len()) > 1 {
        let arg = args[1].to_token_stream();
        let arg_components = arg.into_iter().collect::<Vec<_>>();
        let arg_name = &arg_components[0];
        let repeat = match arg_name {
            proc_macro2::TokenTree::Group(_) => false,
            proc_macro2::TokenTree::Ident(ident) => {
                if ident.to_string() != "repeat" {
                    false
                } else {
                    true
                }
            }
            proc_macro2::TokenTree::Punct(_) => false,
            proc_macro2::TokenTree::Literal(_) => false,
        };
        if !repeat {
            let str = format!("Unexpected macro argument name: {}", arg_name.to_string());
            return proc_macro::TokenStream::from(quote! {
                compile_error!(#str);
            });
        }
        let arg_value = &arg_components[2];
        let res = parse_duration::parse(&arg_value.to_string());
        if let Err(_) = res {
            let str = format!("Can't parse repeat time: {}", arg_value.to_string());
            return proc_macro::TokenStream::from(quote! {
                compile_error!(#str);
            });
        }
        let repeat_duration = res.unwrap();

        return spacetimedb_repeating_reducer(args, item, repeat_duration);
    }

    let original_function = parse_macro_input!(item as ItemFn);
    let func_name = &original_function.sig.ident;
    let reducer_func_name = format_ident!("__reducer__{}", &func_name);

    let mut parse_json_to_args = Vec::new();
    let mut function_call_args = Vec::new();
    let mut arg_num: usize = 0;
    let mut json_arg_num: usize = 0;
    let function_arguments = &original_function.sig.inputs;
    for function_argument in function_arguments {
        match function_argument {
            FnArg::Receiver(_) => {
                return proc_macro::TokenStream::from(quote! {
                    compile_error!("Receiver types in reducer parameters not supported!");
                });
            }
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_token = arg_type.to_token_stream();
                let arg_type_str = arg_token.to_string();
                let var_name = format_ident!("arg_{}", arg_num);

                // First argument must be Hash (sender)
                if arg_num == 0 {
                    if arg_type_str != "spacetimedb_bindings :: hash :: Hash" && arg_type_str != "Hash" {
                        let error_str = format!(
                            "Parameter 1 of reducer {} must be of type \'Hash\'.",
                            func_name.to_string()
                        );
                        return proc_macro::TokenStream::from(quote! {
                            compile_error!(#error_str);
                        });
                    }
                    arg_num += 1;
                    continue;
                }

                // Second argument must be a u64 (timestamp)
                if arg_num == 1 {
                    if arg_type_str != "u64" {
                        let error_str = format!(
                            "Parameter 2 of reducer {} must be of type \'u64\'.",
                            func_name.to_string()
                        );
                        return proc_macro::TokenStream::from(quote! {
                            compile_error!(#error_str);
                        });
                    }
                    arg_num += 1;
                    continue;
                }

                parse_json_to_args.push(quote! {
                    let #var_name : #arg_token = serde_json::from_value(args[#json_arg_num].clone()).unwrap();
                });
                function_call_args.push(var_name);

                json_arg_num += 1;
            }
        }

        arg_num = arg_num + 1;
    }

    let generated_function = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #reducer_func_name(arg_ptr: usize, arg_size: usize) {
            const HEADER_SIZE: usize = 40;
            let arg_ptr = arg_ptr as *mut u8;
            let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size + HEADER_SIZE, arg_size + HEADER_SIZE) };

            let sender = spacetimedb_bindings::hash::Hash::from_slice(&bytes[0..32]);

            let mut buf = [0; 8];
            buf.copy_from_slice(&bytes[32..HEADER_SIZE]);
            let timestamp = u64::from_le_bytes(buf);

            let arg_json: serde_json::Value = serde_json::from_slice(&bytes[HEADER_SIZE..]).unwrap();

            let args = arg_json.as_array().unwrap();

            // Deserialize the json argument list
            #(#parse_json_to_args);*

            // Invoke the function with the deserialized args
            #func_name(sender, timestamp, #(#function_call_args),*);
        }
    };

    spacetimedb_csharp_reducer(original_function.clone());

    proc_macro::TokenStream::from(quote! {
        #generated_function
        #original_function
    })
}

fn spacetimedb_repeating_reducer(_args: AttributeArgs, item: TokenStream, repeat_duration: Duration) -> TokenStream {
    let original_function = parse_macro_input!(item as ItemFn);
    let func_name = &original_function.sig.ident;
    let reducer_func_name = format_ident!("__repeating_reducer__{}", &func_name);

    let mut arg_num: usize = 0;
    let function_arguments = &original_function.sig.inputs;
    if function_arguments.len() != 2 {
        return proc_macro::TokenStream::from(quote! {
            compile_error!("Expected 2 arguments (timestamp: u64, delta_time: u64) for repeating reducer.");
        });
    }
    for function_argument in function_arguments {
        match function_argument {
            FnArg::Receiver(_) => {
                return proc_macro::TokenStream::from(quote! {
                    compile_error!("Receiver types in reducer parameters not supported!");
                });
            }
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_token = arg_type.to_token_stream();
                let arg_type_str = arg_token.to_string();

                // First argument must be a u64 (timestamp)
                if arg_num == 0 {
                    if arg_type_str != "u64" {
                        let error_str = format!(
                            "Parameter 1 of reducer {} must be of type \'u64\'.",
                            func_name.to_string()
                        );
                        return proc_macro::TokenStream::from(quote! {
                            compile_error!(#error_str);
                        });
                    }
                    arg_num += 1;
                    continue;
                }

                // Second argument must be an u64 (delta_time)
                if arg_num == 1 {
                    if arg_type_str != "u64" {
                        let error_str = format!(
                            "Parameter 2 of reducer {} must be of type \'u64\'.",
                            func_name.to_string()
                        );
                        return proc_macro::TokenStream::from(quote! {
                            compile_error!(#error_str);
                        });
                    }
                    arg_num += 1;
                    continue;
                }
            }
        }
        arg_num = arg_num + 1;
    }

    let duration_as_millis = repeat_duration.as_millis() as u64;
    let generated_function = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #reducer_func_name(arg_ptr: usize, arg_size: usize) -> u64 {
            const HEADER_SIZE: usize = 16;
            let arg_ptr = arg_ptr as *mut u8;
            let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size + HEADER_SIZE, arg_size + HEADER_SIZE) };

            let mut buf = [0; 8];
            buf.copy_from_slice(&bytes[0..8]);
            let timestamp = u64::from_le_bytes(buf);

            let mut buf = [0; 8];
            buf.copy_from_slice(&bytes[8..HEADER_SIZE]);
            let delta_time = u64::from_le_bytes(buf);

            // Invoke the function with the deserialized args
            #func_name(timestamp, delta_time);

            return #duration_as_millis;
        }
    };

    proc_macro::TokenStream::from(quote! {
        #generated_function
        #original_function
    })
}

fn spacetimedb_table(args: AttributeArgs, item: TokenStream, table_id: u32) -> TokenStream {
    if *(&args.len()) > 1 {
        let str = format!("Unexpected macro argument: {}", args[1].to_token_stream().to_string());
        return proc_macro::TokenStream::from(quote! {
            compile_error!(#str);
        });
    }

    let original_struct = parse_macro_input!(item as ItemStruct);
    let original_struct_ident = &original_struct.clone().ident;

    match original_struct.clone().fields {
        Named(_) => {
            // let table_id_field: Field = Field {
            //     attrs: Vec::new(),
            //     vis: Visibility::Public(VisPublic { pub_token: Default::default() }),
            //     ident: Some(format_ident!("{}", "table_id")),
            //     colon_token: Some(Colon::default()),
            //     ty: syn::Type::Verbatim(format_ident!("{}", "u32").to_token_stream()),
            // };
            //
            // fields.named.push(table_id_field);
        }
        Unnamed(_) => {
            let str = format!("spacetimedb tables must have named fields.");
            return proc_macro::TokenStream::from(quote! {
                compile_error!(#str);
            });
        }
        Unit => {
            let str = format!("spacetimedb tables must have named fields (unit struct forbidden).");
            return proc_macro::TokenStream::from(quote! {
                compile_error!(#str);
            });
        }
    }

    let mut column_idents: Vec<Ident> = Vec::new();
    let mut row_to_struct_entries: Vec<proc_macro2::TokenStream> = Vec::new();

    // The raw rust type of the primary key column
    let mut primary_key_rust_type: Option<proc_macro2::TokenStream> = None;
    // The TypeValue representation of the primary key's type (e.g. TypeValue::I32, TypeValue::F32, etc.)
    let mut primary_key_type_value: Option<proc_macro2::TokenStream> = None;
    // The primary key column's name (e.g. player_id)
    let mut primary_key_column_ident: Option<Ident> = None;
    // The statement that converts from a raw type to spacetimedb type, e.g. i32 to TypeValue::I32 or Hash to TypeValue::Bytes.
    let mut primary_conversion_from_raw_to_stdb_statement: Option<proc_macro2::TokenStream> = None;
    // The statement for declaring the primary type, e.g. my_value: i32
    let mut primary_key_column_def: Option<proc_macro2::TokenStream> = None;
    // The index of the primary key column, this is typically 0
    let mut primary_key_column_index: Option<u32> = None;

    let mut primary_key_set = false;

    // The identities for each non primary column
    let mut non_primary_column_idents: Vec<Ident> = Vec::new();
    // The types for each non-primary key column
    let mut non_primary_column_types: Vec<proc_macro2::TokenStream> = Vec::new();
    // The statement that converts from a spacetimedb type to a raw type, e.g. TypeValue::I32 to i32 or TypeValue::Bytes to Hash.
    let mut non_primary_index_lookup: Vec<u32> = Vec::new();
    let mut non_primary_column_defs: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut primary_filter_func: proc_macro2::TokenStream = quote!();
    let mut primary_update_func: proc_macro2::TokenStream = quote!();
    let mut primary_delete_func: proc_macro2::TokenStream = quote!();
    let mut non_primary_filter_func: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut insert_columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: u32 = 0;

    for field in &original_struct.fields {
        let col_name = &field.ident.clone().unwrap();

        // The simple name for the type, e.g. Hash
        let col_type: proc_macro2::TokenStream;
        // The fully qualified name for this type, e.g. spacetimedb_bindings::Hash
        let col_type_full: proc_macro2::TokenStream;
        // The TypeValue representation of this type
        let col_type_value: proc_macro2::TokenStream;
        let col_value_insert: proc_macro2::TokenStream;

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(ident) => {
                col_type = field.ty.clone().to_token_stream().to_string().parse().unwrap();
                col_type_full = col_type.clone();
                col_type_value = format!("spacetimedb_bindings::TypeValue::{}", ident).parse().unwrap();
            }
            None => match field.ty.clone().to_token_stream().to_string().as_str() {
                "Hash" => {
                    col_type = "Hash".parse().unwrap();
                    col_type_full = "spacetimedb_bindings::Hash".parse().unwrap();
                    col_type_value = format!("spacetimedb_bindings::TypeValue::Bytes").parse().unwrap();
                }
                custom_type => {
                    col_type = custom_type.parse().unwrap();
                    col_type_full = col_type.clone();
                    col_type_value = "spacetimedb_bindings::TypeValue::Tuple".parse().unwrap();
                }
            },
        }

        col_value_insert = format!("{}({})", col_type_value.clone(), format!("ins.{}", col_name))
            .parse()
            .unwrap();

        let mut is_primary = false;
        for attr in &field.attrs {
            if attr.path.to_token_stream().to_string().eq("primary_key") {
                if primary_key_set {
                    return proc_macro::TokenStream::from(quote! {
                        compile_error!("Only one primary key is allowed per table (for now)");
                    });
                }

                is_primary = true;
                primary_key_set = true;
            }
        }

        match is_primary {
            true => {
                primary_key_column_ident = Some(col_name.clone());
                primary_key_rust_type = Some(col_type.clone());
                primary_key_type_value = Some(col_type_value.clone());
                primary_key_column_def = Some(quote!(
                    #col_name: #col_type
                ));
                primary_key_column_index = Some(col_num);
                primary_conversion_from_raw_to_stdb_statement = Some(quote!(
                    let data = #col_type_value(data);
                ));
            }
            false => {
                non_primary_column_idents.push(col_name.clone());
                non_primary_column_types.push(col_type.clone());
                non_primary_column_defs.push(quote!(
                    #col_name: #col_type_full
                ));
                non_primary_index_lookup.push(col_num);
            }
        }

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(_) => {
                insert_columns.push(quote! {
                    #col_value_insert
                });
            }
            None => match field.ty.clone().to_token_stream().to_string().as_str() {
                "Hash" => {
                    if is_primary {
                        primary_conversion_from_raw_to_stdb_statement = Some(quote!(
                            let data = #col_type_value(data.to_vec());
                        ));
                    }

                    insert_columns.push(quote! {
                        spacetimedb_bindings::TypeValue::Bytes(ins.#col_name.to_vec())
                    });
                }
                _ => {
                    let struct_to_tuple =
                        format_ident!("__struct_to_tuple__{}", col_type.to_token_stream().to_string());
                    insert_columns.push(quote! {
                        #struct_to_tuple(ins.#col_name)
                    });
                }
            },
        }

        column_idents.push(format_ident!("{}", col_name));
        row_to_struct_entries.push(quote!(
            &row.elements[#col_num]
        ));

        col_num = col_num + 1;
    }

    match (
        primary_key_column_ident,
        primary_key_rust_type,
        primary_key_type_value,
        primary_key_column_def,
        primary_key_column_index,
        primary_conversion_from_raw_to_stdb_statement,
    ) {
        (
            Some(primary_key_column_ident),
            Some(primary_key_rust_type),
            Some(primary_key_type_value),
            Some(primary_key_column_def),
            Some(primary_key_column_index),
            Some(primary_conversion_from_raw_to_stdb_statement),
        ) => {
            let filter_func_ident = format_ident!("filter_{}_eq", primary_key_column_ident);
            let update_func_ident = format_ident!("update_{}_eq", primary_key_column_ident);
            let delete_func_ident = format_ident!("delete_{}_eq", primary_key_column_ident);
            let primary_key_tuple_type_str: String = format!("{}", primary_key_type_value);
            let primary_key_column_index_usize = primary_key_column_index as usize;
            let comparison_block = tuple_field_comparison_block(
                original_struct.ident.clone().to_token_stream().to_string().as_str(),
                primary_key_rust_type.to_string().as_str(),
                primary_key_column_ident.clone(),
                true,
            );
            primary_filter_func = quote! {
                #[allow(unused_variables)]
                #[allow(non_snake_case)]
                pub fn #filter_func_ident(#primary_key_column_def) -> Option<#original_struct_ident> {
                    let table_iter = #original_struct_ident::iter();
                    if let Some(table_iter) = table_iter {
                        for row in table_iter {
                            let column_data = row.elements[#primary_key_column_index_usize].clone();
                            #comparison_block
                        }
                    }

                    return None;
                }
            };

            primary_update_func = quote! {
                #[allow(unused_variables)]
                #[allow(non_snake_case)]
                pub fn #update_func_ident(#primary_key_column_def, new_value: #original_struct_ident) -> bool {
                    #original_struct_ident::#delete_func_ident(#primary_key_column_ident);
                    #original_struct_ident::insert(new_value);

                    // For now this is always successful
                    true
                }
            };

            primary_delete_func = quote! {
                #[allow(unused_variables)]
                #[allow(non_snake_case)]
                pub fn #delete_func_ident(#primary_key_column_def) -> bool {
                    let data = #primary_key_column_ident;
                    #primary_conversion_from_raw_to_stdb_statement
                    let equatable = spacetimedb_bindings::EqTypeValue::try_from(data);
                    match equatable {
                        Ok(value) => {
                            let result = spacetimedb_bindings::delete_eq(1, #primary_key_column_index, value);
                            match result {
                                None => {
                                    println!("Internal server error on equatable type: {}", #primary_key_tuple_type_str);
                                    false
                                },
                                Some(count) => {
                                    count > 0
                                }
                            }
                        }, Err(E) => {
                            // We cannot complete this call because this type is not equatable
                            println!("This type is not equatable: {} Error:{}", #primary_key_tuple_type_str, E);
                            false
                        }
                    }
                }
            };
        }
        _ => {
            // We allow tables with no primary key for now
        }
    }

    for (x, non_primary_column_ident) in non_primary_column_idents.iter().enumerate() {
        let filter_func_ident: proc_macro2::TokenStream =
            format!("filter_{}_eq", non_primary_column_ident).parse().unwrap();
        let column_type = non_primary_column_types[x].clone();
        let column_def = non_primary_column_defs[x].clone();
        let row_index = non_primary_index_lookup[x] as usize;

        let comparison_block = tuple_field_comparison_block(
            original_struct_ident.clone().to_token_stream().to_string().as_str(),
            column_type.to_string().as_str(),
            non_primary_column_ident.clone(),
            false,
        );

        if let None = rust_to_spacetimedb_ident(column_type.to_string().as_str()) {
            match column_type.to_string().as_str() {
                "Hash" => {
                    // This is fine
                }
                _ => {
                    // Just skip this, its not supported.
                    continue;
                }
            }
        }

        non_primary_filter_func.push(quote!(
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            pub fn #filter_func_ident(#column_def) -> Vec<#original_struct_ident> {
                let mut result = Vec::<#original_struct_ident>::new();
                let table_iter = #original_struct_ident::iter();
                if let Some(table_iter) = table_iter {
                    for row in table_iter {
                        let column_data = row.elements[#row_index].clone();
                        #comparison_block
                    }
                }

                return result;
            }
        ));
    }

    let db_insert = quote! {
        #[allow(unused_variables)]
        pub fn insert(ins: #original_struct_ident) {
            unsafe {
                spacetimedb_bindings::insert(#table_id, spacetimedb_bindings::TupleValue {
                    elements: vec![
                        #(#insert_columns),*
                    ]
                });
            }
        }
    };

    let db_delete = quote! {
        #[allow(unused_variables)]
        pub fn delete(f: fn (#original_struct_ident) -> bool) -> usize {
            0
        }
    };

    let db_update = quote! {
        #[allow(unused_variables)]
        pub fn update(value: #original_struct_ident) -> bool {
            // delete on primary key
            // insert on primary key
            false
        }
    };

    let db_iter = quote! {
        #[allow(unused_variables)]
        pub fn iter() -> Option<spacetimedb_bindings::TableIter> {
            spacetimedb_bindings::__iter__(#table_id)
        }
    };

    let tuple_to_struct_func = autogen_module_tuple_to_struct(original_struct.clone());
    let struct_to_tuple_func = autogen_module_struct_to_tuple(original_struct.clone());
    let csharp_output = spacetimedb_csharp_tuple(original_struct.clone(), Some(table_id));

    let schema_func = autogen_module_struct_to_schema(original_struct.clone());
    let create_table_func_name = format_ident!("__create_table__{}", original_struct_ident);
    let get_schema_func_name = format_ident!("__get_struct_schema__{}", original_struct_ident);
    let create_table_func = quote! {
        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #create_table_func_name(arg_ptr: usize, arg_size: usize) {
            let def = #get_schema_func_name();
            if let spacetimedb_bindings::TypeDef::Tuple(tuple_def) = def {
                spacetimedb_bindings::create_table(#table_id, tuple_def);
            } else {
                // The type is not a tuple for some reason, table not created.
                std::panic!("This type is not a tuple: {{#original_struct_ident}}");
            }
        }
    };

    // Output all macro data
    proc_macro::TokenStream::from(quote! {
        #[derive(spacetimedb_bindgen::PrimaryKey, spacetimedb_bindgen::Index)]
        #[derive(serde::Serialize, serde::Deserialize)]
        #original_struct
        impl #original_struct_ident {
            #db_insert
            #db_delete
            #db_update
            #primary_filter_func
            #primary_update_func
            #primary_delete_func

            #db_iter
            #(#non_primary_filter_func)*
        }

        #schema_func
        #create_table_func
        #tuple_to_struct_func
        #struct_to_tuple_func
        #csharp_output
    })
}

fn spacetimedb_index(args: AttributeArgs, item: TokenStream) -> TokenStream {
    let mut index_name: String = "default_index".to_string();
    let mut index_fields = Vec::<u32>::new();
    let mut all_fields = Vec::<Ident>::new();
    let index_type: u8; // default index is a btree

    match args[0].to_token_stream().to_string().as_str() {
        "index(btree)" => {
            index_type = 0;
        }
        "index(hash)" => {
            index_type = 1;
        }
        _ => {
            let invalid_index = format!(
                "Invalid index type: {}\nValid options are: index(btree), index(hash)",
                args[0].to_token_stream().to_string()
            );
            return proc_macro::TokenStream::from(quote! {
                compile_error!(#invalid_index);
            });
        }
    }

    let original_struct = parse_macro_input!(item as ItemStruct);
    let table_id_field_name = format_ident!("__table_id__{}", original_struct.ident);
    for field in original_struct.clone().fields {
        all_fields.push(field.ident.unwrap());
    }

    for x in 1..args.len() {
        let arg = &args[x];
        let arg_str = arg.to_token_stream().to_string();
        let name_prefix = "name = ";
        if arg_str.starts_with(name_prefix) {
            index_name = arg_str
                .chars()
                .skip(name_prefix.len() + 1)
                .take(arg_str.len() - name_prefix.len() - 2)
                .collect();
        } else {
            let field_index = all_fields
                .iter()
                .position(|a| a.to_token_stream().to_string() == arg_str);
            match field_index {
                Some(field_index) => {
                    index_fields.push(field_index as u32);
                }
                None => {
                    let invalid_index = format!("Invalid field for index: {}", arg_str);
                    return proc_macro::TokenStream::from(quote! {
                        compile_error!(#invalid_index);
                    });
                }
            }
        }
    }

    let original_struct_name = &original_struct.ident;
    let function_name: Ident = format_ident!("__create_index__{}", format_ident!("{}", index_name.as_str()));

    let output = quote! {
        #original_struct

        impl #original_struct_name {
            #[allow(non_snake_case)]
            fn #function_name(arg_ptr: u32, arg_size: u32) {
                unsafe {
                    spacetimedb_bindings::create_index(#table_id_field_name, #index_type, vec!(#(#index_fields),*));
                }
            }
        }
    };

    proc_macro::TokenStream::from(output)
}

fn spacetimedb_tuple(_: AttributeArgs, item: TokenStream) -> TokenStream {
    let original_struct = parse_macro_input!(item as ItemStruct);
    let original_struct_ident = original_struct.clone().ident;

    match original_struct.clone().fields {
        Named(_) => {}
        Unnamed(_) => {
            let str = format!("spacetimedb tables and types must have named fields.");
            return TokenStream::from(quote! {
                compile_error!(#str);
            });
        }
        Unit => {
            let str = format!("Unit structure not supported.");
            return TokenStream::from(quote! {
                compile_error!(#str);
            });
        }
    }

    let return_schema = autogen_module_struct_to_schema(original_struct.clone());
    let csharp_output = spacetimedb_csharp_tuple(original_struct.clone(), None);
    let tuple_to_struct_func = autogen_module_tuple_to_struct(original_struct.clone());
    let struct_to_tuple_func = autogen_module_struct_to_tuple(original_struct.clone());

    let get_schema_func_name = format_ident!("__get_struct_schema__{}", original_struct_ident);
    let create_tuple_func_name = format_ident!("__create_type__{}", original_struct_ident);
    let create_tuple_func = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #create_tuple_func_name(arg_ptr: usize, arg_size: usize) {
           unsafe {
                let ptr = arg_ptr as *mut u8;
                let def = #get_schema_func_name();
                let mut bytes = Vec::from_raw_parts(ptr, 0, arg_size);
                def.encode(&mut bytes);
            }
        }
    };

    return TokenStream::from(quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        #original_struct
        #return_schema
        #create_tuple_func
        #csharp_output
        #tuple_to_struct_func
        #struct_to_tuple_func
    });
}

fn spacetimedb_migrate(_: AttributeArgs, item: TokenStream) -> TokenStream {
    let original_func = parse_macro_input!(item as ItemFn);
    let func_name = &original_func.sig.ident;

    proc_macro::TokenStream::from(quote! {
        #[allow(non_snake_case)]
        pub extern "C" fn __migrate__(arg_ptr: u32, arg_size: u32) {
            #func_name();
        }
        #original_func
    })
}

fn spacetimedb_connect_disconnect(args: AttributeArgs, item: TokenStream, connect: bool) -> TokenStream {
    if *(&args.len()) > 1 {
        let str = format!("Unexpected macro argument: {}", args[1].to_token_stream().to_string());
        return proc_macro::TokenStream::from(quote! {
            compile_error!(#str);
        });
    }

    let original_function = parse_macro_input!(item as ItemFn);
    let func_name = &original_function.sig.ident;
    let connect_disconnect_func_name = if connect {
        "__identity_connected__"
    } else {
        "__identity_disconnected__"
    };
    let connect_disconnect_ident = format_ident!("{}", connect_disconnect_func_name);

    let mut arg_num: usize = 0;
    for function_argument in original_function.sig.inputs.iter() {
        if arg_num > 0 {
            return proc_macro::TokenStream::from(quote! {
                compile_error!("Client connected/disconnected can only have one argument (u64)");
            });
        }

        match function_argument {
            FnArg::Receiver(_) => {
                return proc_macro::TokenStream::from(quote! {
                    compile_error!("Receiver types in reducer parameters not supported!");
                });
            }
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_token = arg_type.to_token_stream();
                let arg_string = arg_token.to_string();
                let arg_string = arg_string.as_str();

                if !arg_string.eq("u64") {
                    return proc_macro::TokenStream::from(quote! {
                        compile_error!("Client connected/disconnected can only have one argument (u64)");
                    });
                }
            }
        }

        arg_num = arg_num + 1;
    }

    let generated_function = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #connect_disconnect_ident(arg_ptr: usize, arg_size: usize) {
            let arg_ptr = arg_ptr as *mut u8;
            let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size, arg_size) };
            let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let args = arg_json.as_array().unwrap();

            // Deserialize the json argument list
            let actor_id : u64 = serde_json::from_value(args[0].clone()).unwrap();

            // Invoke the function with the deserialized args
            #func_name(actor_id);
        }
    };

    proc_macro::TokenStream::from(quote! {
        #generated_function
        #original_function
    })
}

// This derive is actually a no-op, we need the helper attribute for spacetimedb
#[proc_macro_derive(PrimaryKey, attributes(primary_key))]
pub fn derive_primary_key(_: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_derive(Index, attributes(index))]
pub fn derive_index(_item: TokenStream) -> TokenStream {
    TokenStream::new()
}

pub(crate) fn rust_to_spacetimedb_ident(input_type: &str) -> Option<Ident> {
    return match input_type {
        // These are typically prefixed with spacetimedb_bindings::TypeDef::
        "bool" => Some(format_ident!("Bool")),
        "i8" => Some(format_ident!("I8")),
        "u8" => Some(format_ident!("U8")),
        "i16" => Some(format_ident!("I16")),
        "u16" => Some(format_ident!("U16")),
        "i32" => Some(format_ident!("I32")),
        "u32" => Some(format_ident!("U32")),
        "i64" => Some(format_ident!("I64")),
        "u64" => Some(format_ident!("U64")),
        "i128" => Some(format_ident!("I128")),
        "u128" => Some(format_ident!("U128")),
        "String" => Some(format_ident!("String")),
        "&str" => Some(format_ident!("String")),
        "f32" => Some(format_ident!("F32")),
        "f64" => Some(format_ident!("F64")),
        _ => None,
    };
}

fn tuple_field_comparison_block(
    tuple_type_str: &str,
    filter_field_type_str: &str,
    filter_field_name: Ident,
    is_primary: bool,
) -> proc_macro2::TokenStream {
    let stdb_type_value: proc_macro2::TokenStream;
    let comparison_and_result_statement: proc_macro2::TokenStream;
    let result_statement: proc_macro2::TokenStream;
    let tuple_to_struct_func: proc_macro2::TokenStream =
        format!("__tuple_to_struct__{}", tuple_type_str).parse().unwrap();
    let err_string = format!(
        "Internal stdb error: Can't convert from tuple to struct (wrong version?) {}",
        tuple_type_str
    );

    if is_primary {
        result_statement = quote! {
            let tuple = #tuple_to_struct_func(row);
            if let None = tuple {
                println!(#err_string);
                return None;
            }
            return Some(tuple.unwrap());
        }
    } else {
        result_statement = quote! {
            let tuple = #tuple_to_struct_func(row);
            if let None = tuple {
                println!(#err_string);
                continue;
            }
            result.push(tuple.unwrap());
        }
    }

    match rust_to_spacetimedb_ident(filter_field_type_str) {
        Some(ident) => {
            stdb_type_value = format!("spacetimedb_bindings::TypeValue::{}", ident).parse().unwrap();
            comparison_and_result_statement = quote! {
                if entry_data == #filter_field_name {
                    #result_statement
                }
            };
        }
        None => {
            match filter_field_type_str {
                "Hash" => {
                    stdb_type_value = format!("spacetimedb_bindings::TypeValue::Bytes").parse().unwrap();
                    comparison_and_result_statement = quote! {
                        let entry_data = spacetimedb_bindings::hash::Hash::from_slice(&entry_data[0..32]);
                        if #filter_field_name.eq(&entry_data) {
                            #result_statement
                        }
                    };
                    // Compare hash
                }
                custom_type => {
                    let error_str = format!("Cannot filter on type: {}", custom_type);
                    return quote! {
                        compile_error!(#error_str);
                    };
                }
            }
        }
    }

    return quote! {
        if let #stdb_type_value(entry_data) = column_data.clone() {
            #comparison_and_result_statement
        }
    };
}
