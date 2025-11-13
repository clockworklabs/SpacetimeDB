use crate::sats;
use crate::sym;
use crate::util::{check_duplicate, check_duplicate_msg, ident_to_litstr, match_meta};
use core::slice;
use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::borrow::Cow;
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::parse::Parse;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_quote, Ident, Path, Token};

pub(crate) struct TableArgs {
    access: Option<TableAccess>,
    scheduled: Option<ScheduledArg>,
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

struct ScheduledArg {
    span: Span,
    reducer_or_procedure: Path,
    at: Option<Ident>,
}

struct IndexArg {
    name: Ident,
    is_unique: bool,
    kind: IndexType,
}

impl IndexArg {
    fn new(name: Ident, kind: IndexType) -> Self {
        // We don't know if its unique yet.
        // We'll discover this once we have collected constraints.
        let is_unique = false;
        Self { name, is_unique, kind }
    }
}

enum IndexType {
    BTree { columns: Vec<Ident> },
    Direct { column: Ident },
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
                    scheduled = Some(ScheduledArg::parse_meta(meta)?);
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

impl ScheduledArg {
    fn parse_meta(meta: ParseNestedMeta) -> syn::Result<Self> {
        let span = meta.path.span();
        let mut reducer_or_procedure = None;
        let mut at = None;

        meta.parse_nested_meta(|meta| {
            if meta.input.peek(syn::Token![=]) || meta.input.peek(syn::token::Paren) {
                match_meta!(match meta {
                    sym::at => {
                        check_duplicate(&at, &meta)?;
                        let ident = meta.value()?.parse()?;
                        at = Some(ident);
                    }
                })
            } else {
                check_duplicate_msg(
                    &reducer_or_procedure,
                    &meta,
                    "can only specify one scheduled reducer or procedure",
                )?;
                reducer_or_procedure = Some(meta.path);
            }
            Ok(())
        })?;

        let reducer_or_procedure = reducer_or_procedure.ok_or_else(|| {
            meta.error(
                "must specify scheduled reducer or procedure associated with the table: scheduled(function_name)",
            )
        })?;
        Ok(Self {
            span,
            reducer_or_procedure,
            at,
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
                sym::direct => {
                    check_duplicate_msg(&algo, &meta, "index algorithm specified twice")?;
                    algo = Some(Self::parse_direct(meta)?);
                }
            });
            Ok(())
        })?;
        let name = name.ok_or_else(|| meta.error("missing index name, e.g. name = my_index"))?;
        let kind = algo.ok_or_else(|| {
            meta.error("missing index algorithm, e.g., `btree(columns = [col1, col2])` or `direct(column = col1)`")
        })?;

        Ok(IndexArg::new(name, kind))
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

    fn parse_direct(meta: ParseNestedMeta) -> syn::Result<IndexType> {
        let mut column = None;
        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::column => {
                    check_duplicate(&column, &meta)?;
                    let value = meta.value()?;
                    let inner;
                    syn::bracketed!(inner in value);
                    column = Some(Ident::parse(&inner)?);
                }
            });
            Ok(())
        })?;
        let column = column
            .ok_or_else(|| meta.error("must specify the column for direct index, e.g. `direct(column = col1)`"))?;
        Ok(IndexType::Direct { column })
    }

    /// Parses an inline `#[index(btree)]` or `#[index(direct)]` attribute on a field.
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
                sym::direct => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::Direct { column: field.clone() })
                }
            });
            Ok(())
        })?;
        let kind = kind
            .ok_or_else(|| syn::Error::new_spanned(&attr.meta, "must specify kind of index (`btree` or `direct`)"))?;
        let name = field.clone();
        Ok(IndexArg::new(name, kind))
    }

    fn validate<'a>(&'a self, table_name: &str, cols: &'a [Column<'a>]) -> syn::Result<ValidatedIndex<'a>> {
        let find_column = |ident| find_column(cols, ident);
        let kind = match &self.kind {
            IndexType::BTree { columns } => {
                let cols = columns.iter().map(find_column).collect::<syn::Result<Vec<_>>>()?;
                ValidatedIndexType::BTree { cols }
            }
            IndexType::Direct { column } => {
                let col = find_column(column)?;

                if !self.is_unique {
                    return Err(syn::Error::new(
                        column.span(),
                        "a direct index must be paired with a `#[unique] constraint",
                    ));
                }

                ValidatedIndexType::Direct { col }
            }
        };
        // See crates/schema/src/validate/v9.rs for the format of index names.
        // It's slightly unnerving that we just trust that component to generate this format correctly,
        // but what can you do.
        let (cols, kind_str) = match &kind {
            ValidatedIndexType::BTree { cols } => (&**cols, "btree"),
            ValidatedIndexType::Direct { col } => (&[*col] as &[_], "direct"),
        };
        let cols = cols.iter().map(|col| col.ident.to_string()).collect::<Vec<_>>();
        let cols = cols.join("_");
        let index_name = format!("{table_name}_{cols}_idx_{kind_str}");

        Ok(ValidatedIndex {
            is_unique: self.is_unique,
            index_name,
            accessor_name: &self.name,
            kind,
        })
    }
}

enum AccessorType {
    Read,
    ReadWrite,
}

impl AccessorType {
    fn unique(&self) -> proc_macro2::TokenStream {
        match self {
            AccessorType::Read => quote!(spacetimedb::UniqueColumnReadOnly),
            AccessorType::ReadWrite => quote!(spacetimedb::UniqueColumn),
        }
    }

    fn range(&self) -> proc_macro2::TokenStream {
        match self {
            AccessorType::Read => quote!(spacetimedb::RangedIndexReadOnly),
            AccessorType::ReadWrite => quote!(spacetimedb::RangedIndex),
        }
    }

    fn unique_doc_typename(&self) -> &'static str {
        match self {
            AccessorType::Read => "UniqueColumnReadOnly",
            AccessorType::ReadWrite => "UniqueColumn",
        }
    }

    fn range_doc_typename(&self) -> &'static str {
        match self {
            AccessorType::Read => "RangedIndexReadOnly",
            AccessorType::ReadWrite => "RangedIndex",
        }
    }
}

struct ValidatedIndex<'a> {
    index_name: String,
    accessor_name: &'a Ident,
    is_unique: bool,
    kind: ValidatedIndexType<'a>,
}

enum ValidatedIndexType<'a> {
    BTree { cols: Vec<&'a Column<'a>> },
    Direct { col: &'a Column<'a> },
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
            ValidatedIndexType::Direct { col } => {
                let col_id = col.index;
                quote!(spacetimedb::table::IndexAlgo::Direct {
                    column: #col_id
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

    fn accessor(
        &self,
        vis: &syn::Visibility,
        row_type_ident: &Ident,
        tbl_type_ident: &Ident,
        flavor: AccessorType,
    ) -> TokenStream {
        let cols = match &self.kind {
            ValidatedIndexType::BTree { cols } => &**cols,
            ValidatedIndexType::Direct { col } => slice::from_ref(col),
        };
        if self.is_unique {
            assert_eq!(cols.len(), 1);
            self.unique_accessor(cols[0], row_type_ident, tbl_type_ident, flavor)
        } else {
            self.range_accessor(vis, row_type_ident, tbl_type_ident, cols, flavor)
        }
    }

    fn unique_accessor(
        &self,
        col: &Column<'_>,
        row_type_ident: &Ident,
        tbl_type_ident: &Ident,
        flavor: AccessorType,
    ) -> TokenStream {
        let index_ident = self.accessor_name;
        let vis = col.vis;
        let col_ty = col.ty;
        let column_ident = col.ident;

        let unique_ty = flavor.unique();
        let tbl_token = quote!(#tbl_type_ident);
        let doc_type = flavor.unique_doc_typename();

        let doc = format!(
            "Gets the [`{doc_type}`][spacetimedb::{doc_type}] for the \
             [`{column_ident}`][{row_type_ident}::{column_ident}] column."
        );
        quote! {
            #[doc = #doc]
            #vis fn #column_ident(&self) -> #unique_ty<#tbl_token, #col_ty, __indices::#index_ident> {
                #unique_ty::__NEW
            }
        }
    }

    fn range_accessor(
        &self,
        vis: &syn::Visibility,
        row_type_ident: &Ident,
        tbl_type_ident: &Ident,
        cols: &[&Column<'_>],
        flavor: AccessorType,
    ) -> TokenStream {
        let index_ident = self.accessor_name;
        let col_tys = cols.iter().map(|c| c.ty);

        let range_ty = flavor.range();
        let tbl_token = quote!(#tbl_type_ident);
        let doc_type = flavor.range_doc_typename();

        let mut doc = format!(
            "Gets the `{index_ident}` [`{doc_type}`][spacetimedb::{doc_type}] as defined \
             on this table.\n\nThis B-tree index is defined on the following columns, in order:\n"
        );
        for col in cols {
            use std::fmt::Write;
            writeln!(
                doc,
                "- [`{ident}`][{row_type_ident}#structfield.{ident}]: [`{ty}`]",
                ident = col.ident,
                ty = col.ty.to_token_stream()
            )
            .unwrap();
        }

        quote! {
            #[doc = #doc]
            #vis fn #index_ident(&self) -> #range_ty<#tbl_token, (#(#col_tys,)*), __indices::#index_ident> {
                #range_ty::__NEW
            }
        }
    }

    fn marker_type(&self, vis: &syn::Visibility, tablehandle_ident: &Ident) -> TokenStream {
        let index_ident = self.accessor_name;
        let index_name = &self.index_name;

        let (cols, typeck_direct_index) = match &self.kind {
            ValidatedIndexType::BTree { cols } => (&**cols, None),
            ValidatedIndexType::Direct { col } => {
                let col_ty = col.ty;
                let typeck = quote_spanned!(col_ty.span()=>
                    const _: () = {
                        spacetimedb::spacetimedb_lib::assert_column_type_valid_for_direct_index::<#col_ty>();
                    };
                );
                (slice::from_ref(col), Some(typeck))
            }
        };
        let vis = if self.is_unique {
            assert_eq!(cols.len(), 1);
            cols[0].vis
        } else {
            vis
        };
        let vis = superize_vis(vis);

        let mut decl = quote! {
            #typeck_direct_index

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
        if self.is_unique {
            let col = cols[0];
            let col_ty = col.ty;
            let col_name = col.ident.to_string();
            let field_ident = col.ident;
            decl.extend(quote! {
                impl spacetimedb::table::Column for #index_ident {
                    type Table = #tablehandle_ident;
                    type ColType = #col_ty;
                    const COLUMN_NAME: &'static str = #col_name;
                    fn get_field(row: &<Self::Table as spacetimedb::Table>::Row) -> &Self::ColType {
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

#[derive(Clone)]
struct Column<'a> {
    index: u16,
    vis: &'a syn::Visibility,
    ident: &'a syn::Ident,
    ty: &'a syn::Type,
    default_value: Option<syn::Expr>,
}

fn try_find_column<'a, 'b, T: ?Sized>(cols: &'a [Column<'b>], name: &T) -> Option<&'a Column<'b>>
where
    Ident: PartialEq<T>,
{
    cols.iter().find(|col| col.ident == name)
}

fn find_column<'a, 'b>(cols: &'a [Column<'b>], name: &Ident) -> syn::Result<&'a Column<'b>> {
    try_find_column(cols, name).ok_or_else(|| syn::Error::new(name.span(), "not a column of the table"))
}

enum ColumnAttr {
    Unique(Span),
    AutoInc(Span),
    PrimaryKey(Span),
    Index(IndexArg),
    Default(syn::Expr, Span),
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
        } else if ident == sym::default {
            Some(parse_default_attr(attr, ident)?)
        } else {
            None
        })
    }
}

fn parse_default_attr(attr: &syn::Attribute, ident: &Ident) -> syn::Result<ColumnAttr> {
    if let Ok(expr) = attr.parse_args::<syn::Expr>() {
        return Ok(ColumnAttr::Default(expr, ident.span()));
    }

    Err(syn::Error::new_spanned(
        &attr.meta,
        "expected default value in format `#[default(CONSTANT_VALUE)]`",
    ))
}

pub(crate) fn table_impl(mut args: TableArgs, item: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let vis = &item.vis;
    let sats_ty = sats::sats_type_from_derive(item, quote!(spacetimedb::spacetimedb_lib))?;

    let original_struct_ident = sats_ty.ident;
    let table_ident = &args.name;
    let view_trait_ident = format_ident!("{}__view", table_ident);
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

    for (i, field) in fields.iter().enumerate() {
        let col_num = i as u16;
        let field_ident = field.ident.unwrap();

        let mut unique = None;
        let mut auto_inc = None;
        let mut primary_key = None;
        let mut default_value = None;
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
                ColumnAttr::Default(expr, span) => {
                    check_duplicate(&default_value, span)?;
                    default_value = Some(expr);
                }
            }
        }

        if let Some(default_value) = &default_value {
            if auto_inc.is_some() || primary_key.is_some() || unique.is_some() {
                return Err(syn::Error::new(
                    default_value.span(),
                    "invalid combination: auto_inc, unique index or primary key cannot have a default value",
                ));
            };
        }

        let column = Column {
            index: col_num,
            ident: field_ident,
            vis: field.vis,
            ty: field.ty,
            default_value,
        };

        if unique.is_some() || primary_key.is_some() {
            unique_columns.push(column.clone());
        }
        if auto_inc.is_some() {
            sequenced_columns.push(column.clone());
        }
        if let Some(span) = primary_key {
            check_duplicate_msg(&primary_key_column, span, "can only have one primary key per table")?;
            primary_key_column = Some(column.clone());
        }

        columns.push(column.clone());
    }

    let row_type = quote!(#original_struct_ident);

    // Mark all indices with a single column matching a unique constraint as unique.
    // For all the unpaired unique columns, create a unique index.
    for unique_col in &unique_columns {
        if args.indices.iter_mut().any(|index| {
            let covered_by_index = match &index.kind {
                IndexType::BTree { columns } => &**columns == slice::from_ref(unique_col.ident),
                IndexType::Direct { column } => column == unique_col.ident,
            };
            index.is_unique |= covered_by_index;
            covered_by_index
        }) {
            continue;
        }
        // NOTE(centril): We pick `btree` here if the user does not specify otherwise,
        // as it's the safest choice of index for the general case,
        // even if isn't optimal in specific cases.
        let name = unique_col.ident.clone();
        let columns = vec![name.clone()];
        args.indices.push(IndexArg {
            name,
            is_unique: true,
            kind: IndexType::BTree { columns },
        })
    }

    let mut indices = args
        .indices
        .iter()
        .map(|index| index.validate(&table_name, &columns))
        .collect::<syn::Result<Vec<_>>>()?;
    // Order unique accessors before index accessors.
    indices.sort_by_key(|index| !index.is_unique);

    let tablehandle_ident = format_ident!("{}__TableHandle", table_ident);
    let viewhandle_ident = format_ident!("{}__ViewHandle", table_ident);

    let index_descs = indices.iter().map(|index| index.desc());
    let index_accessors_rw = indices
        .iter()
        .map(|index| index.accessor(vis, original_struct_ident, &tablehandle_ident, AccessorType::ReadWrite));
    let index_accessors_ro = indices
        .iter()
        .map(|index| index.accessor(vis, original_struct_ident, &tablehandle_ident, AccessorType::Read));
    let index_marker_types = indices.iter().map(|index| index.marker_type(vis, &tablehandle_ident));

    // Generate `integrate_generated_columns`
    // which will integrate all generated auto-inc col values into `_row`.
    let integrate_gen_col = sequenced_columns.iter().map(|col| {
        let field = col.ident;
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
    let primary_col_id = primary_key_column.clone().into_iter().map(|col| col.index);
    let sequence_col_ids = sequenced_columns.iter().map(|col| col.index);

    let default_type_check: TokenStream = columns
        .iter()
        .filter_map(|col| {
            if let Some(val) = &col.default_value {
                let ty = &col.ty;
                let ident_span = col.ident.span();
                Some(quote_spanned! { ident_span =>
                    // This closure enforces that `val` is of type `ty` at compile-time.
                    let _check: #ty = #val;
                })
            } else {
                None
            }
        })
        .collect();

    let col_defaults: Vec<TokenStream> = columns.iter().filter_map(|col| {
        if let Some(val) = &col.default_value {
            let col_id = col.index;
            Some(quote! {
                spacetimedb::table::ColumnDefault {
                    col_id: #col_id,
                    value: #val.serialize(spacetimedb::sats::algebraic_value::ser::ValueSerializer).expect("default value serialization failed"),
                },
            })
        } else {
            None
        }
    }).collect();

    let default_fn: TokenStream = quote! {
        fn get_default_col_values() -> Vec<spacetimedb::table::ColumnDefault> {
            [#(#col_defaults)*].to_vec()
        }
    };

    let (schedule, schedule_typecheck) = args
        .scheduled
        .as_ref()
        .map(|sched| {
            let scheduled_at_column = match &sched.at {
                Some(at) => Some(find_column(&columns, at)?),
                None => try_find_column(&columns, "scheduled_at"),
            };
            // better error message when both are missing
            if scheduled_at_column.is_none() && primary_key_column.is_none() {
                return Err(syn::Error::new(
                    sched.span,
                    "scheduled table missing required columns; add these to your struct:\n\
                             #[primary_key]\n\
                             #[auto_inc]\n\
                             scheduled_id: u64,\n\
                             scheduled_at: spacetimedb::ScheduleAt,",
                ));
            }
            let scheduled_at_column = scheduled_at_column.ok_or_else(|| {
                syn::Error::new(
                    sched.span,
                    "scheduled tables must have a `scheduled_at: spacetimedb::ScheduleAt` column. \
                             if the column has a name besides `scheduled_at`, you can specify it with \
                             `scheduled(my_reducer, at = custom_scheduled_at)`",
                )
            })?;
            let primary_key_column = primary_key_column.ok_or_else(|| {
                syn::Error::new(
                    sched.span,
                    "scheduled tables must have a `#[primary_key] #[auto_inc] scheduled_id: u64` column",
                )
            })?;

            let reducer_or_procedure = &sched.reducer_or_procedure;
            let scheduled_at_id = scheduled_at_column.index;
            let desc = quote!(spacetimedb::table::ScheduleDesc {
                reducer_or_procedure_name: <#reducer_or_procedure as spacetimedb::rt::FnInfo>::NAME,
                scheduled_at_column: #scheduled_at_id,
            });

            let primary_key_ty = primary_key_column.ty;
            let scheduled_at_ty = scheduled_at_column.ty;
            let typecheck = quote! {
                spacetimedb::rt::scheduled_typecheck::<
                    #original_struct_ident,
                    <#reducer_or_procedure as spacetimedb::rt::FnInfo>::FnKind,
                >(#reducer_or_procedure);
                spacetimedb::rt::assert_scheduled_table_primary_key::<#primary_key_ty>();
                let _ = |x: #scheduled_at_ty| { let _: spacetimedb::ScheduleAt = x; };
            };

            Ok((desc, typecheck))
        })
        .transpose()?
        .unzip();
    let schedule = schedule.into_iter();

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
        use spacetimedb::Serialize;
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
            #default_fn
        }
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{table_ident}");

    let describe_table_func = quote! {
        #[export_name = #register_describer_symbol]
        extern "C" fn __register_describer() {
            spacetimedb::rt::register_table::<#tablehandle_ident>()
        }
    };

    // Output all macro data
    let trait_def = quote_spanned! {table_ident.span()=>
        #[allow(non_camel_case_types, dead_code)]
        #vis trait #table_ident {
            fn #table_ident(&self) -> &#tablehandle_ident;
        }
        impl #table_ident for spacetimedb::Local {
            fn #table_ident(&self) -> &#tablehandle_ident {
                &#tablehandle_ident {}
            }
        }
    };

    let trait_def_view = quote_spanned! {table_ident.span()=>
        #[allow(non_camel_case_types, dead_code)]
        #vis trait #view_trait_ident {
            fn #table_ident(&self) -> &#viewhandle_ident;
        }
        impl #view_trait_ident for spacetimedb::LocalReadOnly {
            #[inline]
            fn #table_ident(&self) -> &#viewhandle_ident {
                &#viewhandle_ident {}
            }
        }
    };

    let tablehandle_def = quote! {
        #[allow(non_camel_case_types)]
        #[non_exhaustive]
        #vis struct #tablehandle_ident {}
    };

    let viewhandle_def = quote! {
        #[allow(non_camel_case_types)]
        #[non_exhaustive]
        #vis struct #viewhandle_ident {}
    };

    let emission = quote! {
        const _: () = {
            #(let _ = <#field_types as spacetimedb::rt::TableColumn>::_ITEM;)*
            #schedule_typecheck
            #default_type_check
        };

        #trait_def
        #trait_def_view

        #tablehandle_def
        #viewhandle_def

        const _: () = {
            impl #tablehandle_ident {
                #(#index_accessors_rw)*
            }

            impl #viewhandle_ident {
                #(#index_accessors_ro)*
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
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        {
            #![allow(clippy::disallowed_macros)]
            println!("{emission}");
        }
    }

    Ok(emission)
}
