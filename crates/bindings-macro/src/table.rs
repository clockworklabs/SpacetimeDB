use crate::sats::{self, derive_deserialize, derive_satstype, derive_serialize};
use crate::sym;
use crate::util::{check_duplicate, check_duplicate_msg, ident_to_litstr, match_meta};
use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::borrow::Cow;
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_quote, Ident, Path, Token};

pub(crate) struct TableArgs {
    access: Option<TableAccess>,
    scheduled: Option<Path>,
    name: Ident,
    indices: Vec<IndexArg>,
}

enum TableAccess {
    Public(Span),
    Private(Span),
}

impl TableAccess {
    fn to_value(&self) -> TokenStream {
        let (TableAccess::Public(span) | TableAccess::Private(span)) = *self;
        let name = match self {
            TableAccess::Public(_) => "Public",
            TableAccess::Private(_) => "Private",
        };
        let ident = Ident::new(name, span);
        quote_spanned!(span => spacetimedb::table::TableAccess::#ident)
    }
}

struct IndexArg {
    name: Ident,
    kind: IndexType,
}

enum IndexType {
    BTree { columns: Vec<Ident> },
    UniqueBTree { column: Ident },
}

impl TableArgs {
    pub(crate) fn parse(input: TokenStream, struct_ident: &Ident) -> syn::Result<Self> {
        let mut access = None;
        let mut scheduled = None;
        let mut name = None;
        let mut indices = Vec::new();
        syn::meta::parser(|meta| {
            match_meta!(match meta {
                sym::public => {
                    check_duplicate_msg(&access, &meta, "already specified access level")?;
                    access = Some(TableAccess::Public(meta.path.span()));
                }
                sym::private => {
                    check_duplicate_msg(&access, &meta, "already specified access level")?;
                    access = Some(TableAccess::Private(meta.path.span()));
                }
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    let value = meta.value()?;
                    name = Some(value.parse()?);
                }
                sym::index => indices.push(IndexArg::parse_meta(meta)?),
                sym::scheduled => {
                    check_duplicate(&scheduled, &meta)?;
                    let in_parens;
                    syn::parenthesized!(in_parens in meta.input);
                    scheduled = Some(in_parens.parse::<Path>()?);
                }
            });
            Ok(())
        })
        .parse2(input)?;
        let name = name.ok_or_else(|| {
            let table = struct_ident.to_string().to_snake_case();
            syn::Error::new(
                Span::call_site(),
                format_args!("must specify table name, e.g. `#[spacetimedb::table(name = {table})]"),
            )
        })?;
        Ok(TableArgs {
            access,
            scheduled,
            name,
            indices,
        })
    }
}

impl IndexArg {
    fn parse_meta(meta: ParseNestedMeta) -> syn::Result<Self> {
        let mut name = None;
        let mut algo = None;

        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    name = Some(meta.value()?.parse()?);
                }
                sym::btree => {
                    check_duplicate_msg(&algo, &meta, "index algorithm specified twice")?;
                    algo = Some(Self::parse_btree(meta)?);
                }
            });
            Ok(())
        })?;
        let name = name.ok_or_else(|| meta.error("missing index name, e.g. name = my_index"))?;
        let kind = algo.ok_or_else(|| meta.error("missing index algorithm, e.g., `btree(columns = [col1, col2])`"))?;

        Ok(IndexArg { name, kind })
    }

    fn parse_btree(meta: ParseNestedMeta) -> syn::Result<IndexType> {
        let mut columns = None;
        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::columns => {
                    check_duplicate(&columns, &meta)?;
                    let value = meta.value()?;
                    let inner;
                    syn::bracketed!(inner in value);
                    columns = Some(
                        Punctuated::<Ident, Token![,]>::parse_terminated(&inner)?
                            .into_iter()
                            .collect::<Vec<_>>(),
                    );
                }
            });
            Ok(())
        })?;
        let columns = columns
            .ok_or_else(|| meta.error("must specify columns for btree index, e.g. `btree(columns = [col1, col2])`"))?;
        Ok(IndexType::BTree { columns })
    }

    /// Parses an inline `#[index(btree)]` attribute on a field.
    fn parse_index_attr(field: &Ident, attr: &syn::Attribute) -> syn::Result<Self> {
        let mut kind = None;
        attr.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::btree => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::BTree {
                        columns: vec![field.clone()],
                    });
                }
            });
            Ok(())
        })?;
        let kind = kind.ok_or_else(|| syn::Error::new_spanned(&attr.meta, "must specify kind of index (`btree`)"))?;
        let name = field.clone();
        Ok(IndexArg { kind, name })
    }

    fn validate<'a>(&'a self, table_name: &str, cols: &'a [Column<'a>]) -> syn::Result<ValidatedIndex<'_>> {
        let find_column = |ident| {
            cols.iter()
                .find(|col| col.field.ident == Some(ident))
                .ok_or_else(|| syn::Error::new(ident.span(), "not a column of the table"))
        };
        let kind = match &self.kind {
            IndexType::BTree { columns } => {
                let cols = columns.iter().map(find_column).collect::<syn::Result<Vec<_>>>()?;
                ValidatedIndexType::BTree { cols }
            }
            IndexType::UniqueBTree { column } => {
                let col = find_column(column)?;
                ValidatedIndexType::UniqueBTree { col }
            }
        };
        // See crates/schema/src/validate/v9.rs for the format of index names.
        // It's slightly unnerving that we just trust that component to generate this format correctly,
        // but what can you do.
        let index_name = match &kind {
            ValidatedIndexType::BTree { cols } => {
                let cols = cols
                    .iter()
                    .map(|col| col.field.ident.unwrap().to_string())
                    .collect::<Vec<_>>();
                let cols = cols.join("_");
                format!("{table_name}_{cols}_idx_btree")
            }
            ValidatedIndexType::UniqueBTree { col } => {
                let col = col.field.ident.unwrap().to_string();
                format!("{table_name}_{col}_idx_btree")
            }
        };
        Ok(ValidatedIndex {
            index_name,
            accessor_name: &self.name,
            kind,
        })
    }
}

struct ValidatedIndex<'a> {
    index_name: String,
    accessor_name: &'a Ident,
    kind: ValidatedIndexType<'a>,
}

enum ValidatedIndexType<'a> {
    BTree { cols: Vec<&'a Column<'a>> },
    UniqueBTree { col: &'a Column<'a> },
}

impl ValidatedIndex<'_> {
    fn desc(&self) -> TokenStream {
        let algo = match &self.kind {
            ValidatedIndexType::BTree { cols } => {
                let col_ids = cols.iter().map(|col| col.index);
                quote!(spacetimedb::table::IndexAlgo::BTree {
                    columns: &[#(#col_ids),*]
                })
            }
            ValidatedIndexType::UniqueBTree { col } => {
                let col_id = col.index;
                quote!(spacetimedb::table::IndexAlgo::BTree {
                    columns: &[#col_id]
                })
            }
        };
        let accessor_name = ident_to_litstr(self.accessor_name);
        // Note: we do not pass the index_name through here.
        // We trust the schema validation logic to reconstruct the name we've stored in `self.name`.
        quote!(spacetimedb::table::IndexDesc {
            accessor_name: #accessor_name,
            algo: #algo,
        })
    }

    fn accessor(&self, vis: &syn::Visibility, row_type_ident: &Ident) -> TokenStream {
        let index_ident = self.accessor_name;
        match &self.kind {
            ValidatedIndexType::BTree { cols } => {
                let col_tys = cols.iter().map(|col| col.ty);
                let mut doc = format!(
                    "Gets the `{index_ident}` [`BTreeIndex`][spacetimedb::BTreeIndex] as defined \
                     on this table. \n\
                     \n\
                     This B-tree index is defined on the following columns, in order:\n"
                );
                for col in cols {
                    use std::fmt::Write;
                    writeln!(
                        doc,
                        "- [`{ident}`][{row_type_ident}#structfield.{ident}]: [`{ty}`]",
                        ident = col.field.ident.unwrap(),
                        ty = col.ty.to_token_stream()
                    )
                    .unwrap();
                }
                quote! {
                    #[doc = #doc]
                    #vis fn #index_ident(&self) -> spacetimedb::BTreeIndex<Self, (#(#col_tys,)*), __indices::#index_ident> {
                        spacetimedb::BTreeIndex::__NEW
                    }
                }
            }
            ValidatedIndexType::UniqueBTree { col } => {
                let vis = col.field.vis;
                let col_ty = col.field.ty;
                let column_ident = col.field.ident.unwrap();

                let doc = format!(
                    "Gets the [`UniqueColumn`][spacetimedb::UniqueColumn] for the \
                     [`{column_ident}`][{row_type_ident}::{column_ident}] column."
                );
                quote! {
                    #[doc = #doc]
                    #vis fn #column_ident(&self) -> spacetimedb::UniqueColumn<Self, #col_ty, __indices::#index_ident> {
                        spacetimedb::UniqueColumn::__NEW
                    }
                }
            }
        }
    }

    fn marker_type(&self, vis: &syn::Visibility, row_type_ident: &Ident) -> TokenStream {
        let index_ident = self.accessor_name;
        let index_name = &self.index_name;
        let vis = if let ValidatedIndexType::UniqueBTree { col } = self.kind {
            col.field.vis
        } else {
            vis
        };
        let vis = superize_vis(vis);
        let mut decl = quote! {
            #vis struct #index_ident;
            impl spacetimedb::table::Index for #index_ident {
                fn index_id() -> spacetimedb::table::IndexId {
                    static INDEX_ID: std::sync::OnceLock<spacetimedb::table::IndexId> = std::sync::OnceLock::new();
                    *INDEX_ID.get_or_init(|| {
                        spacetimedb::sys::index_id_from_name(#index_name).unwrap()
                    })
                }
            }
        };
        if let ValidatedIndexType::UniqueBTree { col } = self.kind {
            let col_ty = col.ty;
            let col_name = col.field.name.as_deref().unwrap();
            let field_ident = col.field.ident.unwrap();
            decl.extend(quote! {
                impl spacetimedb::table::Column for #index_ident {
                    type Row = #row_type_ident;
                    type ColType = #col_ty;
                    const COLUMN_NAME: &'static str = #col_name;
                    fn get_field(row: &Self::Row) -> &Self::ColType {
                        &row.#field_ident
                    }
                }
            });
        }
        decl
    }
}

/// Transform a visibility marker to one with the same effective visibility, but
/// for use in a child module of the module of the original marker.
fn superize_vis(vis: &syn::Visibility) -> Cow<'_, syn::Visibility> {
    match vis {
        syn::Visibility::Public(_) => Cow::Borrowed(vis),
        syn::Visibility::Restricted(vis_r) => {
            let first = &vis_r.path.segments[0];
            if first.ident == "crate" || vis_r.path.leading_colon.is_some() {
                Cow::Borrowed(vis)
            } else {
                let mut vis_r = vis_r.clone();
                if first.ident == "super" {
                    vis_r.path.segments.insert(0, first.clone())
                } else if first.ident == "self" {
                    vis_r.path.segments[0].ident = Ident::new("super", Span::call_site())
                }
                Cow::Owned(syn::Visibility::Restricted(vis_r))
            }
        }
        syn::Visibility::Inherited => Cow::Owned(parse_quote!(pub(super))),
    }
}

#[derive(Copy, Clone)]
struct Column<'a> {
    index: u16,
    field: &'a sats::SatsField<'a>,
    ty: &'a syn::Type,
}

enum ColumnAttr {
    Unique(Span),
    AutoInc(Span),
    PrimaryKey(Span),
    Index(IndexArg),
    ScheduledAt(Span),
}

impl ColumnAttr {
    fn parse(attr: &syn::Attribute, field_ident: &Ident) -> syn::Result<Option<Self>> {
        let Some(ident) = attr.path().get_ident() else {
            return Ok(None);
        };
        Ok(if ident == sym::index {
            let index = IndexArg::parse_index_attr(field_ident, attr)?;
            Some(ColumnAttr::Index(index))
        } else if ident == sym::unique {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Unique(ident.span()))
        } else if ident == sym::auto_inc {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::AutoInc(ident.span()))
        } else if ident == sym::primary_key {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::PrimaryKey(ident.span()))
        } else if ident == sym::scheduled_at {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::ScheduledAt(ident.span()))
        } else {
            None
        })
    }
}

pub(crate) fn table_impl(mut args: TableArgs, item: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let vis = &item.vis;
    let sats_ty = sats::sats_type_from_derive(item, quote!(spacetimedb::spacetimedb_lib))?;

    let original_struct_ident = sats_ty.ident;
    let table_ident = &args.name;
    let table_name = table_ident.unraw().to_string();
    let sats::SatsTypeData::Product(fields) = &sats_ty.data else {
        return Err(syn::Error::new(Span::call_site(), "spacetimedb table must be a struct"));
    };

    for param in &item.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on tables")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on tables")),
        }
    }

    let table_id_from_name_func = quote! {
        fn table_id() -> spacetimedb::TableId {
            static TABLE_ID: std::sync::OnceLock<spacetimedb::TableId> = std::sync::OnceLock::new();
            *TABLE_ID.get_or_init(|| {
                spacetimedb::table_id_from_name(<Self as spacetimedb::table::TableInternal>::TABLE_NAME)
            })
        }
    };

    if fields.len() > u16::MAX.into() {
        return Err(syn::Error::new_spanned(
            item,
            "too many columns; the most a table can have is 2^16",
        ));
    }

    let mut columns = vec![];
    let mut unique_columns = vec![];
    let mut sequenced_columns = vec![];
    let mut primary_key_column = None;
    let mut scheduled_at_column = None;

    for (i, field) in fields.iter().enumerate() {
        let col_num = i as u16;
        let field_ident = field.ident.unwrap();

        let mut unique = None;
        let mut auto_inc = None;
        let mut primary_key = None;
        let mut scheduled_at = None;
        for attr in field.original_attrs {
            let Some(attr) = ColumnAttr::parse(attr, field_ident)? else {
                continue;
            };
            match attr {
                ColumnAttr::Unique(span) => {
                    check_duplicate(&unique, span)?;
                    unique = Some(span);
                }
                ColumnAttr::AutoInc(span) => {
                    check_duplicate(&auto_inc, span)?;
                    auto_inc = Some(span);
                }
                ColumnAttr::PrimaryKey(span) => {
                    check_duplicate(&primary_key, span)?;
                    primary_key = Some(span);
                }
                ColumnAttr::Index(index_arg) => args.indices.push(index_arg),
                ColumnAttr::ScheduledAt(span) => {
                    check_duplicate(&scheduled_at, span)?;
                    scheduled_at = Some(span);
                }
            }
        }

        let column = Column {
            index: col_num,
            field,
            ty: field.ty,
        };

        if unique.is_some() || primary_key.is_some() {
            unique_columns.push(column);
            args.indices.push(IndexArg {
                name: field_ident.clone(),
                kind: IndexType::UniqueBTree {
                    column: field_ident.clone(),
                },
            });
        }
        if auto_inc.is_some() {
            sequenced_columns.push(column);
        }
        if let Some(span) = primary_key {
            check_duplicate_msg(&primary_key_column, span, "can only have one primary key per table")?;
            primary_key_column = Some(column);
        }
        if let Some(span) = scheduled_at {
            check_duplicate_msg(
                &scheduled_at_column,
                span,
                "can only have one scheduled_at column per table",
            )?;
            scheduled_at_column = Some(column);
        }

        columns.push(column);
    }

    let scheduled_at_typecheck = scheduled_at_column.map(|col| {
        let ty = col.ty;
        quote!(let _ = |x: #ty| { let _: spacetimedb::ScheduleAt = x; };)
    });
    let scheduled_id_typecheck = primary_key_column.filter(|_| args.scheduled.is_some()).map(|col| {
        let ty = col.ty;
        quote!(spacetimedb::rt::assert_scheduled_table_primary_key::<#ty>();)
    });

    let row_type = quote!(#original_struct_ident);

    let mut indices = args
        .indices
        .iter()
        .map(|index| index.validate(&table_name, &columns))
        .collect::<syn::Result<Vec<_>>>()?;

    // order unique accessors before index accessors
    indices.sort_by(|a, b| match (&a.kind, &b.kind) {
        (ValidatedIndexType::UniqueBTree { .. }, ValidatedIndexType::UniqueBTree { .. }) => std::cmp::Ordering::Equal,
        (_, ValidatedIndexType::UniqueBTree { .. }) => std::cmp::Ordering::Greater,
        (ValidatedIndexType::UniqueBTree { .. }, _) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    });

    let index_descs = indices.iter().map(|index| index.desc());
    let index_accessors = indices.iter().map(|index| index.accessor(vis, original_struct_ident));
    let index_marker_types = indices
        .iter()
        .map(|index| index.marker_type(vis, original_struct_ident));

    let tablehandle_ident = format_ident!("{}__TableHandle", table_ident);

    let deserialize_impl = derive_deserialize(&sats_ty);
    let serialize_impl = derive_serialize(&sats_ty);
    let schema_impl = derive_satstype(&sats_ty);

    // Generate `integrate_generated_columns`
    // which will integrate all generated auto-inc col values into `_row`.
    let integrate_gen_col = sequenced_columns.iter().map(|col| {
        let field = col.field.ident.unwrap();
        quote_spanned!(field.span()=>
            spacetimedb::table::SequenceTrigger::maybe_decode_into(&mut __row.#field, &mut __generated_cols);
        )
    });
    let integrate_generated_columns = quote_spanned!(item.span() =>
        fn integrate_generated_columns(__row: &mut #row_type, mut __generated_cols: &[u8]) {
            #(#integrate_gen_col)*
        }
    );

    let table_access = args.access.iter().map(|acc| acc.to_value());
    let unique_col_ids = unique_columns.iter().map(|col| col.index);
    let primary_col_id = primary_key_column.iter().map(|col| col.index);
    let sequence_col_ids = sequenced_columns.iter().map(|col| col.index);

    let scheduled_reducer_type_check = args.scheduled.as_ref().map(|reducer| {
        quote! {
            spacetimedb::rt::scheduled_reducer_typecheck::<#original_struct_ident>(#reducer);
        }
    });
    let schedule = args
        .scheduled
        .as_ref()
        .map(|reducer| {
            // better error message when both are missing
            if scheduled_at_column.is_none() && primary_key_column.is_none() {
                return Err(syn::Error::new(
                    original_struct_ident.span(),
                    "scheduled table missing required columns; add these to your struct:\n\
                             #[primary_key]\n\
                             #[auto_inc]\n\
                             scheduled_id: u64,\n\
                             #[scheduled_at]\n\
                             scheduled_at: spacetimedb::ScheduleAt,",
                ));
            }
            let scheduled_at_column = scheduled_at_column.ok_or_else(|| {
                syn::Error::new(
                    original_struct_ident.span(),
                    "scheduled tables must have a `#[scheduled_at] scheduled_at: spacetimedb::ScheduleAt` column.",
                )
            })?;
            let scheduled_at_id = scheduled_at_column.index;
            if primary_key_column.is_none() {
                return Err(syn::Error::new(
                    original_struct_ident.span(),
                    "scheduled tables should have a `#[primary_key] #[auto_inc] scheduled_id: u64` column",
                ));
            }
            Ok(quote!(spacetimedb::table::ScheduleDesc {
                reducer_name: <#reducer as spacetimedb::rt::ReducerInfo>::NAME,
                scheduled_at_column: #scheduled_at_id,
            }))
        })
        .transpose()?
        .into_iter();

    let unique_err = if !unique_columns.is_empty() {
        quote!(spacetimedb::UniqueConstraintViolation)
    } else {
        quote!(::core::convert::Infallible)
    };
    let autoinc_err = if !sequenced_columns.is_empty() {
        quote!(spacetimedb::AutoIncOverflow)
    } else {
        quote!(::core::convert::Infallible)
    };

    let field_types = fields.iter().map(|f| f.ty).collect::<Vec<_>>();

    let tabletype_impl = quote! {
        impl spacetimedb::Table for #tablehandle_ident {
            type Row = #row_type;

            type UniqueConstraintViolation = #unique_err;
            type AutoIncOverflow = #autoinc_err;

            #integrate_generated_columns
        }
        impl spacetimedb::table::TableInternal for #tablehandle_ident {
            const TABLE_NAME: &'static str = #table_name;
            // the default value if not specified is Private
            #(const TABLE_ACCESS: spacetimedb::table::TableAccess = #table_access;)*
            const UNIQUE_COLUMNS: &'static [u16] = &[#(#unique_col_ids),*];
            const INDEXES: &'static [spacetimedb::table::IndexDesc<'static>] = &[#(#index_descs),*];
            #(const PRIMARY_KEY: Option<u16> = Some(#primary_col_id);)*
            const SEQUENCES: &'static [u16] = &[#(#sequence_col_ids),*];
            #(const SCHEDULE: Option<spacetimedb::table::ScheduleDesc<'static>> = Some(#schedule);)*

            #table_id_from_name_func
        }
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{table_ident}");

    let describe_table_func = quote! {
        #[export_name = #register_describer_symbol]
        extern "C" fn __register_describer() {
            spacetimedb::rt::register_table::<#tablehandle_ident>()
        }
    };

    let row_type_to_table = quote!(<#row_type as spacetimedb::table::__MapRowTypeToTable>::Table);

    // Output all macro data
    let trait_def = quote_spanned! {table_ident.span()=>
        #[allow(non_camel_case_types, dead_code)]
        #vis trait #table_ident {
            fn #table_ident(&self) -> &#row_type_to_table;
        }
        impl #table_ident for spacetimedb::Local {
            fn #table_ident(&self) -> &#row_type_to_table {
                #[allow(non_camel_case_types)]
                type #tablehandle_ident = #row_type_to_table;
                &#tablehandle_ident {}
            }
        }
    };

    let tablehandle_def = quote! {
        #[allow(non_camel_case_types)]
        #[non_exhaustive]
        #vis struct #tablehandle_ident {}
    };

    let emission = quote! {
        const _: () = {
            #(let _ = <#field_types as spacetimedb::rt::TableColumn>::_ITEM;)*
            #scheduled_reducer_type_check
            #scheduled_at_typecheck
            #scheduled_id_typecheck
        };

        #trait_def

        #[cfg(doc)]
        #tablehandle_def

        const _: () = {
            #[cfg(not(doc))]
            #tablehandle_def

            impl spacetimedb::table::__MapRowTypeToTable for #row_type {
                type Table = #tablehandle_ident;
            }

            impl #tablehandle_ident {
                #(#index_accessors)*
            }

            #tabletype_impl

            #[allow(non_camel_case_types)]
            mod __indices {
                #[allow(unused)]
                use super::*;
                #(#index_marker_types)*
            }

            #describe_table_func
        };

        #schema_impl
        #deserialize_impl
        #serialize_impl
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        {
            #![allow(clippy::disallowed_macros)]
            println!("{}", emission);
        }
    }

    Ok(emission)
}
