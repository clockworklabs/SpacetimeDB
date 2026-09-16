use heck::ToUpperCamelCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::{ParseStream, Parser};
use syn::{parse_quote, Result};

use crate::util::{check_duplicate, match_meta, superize_path, HexLiteral};
use crate::{preinit, sats, sym};

pub(crate) struct MigrationArgs {
    function: syn::Path,
    hash: HexLiteral<32>,
}

impl MigrationArgs {
    pub(crate) fn parse(input: TokenStream) -> syn::Result<Self> {
        let mut hash = None;
        let mut function = None;
        syn::meta::parser(|meta| {
            match_meta!(match meta {
                sym::function => {
                    check_duplicate(&function, &meta)?;
                    function = Some(meta.value()?.parse()?);
                }
                sym::hash => {
                    check_duplicate(&hash, &meta)?;
                    hash = Some(meta.value()?.parse()?);
                }
            });
            Ok(())
        })
        .parse2(input)?;
        let hash = hash.ok_or_else(|| syn::Error::new(Span::call_site(), "must specify hash"))?;
        let function = function.ok_or_else(|| syn::Error::new(Span::call_site(), "must specify function"))?;

        Ok(MigrationArgs { hash, function })
    }
}

pub(crate) struct MigrationData<'a> {
    pub(crate) tablehandle_idents: Vec<Ident>,
    pub(crate) db_view_ident: Ident,
    pub(crate) hash: &'a HexLiteral<32>,
}

pub(crate) fn migration_impl(args: MigrationArgs, mod_: &mut syn::ItemMod) -> Result<TokenStream> {
    let Some((_, content)) = &mut mod_.content else {
        return Err(syn::Error::new_spanned(
            mod_,
            "#[spacetimedb::migration] must be applied to an inline module",
        ));
    };

    let mod_ident = &mod_.ident;
    let db_view_ident = Ident::new(&mod_ident.to_string().to_upper_camel_case(), mod_ident.span());

    let mut migration_data = MigrationData {
        tablehandle_idents: vec![],
        db_view_ident: db_view_ident.clone(),
        hash: &args.hash,
    };

    let mut error: Option<syn::Error> = None;
    let mut add_error = |e| {
        if let Some(error) = &mut error {
            error.combine(e)
        } else {
            error = Some(e)
        }
    };

    let mut extra_items: Vec<TokenStream> = vec![];

    for item in &mut *content {
        match process_item(item, &mut migration_data) {
            Ok(tokens) => extra_items.push(tokens),
            Err(e) => add_error(e),
        }
    }

    for tokens in extra_items {
        (|input: ParseStream| {
            while !input.is_empty() {
                content.push(input.parse()?);
            }
            Ok(())
        })
        .parse2(tokens)
        .expect("bad output from table_impl");
    }

    let register = quote!(spacetimedb::rt::register_migration::<#db_view_ident>());
    let register_func = preinit(20, "register_migration", args.hash.as_hex(), register);

    let hash_array = args.hash.to_array();
    let function = superize_path(args.function);
    let table_idents = &migration_data.tablehandle_idents;
    content.extend([
        parse_quote!(#[non_exhaustive] pub(super) struct #db_view_ident {}),
        parse_quote! {
            impl spacetimedb::rt::MigrationModule for #db_view_ident {
                const HASH: spacetimedb::spacetimedb_lib::Hash = spacetimedb::spacetimedb_lib::Hash::from_byte_array(#hash_array);
                fn invoke() -> Result<(), Box<str>> {
                    spacetimedb::rt::invoke_migration(Self {}, #function)
                }
                fn describe_tables(migration: &mut spacetimedb::rt::MigrationBuilder) {
                    #(migration.add_table::<#table_idents>();)*
                }
            }
        },
        syn::parse2(register_func).unwrap(),
    ]);

    let error = error.map(syn::Error::into_compile_error);

    Ok(quote!(use #mod_ident::#db_view_ident; #error))
}

fn process_item(item: &mut syn::Item, migration: &mut MigrationData) -> Result<TokenStream> {
    let syn::Item::Struct(struc) = item else {
        return Err(syn::Error::new_spanned(item, "must be struct"));
    };
    if let syn::Visibility::Inherited = struc.vis {
        struc.vis = parse_quote!(pub(super));
    }
    let Some(attr) = struc
        .attrs
        .extract_if(.., |attr| {
            attr.path().segments.last().is_some_and(|x| x.ident == "table")
        })
        .next()
    else {
        return Ok(Default::default());
    };

    let derive_table_helper = super::derive_table_helper_attr();
    if !struc.attrs.contains(&derive_table_helper) {
        struc.attrs.push(derive_table_helper);
    }

    let sats_ty = sats::extract_sats_type_from_item_struct(struc, quote!(spacetimedb::spacetimedb_lib))?;
    let args = attr.meta.require_list()?.tokens.clone();
    let args = crate::table::TableArgs::parse(args, attr.bracket_token.span.join(), &struc.ident)?;
    crate::table::table_impl_inner(args, &struc.vis, sats_ty, Some(migration))
}
