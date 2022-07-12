#![crate_type = "proc-macro"]

extern crate core;
extern crate proc_macro;

use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote, ToTokens};
use regex::Regex;
use std::fmt::Write;
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
        let str = format!("Unexpected macro argument: {}", args[1].to_token_stream().to_string());
        return proc_macro::TokenStream::from(quote! {
            compile_error!(#str);
        });
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
                        let error_str = format!("Parameter 1 of reducer {} must be of type \'Hash\'.", func_name.to_string());
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
                        let error_str = format!("Parameter 2 of reducer {} must be of type \'u64\'.", func_name.to_string());
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

            println!("Parsing sender");
            let sender = *spacetimedb_bindings::hash::Hash::from_slice(&bytes[0..32]);

            let mut buf = [0; 8];
            buf.copy_from_slice(&bytes[32..HEADER_SIZE]);
            let timestamp = u64::from_le_bytes(buf);

            let arg_json: serde_json::Value = serde_json::from_slice(&bytes[HEADER_SIZE..]).unwrap();

            println!("unwrapping args");
            let args = arg_json.as_array().unwrap();

            println!("deserialize arguments");
            // Deserialize the json argument list
            #(#parse_json_to_args);*

            println!("invoke function call");
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
    let mut table_funcs: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut row_to_struct_entries: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut primary_let_data_statement: Option<proc_macro2::TokenStream> = None;
    let mut primary_key_column_ident: Option<Ident> = None;
    let mut primary_key_column_def: Option<proc_macro2::TokenStream> = None;
    let mut primary_key_column_index: Option<usize> = None;
    let mut primary_key_type_str: Option<syn::Type> = None;
    let mut primary_key_set = false;

    let mut non_primary_conversion_step: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut non_primary_index_lookup: Vec<usize> = Vec::new();
    let mut non_primary_columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut non_primary_columns_idents: Vec<Ident> = Vec::new();
    let mut non_primary_columns_eq_op: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut non_primary_let_data_statements: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut insert_columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: usize = 0;
    let tuple_to_struct_func = format_ident!("__tuple_to_struct__{}", original_struct_ident.clone());

    for field in &original_struct.fields {
        let col_name = &field.ident.clone().unwrap();
        let mut type_value_case = field.ty.to_token_stream().to_string();
        type_value_case[0..1].make_ascii_uppercase();
        let col_value_type: proc_macro2::TokenStream = format!("spacetimedb_bindings::TypeValue::{}", type_value_case)
            .parse()
            .unwrap();
        let col_value_insert: proc_macro2::TokenStream = format!("{}({})", col_value_type, format!("ins.{}", col_name))
            .parse()
            .unwrap();

        let col_type = field.clone().ty;
        let mut is_primary = false;
        for attr in &field.attrs {
            if attr.path.to_token_stream().to_string().eq("primary_key") {
                if primary_key_set {
                    return proc_macro::TokenStream::from(quote! {
                        compile_error!("Only one primary key is allowed per table (for now)");
                    });
                }

                primary_key_column_ident = Some(col_name.clone());
                primary_key_column_def = Some(quote!(
                    #col_name: #col_type
                ));
                primary_key_column_index = Some(col_num);
                primary_key_set = true;
                primary_let_data_statement = Some(quote!(
                    if let #col_value_type(data) = data
                ));
                primary_key_type_str = Some(field.clone().ty);
                is_primary = true;
            }
        }

        if let Some(spacetime_type) = rust_to_spacetimedb_ident(field.ty.clone()) {
            let spacetime_type_str = spacetime_type.to_string();
            match spacetime_type_str.as_str() {
                "Hash" => {
                    // if !is_primary {
                    //     non_primary_columns_idents.push(col_name.clone());
                    //     non_primary_columns.push(quote!(
                    //         #col_name: spacetimedb_bindings::hash::Hash
                    //     ));
                    //     non_primary_let_data_statements.push(quote!(
                    //         if let spacetimedb_bindings::TypeValue::Bytes(data) = data
                    //     ));
                    //     non_primary_columns_eq_op.push(quote!{
                    //         #col_name.to_vec().eq(&data)
                    //     });
                    //     non_primary_index_lookup.push(col_num);
                    //     non_primary_conversion_step.push(quote! {
                    //         // No conversion needed
                    //     });
                    // }

                    insert_columns.push(quote! {
                        spacetimedb_bindings::TypeValue::Bytes(ins.#col_name.to_vec())
                    });
                }
                _ => {
                    if !is_primary {
                        non_primary_columns_idents.push(col_name.clone());
                        non_primary_columns.push(quote!(
                            #col_name: #col_type
                        ));
                        non_primary_let_data_statements.push(quote!(
                            if let #col_value_type(data) = data
                        ));
                        non_primary_columns_eq_op.push(quote! {
                            #col_name == data
                        });
                        non_primary_index_lookup.push(col_num);
                        non_primary_conversion_step.push(quote! {
                            // No conversion needed
                        });
                    }

                    insert_columns.push(quote! {
                        #col_value_insert
                    });
                }
            }
        } else {

            // if !is_primary {
            //
            //     non_primary_columns_idents.push(col_name.clone());
            //     non_primary_columns.push(quote!(
            //         #col_name: #col_type
            //     ));
            //     non_primary_let_data_statements.push(quote!(
            //         if let spacetimedb_bindings::TypeValue::Tuple(data) = data
            //     ));
            //
            //     non_primary_columns_eq_op.push(quote!{
            //         #col_name.eq(&data)
            //     });
            //     non_primary_index_lookup.push(col_num);
            //     let tuple_to_struct = format_ident!("__tuple_to_struct__{}", col_type.to_token_stream().to_string());
            //     non_primary_conversion_step.push(quote! {
            //         let data = #tuple_to_struct(data);
            //     });
            // }

            let struct_to_tuple = format_ident!("__struct_to_tuple__{}", col_type.to_token_stream().to_string());
            insert_columns.push(quote! {
                #struct_to_tuple(ins.#col_name)
            });
        }

        column_idents.push(format_ident!("{}", col_name));
        row_to_struct_entries.push(quote!(
            &row.elements[#col_num]
        ));

        col_num = col_num + 1;
    }

    match (
        primary_key_column_def,
        primary_key_column_ident,
        primary_let_data_statement,
        primary_key_column_index,
        primary_key_type_str,
    ) {
        (
            Some(primary_key_column_def),
            Some(primary_key_column_ident),
            Some(primary_let_data_statement),
            Some(primary_key_column_index),
            Some(primary_key_type_str),
        ) => {
            let filter_func_ident = format_ident!("filter_{}_eq", primary_key_column_ident);
            let update_func_ident = format_ident!("update_{}_eq", primary_key_column_ident);
            let delete_func_ident = format_ident!("delete_{}_eq", primary_key_column_ident);
            let tuple_type = rust_to_spacetimedb_ident(primary_key_type_str).unwrap();
            let full_tuple_type_str = format!("spacetimedb_bindings::TypeValue::{}", tuple_type.to_token_stream().to_string());
            table_funcs.push(quote!(
                #[allow(unused_variables)]
                #[allow(non_snake_case)]
                pub fn #filter_func_ident(#primary_key_column_def) -> Option<#original_struct_ident> {
                    let table_iter = #original_struct_ident::iter();
                    if let Some(table_iter) = table_iter {
                        for row in table_iter {
                            let data = row.elements[#primary_key_column_index].clone();
                            #primary_let_data_statement {
                                if #primary_key_column_ident.eq(&data) {
                                    let value = #tuple_to_struct_func(row);
                                    if let Some(value) = value {
                                        return Some(value);
                                    }
                                }
                            }
                        }
                    }

                    return None;
                }

                #[allow(unused_variables)]
                #[allow(non_snake_case)]
                pub fn #update_func_ident(#primary_key_column_def, new_value: #original_struct_ident) -> bool {
                    #original_struct_ident::#delete_func_ident(#primary_key_column_ident);
                    #original_struct_ident::insert(new_value);

                    // For now this is always successful
                    true
                }

                #[allow(unused_variables)]
                #[allow(non_snake_case)]
                pub fn #delete_func_ident(#primary_key_column_def) -> bool {
                    let data = spacetimedb_bindings::TypeValue::#tuple_type(#primary_key_column_ident);
                    let equatable = spacetimedb_bindings::EqTypeValue::try_from(data);
                    match equatable {
                        Ok(value) => {
                            let result = spacetimedb_bindings::delete_eq(1, 0, value);
                            match result {
                                None => {
                                    println!("Internal server error on equatable type: {}", #full_tuple_type_str);
                                    false
                                },
                                Some(count) => {
                                    count > 0
                                }
                            }
                        }, Err(E) => {
                            // We cannot complete this call because this type is not equatable
                            println!("This type is not equatable: {} Error:{}", #full_tuple_type_str, E);
                            false
                        }
                    }
                }
            ));
        }
        _ => {
            // We allow tables with no primary key for now
        }
    }

    for (x, non_primary_column_ident) in non_primary_columns_idents.iter().enumerate() {
        let filter_func_ident = format_ident!("filter_{}_eq", non_primary_column_ident);
        let delete_func_ident = format_ident!("delete_{}_eq", non_primary_column_ident);
        let column_def = &non_primary_columns[x];
        let let_statement = non_primary_let_data_statements[x].clone();
        let eq_operation = non_primary_columns_eq_op[x].clone();
        let conversion = non_primary_conversion_step[x].clone();
        let row_index = non_primary_index_lookup[x];

        let filter_func = quote!(
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            pub fn #filter_func_ident(#column_def) -> Vec<#original_struct_ident> {
                let mut result = Vec::<#original_struct_ident>::new();
                let table_iter = #original_struct_ident::iter();
                if let Some(table_iter) = table_iter {
                    for row in table_iter {
                        let data = row.elements[#row_index].clone();
                        #conversion
                        #let_statement {
                            if #eq_operation {
                                let value = #tuple_to_struct_func(row);
                                if let Some(value) = value {
                                    result.push(value);
                                }
                            }
                        }
                    }
                }

                return result;
            }
        );
        let delete_func = quote!(
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            pub fn #delete_func_ident(#column_def) -> usize {
                0
            }
        );

        table_funcs.push(filter_func);
        table_funcs.push(delete_func);
    }

    let db_funcs = quote! {
        impl #original_struct_ident {
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

            #[allow(unused_variables)]
            pub fn delete(f: fn (#original_struct_ident) -> bool) -> usize {
                0
            }

            #[allow(unused_variables)]
            pub fn update(value: #original_struct_ident) -> bool {
                // delete on primary key
                // insert on primary key
                false
            }

            #[allow(unused_variables)]
            pub fn iter() -> Option<spacetimedb_bindings::TableIter> {
                spacetimedb_bindings::__iter__(#table_id)
            }

            #(#table_funcs)*
        }
    };

    let tuple_to_struct_func = tuple_value_to_struct_func_generator(original_struct.clone());
    let struct_to_tuple_func = struct_to_tuple_value_func_generator(original_struct.clone());
    let csharp_output = spacetimedb_csharp_tuple(original_struct.clone());

    let schema_func = struct_to_schema_func_generator(original_struct.clone());
    let create_table_func_name = format_ident!("__create_table__{}", original_struct_ident);
    let get_schema_func_name = format_ident!("__get_struct_schema__{}", original_struct_ident);
    let create_table_func = quote! {
        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #create_table_func_name(arg_ptr: usize, arg_size: usize) {
            unsafe {
                let ptr = arg_ptr as *mut u8;
                let def = #get_schema_func_name();
                let mut bytes = Vec::from_raw_parts(ptr, 0, arg_size);
                def.encode(&mut bytes);

                if let spacetimedb_bindings::TypeDef::Tuple(tuple_def) = def {
                    spacetimedb_bindings::create_table(#table_id, tuple_def);
                } else {
                    // The type is not a tuple for some reason, table not created.
                    std::panic!("This type is not a tuple: {{#original_struct_ident}}");
                }
            }
        }
    };

    // Output all macro data
    proc_macro::TokenStream::from(quote! {
        #[derive(serde::Serialize, serde::Deserialize, spacetimedb_bindgen::PrimaryKey, spacetimedb_bindgen::Index)]
        #original_struct
        #db_funcs
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

    let return_schema = struct_to_schema_func_generator(original_struct.clone());
    let csharp_output = spacetimedb_csharp_tuple(original_struct.clone());
    let tuple_to_struct_func = tuple_value_to_struct_func_generator(original_struct.clone());
    let struct_to_tuple_func = struct_to_tuple_value_func_generator(original_struct.clone());

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
        "__client_connected__"
    } else {
        "__client_disconnected__"
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

fn spacetimedb_csharp_tuple(original_struct: ItemStruct) -> proc_macro2::TokenStream {
    let use_namespace = true;
    let namespace = "SpacetimeDB";
    let namespace_tab = if use_namespace { "\t" } else { "" };

    let original_struct_ident = &original_struct.clone().ident;
    let struct_name_pascal_case = original_struct_ident.to_string().to_case(Case::Pascal);

    let mut col_num: usize = 0;
    let mut output_contents: String = String::new();

    write!(
        output_contents, "{}{}",
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE\n",
        "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.\n\n"
    ).unwrap();
    if use_namespace {
        write!(output_contents, "namespace {}\n{{\n", namespace).unwrap();
    }

    write!(output_contents, "{}public partial class {}\n", namespace_tab, struct_name_pascal_case).unwrap();
    write!(output_contents, "{}{{\n", namespace_tab).unwrap();

    for field in &original_struct.fields {
        let col_name = &field.ident.clone().unwrap();

        write!(output_contents, "\t{}[Newtonsoft.Json.JsonProperty(\"{}\")]\n", namespace_tab, col_name).unwrap();
        write!(
            output_contents,
            "\t{}public {} {};\n",
            namespace_tab,
            rust_type_to_csharp(field.ty.to_token_stream().to_string().as_str()),
            col_name.to_token_stream().to_string().to_case(Case::Camel),
        )
            .unwrap();
        col_num = col_num + 1;
    }

    // Insert the GetTypeDef func
    write!(output_contents, "{}", csharp_get_type_def_for_struct(original_struct.clone())).unwrap();

    // class close brace
    write!(output_contents, "{}}}\n", namespace_tab).unwrap();
    // namespace close brace
    write!(output_contents, "}}\n").unwrap();

    // Write the cs output
    if !std::path::Path::new("cs-src").is_dir() {
        std::fs::create_dir(std::path::Path::new("cs-src")).unwrap();
    }
    let path = format!("cs-src/{}.cs", struct_name_pascal_case);
    std::fs::write(path, output_contents).unwrap();

    // Output all macro data
    quote! {
        // C# generated
    }
}

fn spacetimedb_csharp_reducer(original_function: ItemFn) -> TokenStream {
    let func_name = &original_function.sig.ident;
    let reducer_pascal_name = func_name.to_token_stream().to_string().to_case(Case::Pascal);
    let use_namespace = true;
    let namespace = "SpacetimeDB";
    let namespace_tab = if use_namespace { "\t" } else { "" };
    let func_name_pascal_case = func_name.to_string().to_case(Case::Pascal);

    let mut output_contents: String = String::new();
    let mut func_arguments: String = String::new();
    let mut arg_names: String = String::new();

    write!(output_contents, "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE\n").unwrap();
    write!(output_contents, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.\n\n").unwrap();

    if use_namespace {
        write!(output_contents, "namespace {} \n{{\n", namespace).unwrap();
    }

    write!(output_contents, "{}public static partial class Reducer \n{}{{\n", namespace_tab, namespace_tab).unwrap();

    let mut arg_num: usize = 0;
    let mut inserted_args: usize = 0;
    for function_argument in original_function.sig.inputs.iter() {
        match function_argument {
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_type = arg_type.to_token_stream().to_string();
                let arg_name = &typed.pat.to_token_stream().to_string().to_case(Case::Camel);

                let csharp_type = rust_type_to_csharp(arg_type.as_str());

                // Skip any arguments that are supplied by spacetimedb
                if arg_num == 0 && typed.ty.to_token_stream().to_string() == "Hash"
                    || arg_num == 1 && typed.ty.to_token_stream().to_string() == "u64" {
                    arg_num = arg_num + 1;
                    continue;
                }

                if inserted_args > 0 {
                    write!(func_arguments, ", ").unwrap();
                    write!(arg_names, ", ").unwrap();
                }

                write!(func_arguments, "{} {}", csharp_type, arg_name).unwrap();
                write!(arg_names, "{}", arg_name.clone()).unwrap();
                inserted_args += 1;
            }
            _ => {}
        }

        arg_num = arg_num + 1;
    }

    write!(output_contents, "{}\tpublic static void {}({})\n", namespace_tab, func_name_pascal_case, func_arguments).unwrap();
    write!(output_contents, "{}\t{{\n", namespace_tab).unwrap();

    //            StdbNetworkManager.instance.InternalCallReducer(new StdbNetworkManager.Message
    // 			{
    // 				fn = "create_new_player",
    // 				args = new object[] { playerId, position },
    // 			});

    // Tell the network manager to send this message
    // UPGRADE FOR LATER
    // write!(output_contents, "{}\t\tStdbNetworkManager.instance.InternalCallReducer(new Websocket.FunctionCall\n", namespace_tab).unwrap();
    // write!(output_contents, "{}\t\t{{\n", namespace_tab).unwrap();
    // write!(output_contents, "{}\t\t\tReducer = \"{}\",\n", namespace_tab, func_name).unwrap();
    // write!(output_contents, "{}\t\t\tArgBytes = Google.Protobuf.ByteString.CopyFrom(Newtonsoft.Json.JsonConvert.SerializeObject(new object[] {{ {} }}), System.Text.Encoding.UTF8),\n", namespace_tab, arg_names).unwrap();
    // write!(output_contents, "{}\t\t}});\n", namespace_tab).unwrap();

    // TEMPORARY OLD FUNCTIONALITY
    write!(output_contents, "{}\t\tStdbNetworkManager.instance.InternalCallReducer(new StdbNetworkManager.Message\n", namespace_tab).unwrap();
    write!(output_contents, "{}\t\t{{\n", namespace_tab).unwrap();
    write!(output_contents, "{}\t\t\tfn = \"{}\",\n", namespace_tab, func_name).unwrap();
    write!(output_contents, "{}\t\t\targs = new object[] {{ {} }},\n", namespace_tab, arg_names).unwrap();
    write!(output_contents, "{}\t\t}});\n", namespace_tab).unwrap();

    // Closing brace for reducer
    write!(output_contents, "{}\t}}\n", namespace_tab).unwrap();
    // Closing brace for class
    write!(output_contents, "{}}}\n", namespace_tab).unwrap();

    if use_namespace {
        write!(output_contents, "}}\n").unwrap();
    }

    // Write the csharp output
    if !std::path::Path::new("cs-src").is_dir() {
        std::fs::create_dir(std::path::Path::new("cs-src")).unwrap();
    }
    let path = format!("cs-src/{}Reducer.cs", reducer_pascal_name);
    std::fs::write(path, output_contents).unwrap();

    proc_macro::TokenStream::from(quote! {
        // Reducer C# generation
    })
}

/// This returns a function which will return the schema (TypeDef) for a struct. The signature
/// for this function is as follows:
/// fn __get_struct_schema__<struct_type_ident>() -> spacetimedb_bindings::TypeDef {
///   ...
/// }
fn struct_to_schema_func_generator(original_struct: ItemStruct) -> proc_macro2::TokenStream {
    let original_struct_ident = &original_struct.clone().ident;
    let mut fields: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: u8 = 0;

    for field in &original_struct.fields {
        let field_type = field.ty.clone().to_token_stream().to_string();
        let field_type = field_type.as_str();

        match rust_to_spacetimedb_ident(field.ty.clone()) {
            None => {
                let get_func = format_ident!("__get_struct_schema__{}", field_type);
                fields.push(quote! {
                    spacetimedb_bindings::ElementDef {
                        tag: #col_num,
                        element_type: #get_func(),
                    }
                });
            }
            Some(spacetimedb_type) => {
                match spacetimedb_type.to_string().as_str() {
                    "Hash" => {
                        fields.push(quote! {
                            spacetimedb_bindings::ElementDef {
                                tag: #col_num,
                                element_type: spacetimedb_bindings::TypeDef::Bytes,
                            }
                        });
                    }
                    "Vec" => {
                        fields.push(quote! {
                            spacetimedb_bindings::ElementDef {
                                tag: #col_num,
                                element_type: spacetimedb_bindings::TypeDef::Vec{
                                    element_type:
                                },
                            }
                        });
                    }
                    _ => {
                        fields.push(quote! {
                            spacetimedb_bindings::ElementDef {
                                tag: #col_num,
                                element_type: spacetimedb_bindings::TypeDef::#spacetimedb_type,
                            }
                        });
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

/// This returns a function which will return the schema (TypeDef) for a struct. The signature
/// for this function is as follows:
/// fn __get_struct_schema__<struct_type_ident>() -> spacetimedb_bindings::TypeDef {
///   ...
/// }
fn csharp_get_type_def_for_struct(original_struct: ItemStruct) -> String {
    let mut col_num: u8 = 0;
    let mut element_defs : String = String::new();

    for field in &original_struct.fields {
        let field_type = field.ty.clone().to_token_stream().to_string();

        match rust_to_spacetimedb_ident(field.ty.clone()) {
            None => {
                let csharp_type = field_type.to_case(Case::Pascal);
                write!(element_defs, "\t\t\t\tnew SpacetimeDB.ElementDef({}, SpacetimeDB.{}.GetTypeDef()),\n", col_num, csharp_type).unwrap();
            }
            Some(spacetimedb_type) => {
                match spacetimedb_type.to_string().as_str() {
                    "Hash" => {
                        write!(element_defs, "\t\t\t\tnew SpacetimeDB.ElementDef({}, SpacetimeDB.TypeDef.BuiltInType(SpacetimeDB.TypeDef.BuiltInType(SpacetimeDB.TypeDef.Def.Bytes))),\n", col_num).unwrap();
                    }
                    "Vec" => {
                        // This really sucks here, we have to process the vec generic type and see how that needs to be broken down.
                        panic!("Please no vecs in tuple defs for now!");
                        // match get_type_from_vec(field.ty.clone()) {
                        //     Some(t) => {
                        //         write!(element_defs, "\t\telement_type = SpacetimeDB.TypeDef.GetVec(SpacetimeDB.TypeDef.{}),\n", ).unwrap();
                        //     }, None => {
                        //         panic!("Internal error: This type is not a vec: {}", spacetimedb_type.to_string())
                        //     }
                        // }

                    }
                    _ => {
                        write!(element_defs, "\t\t\t\tnew SpacetimeDB.ElementDef({}, SpacetimeDB.TypeDef.BuiltInType(SpacetimeDB.TypeDef.Def.{})),\n", col_num, spacetimedb_type).unwrap();
                    }
                }
            }
        }

        col_num = col_num + 1;
    }

    let mut result : String = String::new();

    write!(result, "\t\tpublic static TypeDef GetTypeDef()\n").unwrap();
    write!(result, "\t\t{{\n").unwrap();
    write!(result, "\t\t\treturn TypeDef.Tuple(new ElementDef[]\n").unwrap();
    write!(result, "\t\t\t{{\n").unwrap();
    write!(result, "{}", element_defs).unwrap();
    write!(result, "\t\t\t}});\n").unwrap();
    write!(result, "\t\t}}\n").unwrap();
    return result;
}

/// Returns a generated function that will return a struct value from a TupleValue. The signature
/// for this function is as follows:
///
/// fn __tuple_to_struct__<struct_type_ident>(value: TupleValue) -> <struct_type_ident> {
///   ...
/// }
///
/// If the TupleValue's structure does not match the expected fields of the struct, we panic.
fn tuple_value_to_struct_func_generator(original_struct: ItemStruct) -> proc_macro2::TokenStream {
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

        // let my_vec : Vec<u8> = vec! [
        //   1, 2, 3
        // ];

        // Hash::from_slice(&bytes[read_count..read_count + 32]);
        // let field_1 =
        //     spacetimedb_bindings::hash::Hash::from_slice(&my_vec.as_slice());


        match rust_to_spacetimedb_ident(field.ty.clone()) {
            None => {
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
            Some(spacetimedb_type) => {
                let spacetimedb_type_str = spacetimedb_type.to_token_stream().to_string();
                match spacetimedb_type_str.as_str() {
                    "Hash" => {
                        match_paren2.push(quote! {
                            spacetimedb_bindings::TypeValue::Bytes(#tmp_name)
                        });
                        extra_assignments.push(quote! {
                           let #tmp_name = spacetimedb_bindings::hash::Hash::from_slice(#tmp_name.as_slice());
                        });
                    }
                    _ => {
                        match_paren2.push(quote! {
                            spacetimedb_bindings::TypeValue::#spacetimedb_type(#tmp_name)
                        });
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
fn struct_to_tuple_value_func_generator(original_struct: ItemStruct) -> proc_macro2::TokenStream {
    let original_struct_ident = &original_struct.clone().ident;
    let mut type_values: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: usize = 0;

    for field in &original_struct.fields {
        let field_ident = field.clone().ident.unwrap();
        let field_type_str = field.ty.clone().to_token_stream().to_string();
        match rust_to_spacetimedb_ident(field.ty.clone()) {
            Some(spacetimedb_type) => {
                match spacetimedb_type.to_string().as_str() {
                    "Hash" => {
                        type_values.push(quote! {
                            spacetimedb_bindings::TypeValue::Bytes(value.#field_ident.to_vec())
                        });
                    }
                    _ => {
                        type_values.push(quote! {
                            spacetimedb_bindings::TypeValue::#spacetimedb_type(value.#field_ident)
                        });
                    }
                }
            }
            _ => {
                let struct_to_tuple_value_func_name = format_ident!("__struct_to_tuple__{}", field_type_str);
                type_values.push(quote! {
                    #struct_to_tuple_value_func_name(value.#field_ident)
                });
            }
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

// This derive is actually a no-op, we need the helper attribute for spacetimedb
#[proc_macro_derive(PrimaryKey, attributes(primary_key))]
pub fn derive_primary_key(_: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_derive(Index, attributes(index))]
pub fn derive_index(_item: TokenStream) -> TokenStream {
    TokenStream::new()
}

fn rust_type_to_csharp(type_string: &str) -> &str {
    return match type_string {
        "bool" => "bool",
        "i8" => "sbyte",
        "u8" => "byte",
        "i16" => "short",
        "u16" => "ushort",
        "i32" => "int",
        "u32" => "uint",
        "i64" => "long",
        "u64" => "ulong",
        // "i128" => "int128", Not a supported type in csharp
        // "u128" => "uint128", Not a supported type in csharp
        "String" => "string",
        "&str" => "string",
        "f32" => "float",
        "f64" => "double",
        "Hash" => "byte[]",
        managed_type => {
            return managed_type;
        }
    };
}

fn rust_to_spacetimedb_ident(input_type: syn::Type) -> Option<Ident> {
    let type_string = input_type.to_token_stream().to_string();
    let type_string = type_string.as_str();
    if type_string.starts_with("Vec") {
        return Some(format_ident!("Vec"));
    }
    if type_string == "Hash" || type_string == "spacetimedb:bindings :: hash :: Hash" {
        return Some(format_ident!("Hash"));
    }

    return match type_string {
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
        _ => {
            None
        }
    };
}

fn get_type_from_vec(ty: syn::Type) -> Option<Ident> {
    return None;
}