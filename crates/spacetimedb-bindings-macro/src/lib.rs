#![crate_type = "proc-macro"]

mod module;

extern crate core;
extern crate proc_macro;

use crate::module::{autogen_module_struct_to_schema, autogen_module_struct_to_tuple, autogen_module_tuple_to_struct};
use module::type_to_tuple_schema;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, ToTokens, TokenStreamExt};
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
                "table" => spacetimedb_table(meta, other_args, item),
                "reducer" => spacetimedb_reducer(meta, other_args, item),
                "connect" => spacetimedb_connect_disconnect(meta, other_args, item, true),
                "disconnect" => spacetimedb_connect_disconnect(meta, other_args, item, false),
                "migrate" => spacetimedb_migrate(meta, other_args, item),
                "tuple" => spacetimedb_tuple(meta, other_args, item),
                "index" => spacetimedb_index(meta, other_args, item),
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

fn validate_reducer_args<'a>(
    sig: &'a syn::Signature,
    noctxargs: &str,
) -> syn::Result<(
    [&'a syn::Type; 2],
    impl ExactSizeIterator<Item = &'a syn::PatType> + Clone,
)> {
    let func_inputs = &sig.inputs;
    let (mut ctx_args, args) = (func_inputs.iter().take(2), func_inputs.iter().skip(2));

    let mut ctx_arg = || {
        let arg = ctx_args
            .next()
            .ok_or_else(|| syn::Error::new_spanned(func_inputs, noctxargs))?;
        match arg {
            FnArg::Receiver(_) => Err(syn::Error::new_spanned(
                arg,
                "self in reducer parameters is not supported!",
            )),
            FnArg::Typed(x) => Ok(&*x.ty),
        }
    };

    let ctx_args = [ctx_arg()?, ctx_arg()?];

    let args = args.map(|x| match x {
        // self can only be the first arg of a function anyway
        FnArg::Receiver(_) => unreachable!(),
        FnArg::Typed(x) => x,
    });
    Ok((ctx_args, args))
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

    let reducer_name = func_name.to_string();

    let errmsg = "reducer should have at least 2 arguments: (identity: Hash, timestamp: u64, ...)";
    let ([arg1, arg2], args) = validate_reducer_args(&original_function.sig, errmsg)?;

    // TODO: better (non-string-based) validation for these
    if !matches!(
        &*arg1.to_token_stream().to_string(),
        "spacetimedb::spacetimedb_lib::hash::Hash" | "Hash"
    ) {
        return Err(syn::Error::new_spanned(
            &arg1,
            "1st parameter of a reducer must be of type \'u64\'.",
        ));
    }
    if arg2.to_token_stream().to_string() != "u64" {
        return Err(syn::Error::new_spanned(
            &arg2,
            "2nd parameter of a reducer must be of type \'u64\'.",
        ));
    }

    let num_args = args.len();

    let args_schemas = args.clone().enumerate().map(|(col_num, arg)| {
        let arg_name = if let syn::Pat::Ident(i) = &*arg.pat {
            Some(i.ident.to_string())
        } else {
            None
        };
        type_to_tuple_schema(arg_name, col_num.try_into().unwrap(), &arg.ty)
    });

    let value_varnames = (0..num_args).map(|i| format_ident!("value_{}", i));
    let value_varnames2 = value_varnames.clone();
    let arg_tys = args.map(|arg| &arg.ty);
    let get_args = quote! {
        core::convert::TryFrom::try_from(arguments.arguments.elements).ok().and_then(|args: Box<[_; #num_args]>| {
            let [#(#value_varnames),*] = *args;
            Some((#(<#arg_tys as spacetimedb::FromValue>::from_value(#value_varnames2)?,)*))
        })
        .unwrap_or_else(|| panic!("bad reducer arguments for {}", #reducer_name))
    };

    let arg_indices = (0..num_args).map(syn::Index::from);

    let reducer_symbol = format!("__reducer__{reducer_name}");
    let descriptor_symbol = format!("__describe_reducer__{reducer_name}");

    let generated_function = quote! {
        #[export_name = #reducer_symbol]
        extern "C" fn __reducer(__arg_ptr: *mut u8, __arg_size: usize) {
            let (__identity, __timestamp, __args) = {
                let bytes = unsafe { std::boxed::Box::from_raw(std::ptr::slice_from_raw_parts_mut(__arg_ptr, __arg_size)) };
                let mut rdr = &bytes[..];
                let arguments =
                    spacetimedb::spacetimedb_lib::args::ReducerArguments::decode(&mut rdr, &__REDUCERDEF).expect("Unable to decode module arguments");

                (arguments.identity, arguments.timestamp, #get_args)
            };

            // Invoke the function with the deserialized args
            #func_name(__identity, __timestamp, #(__args.#arg_indices),*);
        }
    };

    let reducerdef_static = quote! {
        static __REDUCERDEF: spacetimedb::__private::Lazy<spacetimedb::spacetimedb_lib::ReducerDef> =
            spacetimedb::__private::Lazy::new(|| {
                spacetimedb::spacetimedb_lib::ReducerDef {
                    name: Some(#reducer_name.into()),
                    args: vec![
                        #(#args_schemas),*
                    ],
                }
            });
    };

    let generated_describe_function = quote! {
        #[export_name = #descriptor_symbol]
        pub extern "C" fn __descriptor() -> u64 {
            let reducerdef = &*__REDUCERDEF;
            let mut bytes = vec![];
            reducerdef.encode(&mut bytes);
            spacetimedb::sys::pack_slice(bytes.into())
        }
    };

    Ok(quote! {
        const _: () = {
            #reducerdef_static
            #generated_function
            #generated_describe_function
        };
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
    let reducer_name = func_name.to_string();

    let errmsg = "repeating reducers should have exactly 2 arguments: (timestamp: u64, delta_time: u64)";
    let ([arg1, arg2], others) = validate_reducer_args(&original_function.sig, errmsg)?;
    if others.len() != 0 {
        let mut tokens = TokenStream::new();
        tokens.append_all(others);
        let errmsg = "repeating reducers should have exactly 2 arguments";
        return Err(syn::Error::new_spanned(tokens, errmsg));
    }

    // TODO: better (non-string-based) validation for these
    if arg1.to_token_stream().to_string() != "u64" {
        let error_str = "1st parameter of a repeating reducer must be of type \'u64\'.";
        return Err(syn::Error::new_spanned(arg1, error_str));
    }

    // Second argument must be an u64 (delta_time)
    if arg2.to_token_stream().to_string() != "u64" {
        let error_str = "2nd parameter of a repeating reducer must be of type \'u64\'.";
        return Err(syn::Error::new_spanned(arg2, error_str));
    }

    let duration_as_millis = repeat_duration.as_millis() as u64;

    let reducer_symbol = format!("__repeating_reducer__{reducer_name}");
    let descriptor_symbol = format!("__describe_repeating_reducer__{reducer_name}");

    let generated_function = quote! {
        #[export_name = #descriptor_symbol]
        pub extern "C" fn __descriptor() -> u64 {
            let tupledef = spacetimedb::spacetimedb_lib::RepeaterDef {
                name: Some(#reducer_name.into()),
            };
            let mut bytes = vec![];
            tupledef.encode(&mut bytes);
            spacetimedb::sys::pack_slice(bytes.into())
        }

        #[export_name = #reducer_symbol]
        extern "C" fn __reducer(__arg_ptr: *mut u8, __arg_size: usize) -> u64 {
            let __args = {
                let bytes = unsafe { std::boxed::Box::from_raw(std::ptr::slice_from_raw_parts_mut(__arg_ptr, __arg_size)) };
                // Deserialize the arguments
                spacetimedb::spacetimedb_lib::args::RepeatingReducerArguments::decode(&mut &bytes[..]).expect("Unable to decode module arguments")
            };

            // Invoke the function with the deserialized args
            #func_name(__args.timestamp, __args.delta_time);

            #duration_as_millis
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_function
        };
        #original_function
    })
}

// TODO: We actually need to add a constraint that requires this column to be unique!
struct Column<'a> {
    vis: &'a syn::Visibility,
    ty: &'a syn::Type,
    ident: &'a Ident,
    index: u8,
}

fn table_name(struc: &ItemStruct) -> String {
    struc.ident.to_string()
}

fn spacetimedb_table(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    assert_no_args(args)?;

    let mut original_struct = syn::parse2::<ItemStruct>(item)?;
    let original_struct_ident = &original_struct.ident;

    let table_name = table_name(&original_struct);

    match &original_struct.fields {
        Named(_) => {}
        Unnamed(_) => {
            return Err(syn::Error::new_spanned(
                &original_struct.fields,
                "spacetimedb tables must have named fields.",
            ));
        }
        Unit => {
            return Err(syn::Error::new_spanned(
                &original_struct.fields,
                "spacetimedb tables must have named fields (unit struct forbidden).",
            ));
        }
    }

    let mut unique_columns = Vec::<Column>::new();
    let mut nonunique_columns = Vec::<Column>::new();

    let get_table_id_func = quote! {
        fn table_id() -> u32 {
            static TABLE_ID: spacetimedb::__private::OnceCell<u32> = spacetimedb::__private::OnceCell::new();
            *TABLE_ID.get_or_init(|| {
                spacetimedb::get_table_id(<Self as spacetimedb::TableType>::TABLE_NAME)
            })
        }
    };

    for (col_num, field) in original_struct.fields.iter_mut().enumerate() {
        let col_num: u8 = col_num
            .try_into()
            .map_err(|_| syn::Error::new_spanned(&field, "too many columns; the most a table can have is 256"))?;
        let col_name = field.ident.as_ref().unwrap();

        let mut is_unique = false;
        let mut remove_idxs = vec![];
        for (i, attr) in field.attrs.iter().enumerate() {
            if attr.path.is_ident("unique") {
                is_unique = true;
                remove_idxs.push(i);
            }
        }
        for i in remove_idxs.into_iter().rev() {
            field.attrs.remove(i);
        }
        let column = Column {
            vis: &field.vis,
            ty: &field.ty,
            ident: col_name,
            index: col_num,
        };

        if is_unique {
            unique_columns.push(column);
        } else {
            nonunique_columns.push(column);
        }
    }

    let mut unique_filter_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_update_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_delete_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_fields = Vec::with_capacity(unique_columns.len());
    for unique in unique_columns {
        let filter_func_ident = format_ident!("filter_by_{}", unique.ident);
        let update_func_ident = format_ident!("update_by_{}", unique.ident);
        let delete_func_ident = format_ident!("delete_by_{}", unique.ident);

        let Column {
            vis,
            ty: column_type,
            ident: column_ident,
            index: column_index,
        } = unique;

        unique_fields.push(column_index);

        unique_filter_funcs.push(quote! {
            #vis fn #filter_func_ident(#column_ident: #column_type) -> Option<Self> {
                spacetimedb::query::filter_by_unique_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });

        unique_update_funcs.push(quote! {
            #vis fn #update_func_ident(#column_ident: #column_type, value: Self) -> bool {
                spacetimedb::query::update_by_field::<Self, #column_type, #column_index>(#column_ident, value)
            }
        });

        unique_delete_funcs.push(quote! {
            #vis fn #delete_func_ident(#column_ident: #column_type) -> bool {
                spacetimedb::query::delete_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });
    }

    let non_primary_filter_func = nonunique_columns.into_iter().filter_map(|column| {
        let filter_func_ident = format_ident!("filter_by_{}", column.ident);

        let vis = column.vis;
        let column_ident = column.ident;
        let column_type = column.ty;
        let column_index = column.index;

        let skip = if let syn::Type::Path(p) = column_type {
            // TODO: this is janky as heck
            !matches!(
                &*p.path.segments.last().unwrap().ident.to_string(),
                "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "Hash"
            )
        } else {
            true
        };

        if skip {
            return None;
        }

        Some(quote! {
            // TODO: should we expose spacetimedb::query::FilterByIter ?
            #vis fn #filter_func_ident(#column_ident: #column_type) -> impl Iterator<Item = Self> {
                spacetimedb::query::filter_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
        })
    });
    let non_primary_filter_func = non_primary_filter_func.collect::<Vec<_>>();

    let db_insert = quote! {
        #[allow(unused_variables)]
        pub fn insert(ins: #original_struct_ident) {
            <Self as spacetimedb::TableType>::insert(ins)
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
        pub fn iter_tuples() -> spacetimedb::RawTableIter {
            <Self as spacetimedb::TableType>::iter_tuples()
        }
    };

    let db_iter = quote! {
        #[allow(unused_variables)]
        pub fn iter() -> spacetimedb::TableIter<Self> {
            <Self as spacetimedb::TableType>::iter()
        }
    };

    let from_value_impl = autogen_module_tuple_to_struct(&original_struct)?;
    let into_value_impl = autogen_module_struct_to_tuple(&original_struct)?;
    let schema_impl = autogen_module_struct_to_schema(&original_struct, &table_name)?;
    let tabletype_impl = quote! {
        impl spacetimedb::TableType for #original_struct_ident {
            const TABLE_NAME: &'static str = #table_name;
            const UNIQUE_COLUMNS: &'static [u8] = &[#(#unique_fields),*];
            #[inline]
            fn __tabledef_cell() -> &'static spacetimedb::__private::OnceCell<spacetimedb::spacetimedb_lib::TableDef> {
                static TABLEDEF: spacetimedb::__private::OnceCell<spacetimedb::spacetimedb_lib::TableDef> = spacetimedb::__private::OnceCell::new();
                &TABLEDEF
            }
            #get_table_id_func
        }
    };

    let describe_table_symbol = format!("__describe_table__{table_name}");

    let describe_table_func = quote! {
        #[export_name = #describe_table_symbol]
        extern "C" fn __describe_table() -> u64 {
            spacetimedb::sys::pack_slice(
                <#original_struct_ident as spacetimedb::TableType>::describe_table()
            )
        }
    };

    // Output all macro data
    let emission = quote! {
        const _: () = {
            #describe_table_func
        };

        #original_struct

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
        }

        #schema_impl
        #from_value_impl
        #into_value_impl
        #tabletype_impl
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission);
    }

    Ok(emission)
}

fn spacetimedb_index(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    let mut index_fields = Vec::<u32>::new();
    #[derive(Debug)]
    enum IndexType {
        BTree,
        Hash,
    }

    let generic_err = "index() must have index type passed; try index(btree) or index(hash)";
    let index_type = match meta {
        Meta::List(l) => match l.nested.len() {
            0 => return Err(syn::Error::new_spanned(meta, generic_err)),
            1 => {
                let err = || syn::Error::new_spanned(&l.nested[0], "index() only accepts `btree` or `hash`");
                match &l.nested[0] {
                    NestedMeta::Meta(Meta::Path(p)) => {
                        if p.is_ident("btree") {
                            IndexType::BTree
                        } else if p.is_ident("hash") {
                            IndexType::Hash
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
    };

    let original_struct = syn::parse2::<ItemStruct>(item)?;

    let mut index_name = None;
    for arg in args {
        match arg {
            NestedMeta::Meta(Meta::NameValue(nv)) => {
                if nv.path.is_ident("name") {
                    if index_name.is_some() {
                        return Err(syn::Error::new_spanned(nv, "can only define name once"));
                    }
                    if let syn::Lit::Str(s) = &nv.lit {
                        index_name = Some(s.value())
                    } else {
                        return Err(syn::Error::new_spanned(&nv.lit, "name must be a string"));
                    }
                }
            }
            NestedMeta::Meta(Meta::Path(p)) => {
                let field_name = p
                    .get_ident()
                    .ok_or_else(|| syn::Error::new_spanned(p, "field name must be single ident"))?;
                let i = original_struct
                    .fields
                    .iter()
                    .position(|field| field.ident.as_ref().unwrap() == field_name)
                    .ok_or_else(|| syn::Error::new_spanned(field_name, "not a field of the struct"))?;
                index_fields.push(i.try_into().unwrap());
            }
            _ => return Err(syn::Error::new_spanned(arg, "unknown arg for index")),
        }
    }
    let index_name = index_name.as_deref().unwrap_or("default_index");

    let original_struct_name = &original_struct.ident;
    let table_name = table_name(&original_struct);
    let function_symbol = format!("__create_index__{}__{}", table_name, index_name);

    let index_type = format_ident!("{}", format!("{:?}", index_type));
    let output = quote! {
        #original_struct

        const _: () = {
            #[export_name = #function_symbol]
            extern "C" fn __create_index(__arg_ptr: u32, __arg_size: u32) {
                spacetimedb::create_index(
                    <#original_struct_name as spacetimedb::TableType>::table_id(),
                    spacetimedb::IndexType::#index_type,
                    vec!(#(#index_fields),*)
                );
            }
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", output);
    }

    Ok(output)
}

fn spacetimedb_tuple(meta: &Meta, _: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    let original_struct = syn::parse2::<ItemStruct>(item)?;
    let original_struct_ident = original_struct.clone().ident;
    let tuple_name = original_struct_ident.to_string();

    match original_struct.fields {
        Named(_) => {}
        Unnamed(_) => {
            return Err(syn::Error::new_spanned(
                &original_struct.fields,
                "spacetimedb tables and types must have named fields.",
            ));
        }
        Unit => {
            return Err(syn::Error::new_spanned(
                &original_struct.fields,
                "Unit structure not supported.",
            ));
        }
    }

    let schema_impl = autogen_module_struct_to_schema(&original_struct, &tuple_name)?;
    let from_value_impl = autogen_module_tuple_to_struct(&original_struct)?;
    let into_value_impl = autogen_module_struct_to_tuple(&original_struct)?;

    let describe_tuple_symbol = format!("__describe_tuple__{tuple_name}");

    let emission = quote! {
        #original_struct
        #schema_impl
        #from_value_impl
        #into_value_impl

        const _: () = {
            #[export_name = #describe_tuple_symbol]
            extern "C" fn __describe_symbol() -> u64 {
                spacetimedb::sys::pack_slice(
                    <#original_struct_ident as spacetimedb::TupleType>::describe_tuple()
                )
            }
        };
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission);
    }

    Ok(emission)
}

fn spacetimedb_migrate(meta: &Meta, _: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    let original_func = syn::parse2::<ItemFn>(item)?;
    let func_name = &original_func.sig.ident;

    let emission = quote! {
        #[allow(non_snake_case)]
        pub extern "C" fn __migrate__(__arg_ptr: u32, __arg_size: u32) {
            #func_name();
        }
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission);
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
    let connect_disconnect_symbol = if connect {
        "__identity_connected__"
    } else {
        "__identity_disconnected__"
    };

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
                            func_name
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
                            func_name
                        );
                        return Err(syn::Error::new_spanned(arg_type, error_str));
                    }
                    arg_num += 1;
                    continue;
                }
            }
        }

        arg_num += 1;
    }

    let emission = quote! {
        const _: () = {
            #[export_name = #connect_disconnect_symbol]
            extern "C" fn __connect_disconnect(__arg_ptr: *mut u8, __arg_size: usize) {
                let __args = {
                    let bytes = unsafe { std::boxed::Box::from_raw(std::ptr::slice_from_raw_parts_mut(__arg_ptr, __arg_size)) };
                    spacetimedb::spacetimedb_lib::args::ConnectDisconnectArguments::decode(&mut &bytes[..]).expect("Unable to decode module arguments")
                };

                // Invoke the function with the deserialized args
                #func_name(__args.identity, __args.timestamp,);
            }
        };

        #original_function
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", emission);
    }

    Ok(emission)
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
