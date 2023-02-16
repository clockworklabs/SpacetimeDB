#![crate_type = "proc-macro"]

mod module;

extern crate core;
extern crate proc_macro;

use std::time::Duration;

use crate::module::{autogen_module_struct_to_schema, derive_deserialize_struct, derive_serialize_struct};
use module::{derive_deserialize, derive_serialize};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
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
                "init" => spacetimedb_init(meta, other_args, item),
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

fn duration_totokens(dur: Duration) -> TokenStream {
    let (secs, nanos) = (dur.as_secs(), dur.subsec_nanos());
    quote!({
        const DUR: ::core::time::Duration = ::core::time::Duration::new(#secs, #nanos);
        DUR
    })
}

fn spacetimedb_reducer(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    let repeat_dur = if let Some((first, args)) = args.split_first() {
        let value = match first {
            NestedMeta::Meta(Meta::NameValue(p)) if p.path.is_ident("repeat") => &p.lit,
            _ => {
                return Err(syn::Error::new_spanned(
                    first,
                    r#"unknown argument. did you mean `repeat = "..."`?"#,
                ))
            }
        };
        let dur = parse_duration(value, "repeat argument")?;

        assert_no_args(args)?;

        ReducerExtra::Repeat(dur)
    } else {
        ReducerExtra::None
    };

    let original_function = syn::parse2::<ItemFn>(item)?;

    let reducer_name = original_function.sig.ident.to_string();
    if reducer_name == "__init__" {
        return Err(syn::Error::new_spanned(
            &original_function.sig.ident,
            "reserved reducer name",
        ));
    }

    gen_reducer(original_function, &reducer_name, repeat_dur)
}

fn spacetimedb_init(meta: &Meta, args: &[NestedMeta], item: TokenStream) -> syn::Result<TokenStream> {
    assert_no_args_meta(meta)?;
    assert_no_args(args)?;

    let original_function = syn::parse2::<ItemFn>(item)?;

    gen_reducer(original_function, "__init__", ReducerExtra::Init)
}

enum ReducerExtra {
    None,
    Repeat(Duration),
    Init,
}

fn gen_reducer(original_function: ItemFn, reducer_name: &str, extra: ReducerExtra) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    // let errmsg = "reducer should have at least 2 arguments: (identity: Hash, timestamp: u64, ...)";
    // let ([arg1, arg2], args) = validate_reducer_args(&original_function.sig, errmsg)?;

    // // TODO: better (non-string-based) validation for these
    // if !matches!(
    //     &*arg1.to_token_stream().to_string(),
    //     "spacetimedb::spacetimedb_lib::hash::Hash" | "Hash"
    // ) {
    //     return Err(syn::Error::new_spanned(
    //         &arg1,
    //         "1st parameter of a reducer must be of type \'u64\'.",
    //     ));
    // }
    // if arg2.to_token_stream().to_string() != "u64" {
    //     return Err(syn::Error::new_spanned(
    //         &arg2,
    //         "2nd parameter of a reducer must be of type \'u64\'.",
    //     ));
    // }

    let args = original_function.sig.inputs.iter().map(|x| match x {
        FnArg::Receiver(_) => panic!(),
        FnArg::Typed(x) => x,
    });

    let arg_names = args.clone().map(|arg| {
        if let syn::Pat::Ident(i) = &*arg.pat {
            let name = i.ident.to_string();
            quote!(Some(#name))
        } else {
            quote!(None)
        }
    });

    let arg_tys = args.map(|arg| &arg.ty);

    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let reducer_symbol = format!("__reducer__{reducer_name}");
    let descriptor_symbol = format!("__describe_reducer__{reducer_name}");

    let epilogue = match &extra {
        ReducerExtra::None => quote!(),
        ReducerExtra::Repeat(repeat_dur) => {
            let repeat_dur = duration_totokens(*repeat_dur);
            quote! {
                if _res.is_ok() {
                    spacetimedb::rt::schedule_repeater(#func_name, #reducer_name, #repeat_dur)
                }
            }
        }
        ReducerExtra::Init => quote!(),
    };

    let generated_function = quote! {
        #[export_name = #reducer_symbol]
        extern "C" fn __reducer(__sender: spacetimedb::sys::Buffer, __timestamp: u64, __args: spacetimedb::sys::Buffer) -> spacetimedb::sys::Buffer {
            #(spacetimedb::rt::assert_reducerarg::<#arg_tys>();)*
            #(spacetimedb::rt::assert_reducerret::<#ret_ty>();)*
            unsafe { spacetimedb::rt::invoke_reducer(#func_name, __sender, __timestamp, &spacetimedb::sys::Buffer::read(__args), |_res| { #epilogue }) }
        }
    };

    let generated_describe_function = quote! {
        #[export_name = #descriptor_symbol]
        pub extern "C" fn __descriptor() -> spacetimedb::sys::Buffer {
            let reducerdef = spacetimedb::rt::schema_of_func(#func_name, #reducer_name, &[#(#arg_names),*]);
            let mut bytes = vec![];
            reducerdef.encode(&mut bytes);
            spacetimedb::sys::Buffer::alloc(&bytes)
        }
    };

    let mut schedule_func_sig = original_function.sig.clone();
    let schedule_func_body = {
        schedule_func_sig.ident = format_ident!("schedule");
        schedule_func_sig.output = syn::ReturnType::Default;
        let arg_names = schedule_func_sig.inputs.iter_mut().enumerate().map(|(i, arg)| {
            let syn::FnArg::Typed(arg) = arg else { panic!() };
            match &mut *arg.pat {
                syn::Pat::Ident(id) => {
                    id.by_ref = None;
                    id.mutability = None;
                    id.ident.clone()
                }
                _ => {
                    let ident = format_ident!("__arg{}", i);
                    arg.pat = Box::new(syn::parse_quote!(#ident));
                    ident
                }
            }
        });
        let schedule_args = quote!((#(#arg_names,)*));
        let time_arg = format_ident!("__time");
        schedule_func_sig
            .inputs
            .insert(0, syn::parse_quote!(#time_arg: spacetimedb::Timestamp));
        quote! {
            spacetimedb::rt::schedule(#reducer_name, #time_arg, #schedule_args)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_function
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        impl #func_name {
            #vis #schedule_func_sig { #schedule_func_body }
        }
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
            static TABLE_ID: spacetimedb::rt::OnceCell<u32> = spacetimedb::rt::OnceCell::new();
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

    let db_iter = quote! {
        #[allow(unused_variables)]
        pub fn iter() -> spacetimedb::TableIter<Self> {
            <Self as spacetimedb::TableType>::iter()
        }
    };

    let spacetimedb_lib = quote!(spacetimedb::spacetimedb_lib);
    let deserialize_impl = derive_deserialize_struct(&original_struct, &spacetimedb_lib)?;
    let serialize_impl = derive_serialize_struct(&original_struct, &spacetimedb_lib)?;
    let schema_impl = autogen_module_struct_to_schema(&original_struct, &table_name)?;
    let tabletype_impl = quote! {
        impl spacetimedb::TableType for #original_struct_ident {
            const TABLE_NAME: &'static str = #table_name;
            const UNIQUE_COLUMNS: &'static [u8] = &[#(#unique_fields),*];
            #get_table_id_func
        }
    };

    let describe_table_symbol = format!("__describe_table__{table_name}");

    let describe_table_func = quote! {
        #[export_name = #describe_table_symbol]
        extern "C" fn __describe_table() -> spacetimedb::sys::Buffer {
            spacetimedb::rt::describe_table::<#original_struct_ident>()
        }
    };

    let field_names = original_struct.fields.iter().map(|f| f.ident.as_ref().unwrap());
    let field_types = original_struct.fields.iter().map(|f| &f.ty);
    let col_num = 0u8..;
    let field_access_impls = quote! {
        #(impl spacetimedb::query::FieldAccess<#col_num> for #original_struct_ident {
            type Field = #field_types;
            fn get_field(&self) -> &Self::Field {
                &self.#field_names
            }
        })*
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
            #(#non_primary_filter_func)*
        }

        #schema_impl
        #deserialize_impl
        #serialize_impl
        #tabletype_impl

        #field_access_impls
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
    let spacetimedb_lib = quote!(spacetimedb::spacetimedb_lib);
    let deserialize_impl = derive_deserialize_struct(&original_struct, &spacetimedb_lib)?;
    let serialize_impl = derive_serialize_struct(&original_struct, &spacetimedb_lib)?;

    let describe_typealias_symbol = format!("__describe_type_alias__{tuple_name}");

    let emission = quote! {
        #original_struct
        #schema_impl
        #deserialize_impl
        #serialize_impl

        const _: () = {
            #[export_name = #describe_typealias_symbol]
            extern "C" fn __describe_symbol() -> u32 {
                spacetimedb::rt::describe_reftype::<#original_struct_ident>()
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

    let emission = quote! {
        const _: () = {
            #[export_name = #connect_disconnect_symbol]
            extern "C" fn __connect_disconnect(__sender: spacetimedb::sys::Buffer, __timestamp: u64) -> spacetimedb::sys::Buffer {
                unsafe { spacetimedb::rt::invoke_connection_func(#func_name, __sender, __timestamp) }
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

#[proc_macro]
pub fn duration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = TokenStream::from(input);
    duration_impl(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn duration_impl(input: TokenStream) -> syn::Result<TokenStream> {
    let lit = syn::parse2::<syn::Lit>(input)?;
    let dur = parse_duration(&lit, "duration!() argument")?;
    Ok(duration_totokens(dur))
}

fn parse_duration(lit: &syn::Lit, ctx: &str) -> syn::Result<Duration> {
    let s = match lit {
        syn::Lit::Str(s) => s.value(),
        syn::Lit::Int(i) => i.to_string(),
        _ => {
            return Err(syn::Error::new_spanned(
                lit,
                format_args!("{ctx} must be a string or an int with a suffix"),
            ))
        }
    };

    parse_duration::parse(&s)
        .map_err(|e| syn::Error::new_spanned(lit, format_args!("Can't parse {ctx} as duration: {e}")))
}

fn find_crate(attrs: &[syn::Attribute]) -> TokenStream {
    for attr in attrs {
        if !attr.path.is_ident("sats") {
            continue;
        }
        let Ok(Meta::List(l)) = attr.parse_meta() else { continue };
        for meta in l.nested {
            match meta {
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("crate") => {
                    let syn::Lit::Str(s) = nv.lit else { continue };
                    return s.parse().unwrap();
                }
                _ => {}
            }
        }
    }
    quote!(spacetimedb_lib)
}

#[proc_macro_derive(Deserialize, attributes(sats))]
pub fn deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    let krate = find_crate(&input.attrs);
    derive_deserialize(&input, &krate)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Serialize, attributes(sats))]
pub fn serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    let krate = find_crate(&input.attrs);
    derive_serialize(&input, &krate)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
