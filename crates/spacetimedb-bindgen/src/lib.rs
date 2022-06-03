#![crate_type = "proc-macro"]

extern crate proc_macro;
extern crate core;

use proc_macro::{TokenStream};
use proc_macro2::Ident;
use quote::{format_ident, quote, ToTokens};
use syn::{AttributeArgs, FnArg, ItemStruct, parse_macro_input, ItemFn};
use syn::Fields::{Named, Unit, Unnamed};

#[proc_macro_attribute]
pub fn spacetimedb(macro_args: TokenStream, item: TokenStream) -> TokenStream {
    let attribute_args = parse_macro_input!(macro_args as AttributeArgs);
    let attribute_str = attribute_args[0].to_token_stream().to_string();
    let attribute_str = attribute_str.as_str();
    match attribute_str {
        "reducer" => spacetimedb_reducer(attribute_args, item),
        "table" => spacetimedb_table(attribute_args, item),
        "migrate" => spacetimedb_migrate(attribute_args, item),
        "index(btree)" => spacetimedb_index(attribute_args, item),
        "index(hash)" => spacetimedb_index(attribute_args, item),
        _ => proc_macro::TokenStream::from(quote! {
            compile_error!("Please pass a valid attribute to the spacetimedb macro: reducer, ...");
        })
    }
}


fn spacetimedb_reducer(args: AttributeArgs, item: TokenStream) -> TokenStream {
    if *(&args.len()) > 1 {
        let str = format!("Unexpected macro argument: {}",
                          args[1].to_token_stream().to_string());
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
    for function_argument in original_function.sig.inputs.iter() {
        match function_argument {
            FnArg::Receiver(_) => {
                return proc_macro::TokenStream::from(quote! {
                    compile_error!("Receiver types in reducer parameters not supported!");
                });
            }
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_token = arg_type.to_token_stream();
                let var_name = format_ident!("arg_{}", arg_num);

                parse_json_to_args.push(quote! {
                    let #var_name : #arg_token = serde_json::from_value(args[#arg_num].clone()).unwrap();
                });
                function_call_args.push(var_name);
            }
        }

        arg_num = arg_num + 1;
    }

    let generated_function = quote! {
        #[no_mangle]
        pub extern "C" fn #reducer_func_name(arg_ptr: u32, arg_size: u32) {
            let arg_ptr = arg_ptr as *mut u8;
            let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size as usize, arg_size as usize) };
            let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let args = arg_json.as_array().unwrap();

            // Deserialize the json argument list
            #(#parse_json_to_args);*

            // Invoke the function with the deserialized args
            #func_name(#(#function_call_args),*);
        }
    };

    proc_macro::TokenStream::from(quote! {
        #generated_function
        #original_function
    })
}

fn spacetimedb_table(args: AttributeArgs, item: TokenStream) -> TokenStream {
    if *(&args.len()) > 1 {
        let str = format!("Unexpected macro argument: {}",
                          args[1].to_token_stream().to_string());
        return proc_macro::TokenStream::from(quote! {
            compile_error!(#str);
        });
    }

    let original_struct = parse_macro_input!(item as ItemStruct);
    let original_struct_ident = &original_struct.clone().ident;
    let table_id_field_name = format_ident!("__table_id__{}", original_struct_ident.to_token_stream().to_string());
    let table_id_field = quote!(
        static mut #table_id_field_name: u32 = 0;
    );

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
    let mut columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut table_funcs: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut row_to_struct_entries: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut row_to_struct_entries_values: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut non_primary_columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut non_primary_columns_idents: Vec<Ident> = Vec::new();
    let mut insert_columns: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut row_to_struct_let_values: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut non_primary_let_data_statements: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut primary_let_data_statement: Option<proc_macro2::TokenStream> = None;
    let mut primary_key_column_ident: Option<Ident> = None;
    let mut primary_key_column_def: Option<proc_macro2::TokenStream> = None;
    let mut primary_key_column_index: Option<usize> = None;
    let mut primary_key_set = false;
    let mut col_num: usize = 0;


    for field in &original_struct.fields {
        let col_name = &field.ident.clone().unwrap();
        let col_type_tok: proc_macro2::TokenStream = format!("spacetimedb_bindings::ColType::{}",
                                                             field.ty.to_token_stream().to_string().to_uppercase()).parse().unwrap();
        let col_value_type: proc_macro2::TokenStream = format!("spacetimedb_bindings::ColValue::{}",
                                                               field.ty.to_token_stream().to_string().to_uppercase()).parse().unwrap();
        let col_value_insert: proc_macro2::TokenStream = format!("{}({})", col_value_type,
                                                                 format!("ins.{}", col_name)).parse().unwrap();
        let col_num_u32: u32 = col_num as u32;

        row_to_struct_let_values.push(quote!(
            let #col_value_type(#col_name) = entry[#col_num];
        ));

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
                is_primary = true;
            }
        }

        if !is_primary {
            non_primary_columns_idents.push(col_name.clone());
            non_primary_columns.push(quote!(
                    #col_name: #col_type
                ));
            non_primary_let_data_statements.push(quote!(
                if let #col_value_type(data) = data
            ));
        }

        columns.push(quote! {
            spacetimedb_bindings::Column {
                col_id: #col_num_u32,
                col_type: #col_type_tok,
            }
        });

        insert_columns.push(quote! {
            #col_value_insert
        });

        column_idents.push(format_ident!("{}", col_name));
        row_to_struct_entries.push(quote!(
            entry[#col_num]
        ));

        row_to_struct_entries_values.push(quote!(
            #col_value_type(#col_name)
        ));

        col_num = col_num + 1;
    }

    let table_func_name = format_ident!("__create_table__{}", original_struct_ident.to_token_stream().to_string());
    let table_func = quote! {
        #[no_mangle]
        pub extern "C" fn #table_func_name(arg_ptr: u32, arg_size: u32) {
            unsafe {
                #table_id_field_name = 0;
            }
            spacetimedb_bindings::create_table(0, vec![
                    #(#columns),*
                ]);
        }
    };

    match (primary_key_column_def, primary_key_column_ident, primary_let_data_statement, primary_key_column_index) {
        (Some(primary_key_column_def), Some(primary_key_column_ident),
            Some(primary_let_data_statement), Some(primary_key_column_index)) => {
            let filter_func_ident = format_ident!("filter_{}_eq", primary_key_column_ident);
            let delete_func_ident = format_ident!("delete_{}_eq", primary_key_column_ident);
            table_funcs.push(quote!(
                pub fn #filter_func_ident(#primary_key_column_def) -> Option<#original_struct_ident> {
                    unsafe {
                        let table_iter = spacetimedb_bindings::iter(#table_id_field_name);
                        if let Some(table_iter) = table_iter {
                            for entry in table_iter {
                                let data = entry[#primary_key_column_index];
                                #primary_let_data_statement {
                                    if #primary_key_column_ident == data {
                                        let value = #original_struct_ident::table_row_to_struct(entry);
                                        if let Some(value) = value {
                                            return Some(value);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    return None;
                }
                pub fn #delete_func_ident(#primary_key_column_def) -> bool {
                    false
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

        let filter_func = quote!(
            pub fn #filter_func_ident(#column_def) -> Vec<#original_struct_ident> {
                unsafe {
                   let mut result = Vec::<#original_struct_ident>::new();
                    let table_iter = spacetimedb_bindings::iter(#table_id_field_name);
                    if let Some(table_iter) = table_iter {
                        for entry in table_iter {
                            let data = entry[#x];
                            #let_statement {
                                if #non_primary_column_ident == data {
                                    let value = #original_struct_ident::table_row_to_struct(entry);
                                    if let Some(value) = value {
                                        result.push(value);
                                    }
                                }
                            }
                        }
                    }

                    return result;
                }
            }
        );
        let delete_func = quote!(
                pub fn #delete_func_ident(#column_def) -> usize {
                    0
                }
            );

        table_funcs.push(filter_func);
        table_funcs.push(delete_func);
    }


    let db_funcs = quote! {
        impl #original_struct_ident {
            pub fn insert(ins: #original_struct_ident) {
                unsafe {
                    spacetimedb_bindings::insert(#table_id_field_name, vec![
                        #(#insert_columns),*
                    ]);
                }
            }

            pub fn delete(f: fn (#original_struct_ident) -> bool) -> usize {
                0
            }

            pub fn update(value: #original_struct_ident) -> bool {
                // delete on primary key
                // insert on primary key
                false
            }

            fn table_row_to_struct(entry: Vec<ColValue>) -> Option<#original_struct_ident> {
                return match (#(#row_to_struct_entries),*) {
                    (#(#row_to_struct_entries_values),*) => {
                        Some(#original_struct_ident {
                            #(#column_idents),*
                        })
                    }
                    _ => {
                        None
                    }
                }
            }

            #(#table_funcs)*
        }
    };

    proc_macro::TokenStream::from(quote! {
        #[derive(serde::Serialize, serde::Deserialize, spacetimedb_bindgen::PrimaryKey, spacetimedb_bindgen::Index)]
        #original_struct
        #table_func
        #table_id_field
        #db_funcs
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
            let invalid_index = format!("Invalid index type: {}\nValid options are: index(btree), index(hash)",
                                        args[0].to_token_stream().to_string());
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
            index_name = arg_str.chars().skip(name_prefix.len() + 1).take(
                arg_str.len() - name_prefix.len() - 2).collect();
        } else {
            let field_index = all_fields.iter().position(|a|
                a.to_token_stream().to_string() == arg_str);
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
            fn #function_name(arg_ptr: u32, arg_size: u32) {
                unsafe {
                    spacetimedb_bindings::create_index(#table_id_field_name, #index_type, vec!(#(#index_fields),*));
                }
            }
        }
    };

    proc_macro::TokenStream::from(output)
}

fn spacetimedb_migrate(_: AttributeArgs, item: TokenStream) -> TokenStream {
    let original_func = parse_macro_input!(item as ItemFn);
    let func_name = &original_func.sig.ident;

    proc_macro::TokenStream::from(quote! {
        pub extern "C" fn __migrate__(arg_ptr: u32, arg_size: u32) {
            #func_name();
        }
        #original_func
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