#![crate_type = "proc-macro"]

// mod csharp;
mod module;

extern crate core;
extern crate proc_macro;

use crate::module::{
    args_to_tuple_schema, autogen_module_struct_to_schema, autogen_module_struct_to_tuple,
    autogen_module_tuple_to_struct,
};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use std::time::Duration;
use syn::Fields::{Named, Unit, Unnamed};
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, ItemStruct, Meta, NestedMeta};

#[proc_macro_attribute]
pub fn spacetimedb(macro_args: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = item.into();
    let attribute_args = parse_macro_input!(macro_args as AttributeArgs);
    let (attr_arg_0, other_args) = match attribute_args.split_first() {
        Some(x) => x,
        None => {
            return syn::Error::new(Span::call_site(), "must provide arg to #[spacetimedb]")
                .into_compile_error()
                .into()
        }
    };

    let res = match attr_arg_0 {
        NestedMeta::Lit(_) => None,
        NestedMeta::Meta(meta) => meta.path().get_ident().and_then(|id| {
            let res = match &*id.to_string() {
                "table" => spacetimedb_table(&meta, other_args, item),
                "reducer" => spacetimedb_reducer(&meta, other_args, item),
                "connect" => spacetimedb_connect_disconnect(&meta, other_args, item, true),
                "disconnect" => spacetimedb_connect_disconnect(&meta, other_args, item, false),
                "migrate" => spacetimedb_migrate(&meta, other_args, item),
                "tuple" => spacetimedb_tuple(&meta, other_args, item),
                "index" => spacetimedb_index(&meta, other_args, item),
                _ => return None,
            };
            Some(res)
        }),
    };
    let res = res.unwrap_or_else(|| {
        Err(syn::Error::new_spanned(
            attr_arg_0,
            "Please pass a valid attribute to the spacetimedb macro: \
                 reducer, table, connect, disconnect, migrate, tuple, index, ...",
        ))
    });
    res.unwrap_or_else(syn::Error::into_compile_error).into()
}

fn spacetimedb_reducer(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    if let Some((first, args)) = args.split_first() {
        let value = match first {
            NestedMeta::Meta(Meta::NameValue(p)) if p.path.is_ident("repeat") => &p.lit,
            _ => {
                return Err(syn::Error::new_spanned(
                    first,
                    r#"unknown argument. did you mean `repeat = "..."`?"#,
                ))
            }
        };
        let s = match value {
            syn::Lit::Str(s) => s.value(),
            syn::Lit::Int(i) => i.to_string(),
            _ => {
                return Err(syn::Error::new_spanned(
                    value,
                    "repeat argument must be a string or an int with a suffix",
                ))
            }
        };

        let repeat_duration = parse_duration::parse(&s)
            .map_err(|e| syn::Error::new_spanned(s, format!("Can't parse repeat time: {e}")))?;

        return spacetimedb_repeating_reducer(args, item, repeat_duration);
    }

    let original_function = syn::parse2::<ItemFn>(item)?;
    let func_name = &original_function.sig.ident;
    let reducer_func_name = format_ident!("__reducer__{}", &func_name);
    let descriptor_func_name = format_ident!("__describe_reducer__{}", &func_name);

    let mut parse_json_to_args = Vec::new();
    let mut function_call_arg_names = Vec::new();
    let mut arg_num: usize = 0;
    let mut json_arg_num: usize = 0;
    let function_arguments = &original_function.sig.inputs;

    let function_call_arg_types = args_to_tuple_schema(function_arguments.iter().skip(2));

    for function_argument in function_arguments {
        match function_argument {
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    function_argument,
                    "Receiver types in reducer parameters not supported!",
                ));
            }
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_token = arg_type.to_token_stream();
                let arg_type_str = arg_token.to_string();
                let var_name = format_ident!("arg_{}", arg_num);

                // First argument must be Hash (sender)
                if arg_num == 0 {
                    if arg_type_str != "spacetimedb::spacetimedb_lib::hash::Hash" && arg_type_str != "Hash" {
                        let error_str = format!(
                            "Parameter 1 of reducer {} must be of type \'Hash\'.",
                            func_name.to_string()
                        );
                        return Err(syn::Error::new_spanned(arg_type, error_str));
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
                        return Err(syn::Error::new_spanned(arg_type, error_str));
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
        pub extern "C" fn #reducer_func_name(arg_ptr: *const u8, arg_size: usize) {
            let arguments = unsafe { spacetimedb::spacetimedb_lib::args::ReducerArguments::decode_mem(
                arg_ptr, arg_size).expect("Unable to decode module arguments") };

            // Unwrap extra arguments, conditional on whether or not there are extra args.
            #unwrap_args

            // Deserialize the json argument list
            #(#parse_json_to_args);*

            // Invoke the function with the deserialized args
            #func_name(arguments.identity, arguments.timestamp, #(#function_call_arg_names),*);
        }
    };

    let reducer_name = func_name.to_string();
    let generated_describe_function = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        // u64 is offset << 32 | length
        pub extern "C" fn #descriptor_func_name() -> u64 {
            let tupledef = spacetimedb::spacetimedb_lib::ReducerDef {
                name: Some(#reducer_name.into()),
                args: vec![
                    #(#function_call_arg_types),*
                ],
            };
            let mut bytes = vec![];
            tupledef.encode(&mut bytes);
            let offset = bytes.as_ptr() as u64;
            let length = bytes.len() as u64;
            std::mem::forget(bytes);
            return offset << 32 | length;
        }
    };

    // autogen_csharp_reducer(original_function.clone());

    Ok(quote! {
        #generated_function
        #generated_describe_function
        #original_function
    })
}

fn spacetimedb_repeating_reducer(
    args: &[NestedMeta],
    item: TokenStream,
    repeat_duration: Duration,
) -> syn::Result<TokenStream> {
    assert_no_args(args)?;

    let original_function = syn::parse2::<ItemFn>(item)?;
    let func_name = &original_function.sig.ident;
    let reducer_func_name = format_ident!("__repeating_reducer__{}", &func_name);

    let mut arg_num: usize = 0;
    let function_arguments = &original_function.sig.inputs;
    if function_arguments.len() != 2 {
        return Err(syn::Error::new_spanned(
            function_arguments,
            "Expected 2 arguments (timestamp: u64, delta_time: u64) for repeating reducer.",
        ));
    }
    for function_argument in function_arguments {
        match function_argument {
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    function_argument,
                    "Receiver types in reducer parameters not supported!",
                ));
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
                        return Err(syn::Error::new_spanned(arg_type, error_str));
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
                        return Err(syn::Error::new_spanned(arg_type, error_str));
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
        pub extern "C" fn #reducer_func_name(arg_ptr: *const u8, arg_size: usize) -> u64 {
            // Deserialize the arguments
            let arguments = unsafe { spacetimedb::spacetimedb_lib::args::RepeatingReducerArguments::decode_mem(
                arg_ptr, arg_size).expect("Unable to decode module arguments") };

            // Invoke the function with the deserialized args
            #func_name(arguments.timestamp, arguments.delta_time);

            return #duration_as_millis;
        }
    };

    Ok(quote! {
        #generated_function
        #original_function
    })
}

// TODO: We actually need to add a constraint that requires this column to be unique!
struct Column {
    ty: syn::Type,
    ident: Ident,
    index: u8,
    convert_to_typevalue: proc_macro2::TokenStream,
}

fn spacetimedb_table(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    assert_no_args(args)?;

    let mut original_struct = syn::parse2::<ItemStruct>(item)?;
    let original_struct_ident = &original_struct.ident;

    match &original_struct.fields {
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
            return Err(syn::Error::new_spanned(&original_struct.fields, str));
        }
        Unit => {
            let str = format!("spacetimedb tables must have named fields (unit struct forbidden).");
            return Err(syn::Error::new_spanned(&original_struct.fields, str));
        }
    }

    let mut unique_columns = Vec::<Column>::new();
    let mut filterable_columns = Vec::<Column>::new();

    let table_id_static_var_name = format_ident!("__table_id__{}", original_struct.ident);
    let get_table_id_func = quote! {
        pub fn table_id() -> u32 {
            *#table_id_static_var_name.get_or_init(|| {
                spacetimedb::get_table_id(<Self as spacetimedb::TableType>::TABLE_NAME)
            })
        }
    };

    for (col_num, field) in original_struct.fields.iter_mut().enumerate() {
        let col_num: u8 = col_num
            .try_into()
            .map_err(|_| syn::Error::new_spanned(&field, "too many columns; the most a table can have is 256"))?;
        let col_name = &field.ident.clone().unwrap();

        // The TypeValue representation of this type
        let convert_to_typevalue: proc_macro2::TokenStream;

        match rust_to_spacetimedb_ident(field.ty.clone().to_token_stream().to_string().as_str()) {
            Some(ident) => {
                convert_to_typevalue = quote!(
                    let value = spacetimedb::spacetimedb_lib::TypeValue::#ident(value);
                );
            }
            None => match field.ty.clone().to_token_stream().to_string().as_str() {
                "Hash" => {
                    convert_to_typevalue = quote!(
                        let value = spacetimedb::spacetimedb_lib::TypeValue::Hash(Box::new(value));
                    );
                }
                "Vec < u8 >" => {
                    // TODO: We are aliasing Vec<u8> to Bytes for now, we should deconstruct the vec here.
                    convert_to_typevalue = quote!(
                        let value = spacetimedb::spacetimedb_lib::TypeValue::Bytes(value);
                    );
                }
                _custom_type => {
                    convert_to_typevalue = quote!(
                        let value = spacetimedb::spacetimedb_lib::TypeValue::Tuple(value);
                    );
                }
            },
        }
        // // The simple name for the type, e.g. Hash
        // let col_type: proc_macro2::TokenStream;
        // // The fully qualified name for this type, e.g. spacetimedb::spacetimedb_lib::Hash
        // let col_type_full: proc_macro2::TokenStream;
        // // The TypeValue representation of this type
        // let col_type_value: proc_macro2::TokenStream;
        // let col_value_insert: proc_macro2::TokenStream;

        // col_value_insert = format!("{}({})", col_type_value.clone(), format!("ins.{}", col_name))
        //     .parse()
        //     .unwrap();

        let mut is_unique = false;
        let mut is_filterable = false;
        let mut remove_idxs = vec![];
        for (i, attr) in field.attrs.iter().enumerate() {
            if attr.path.is_ident("unique") {
                if is_filterable {
                    return Err(syn::Error::new_spanned(
                        field,
                        "a field cannot be unique and filterable_by",
                    ));
                }
                is_unique = true;
                remove_idxs.push(i);
            } else if attr.path.is_ident("filterable_by") {
                if is_unique {
                    return Err(syn::Error::new_spanned(
                        field,
                        "a field cannot be unique and filterable_by",
                    ));
                }
                is_filterable = true;
                remove_idxs.push(i);
            }
        }
        for i in remove_idxs.into_iter().rev() {
            field.attrs.remove(i);
        }
        let column = || Column {
            ty: field.ty.clone(),
            ident: col_name.clone(),
            index: col_num,
            convert_to_typevalue,
        };

        if is_unique {
            unique_columns.push(column());
        } else if is_filterable {
            filterable_columns.push(column());
        }
    }

    let mut unique_filter_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_update_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_delete_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_fields = Vec::with_capacity(unique_columns.len());
    for unique in unique_columns {
        let filter_func_ident = format_ident!("filter_{}_eq", unique.ident);
        let update_func_ident = format_ident!("update_{}_eq", unique.ident);
        let delete_func_ident = format_ident!("delete_{}_eq", unique.ident);
        let comparison_block = tuple_field_comparison_block(&original_struct.ident, &unique.ident, true);

        let Column {
            ty: column_type,
            ident: column_ident,
            index: column_index,
            convert_to_typevalue,
        } = unique;
        let column_index_usize: usize = column_index.into();

        unique_fields.push(column_index);

        unique_filter_funcs.push(quote! {
            #[allow(unused_variables)]
            #[allow(non_snake_case)]
            pub fn #filter_func_ident(#column_ident: #column_type) -> Option<Self> {
                let table_iter = #original_struct_ident::iter_tuples();
                for row in table_iter {
                    let column_data = row.elements[#column_index_usize].clone();
                    #comparison_block
                }

                return None;
            }
        });

        unique_update_funcs.push(quote! {
            #[allow(unused_variables)]
            #[allow(non_snake_case)]
            pub fn #update_func_ident(value: #column_type, new_value: Self) -> bool {
                #original_struct_ident::#delete_func_ident(value);
                #original_struct_ident::insert(new_value);

                // For now this is always successful
                true
            }
        });

        unique_delete_funcs.push(quote! {
            #[allow(unused_variables)]
            #[allow(non_snake_case)]
            pub fn #delete_func_ident(value: #column_type) -> bool {
                #convert_to_typevalue
                let result = spacetimedb::delete_eq(Self::table_id(), #column_index, value);
                match result {
                    None => {
                        //TODO: Returning here was supposed to signify an error, but it can also return none when there is nothing to delete.
                        //spacetimedb::println!("Internal server error on equatable type: {}", #primary_key_tuple_type_str);
                        false
                    },
                    Some(count) => {
                        count > 0
                    }
                }
            }
        });
    }

    let mut non_primary_filter_func = Vec::with_capacity(filterable_columns.len());
    for column in filterable_columns {
        let filter_func_ident: proc_macro2::TokenStream = format!("filter_{}_eq", column.ident).parse().unwrap();

        let comparison_block = tuple_field_comparison_block(&original_struct_ident, &column.ident, false);

        let column_ident = column.ident;
        let column_type = column.ty;
        let row_index: usize = column.index.into();

        non_primary_filter_func.push(quote! {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            pub fn #filter_func_ident(#column_ident: #column_type) -> Vec<Self> {
                let mut result = Vec::<Self>::new();
                let table_iter = Self::iter_tuples();
                for row in table_iter {
                    let column_data = row.elements[#row_index].clone();
                    #comparison_block
                }

                result
            }
        });
    }

    let db_insert = quote! {
        #[allow(unused_variables)]
        pub fn insert(ins: #original_struct_ident) {
            spacetimedb::insert(Self::table_id(), spacetimedb::IntoTuple::into_tuple(ins));
        }
    };

    let db_delete = quote! {
        #[allow(unused_variables)]
        pub fn delete(f: fn (#original_struct_ident) -> bool) -> usize {
            panic!("Delete using a function is not supported yet!");
        }
    };

    let db_update = quote! {
        #[allow(unused_variables)]
        pub fn update(value: #original_struct_ident) -> bool {
            panic!("Update using a value is not supported yet!");
        }
    };

    let db_iter_tuples = quote! {
        #[allow(unused_variables)]
        pub fn iter_tuples() -> spacetimedb::TableIter {
            spacetimedb::__iter__(Self::table_id()).expect("Failed to get iterator from table.")
        }
    };

    let db_iter_ident = format_ident!("{}{}", original_struct_ident, "Iter");
    let db_iter_struct = quote! {
        pub struct #db_iter_ident {
            iter: spacetimedb::TableIter,
        }

        impl Iterator for #db_iter_ident {
            type Item = #original_struct_ident;

            fn next(&mut self) -> Option<Self::Item> {
                if let Some(tuple) = self.iter.next() {
                    Some(spacetimedb::FromTuple::from_tuple(tuple).expect("Failed to convert tuple to struct."))
                } else {
                    None
                }
            }
        }
    };

    let db_iter = quote! {
        #[allow(unused_variables)]
        pub fn iter() -> #db_iter_ident {
            #db_iter_ident {
                iter: Self::iter_tuples()
            }
        }
    };

    let from_value_impl = autogen_module_tuple_to_struct(&original_struct)?;
    let into_value_impl = autogen_module_struct_to_tuple(&original_struct)?;
    let schema_impl = autogen_module_struct_to_schema(&original_struct)?;
    let table_name = original_struct_ident.to_string();
    let tabletype_impl = quote! {
        impl spacetimedb::TableType for #original_struct_ident {
            const TABLE_NAME: &'static str = #table_name;
            const UNIQUE_COLUMNS: &'static [u8] = &[#(#unique_fields),*];
        }
    };

    // let csharp_output = autogen_csharp_tuple(original_struct.clone(), Some(original_struct_ident.to_string()));

    let create_table_func_name = format_ident!("__create_table__{}", original_struct_ident);
    let describe_table_func_name = format_ident!("__describe_table__{}", original_struct_ident);

    let table_id_static_var = quote! {
        #[allow(non_upper_case_globals)]
        static #table_id_static_var_name: spacetimedb::__private::OnceCell<u32> = spacetimedb::__private::OnceCell::new();
    };

    let create_table_func = quote! {
        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #create_table_func_name(arg_ptr: usize, arg_size: usize) {
            let table_id = <#original_struct_ident as spacetimedb::TableType>::create_table();
            #table_id_static_var_name.set(table_id).unwrap_or_else(|_| {
                // TODO: this is okay? or should we panic? can this even happen?
            });
        }
    };

    let describe_table_func = quote! {
        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #describe_table_func_name() -> u64 {
            <#original_struct_ident as spacetimedb::TableType>::describe_table()
        }
    };

    // Output all macro data
    let emission = quote! {
        #table_id_static_var

        #create_table_func
        #describe_table_func
        // #csharp_output

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

            #get_table_id_func
        }

        #schema_impl
        #from_value_impl
        #into_value_impl
        #tabletype_impl
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    Ok(emission)
}

fn spacetimedb_index(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    let mut index_name: String = "default_index".to_string();
    let mut index_fields = Vec::<u32>::new();
    let mut all_fields = Vec::<Ident>::new();
    let index_type: u8; // default index is a btree

    let generic_err = "index() must have index type passed; try index(btree) or index(hash)";
    match meta {
        Meta::List(l) => match l.nested.len() {
            0 => return Err(syn::Error::new_spanned(meta, generic_err)),
            1 => {
                let err = || syn::Error::new_spanned(&l.nested[0], "index() only accepts `btree` or `hash`");
                match &l.nested[0] {
                    NestedMeta::Meta(Meta::Path(p)) => {
                        if p.is_ident("btree") {
                            index_type = 0;
                        } else if p.is_ident("hash") {
                            index_type = 1;
                        } else {
                            return Err(err());
                        }
                    }
                    _ => return Err(err()),
                }
            }
            _ => return Err(syn::Error::new_spanned(l, "index() only takes one argument")),
        },
        _ => return Err(syn::Error::new_spanned(meta, generic_err)),
    }

    let original_struct = syn::parse2::<ItemStruct>(item)?;
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
                    return Err(syn::Error::new_spanned(arg, invalid_index));
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
                spacetimedb::create_index(Self::table_id(), #index_type, vec!(#(#index_fields),*));
            }
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", output.to_string());
    }

    Ok(output)
}

fn spacetimedb_tuple(meta: &Meta, _: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    let original_struct = syn::parse2::<ItemStruct>(item)?;
    let original_struct_ident = original_struct.clone().ident;

    match original_struct.clone().fields {
        Named(_) => {}
        Unnamed(_) => {
            let str = format!("spacetimedb tables and types must have named fields.");
            return Err(syn::Error::new_spanned(&original_struct.fields, str));
        }
        Unit => {
            let str = format!("Unit structure not supported.");
            return Err(syn::Error::new_spanned(&original_struct.fields, str));
        }
    }

    // let csharp_output = autogen_csharp_tuple(original_struct.clone(), None);
    let schema_impl = autogen_module_struct_to_schema(&original_struct)?;
    let from_value_impl = autogen_module_tuple_to_struct(&original_struct)?;
    let into_value_impl = autogen_module_struct_to_tuple(&original_struct)?;

    let create_tuple_func_name = format_ident!("__create_type__{}", original_struct_ident);
    let create_tuple_func = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #create_tuple_func_name(ptr: *mut u8, arg_size: usize) {
            let def = <#original_struct_ident as spacetimedb::SchemaType>::get_schema();
            let mut bytes = unsafe { Vec::from_raw_parts(ptr, 0, arg_size) };
            def.encode(&mut bytes);
        }
    };

    let describe_tuple_func_name = format_ident!("__describe_tuple__{}", original_struct_ident);

    let emission = quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        #original_struct
        #schema_impl
        #from_value_impl
        #into_value_impl
        #create_tuple_func

        #[allow(non_snake_case)]
        #[no_mangle]
        pub extern "C" fn #describe_tuple_func_name() -> u64 {
            <#original_struct_ident as spacetimedb::TupleType>::describe_tuple()
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    Ok(emission)
}

fn spacetimedb_migrate(meta: &Meta, _: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    let original_func = syn::parse2::<ItemFn>(item)?;
    let func_name = &original_func.sig.ident;

    let emission = quote! {
        #[allow(non_snake_case)]
        pub extern "C" fn __migrate__(arg_ptr: u32, arg_size: u32) {
            #func_name();
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    Ok(emission)
}

fn spacetimedb_connect_disconnect(
    meta: &Meta,
    args: &[NestedMeta],
    item: TokenStream,
    connect: bool,
) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    assert_no_args(args)?;

    let original_function = syn::parse2::<ItemFn>(item)?;
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
            return Err(syn::Error::new_spanned(
                function_argument,
                "Client connect/disconnect can only have one argument (identity: Hash)",
            ));
        }

        match function_argument {
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    function_argument,
                    "Receiver types in reducer parameters not supported!",
                ))
            }
            FnArg::Typed(typed) => {
                let arg_type = &typed.ty;
                let arg_token = arg_type.to_token_stream();
                let arg_type_str = arg_token.to_string();

                // First argument must be Hash (sender)
                if arg_num == 0 {
                    if arg_type_str != "spacetimedb::spacetimedb_lib::hash::Hash" && arg_type_str != "Hash" {
                        let error_str = format!(
                            "Parameter 1 of connect/disconnect {} must be of type \'Hash\'.",
                            func_name.to_string()
                        );
                        return Err(syn::Error::new_spanned(arg_type, error_str));
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
                        return Err(syn::Error::new_spanned(arg_type, error_str));
                    }
                    arg_num += 1;
                    continue;
                }
            }
        }

        arg_num = arg_num + 1;
    }

    let emission = quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        pub extern "C" fn #connect_disconnect_ident(arg_ptr: *const u8, arg_size: usize) {
            let arguments = unsafe { spacetimedb::spacetimedb_lib::args::ConnectDisconnectArguments::decode_mem(
                arg_ptr, arg_size).expect("Unable to decode module arguments") };

            // Invoke the function with the deserialized args
            #func_name(arguments.identity, arguments.timestamp,);
        }

        #original_function
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission.to_string());
    }

    Ok(emission)
}

pub(crate) fn rust_to_spacetimedb_ident(input_type: &str) -> Option<Ident> {
    return match input_type {
        // These are typically prefixed with spacetimedb::spacetimedb_lib::TypeDef::
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
    tuple_type: &Ident,
    filter_field_name: &Ident,
    is_unique: bool,
) -> proc_macro2::TokenStream {
    let err_string = format!(
        "Internal stdb error: Can't convert from tuple to struct (wrong version?) {}",
        tuple_type
    );

    let result_statement = if is_unique {
        quote! {
            let tuple = <Self as spacetimedb::FromTuple>::from_tuple(row);
            if tuple.is_none() {
                spacetimedb::println!(#err_string);
            }
            return tuple;
        }
    } else {
        quote! {
            let tuple = <Self as spacetimedb::FromTuple>::from_tuple(row);
            match tuple {
                Some(value) => result.push(value),
                None => {
                    spacetimedb::println!(#err_string);
                    continue;
                }
            }
        }
    };

    quote! {
        if spacetimedb::FilterableValue::equals(&#filter_field_name, &column_data) {
            #result_statement
        }
    }
}

fn assert_no_args_meta(meta: &Meta) -> syn::Result<()> {
    match meta {
        Meta::Path(_) => Ok(()),
        _ => Err(syn::Error::new_spanned(
            meta,
            format!(
                "#[spacetimedb({})] doesn't take any args",
                meta.path().get_ident().unwrap()
            ),
        )),
    }
}
fn assert_no_args(args: &[NestedMeta]) -> syn::Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            quote!(#(#args)*),
            "unexpected macro argument(s)",
        ))
    }
}
