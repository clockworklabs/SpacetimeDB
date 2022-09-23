#![crate_type = "proc-macro"]

mod csharp;
mod module;

extern crate core;
extern crate proc_macro;

use crate::csharp::{autogen_csharp_reducer, autogen_csharp_tuple};
use crate::module::{
    args_to_tuple_schema, autogen_module_struct_to_schema, autogen_module_struct_to_tuple,
    autogen_module_tuple_to_struct, parse_generic_arg,
};
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote, ToTokens};
use std::time::Duration;
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

    match attribute_str {
        "table" => spacetimedb_table(attribute_args, item),
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
    let descriptor_func_name = format_ident!("__describe_reducer__{}", &func_name);

    let mut parse_json_to_args = Vec::new();
    let mut function_call_arg_names = Vec::new();
    let mut arg_num: usize = 0;
    let mut json_arg_num: usize = 0;
    let function_arguments = &original_function.sig.inputs;

    let function_call_arg_types = args_to_tuple_schema(function_arguments.into_iter());

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
                    if arg_type_str != "spacetimedb_lib :: hash :: Hash" && arg_type_str != "Hash" {
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

                // Stash the function
                parse_json_to_args.push(quote! {
                    let #var_name : #arg_token = serde_json::from_value(args[#json_arg_num].clone()).unwrap();
                });

                function_call_arg_names.push(var_name);
                json_arg_num += 1;
            }
        }

        arg_num = arg_num + 1;
    }

    let unwrap_args = match arg_num > 2 {
        true => {
            quote! {
                let arg_json: serde_json::Value = serde_json::from_slice(
                    arguments.argument_bytes.as_slice()).
                expect(format!("Unable to parse arguments as JSON: {} bytes/arg_size: {}: {:?}",
                    arguments.argument_bytes.len(), arg_size, arguments.argument_bytes).as_str());
                let args = arg_json.as_array().expect("Unable to extract reducer arguments list");
            }
        }
        false => {
            quote! {}
        }
    };

    let generated_function = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #reducer_func_name(arg_ptr: usize, arg_size: usize) {
            let arguments = spacetimedb_lib::args::ReducerArguments::decode_mem(
                unsafe { arg_ptr as *mut u8 }, arg_size).expect("Unable to decode module arguments");

            // Unwrap extra arguments, conditional on whether or not there are extra args.
            #unwrap_args

            // Deserialize the json argument list
            #(#parse_json_to_args);*

            // Invoke the function with the deserialized args
            #func_name(arguments.identity, arguments.timestamp, #(#function_call_arg_names),*);
        }
    };

    let generated_describe_function = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        // u64 is offset << 32 | length
        pub extern "C" fn #descriptor_func_name() -> u64 {
            let tupledef = spacetimedb_lib::TupleDef { elements: vec![
                    #(#function_call_arg_types),*
                ] };
            let mut bytes = vec![];
            tupledef.encode(&mut bytes);
            let offset = bytes.as_ptr() as u64;
            let length = bytes.len() as u64;
            std::mem::forget(bytes);
            return offset << 32 | length;
        }
    };

    autogen_csharp_reducer(original_function.clone());

    proc_macro::TokenStream::from(quote! {
        #generated_function
        #generated_describe_function
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
            // Deserialize the arguments
            let arguments = spacetimedb_lib::args::RepeatingReducerArguments::decode_mem(
                unsafe { arg_ptr as *mut u8 }, arg_size).expect("Unable to decode module arguments");

            // Invoke the function with the deserialized args
            #func_name(arguments.timestamp, arguments.delta_time);

            return #duration_as_millis;
        }
    };

    proc_macro::TokenStream::from(quote! {
        #generated_function
        #original_function
    })
}

// TODO: We actually need to add a constraint that requires this column to be unique!
struct UniqueColumn {
    // The raw rust type as a token stream
    rust_type: proc_macro2::TokenStream,
    // The TypeValue representation of the unique column's type (e.g. TypeValue::I32, TypeValue::F32, etc.)
    type_value: proc_macro2::TokenStream,
    // The column's name as an identity (e.g. player_id)
    column_ident: Ident,
    // The statement that converts from a raw type to spacetimedb type, e.g. i32 to TypeValue::I32 or Hash to TypeValue::Bytes.
    conversion_from_raw_to_stdb_statement: proc_macro2::TokenStream,
    // The statement for declaring the unique column, e.g. my_value: i32
    column_def: proc_macro2::TokenStream,
    // The index of the unique column
    column_index: u32,
}

fn spacetimedb_table(args: AttributeArgs, item: TokenStream) -> TokenStream {
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

    let mut unique_columns = Vec::<UniqueColumn>::new();

    // The identities for each non primary column
    let mut non_primary_column_idents: Vec<Ident> = Vec::new();
    // The types for each non-primary key column
    let mut non_primary_column_types: Vec<proc_macro2::TokenStream> = Vec::new();
    // The statement that converts from a spacetimedb type to a raw type, e.g. TypeValue::I32 to i32 or TypeValue::Bytes to Hash.
    let mut non_primary_index_lookup: Vec<u32> = Vec::new();
    let mut non_primary_column_defs: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut unique_filter_funcs: Vec<proc_macro2::TokenStream> = Vec::<proc_macro2::TokenStream>::new();
    let mut unique_update_funcs: Vec<proc_macro2::TokenStream> = Vec::<proc_macro2::TokenStream>::new();
    let mut unique_delete_funcs: Vec<proc_macro2::TokenStream> = Vec::<proc_macro2::TokenStream>::new();
    let mut non_primary_filter_func: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut insert_columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut insert_vec_construction: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut col_num: u32 = 0;

    let table_id_static_var_name = format_ident!("__table_id__{}", original_struct.ident);
    let original_struct_name = &original_struct.ident.to_string();
    let get_table_id_func = match parse_generated_func(quote! {
        pub fn table_id() -> u32 {
            if let Some(t_id)  = unsafe { #table_id_static_var_name } {
                return t_id;
            }
            let t_id = spacetimedb_bindings::get_table_id(#original_struct_name);
            unsafe { #table_id_static_var_name = Some(t_id) };
            return t_id;
        }
    }) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };

    for field in &original_struct.fields {
        let col_name = &field.ident.clone().unwrap();

        // The simple name for the type, e.g. Hash
        let col_type: proc_macro2::TokenStream;
        // The fully qualified name for this type, e.g. spacetimedb_lib::Hash
        let col_type_full: proc_macro2::TokenStream;
        // The TypeValue representation of this type
        let col_type_value: proc_macro2::TokenStream;
        let col_value_insert: proc_macro2::TokenStream;

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(ident) => {
                col_type = field.ty.clone().to_token_stream().to_string().parse().unwrap();
                col_type_full = col_type.clone();
                col_type_value = format!("spacetimedb_lib::TypeValue::{}", ident).parse().unwrap();
            }
            None => match field.ty.clone().to_token_stream().to_string().as_str() {
                "Hash" => {
                    col_type = "Hash".parse().unwrap();
                    col_type_full = "spacetimedb_lib::Hash".parse().unwrap();
                    col_type_value = format!("spacetimedb_lib::TypeValue::Bytes").parse().unwrap();
                }
                "Vec < u8 >" => {
                    // TODO: We are aliasing Vec<u8> to Bytes for now, we should deconstruct the vec here.
                    col_type = "Vec<u8>".parse().unwrap();
                    col_type_full = "std::vec::Vec<u8>".parse().unwrap();
                    col_type_value = format!("spacetimedb_lib::TypeValue::Bytes").parse().unwrap();
                }
                custom_type => {
                    col_type = custom_type.parse().unwrap();
                    col_type_full = col_type.clone();
                    col_type_value = "spacetimedb_lib::TypeValue::Tuple".parse().unwrap();
                }
            },
        }

        col_value_insert = format!("{}({})", col_type_value.clone(), format!("ins.{}", col_name))
            .parse()
            .unwrap();

        let mut unique_column: Option<UniqueColumn> = None;
        for attr in &field.attrs {
            if attr.path.to_token_stream().to_string().eq("unique") {
                unique_column = Some(UniqueColumn {
                    rust_type: col_type.clone(),
                    type_value: col_type_value.clone(),
                    column_ident: col_name.clone(),
                    conversion_from_raw_to_stdb_statement: quote!(
                        let data = #col_type_value(data);
                    ),
                    column_def: quote!(
                        #col_name: #col_type
                    ),
                    column_index: col_num,
                });
            }
        }

        if let None = unique_column {
            non_primary_column_idents.push(col_name.clone());
            non_primary_column_types.push(col_type.clone());
            non_primary_column_defs.push(quote!(
                #col_name: #col_type_full
            ));
            non_primary_index_lookup.push(col_num);
        }

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(_) => {
                insert_columns.push(quote! {
                    #col_value_insert
                });
            }
            None => {
                if let syn::Type::Path(syn::TypePath { ref path, .. }) = field.ty {
                    if path.segments.len() > 0 {
                        match path.segments[0].ident.to_token_stream().to_string().as_str() {
                            "Hash" => {
                                match unique_column {
                                    Some(mut some) => {
                                        some.conversion_from_raw_to_stdb_statement = quote!(
                                            let data = #col_type_value(data.to_vec());
                                        );
                                        unique_column = Some(some);
                                    }
                                    None => {}
                                }

                                insert_columns.push(quote! {
                                    spacetimedb_lib::TypeValue::Bytes(ins.#col_name.to_vec())
                                });
                            }
                            "Vec" => {
                                let vec_param = parse_generic_arg(path.segments[0].arguments.to_token_stream());
                                match vec_param {
                                    Ok(param) => {
                                        let vec_name: proc_macro2::TokenStream =
                                            format!("type_value_vec_{}", col_name).parse().unwrap();

                                        match rust_to_spacetimedb_ident(param.to_string().as_str()) {
                                            Some(spacetimedb_type) => {
                                                match spacetimedb_type.to_string().as_str() {
                                                    "U8" => {
                                                        // Vec<u8> is aliased to the Bytes type
                                                        insert_columns.push(quote! {
                                                            #col_value_insert
                                                        });
                                                    }
                                                    _ => {
                                                        insert_columns.push(quote! {
                                                            spacetimedb_lib::TypeValue::Vec(#vec_name)
                                                        });
                                                        insert_vec_construction.push(quote! {
                                                            let mut #vec_name: Vec<spacetimedb_lib::TypeValue> = Vec::<spacetimedb_lib::TypeValue>::new();
                                                            for value in ins.#col_name {
                                                                #vec_name.push(spacetimedb_lib::TypeValue::#spacetimedb_type(value));
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                            None => match param.to_string().as_str() {
                                                "Hash" => {
                                                    return quote! {
                                                        compile_error!("TODO: Implement vec support for hashes")
                                                    }
                                                    .into();
                                                }
                                                other_type => {
                                                    let other_type = format_ident!("{}", other_type);
                                                    insert_columns.push(quote! {
                                                        spacetimedb_lib::TypeValue::Vec(#vec_name)
                                                    });
                                                    insert_vec_construction.push(quote! {
                                                        let mut #vec_name: Vec<spacetimedb_lib::TypeValue> = Vec::<spacetimedb_lib::TypeValue>::new();
                                                        for value in ins.#col_name {
                                                            #vec_name.push(#other_type::struct_to_tuple(value));
                                                        }
                                                    });
                                                }
                                            },
                                        }
                                    }
                                    Err(e) => {
                                        return quote! {
                                            compile_error!(#e)
                                        }
                                        .into();
                                    }
                                }
                            }
                            other_type => {
                                let other_type = format_ident!("{}", other_type);
                                insert_columns.push(quote! {
                                    #other_type::struct_to_tuple(ins.#col_name)
                                });
                            }
                        }
                    }
                }
            }
        }

        column_idents.push(format_ident!("{}", col_name));
        row_to_struct_entries.push(quote!(
            &row.elements[#col_num]
        ));

        if let Some(col) = unique_column {
            unique_columns.push(col);
        }

        col_num = col_num + 1;
    }

    for unique in unique_columns {
        let filter_func_ident = format_ident!("filter_{}_eq", unique.column_ident);
        let update_func_ident = format_ident!("update_{}_eq", unique.column_ident);
        let delete_func_ident = format_ident!("delete_{}_eq", unique.column_ident);
        let unique_tuple_type_str: String = format!("{}", unique.type_value);
        let unique_column_index_usize = unique.column_index as usize;
        let comparison_block = tuple_field_comparison_block(
            original_struct.ident.clone().to_token_stream().to_string().as_str(),
            unique.rust_type.to_string().as_str(),
            unique.column_ident.clone(),
            true,
        );

        let unique_column_def = unique.column_def;
        let unique_column_ident = unique.column_ident;
        let unique_conversion_from_raw_to_stdb_statement = unique.conversion_from_raw_to_stdb_statement;
        let unique_column_index = unique.column_index;

        match parse_generated_func(quote! {
            #[allow(unused_variables)]
            #[allow(non_snake_case)]
            pub fn #filter_func_ident(#unique_column_def) -> Option<#original_struct_ident> {
                let table_iter = #original_struct_ident::iter_tuples();
                for row in table_iter {
                    let column_data = row.elements[#unique_column_index_usize].clone();
                    #comparison_block
                }

                return None;
            }
        }) {
            Ok(func) => unique_filter_funcs.push(func),
            Err(err) => {
                return proc_macro::TokenStream::from(err);
            }
        }

        match parse_generated_func(quote! {
            #[allow(unused_variables)]
            #[allow(non_snake_case)]
           pub fn #update_func_ident(#unique_column_def, new_value: #original_struct_ident) -> bool {
                #original_struct_ident::#delete_func_ident(#unique_column_ident);
                #original_struct_ident::insert(new_value);

                // For now this is always successful
                true
            }
        }) {
            Ok(func) => unique_update_funcs.push(func),
            Err(err) => {
                return proc_macro::TokenStream::from(err);
            }
        }

        match parse_generated_func(quote! {
            #[allow(unused_variables)]
            #[allow(non_snake_case)]
            pub fn #delete_func_ident(#unique_column_def) -> bool {
                let data = #unique_column_ident;
                #unique_conversion_from_raw_to_stdb_statement
                let equatable = spacetimedb_lib::EqTypeValue::try_from(data);
                match equatable {
                    Ok(value) => {
                        let result = spacetimedb_bindings::delete_eq(Self::table_id(), #unique_column_index, value);
                        match result {
                            None => {
                                //TODO: Returning here was supposed to signify an error, but it can also return none when there is nothing to delete.
                                //spacetimedb_bindings::println!("Internal server error on equatable type: {}", #primary_key_tuple_type_str);
                                false
                            },
                            Some(count) => {
                                count > 0
                            }
                        }
                    }, Err(e) => {
                        // We cannot complete this call because this type is not equatable
                        spacetimedb_bindings::println!("This type is not equatable: {} Error:{}", #unique_tuple_type_str, e);
                        false
                    }
                }
            }
        }) {
            Ok(func) => unique_delete_funcs.push(func),
            Err(err) => {
                return proc_macro::TokenStream::from(err);
            }
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

        match parse_generated_func(quote! {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            pub fn #filter_func_ident(#column_def) -> Vec <#original_struct_ident> {
                let mut result = Vec::<#original_struct_ident>::new();
                let table_iter = #original_struct_ident::iter_tuples();
                for row in table_iter {
                    let column_data = row.elements[#row_index].clone();
                    #comparison_block
                }

                return result;
            }
        }) {
            Ok(func) => non_primary_filter_func.push(func),
            Err(err) => {
                return proc_macro::TokenStream::from(err);
            }
        }
    }

    let db_insert: proc_macro2::TokenStream;
    match parse_generated_func(quote! {
        #[allow(unused_variables)]
        pub fn insert(ins: #original_struct_ident) {
            #(#insert_vec_construction)*
            spacetimedb_bindings::insert(Self::table_id(), spacetimedb_lib::TupleValue {
                elements: vec![
                    #(#insert_columns),*
                ]
            });
        }
    }) {
        Ok(func) => db_insert = func,
        Err(err) => {
            return proc_macro::TokenStream::from(err);
        }
    }

    let db_delete: proc_macro2::TokenStream;
    match parse_generated_func(quote! {
    #[allow(unused_variables)]
    pub fn delete(f: fn (#original_struct_ident) -> bool) -> usize {
        panic!("Delete using a function is not supported yet!");
    }}) {
        Ok(func) => db_delete = func,
        Err(err) => {
            return proc_macro::TokenStream::from(err);
        }
    }

    let db_update: proc_macro2::TokenStream;
    match parse_generated_func(quote! {
    #[allow(unused_variables)]
    pub fn update(value: #original_struct_ident) -> bool {
        panic!("Update using a value is not supported yet!");
    }}) {
        Ok(func) => db_update = func,
        Err(err) => {
            return proc_macro::TokenStream::from(err);
        }
    }

    let db_iter_tuples: proc_macro2::TokenStream;
    match parse_generated_func(quote! {
        #[allow(unused_variables)]
        pub fn iter_tuples() -> spacetimedb_bindings::TableIter {
            spacetimedb_bindings::__iter__(Self::table_id()).expect("Failed to get iterator from table.")
        }
    }) {
        Ok(func) => db_iter_tuples = func,
        Err(err) => {
            return proc_macro::TokenStream::from(err);
        }
    }

    let db_iter_ident = format_ident!("{}{}", original_struct_ident, "Iter");
    let db_iter_struct = quote! {
        pub struct #db_iter_ident {
            iter: spacetimedb_bindings::TableIter,
        }

        impl Iterator for #db_iter_ident {
            type Item = #original_struct_ident;

            fn next(&mut self) -> Option<Self::Item> {
                if let Some(tuple) = self.iter.next() {
                    Some(#original_struct_ident::tuple_to_struct(tuple).expect("Failed to convert tuple to struct."))
                } else {
                    None
                }
            }
        }
    };

    let db_iter: proc_macro2::TokenStream;
    match parse_generated_func(quote! {
        #[allow(unused_variables)]
        pub fn iter() -> #db_iter_ident {
            #db_iter_ident {
                iter: Self::iter_tuples()
            }
        }
    }) {
        Ok(func) => db_iter = func,
        Err(err) => {
            return proc_macro::TokenStream::from(err);
        }
    }

    let tuple_to_struct_func = match autogen_module_tuple_to_struct(original_struct.clone()) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };
    let struct_to_tuple_func = match autogen_module_struct_to_tuple(original_struct.clone()) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };
    let schema_func = match autogen_module_struct_to_schema(original_struct.clone()) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };

    let csharp_output = autogen_csharp_tuple(original_struct.clone(), Some(original_struct_ident.to_string()));

    let create_table_func_name = format_ident!("__create_table__{}", original_struct_ident);
    let describe_table_func_name = format_ident!("__describe_table__{}", original_struct_ident);
    let table_name = original_struct_ident.to_string();

    let table_id_static_var = quote! {
        #[allow(non_upper_case_globals)]
        static mut #table_id_static_var_name: Option<u32> = None;
    };

    let create_table_func = match parse_generated_func(quote! {
        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #create_table_func_name(arg_ptr: usize, arg_size: usize) {
            let def = #original_struct_ident::get_struct_schema();
            if let spacetimedb_lib::TypeDef::Tuple(tuple_def) = def {
                let table_id = spacetimedb_bindings::create_table(#table_name, tuple_def);
                unsafe { #table_id_static_var_name = Some(table_id) }
            } else {
                // The type is not a tuple for some reason, table not created.
                std::panic!("This type is not a tuple: {{#original_struct_ident}}");
            }
        }
    }) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };

    let describe_table_func = match parse_generated_func(quote! {
        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #describe_table_func_name() -> u64 {
            let def = #original_struct_ident::get_struct_schema();
            if let spacetimedb_lib::TypeDef::Tuple(tuple_def) = def {
                let mut bytes = vec![];
                tuple_def.encode(&mut bytes);
                let offset = bytes.as_ptr() as u64;
                let length = bytes.len() as u64;
                std::mem::forget(bytes);
                return offset << 32 | length;
            } else {
                // The type is not a tuple for some reason, table not created.
                std::panic!("This type is not a tuple: {{#original_struct_ident}}");
            }
        }
    }) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };

    // Output all macro data
    let emission = quote! {
        #table_id_static_var

        #create_table_func
        #describe_table_func
        #csharp_output

        #[derive(spacetimedb_bindgen::Unique, spacetimedb_bindgen::Index)]
        #[derive(serde::Serialize, serde::Deserialize)]
        #original_struct

        #db_iter_struct
        impl #original_struct_ident {
            #db_insert
            #db_delete
            #db_update
            #(#unique_filter_funcs)*
            #(#unique_update_funcs)*
            #(#unique_delete_funcs)*

            #db_iter
            #db_iter_tuples
            #(#non_primary_filter_func)*

            #tuple_to_struct_func
            #struct_to_tuple_func
            #schema_func
            #get_table_id_func
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    proc_macro::TokenStream::from(emission)
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
                spacetimedb_bindings::create_index(Self::table_id(), #index_type, vec!(#(#index_fields),*));
            }
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", output.to_string());
    }

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

    let csharp_output = autogen_csharp_tuple(original_struct.clone(), None);
    let return_schema = match autogen_module_struct_to_schema(original_struct.clone()) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };
    let tuple_to_struct_func = match autogen_module_tuple_to_struct(original_struct.clone()) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };
    let struct_to_tuple_func = match autogen_module_struct_to_tuple(original_struct.clone()) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };

    let create_tuple_func_name = format_ident!("__create_type__{}", original_struct_ident);
    let create_tuple_func = match parse_generated_func(quote! {
    #[no_mangle]
    #[allow(non_snake_case)]
    pub extern "C" fn #create_tuple_func_name(arg_ptr: usize, arg_size: usize) {
        let ptr = unsafe { arg_ptr as *mut u8 };
        let def = #original_struct_ident::get_struct_schema();
        let mut bytes = unsafe { Vec::from_raw_parts(ptr, 0, arg_size) };
        def.encode(&mut bytes);
    }}) {
        Ok(func) => func,
        Err(err) => {
            return TokenStream::from(err);
        }
    };

    let emission = quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        #original_struct
        impl #original_struct_ident {
            #tuple_to_struct_func
            #struct_to_tuple_func
            #return_schema
        }
        #create_tuple_func
        #csharp_output
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    return TokenStream::from(emission);
}

fn spacetimedb_migrate(_: AttributeArgs, item: TokenStream) -> TokenStream {
    let original_func = parse_macro_input!(item as ItemFn);
    let func_name = &original_func.sig.ident;

    let emission = match parse_generated_func(quote! {
    #[allow(non_snake_case)]
    pub extern "C" fn __migrate__(arg_ptr: u32, arg_size: u32) {
        #func_name();
    }}) {
        Ok(func) => {
            quote! {
                #func
                #original_func
            }
        }
        Err(err) => err,
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    proc_macro::TokenStream::from(emission)
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
        if arg_num > 1 {
            return proc_macro::TokenStream::from(quote! {
                compile_error!("Client connect/disconnect can only have one argument (identity: Hash)");
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
                let arg_type_str = arg_token.to_string();

                // First argument must be Hash (sender)
                if arg_num == 0 {
                    if arg_type_str != "spacetimedb_lib :: hash :: Hash" && arg_type_str != "Hash" {
                        let error_str = format!(
                            "Parameter 1 of connect/disconnect {} must be of type \'Hash\'.",
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
                            "Parameter 1 of connect/disconnect {} must be of type \'Hash\'.",
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

    let emission = match parse_generated_func(quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #connect_disconnect_ident(arg_ptr: usize, arg_size: usize) {
            let arguments = spacetimedb_lib::args::ConnectDisconnectArguments::decode_mem(
                unsafe { arg_ptr as *mut u8 }, arg_size).expect("Unable to decode module arguments");

            // Invoke the function with the deserialized args
            #func_name(arguments.identity, arguments.timestamp,);
        }
    }) {
        Ok(func) => quote! {
            #func
            #original_function
        },
        Err(err) => err,
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    proc_macro::TokenStream::from(emission)
}

// This derive is actually a no-op, we need the helper attribute for spacetimedb
#[proc_macro_derive(Unique, attributes(unique))]
pub fn derive_unique(_: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_derive(Index, attributes(index))]
pub fn derive_index(_item: TokenStream) -> TokenStream {
    TokenStream::new()
}

pub(crate) fn rust_to_spacetimedb_ident(input_type: &str) -> Option<Ident> {
    return match input_type {
        // These are typically prefixed with spacetimedb_lib::TypeDef::
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
    is_unique: bool,
) -> proc_macro2::TokenStream {
    let stdb_type_value: proc_macro2::TokenStream;
    let comparison_and_result_statement: proc_macro2::TokenStream;
    let result_statement: proc_macro2::TokenStream;
    let tuple_to_struct_func: proc_macro2::TokenStream = format!("tuple_to_struct").parse().unwrap();
    let err_string = format!(
        "Internal stdb error: Can't convert from tuple to struct (wrong version?) {}",
        tuple_type_str
    );

    if is_unique {
        result_statement = quote! {
            let tuple = Self::#tuple_to_struct_func(row);
            if let None = tuple {
                spacetimedb_bindings::println!(#err_string);
                return None;
            }
            return Some(tuple.unwrap());
        }
    } else {
        result_statement = quote! {
            let tuple = Self::#tuple_to_struct_func(row);
            if let None = tuple {
                spacetimedb_bindings::println!(#err_string);
                continue;
            }
            result.push(tuple.unwrap());
        }
    }

    match rust_to_spacetimedb_ident(filter_field_type_str) {
        Some(ident) => {
            stdb_type_value = format!("spacetimedb_lib::TypeValue::{}", ident).parse().unwrap();
            comparison_and_result_statement = quote! {
                if entry_data == #filter_field_name {
                    #result_statement
                }
            };
        }
        None => {
            match filter_field_type_str {
                "Hash" => {
                    stdb_type_value = format!("spacetimedb_lib::TypeValue::Bytes").parse().unwrap();
                    comparison_and_result_statement = quote! {
                        let entry_data = spacetimedb_lib::hash::Hash::from_slice(&entry_data[0..32]);
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

fn parse_generated_func(
    func_stream: proc_macro2::TokenStream,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    if !syn::parse2::<ItemFn>(func_stream.clone()).is_ok() {
        println!(
            "This function has an invalid generation:\n{}",
            func_stream.clone().to_string()
        );
        return Err(quote! {
            compile_error!("Invalid function produced by spacetimedb macro.");
        });
    }
    Ok(func_stream)
}
