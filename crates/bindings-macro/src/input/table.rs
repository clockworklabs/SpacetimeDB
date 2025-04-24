use super::sats;
use super::sats::SatsField;
use super::sym;
use super::util::{check_duplicate, check_duplicate_msg, match_meta};
use core::slice;
use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::meta::ParseNestedMeta;
use syn::parse::Parse;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Ident, Path, Token};

pub struct TableArgs {
    pub access: Option<TableAccess>,
    pub scheduled: Option<ScheduledArg>,
    pub name: Ident,
    pub indices: Vec<IndexArg>,
}

pub enum TableAccess {
    Public(Span),
    Private(Span),
}

pub struct ScheduledArg {
    pub span: Span,
    pub reducer: Path,
    pub at: Option<Ident>,
}

pub struct IndexArg {
    pub name: Ident,
    pub is_unique: bool,
    pub kind: IndexType,
}

impl IndexArg {
    fn new(name: Ident, kind: IndexType) -> Self {
        // We don't know if its unique yet.
        // We'll discover this once we have collected constraints.
        let is_unique = false;
        Self { name, is_unique, kind }
    }
}

pub enum IndexType {
    BTree { columns: Vec<Ident> },
    Direct { column: Ident },
}

impl TableArgs {
    pub fn parse(input: TokenStream, item: &syn::DeriveInput) -> syn::Result<Self> {
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
        let name: Ident = name.ok_or_else(|| {
            let table = item.ident.to_string().to_snake_case();
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
        let mut reducer = None;
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
                check_duplicate_msg(&reducer, &meta, "can only specify one scheduled reducer")?;
                reducer = Some(meta.path);
            }
            Ok(())
        })?;

        let reducer = reducer.ok_or_else(|| {
            meta.error("must specify scheduled reducer associated with the table: scheduled(reducer_name)")
        })?;
        Ok(Self { span, reducer, at })
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
}

pub struct ColumnArgs<'a> {
    pub original_struct_name: Ident,
    pub fields: Vec<SatsField<'a>>,
    pub columns: Vec<Column<'a>>,
    pub unique_columns: Vec<Column<'a>>,
    pub sequenced_columns: Vec<Column<'a>>,
    pub primary_key_column: Option<Column<'a>>,
}

impl<'a> ColumnArgs<'a> {
    pub fn parse(mut table: TableArgs, item: &'a syn::DeriveInput) -> syn::Result<(TableArgs, Self)> {
        let sats_ty = sats::sats_type_from_derive(item, quote!(spacetimedb::spacetimedb_lib))?;

        let original_struct_name = sats_ty.ident.clone();

        let sats::SatsTypeData::Product(fields) = &sats_ty.data else {
            return Err(syn::Error::new(Span::call_site(), "spacetimedb table must be a struct"));
        };

        if fields.len() > u16::MAX.into() {
            return Err(syn::Error::new_spanned(
                item,
                "too many columns; the most a table can have is 2^16",
            ));
        }

        let mut columns: Vec<Column<'a>> = vec![];
        let mut unique_columns: Vec<Column<'a>> = vec![];
        let mut sequenced_columns: Vec<Column<'a>> = vec![];
        let mut primary_key_column: Option<Column<'a>> = None;

        for (i, field) in fields.iter().enumerate() {
            let col_num = i as u16;
            let field_ident = field.ident.unwrap();

            let mut unique = None;
            let mut auto_inc = None;
            let mut primary_key = None;
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
                    ColumnAttr::Index(index_arg) => table.indices.push(index_arg),
                }
            }

            let column: Column<'a> = Column {
                index: col_num,
                ident: field_ident,
                vis: field.vis,
                ty: field.ty,
            };

            if unique.is_some() || primary_key.is_some() {
                unique_columns.push(column);
            }
            if auto_inc.is_some() {
                sequenced_columns.push(column);
            }
            if let Some(span) = primary_key {
                check_duplicate_msg(&primary_key_column, span, "can only have one primary key per table")?;
                primary_key_column = Some(column);
            }

            columns.push(column);
        }

        // Mark all indices with a single column matching a unique constraint as unique.
        // For all the unpaired unique columns, create a unique index.
        for unique_col in &unique_columns {
            if table.indices.iter_mut().any(|index| {
                let covered_by_index = match &index.kind {
                    IndexType::BTree { columns } => &*columns == slice::from_ref(unique_col.ident),
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
            table.indices.push(IndexArg {
                name,
                is_unique: true,
                kind: IndexType::BTree { columns },
            })
        }

        Ok((
            table,
            ColumnArgs {
                original_struct_name,
                fields: fields.to_vec(),
                columns,
                unique_columns,
                sequenced_columns,
                primary_key_column,
            },
        ))
    }
}

#[derive(Copy, Clone)]
pub struct Column<'a> {
    pub index: u16,
    pub vis: &'a syn::Visibility,
    pub ident: &'a syn::Ident,
    pub ty: &'a syn::Type,
}

pub enum ColumnAttr {
    Unique(Span),
    AutoInc(Span),
    PrimaryKey(Span),
    Index(IndexArg),
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
        } else {
            None
        })
    }
}
