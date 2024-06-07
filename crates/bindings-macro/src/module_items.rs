use bitflags::Flags;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, TokenStreamExt};
use spacetimedb_primitives::ColumnAttribute;
use std::collections::HashMap;
use syn::meta::ParseNestedMeta;
use syn::parse::{Nothing, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{FnArg, Ident, ItemFn, LitStr, Path, Token, TypePath};

use crate::util::{check_duplicate, check_duplicate_msg};
use crate::{sats, sym};

#[derive(Debug)]
enum IndexType {
    BTree,
    Hash,
}

impl quote::ToTokens for IndexType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(Ident::new(&format!("{self:?}"), Span::call_site()))
    }
}

pub(crate) fn reducer_impl(_args: Nothing, original_function: ItemFn) -> syn::Result<TokenStream> {
    // Extract reducer name, making sure it's not `__XXX__` as that's the form we reserve for special reducers.
    let reducer_name = original_function.sig.ident.to_string();
    if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
        return Err(syn::Error::new_spanned(
            &original_function.sig.ident,
            "reserved reducer name",
        ));
    }

    gen_reducer(original_function, &reducer_name, ReducerExtra::Schedule)
}

pub(crate) fn special_reducer(reducer_name: &'static str) -> impl Fn(Nothing, ItemFn) -> syn::Result<TokenStream> {
    |Nothing, original_function| gen_reducer(original_function, reducer_name, ReducerExtra::None)
}

enum ReducerExtra {
    None,
    Schedule,
}

fn gen_reducer(original_function: ItemFn, reducer_name: &str, extra: ReducerExtra) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    // let errmsg = "reducer should have at least 2 arguments: (identity: Identity, timestamp: u64, ...)";
    // let ([arg1, arg2], args) = validate_reducer_args(&original_function.sig, errmsg)?;

    // // TODO: better (non-string-based) validation for these
    // if !matches!(
    //     &*arg1.to_token_stream().to_string(),
    //     "spacetimedb::spacetimedb_sats::hash::Hash" | "Hash"
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

    // Extract all function parameters, except for `self` ones that aren't allowed.
    let typed_args = original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            _ => Err(syn::Error::new_spanned(arg, "expected typed argument")),
        })
        .collect::<syn::Result<Vec<_>>>()?;

    // Extract all function parameter names.
    let opt_arg_names = typed_args.iter().map(|arg| {
        if let syn::Pat::Ident(i) = &*arg.pat {
            let name = i.ident.to_string();
            quote!(Some(#name))
        } else {
            quote!(None)
        }
    });

    let arg_tys = typed_args.iter().map(|arg| arg.ty.as_ref()).collect::<Vec<_>>();

    // Extract the return type.
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let register_describer_symbol = format!("__preinit__20_register_describer_{reducer_name}");

    let mut extra_impls = TokenStream::new();

    if !matches!(extra, ReducerExtra::None) {
        let arg_names = typed_args
            .iter()
            .enumerate()
            .map(|(i, arg)| match &*arg.pat {
                syn::Pat::Ident(pat) => pat.ident.clone(),
                _ => format_ident!("__arg{}", i),
            })
            .collect::<Vec<_>>();

        extra_impls.extend(quote!(impl #func_name {
            pub fn schedule(__time: spacetimedb::Timestamp #(, #arg_names: #arg_tys)*) -> spacetimedb::ScheduleToken<#func_name> {
                spacetimedb::rt::schedule(__time, (#(#arg_names,)*))
            }
        }));
    }

    let generated_function = quote! {
        fn __reducer(
            __sender: spacetimedb::sys::Buffer,
            __caller_address: spacetimedb::sys::Buffer,
            __timestamp: u64,
            __args: &[u8]
        ) -> spacetimedb::sys::Buffer {
            #(spacetimedb::rt::assert_reducer_arg::<#arg_tys>();)*
            #(spacetimedb::rt::assert_reducer_ret::<#ret_ty>();)*
            spacetimedb::rt::invoke_reducer(
                #func_name,
                __sender,
                __caller_address,
                __timestamp,
                __args,
            )
        }
    };

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_reducer::<_, _, #func_name>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        impl spacetimedb::rt::ReducerInfo for #func_name {
            const NAME: &'static str = #reducer_name;
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];
            const INVOKE: spacetimedb::rt::ReducerFn = {
                #generated_function
                __reducer
            };
        }
        #extra_impls
    })
}

#[derive(Default)]
pub(crate) struct TableArgs {
    public: Option<Span>,
    name: Option<LitStr>,
    indices: Vec<IndexArg>,
}

pub(crate) struct IndexArg {
    kind: IndexType,
    name: Option<LitStr>,
    columns: Vec<Ident>,
}

impl crate::ParseArgs for TableArgs {
    fn parse_from_args(input: TokenStream) -> syn::Result<Self> {
        let mut specified_access = false;
        let mut args = TableArgs::default();
        syn::meta::parser(|meta| {
            let mut specified_access = || {
                if specified_access {
                    return Err(meta.error("already specified access level"));
                }
                specified_access = true;
                Ok(())
            };
            match_meta!(match meta {
                sym::public => {
                    specified_access()?;
                    args.public = Some(meta.path.span());
                }
                sym::private => {
                    specified_access()?;
                }
                sym::name => {
                    check_duplicate(&args.name, &meta)?;
                    let value = meta.value()?;
                    args.name = Some(value.parse()?);
                }
                sym::index => args.indices.push(IndexArg::parse_meta(meta)?),
            });
            Ok(())
        })
        .parse2(input)?;
        Ok(args)
    }
}

impl IndexArg {
    fn parse_meta(meta: ParseNestedMeta) -> syn::Result<Self> {
        let mut kind = None;
        let mut name = None;
        let mut columns = None;
        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::btree => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::BTree);
                }
                sym::hash => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::Hash);
                }
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    let value = meta.value()?;
                    name = Some(value.parse()?);
                }
                sym::columns => {
                    check_duplicate(&columns, &meta)?;
                    let value = meta.value()?;
                    let inner;
                    syn::bracketed!(inner in value);
                    let cols = Punctuated::<Ident, Token![,]>::parse_terminated(&inner)?;
                    columns = Some(cols.into_iter().collect());
                }
            });
            Ok(())
        })?;
        let kind = kind.ok_or_else(|| meta.error("must specify either `btree` or `hash` for index"))?;
        let columns = columns.ok_or_else(|| meta.error("must specify columns = [col1, col2] for index"))?;
        Ok(IndexArg { kind, name, columns })
    }

    /// Parses an inline `#[index(btree | hash)]` attribute on a field.
    fn parse_index_attr(field: &Ident, attr: &syn::Attribute) -> syn::Result<Self> {
        let mut kind = None;
        attr.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::btree => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::BTree);
                }
                sym::hash => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::Hash);
                }
            });
            Ok(())
        })?;
        let kind =
            kind.ok_or_else(|| syn::Error::new_spanned(&attr.meta, "must specify kind of index (`btree` or `hash`)"))?;
        Ok(IndexArg {
            kind,
            name: None,
            columns: vec![field.clone()],
        })
    }
}

// TODO: We actually need to add a constraint that requires this column to be unique!
struct Column<'a> {
    index: u8,
    field: &'a sats::SatsField<'a>,
    attr: ColumnAttribute,
}

enum ColumnAttr {
    Unique(Span),
    Autoinc(Span),
    Primarykey(Span),
}

impl ColumnAttr {
    fn parse(attr: &syn::Attribute) -> syn::Result<Option<Self>> {
        let Some(ident) = attr.path().get_ident() else {
            return Ok(None);
        };
        Ok(if ident == sym::unique {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Unique(ident.span()))
        } else if ident == sym::autoinc {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Autoinc(ident.span()))
        } else if ident == sym::primarykey {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Primarykey(ident.span()))
        } else {
            None
        })
    }
}

/// Heuristically determine if the path `p` is one of Rust's primitive integer types.
/// This is an approximation, as the user could do `use String as u8`.
fn is_integer_type(p: &Path) -> bool {
    p.get_ident().map_or(false, |i| {
        matches!(
            i.to_string().as_str(),
            "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "u128" | "i128"
        )
    })
}

/// Generates code for treating this type as a table.
///
/// Among other things, this derives `Serialize`, `Deserialize`,
/// `SpacetimeType`, and `TableType` for our type.
///
/// A table type must be a `struct`, whose fields may be annotated with the following attributes:
///
/// * `#[autoinc]`
///
///    Creates a database sequence.
///
///    When a row is inserted with the annotated field set to `0` (zero),
///    the sequence is incremented, and this value is used instead.
///    Can only be used on numeric types and may be combined with indexes.
///
///    Note that using `#[autoinc]` on a field does not also imply `#[primarykey]` or `#[unique]`.
///    If those semantics are desired, those attributes should also be used.
///
/// * `#[unique]`
///
///    Creates an index and unique constraint for the annotated field.
///
/// * `#[primarykey]`
///
///    Similar to `#[unique]`, but generates additional CRUD methods.
pub(crate) fn table_impl(mut args: TableArgs, item: syn::DeriveInput) -> syn::Result<TokenStream> {
    let mut sats_ty = sats::sats_type_from_derive(&item, quote!(spacetimedb::spacetimedb_lib))?;

    let original_struct_ident = sats_ty.ident;
    // TODO: error on setting sats name for a table
    let table_name = args
        .name
        .map(|s| s.value())
        .unwrap_or_else(|| original_struct_ident.to_string());
    sats_ty.name = table_name.clone();
    let sats::SatsTypeData::Product(fields) = &sats_ty.data else {
        return Err(syn::Error::new(Span::call_site(), "spacetimedb table must be a struct"));
    };

    let mut columns = Vec::<Column>::new();

    let get_table_id_func = quote! {
        fn table_id() -> spacetimedb::TableId {
            static TABLE_ID: std::sync::OnceLock<spacetimedb::TableId> = std::sync::OnceLock::new();
            *TABLE_ID.get_or_init(|| {
                spacetimedb::get_table_id(<Self as spacetimedb::TableType>::TABLE_NAME)
            })
        }
    };

    for (i, field) in fields.iter().enumerate() {
        let col_num: u8 = i
            .try_into()
            .map_err(|_| syn::Error::new_spanned(field.ident, "too many columns; the most a table can have is 256"))?;

        let mut col_attr = ColumnAttribute::UNSET;
        for attr in field.original_attrs {
            if attr.path() == sym::index {
                let index = IndexArg::parse_index_attr(field.ident.unwrap(), attr)?;
                args.indices.push(index);
                continue;
            }
            let Some(attr) = ColumnAttr::parse(attr)? else { continue };
            let duplicate = |span| syn::Error::new(span, "duplicate attribute");
            let (extra_col_attr, span) = match attr {
                ColumnAttr::Unique(span) => (ColumnAttribute::UNIQUE, span),
                ColumnAttr::Autoinc(span) => (ColumnAttribute::AUTO_INC, span),
                ColumnAttr::Primarykey(span) => (ColumnAttribute::PRIMARY_KEY, span),
            };
            // do those attributes intersect (not counting the INDEXED bit which is present in all attributes)?
            // this will check that no two attributes both have UNIQUE, AUTOINC or PRIMARY_KEY bits set
            if !(col_attr & extra_col_attr)
                .difference(ColumnAttribute::INDEXED)
                .is_empty()
            {
                return Err(duplicate(span));
            }
            col_attr |= extra_col_attr;
        }

        if col_attr.contains(ColumnAttribute::AUTO_INC)
            && !matches!(field.ty, syn::Type::Path(p) if is_integer_type(&p.path))
        {
            return Err(syn::Error::new(field.ident.unwrap().span(), "An `autoinc` or `identity` column must be one of the integer types: u8, i8, u16, i16, u32, i32, u64, i64, u128, i128"));
        }

        let column = Column {
            index: col_num,
            field,
            attr: col_attr,
        };

        columns.push(column);
    }

    let mut indexes = vec![];

    for index in args.indices {
        let cols = index
            .columns
            .iter()
            .map(|ident| {
                let col = columns
                    .iter()
                    .find(|col| col.field.ident == Some(ident))
                    .ok_or_else(|| syn::Error::new(ident.span(), "not a column of the table"))?;
                Ok(col)
            })
            .collect::<syn::Result<Vec<_>>>()?;
        let name = index.name.map(|s| s.value()).unwrap_or_else(|| {
            [&*table_name]
                .into_iter()
                .chain(cols.iter().map(|col| col.field.name.as_deref().unwrap()))
                .collect::<Vec<_>>()
                .join("_")
        });
        let col_ids = cols.iter().map(|col| col.index);
        let ty = index.kind;
        indexes.push(quote!(spacetimedb::IndexDesc {
            name: #name,
            ty: spacetimedb::sats::db::def::IndexType::#ty,
            col_ids: &[#(#col_ids),*],
        }));
    }

    let (unique_columns, nonunique_columns): (Vec<_>, Vec<_>) =
        columns.iter().partition(|x| x.attr.contains(ColumnAttribute::UNIQUE));

    let has_unique = !unique_columns.is_empty();

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
            #vis fn #filter_func_ident(#column_ident: &#column_type) -> Option<Self> {
                spacetimedb::query::filter_by_unique_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });

        unique_update_funcs.push(quote! {
            #vis fn #update_func_ident(#column_ident: &#column_type, value: Self) -> bool {
                spacetimedb::query::update_by_field::<Self, #column_type, #column_index>(#column_ident, value)
            }
        });

        unique_delete_funcs.push(quote! {
            #vis fn #delete_func_ident(#column_ident: &#column_type) -> bool {
                spacetimedb::query::delete_by_unique_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });
    }

    let non_primary_filter_func = nonunique_columns.into_iter().filter_map(|column| {
        let vis = column.field.vis;
        let column_ident = column.field.ident.unwrap();
        let column_type = column.field.ty;
        let column_index = column.index;

        let filter_func_ident = format_ident!("filter_by_{}", column_ident);
        let delete_func_ident = format_ident!("delete_by_{}", column_ident);

        let is_filterable = if let syn::Type::Path(TypePath { path, .. }) = column_type {
            // TODO: this is janky as heck
            is_integer_type(path)
                || path.is_ident("String")
                || path.is_ident("bool")
                // For these we use the last element of the path because they can be more commonly namespaced.
                || matches!(
                    &*path.segments.last().unwrap().ident.to_string(),
                    "Address" | "Identity"
                )
        } else {
            false
        };

        if !is_filterable {
            return None;
        }

        Some(quote! {
            // TODO: should we expose spacetimedb::query::FilterByIter ?
            #vis fn #filter_func_ident<'a>(#column_ident: &#column_type) -> impl Iterator<Item = Self> {
                spacetimedb::query::filter_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
            #vis fn #delete_func_ident(#column_ident: &#column_type) -> u32 {
                spacetimedb::query::delete_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
        })
    });
    let non_primary_filter_func = non_primary_filter_func.collect::<Vec<_>>();

    let insert_result = if has_unique {
        quote!(std::result::Result<Self, spacetimedb::UniqueConstraintViolation<Self>>)
    } else {
        quote!(Self)
    };

    let db_insert = quote! {
        #[allow(unused_variables)]
        pub fn insert(ins: #original_struct_ident) -> #insert_result {
            <Self as spacetimedb::TableType>::insert(ins)
        }
    };

    let db_iter = quote! {
        #[allow(unused_variables)]
        pub fn iter() -> spacetimedb::TableIter<Self> {
            <Self as spacetimedb::TableType>::iter()
        }
    };

    let table_access = if let Some(span) = args.public {
        quote_spanned!(span=> spacetimedb::sats::db::auth::StAccess::Public)
    } else {
        quote!(spacetimedb::sats::db::auth::StAccess::Private)
    };

    let deserialize_impl = sats::derive_deserialize(&sats_ty);
    let serialize_impl = sats::derive_serialize(&sats_ty);
    let schema_impl = sats::derive_satstype(&sats_ty, false);
    let column_attrs = columns.iter().map(|col| {
        Ident::new(
            ColumnAttribute::FLAGS
                .iter()
                .find_map(|f| (col.attr == *f.value()).then_some(f.name()))
                .expect("Invalid column attribute"),
            Span::call_site(),
        )
    });
    let tabletype_impl = quote! {
        impl spacetimedb::TableType for #original_struct_ident {
            const TABLE_NAME: &'static str = #table_name;
            const TABLE_ACCESS: spacetimedb::sats::db::auth::StAccess = #table_access;
            const COLUMN_ATTRS: &'static [spacetimedb::sats::db::attr::ColumnAttribute] = &[
                #(spacetimedb::sats::db::attr::ColumnAttribute::#column_attrs),*
            ];
            const INDEXES: &'static [spacetimedb::IndexDesc<'static>] = &[#(#indexes),*];
            type InsertResult = #insert_result;
            #get_table_id_func
        }
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{table_name}");

    let describe_table_func = quote! {
        #[export_name = #register_describer_symbol]
        extern "C" fn __register_describer() {
            spacetimedb::rt::register_table::<#original_struct_ident>()
        }
    };

    let field_names = fields.iter().map(|f| f.ident.unwrap()).collect::<Vec<_>>();
    let field_types = fields.iter().map(|f| f.ty).collect::<Vec<_>>();

    let col_num = 0u8..;
    let field_access_impls = quote! {
        #(impl spacetimedb::query::FieldAccess<#col_num> for #original_struct_ident {
            type Field = #field_types;
            fn get_field(&self) -> &Self::Field {
                &self.#field_names
            }
        })*
    };

    let filter_impl = quote! {
        const _: () = {
            #[derive(Debug, spacetimedb::Serialize, spacetimedb::Deserialize)]
            #[sats(crate = spacetimedb::spacetimedb_lib)]
            #[repr(u8)]
            #[allow(non_camel_case_types)]
            pub enum FieldIndex {
                #(#field_names),*
            }

            impl spacetimedb::spacetimedb_lib::filter::Table for #original_struct_ident {
                type FieldIndex = FieldIndex;
            }
        };
    };

    // Attempt to improve the compile error when a table field doesn't satisfy
    // the supertraits of `TableType`. We make it so the field span indicates
    // which fields are offenders, and error reporting stops if the field doesn't
    // implement `SpacetimeType` (which happens to be the derive macro one is
    // supposed to use). That is, the user doesn't see errors about `Serialize`,
    // `Deserialize` not being satisfied, which they wouldn't know what to do
    // about.
    let assert_fields_are_spacetimetypes = {
        let trait_ident = Ident::new("AssertSpacetimeFields", Span::call_site());
        let field_impls = fields
            .iter()
            .map(|field| (field.ty, field.span))
            .collect::<HashMap<_, _>>()
            .into_iter()
            .map(|(ty, span)| quote_spanned!(span=> impl #trait_ident for #ty {}));

        quote_spanned! {item.span()=>
            trait #trait_ident: spacetimedb::SpacetimeType {}
            #(#field_impls)*
        }
    };

    // Output all macro data
    let emission = quote! {
        const _: () = {
            #describe_table_func
        };

        const _: () = {
            #assert_fields_are_spacetimetypes
        };

        impl #original_struct_ident {
            #db_insert
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
        #filter_impl
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        {
            #![allow(clippy::disallowed_macros)]
            println!("{}", emission);
        }
    }

    Ok(emission)
}
