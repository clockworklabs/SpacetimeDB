use crate::util::cvt_attr;
use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use syn::{parse::Parse, parse::ParseStream, token::Comma, Ident, ItemStruct, LitStr, Token};

/// Implements the `#[spacetimedb::guard(...)]` attribute,
/// which wraps the existing RLS mechanism by generating a `const` annotated with
/// `#[client_visibility_filter]` that registers the provided SQL.
///
/// Supported forms:
/// - Positional SQL literal only:
///     #[spacetimedb::guard("SELECT ...")]
/// - Optional name plus SQL literal:
///     #[spacetimedb::guard(MY_FILTER_NAME, "SELECT ...")]
///
/// If no explicit name is provided, a unique const identifier is generated using a hash
/// of the struct name and SQL, allowing multiple guards on the same table.
pub fn guard(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    cvt_attr::<ItemStruct>(
        args,
        item,
        quote!(),
        |args_ts: TokenStream, original_struct: &ItemStruct| {
            let parsed: GuardArgs = match syn::parse2::<GuardArgs>(args_ts.clone()) {
                Ok(p) => p,
                Err(_) => {
                    let sql: LitStr = syn::parse2(args_ts)?;
                    GuardArgs { name: None, sql }
                }
            };
            let GuardArgs { name, sql } = parsed;

            // Choose const identifier:
            // - If `name` was provided, use it.
            // - Otherwise, derive a unique name from the struct and SQL using a stable hash.
            let guard_ident = if let Some(name) = name {
                name
            } else {
                let base = original_struct.ident.to_string();
                let mut hasher = DefaultHasher::new();
                base.hash(&mut hasher);
                sql.value().hash(&mut hasher);
                let h = hasher.finish();
                format_ident!("__{}_RLS_GUARD_{:016X}", base.to_uppercase(), h)
            };

            // Generate a const binding that reuses the existing RLS registration machinery.
            // We fully qualify `spacetimedb::Filter` and the `#[spacetimedb::client_visibility_filter]` attribute
            // so users don't need to import anything extra.
            Ok(quote! {
                #[spacetimedb::client_visibility_filter]
                const #guard_ident: spacetimedb::Filter = spacetimedb::Filter::Sql(#sql);
            })
        },
    )
}

struct GuardArgs {
    name: Option<Ident>,
    sql: LitStr,
}

impl Parse for GuardArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error("expected a string literal SQL argument, or (NAME, \"SQL...\")"));
        }

        // If it starts with a string literal, it's the simple form.
        if input.peek(LitStr) {
            let sql: LitStr = input.parse()?;
            if !input.is_empty() {
                // If anything remains, require it to be trailing commas/whitespace only.
                let _ = input.parse::<Comma>();
                if !input.is_empty() {
                    return Err(input.error("unexpected tokens after SQL literal"));
                }
            }
            return Ok(Self { name: None, sql });
        }

        // Otherwise, expect IDENT, ',', LITSTR
        let name: Ident = input.parse()?;
        let _comma: Token![,] = input.parse()?;
        let sql: LitStr = input.parse()?;
        if !input.is_empty() {
            let _ = input.parse::<Comma>();
            if !input.is_empty() {
                return Err(input.error("unexpected tokens after (NAME, \"SQL...\")"));
            }
        }
        Ok(Self { name: Some(name), sql })
    }
}
