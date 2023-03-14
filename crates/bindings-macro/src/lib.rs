#![crate_type = "proc-macro"]

#[macro_use]
mod macros;

mod module;

extern crate core;
extern crate proc_macro;

use std::time::Duration;

use module::{derive_deserialize, derive_serialize, derive_spacetimetype};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::Fields::{Named, Unit, Unnamed};
use syn::{FnArg, Ident, ItemFn, ItemStruct, Token};

#[proc_macro_attribute]
pub fn spacetimedb(macro_args: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item: TokenStream = item.into();
    let orig_input = item.clone();
    let input = syn::parse::<MacroInput>(macro_args);

    let res = input.and_then(|input| match input {
        MacroInput::Table => spacetimedb_table(item),
        MacroInput::Init => spacetimedb_init(item),
        MacroInput::Reducer { repeat } => spacetimedb_reducer(repeat, item),
        MacroInput::Connect => spacetimedb_connect_disconnect(item, true),
        MacroInput::Disconnect => spacetimedb_connect_disconnect(item, false),
        MacroInput::Migrate => spacetimedb_migrate(item),
        MacroInput::Index { ty, name, field_names } => spacetimedb_index(ty, name, field_names, item),
    });

    res.unwrap_or_else(|x| {
        let mut out = orig_input;
        out.extend(x.into_compile_error());
        out
    })
    .into()
}

fn duration_totokens(dur: Duration) -> TokenStream {
    let (secs, nanos) = (dur.as_secs(), dur.subsec_nanos());
    quote!({
        const DUR: ::core::time::Duration = ::core::time::Duration::new(#secs, #nanos);
        DUR
    })
}

enum MacroInput {
    Table,
    Init,
    Reducer {
        repeat: Option<Duration>,
    },
    Connect,
    Disconnect,
    Migrate,
    Index {
        ty: IndexType,
        name: Option<String>,
        field_names: Vec<Ident>,
    },
}

fn comma_delimited(input: syn::parse::ParseStream, mut f: impl FnMut() -> syn::Result<()>) -> syn::Result<()> {
    loop {
        if input.is_empty() {
            break;
        }
        f()?;
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(())
}
fn comma_delim_parser(
    mut f: impl FnMut(syn::parse::ParseStream) -> syn::Result<()>,
) -> impl syn::parse::Parser<Output = ()> {
    move |input: syn::parse::ParseStream| comma_delimited(input, || f(input))
}
fn comma_then_comma_delimited(
    input: syn::parse::ParseStream,
    mut f: impl FnMut() -> syn::Result<()>,
) -> syn::Result<()> {
    loop {
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
        f()?;
    }
    Ok(())
}

fn check_duplicate<T>(x: &Option<T>, span: Span) -> syn::Result<()> {
    if x.is_none() {
        Ok(())
    } else {
        Err(syn::Error::new(span, "duplicate attribute"))
    }
}

impl syn::parse::Parse for MacroInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(match_tok!(match input {
            kw::table => Self::Table,
            kw::init => Self::Init,
            kw::reducer => {
                let mut repeat = None;
                comma_then_comma_delimited(input, || {
                    match_tok!(match input {
                        tok @ kw::repeat => {
                            check_duplicate(&repeat, tok.span)?;
                            input.parse::<Token![=]>()?;
                            let v = input.call(parse_duration)?;
                            repeat = Some(v);
                        }
                    });
                    Ok(())
                })?;
                Self::Reducer { repeat }
            }
            kw::connect => Self::Connect,
            kw::disconnect => Self::Disconnect,
            kw::migrate => Self::Migrate,
            kw::index => {
                let in_parens;
                syn::parenthesized!(in_parens in input);
                let ty: IndexType = in_parens.parse()?;

                let mut name = None;
                let mut field_names = Vec::new();
                comma_then_comma_delimited(input, || {
                    match_tok!(match input {
                        (tok, _) @ (kw::name, Token![=]) => {
                            check_duplicate(&name, tok.span)?;
                            let v = input.parse::<syn::LitStr>()?;
                            name = Some(v.value())
                        }
                        ident @ Ident => field_names.push(ident),
                    });
                    Ok(())
                })?;
                Self::Index { ty, name, field_names }
            }
        }))
    }
}

#[derive(Debug)]
enum IndexType {
    BTree,
    Hash,
}

impl syn::parse::Parse for IndexType {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(match_tok!(match input {
            kw::btree => Self::BTree,
            kw::hash => Self::Hash,
        }))
    }
}

mod kw {
    syn::custom_keyword!(table);
    syn::custom_keyword!(init);
    syn::custom_keyword!(reducer);
    syn::custom_keyword!(connect);
    syn::custom_keyword!(disconnect);
    syn::custom_keyword!(migrate);
    syn::custom_keyword!(index);
    syn::custom_keyword!(btree);
    syn::custom_keyword!(hash);
    syn::custom_keyword!(name);
    syn::custom_keyword!(repeat);
}

fn spacetimedb_reducer(repeat: Option<Duration>, item: TokenStream) -> syn::Result<TokenStream> {
    let repeat_dur = repeat.map_or(ReducerExtra::None, ReducerExtra::Repeat);

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

fn spacetimedb_init(item: TokenStream) -> syn::Result<TokenStream> {
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

    // let errmsg = "reducer should have at least 2 arguments: (identity: Identity, timestamp: u64, ...)";
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
    index: u8,
    field: &'a module::SatsField<'a>,
    attr: ColumnIndexAttribute,
}

#[derive(Debug)]
enum ColumnIndexAttribute {
    UnSet = 0,
    /// Unique + AutoInc
    Identity = 1,
    /// Index unique
    Unique = 2,
    ///  Index no unique
    #[allow(unused)]
    Indexed = 3,
    /// Generate the next [Sequence]
    AutoInc = 4,
}

fn spacetimedb_table(item: TokenStream) -> syn::Result<TokenStream> {
    let original_struct = syn::parse2::<ItemStruct>(item)?;

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

    let data = module::extract_sats_struct(&original_struct.fields)?;
    let sats_ty = module::extract_sats_type(
        &original_struct.ident,
        &original_struct.generics,
        &original_struct.attrs,
        data,
        quote!(spacetimedb::spacetimedb_lib),
    )?;

    let original_struct_ident = &original_struct.ident;
    let table_name = &sats_ty.name;
    let module::SatsTypeData::Product(fields) = &sats_ty.data else { unreachable!() };

    let mut columns = Vec::<Column>::new();

    let get_table_id_func = quote! {
        fn table_id() -> u32 {
            static TABLE_ID: spacetimedb::rt::OnceCell<u32> = spacetimedb::rt::OnceCell::new();
            *TABLE_ID.get_or_init(|| {
                spacetimedb::get_table_id(<Self as spacetimedb::TableType>::TABLE_NAME)
            })
        }
    };

    for (i, field) in fields.iter().enumerate() {
        let col_num: u8 = i
            .try_into()
            .map_err(|_| syn::Error::new_spanned(field.ident, "too many columns; the most a table can have is 256"))?;

        let mut col_attr = ColumnIndexAttribute::UnSet;
        for attr in field.original_attrs {
            let Some(ident) = attr.path.get_ident() else { continue };
            let duplicate = || syn::Error::new(ident.span(), "duplicate attribute");
            use ColumnIndexAttribute::*;
            match &*ident.to_string() {
                "unique" => match col_attr {
                    UnSet => col_attr = Unique,
                    Identity | Unique => return Err(duplicate()),
                    Indexed => unreachable!(),
                    AutoInc => col_attr = Identity,
                },
                "autoinc" => match col_attr {
                    UnSet => col_attr = AutoInc,
                    Identity | AutoInc => return Err(duplicate()),
                    Unique => col_attr = Identity,
                    Indexed => unreachable!(),
                },
                _ => {}
            }
        }
        let column = Column {
            index: col_num,
            field,
            attr: col_attr,
        };

        columns.push(column);
    }

    let (unique_columns, nonunique_columns): (Vec<_>, Vec<_>) = columns
        .iter()
        .partition(|x| matches!(x.attr, ColumnIndexAttribute::Identity | ColumnIndexAttribute::Unique));

    let mut unique_filter_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_update_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_delete_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_fields = Vec::with_capacity(unique_columns.len());
    for unique in unique_columns {
        let column_index = unique.index;
        let vis = unique.field.vis;
        let column_type = unique.field.ty;
        let column_ident = unique.field.ident.unwrap();

        let filter_func_ident = format_ident!("filter_by_{}", column_ident);
        let update_func_ident = format_ident!("update_by_{}", column_ident);
        let delete_func_ident = format_ident!("delete_by_{}", column_ident);

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
        let vis = column.field.vis;
        let column_ident = column.field.ident.unwrap();
        let column_type = column.field.ty;
        let column_index = column.index;

        let filter_func_ident = format_ident!("filter_by_{}", column_ident);

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

    let deserialize_impl = derive_deserialize(&sats_ty);
    let serialize_impl = derive_serialize(&sats_ty);
    let schema_impl = derive_spacetimetype(&sats_ty);
    let column_attrs = columns
        .iter()
        .map(|col| Ident::new(&format!("{:?}", col.attr), Span::call_site()));
    let tabletype_impl = quote! {
        impl spacetimedb::TableType for #original_struct_ident {
            const TABLE_NAME: &'static str = #table_name;
            const COLUMN_ATTRS: &'static [spacetimedb::spacetimedb_lib::ColumnIndexAttribute] = &[
                #(spacetimedb::spacetimedb_lib::ColumnIndexAttribute::#column_attrs),*
            ];
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

    let field_names = fields.iter().map(|f| f.ident.unwrap());
    let field_types = fields.iter().map(|f| f.ty);
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

        #[derive(spacetimedb::__TableHelper)]
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

#[proc_macro_derive(TableHelper, attributes(sats, unique, autoinc))]
pub fn table_attr_helper(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

fn spacetimedb_index(
    index_type: IndexType,
    index_name: Option<String>,
    field_names: Vec<Ident>,
    item: TokenStream,
) -> syn::Result<TokenStream> {
    let original_struct = syn::parse2::<ItemStruct>(item)?;

    let index_fields = field_names
        .iter()
        .map(|field_name| {
            original_struct
                .fields
                .iter()
                .position(|field| field.ident.as_ref().unwrap() == field_name)
                .ok_or_else(|| syn::Error::new(field_name.span(), "not a field of the struct"))
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let index_name = index_name.as_deref().unwrap_or("default_index");

    let original_struct_name = &original_struct.ident;
    let table_name = original_struct_name.to_string();
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
        };
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        println!("{}", output);
    }

    Ok(output)
}

fn spacetimedb_migrate(item: TokenStream) -> syn::Result<TokenStream> {
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

fn spacetimedb_connect_disconnect(item: TokenStream, connect: bool) -> syn::Result<TokenStream> {
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

#[proc_macro]
pub fn duration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let dur = syn::parse_macro_input!(input with parse_duration);
    duration_totokens(dur).into()
}

fn parse_duration(input: syn::parse::ParseStream) -> syn::Result<Duration> {
    let (s, span) = match_tok!(match input {
        s @ syn::LitStr => (s.value(), s.span()),
        i @ syn::LitInt => (i.to_string(), i.span()),
    });
    humantime::parse_duration(&s).map_err(|e| syn::Error::new(span, format_args!("can't parse as duration: {e}")))
}

#[proc_macro_derive(Deserialize, attributes(sats))]
pub fn deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    module::sats_type_from_derive(&input, quote!(spacetimedb_lib))
        .map(|ty| derive_deserialize(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Serialize, attributes(sats))]
pub fn serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    module::sats_type_from_derive(&input, quote!(spacetimedb_lib))
        .map(|ty| derive_serialize(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(SpacetimeType, attributes(sats))]
pub fn schema_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    (|| {
        let ty = module::sats_type_from_derive(&input, quote!(spacetimedb::spacetimedb_lib))?;

        let ident = ty.ident;

        let schema_impl = derive_spacetimetype(&ty);
        let deserialize_impl = derive_deserialize(&ty);
        let serialize_impl = derive_serialize(&ty);

        let describe_typealias_symbol = format!("__describe_type_alias__{}", ty.name);

        let emission = quote! {
            #schema_impl
            #deserialize_impl
            #serialize_impl

            const _: () = {
                #[export_name = #describe_typealias_symbol]
                extern "C" fn __describe_symbol() -> u32 {
                    spacetimedb::rt::describe_reftype::<#ident>()
                }
            };
        };

        if std::env::var("PROC_MACRO_DEBUG").is_ok() {
            println!("{}", emission);
        }

        Ok(emission)
    })()
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}
