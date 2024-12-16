use std::hash::{self, BuildHasher};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::LitStr;

pub(crate) struct FilterArg {
    sql: LitStr,
}
impl Parse for FilterArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        use spacetimedb_sql_parser::parser::sub;

        let sql = input.parse::<LitStr>()?;
        // Checks the query is syntactically valid
        let _ = sub::parse_subscription(&sql.value()).map_err(|e| syn::Error::new(sql.span(), e))?;

        Ok(Self { sql })
    }
}

// DefaultHasher::default() is not randomized, so the macro is still deterministic.
type Hasher = hash::BuildHasherDefault<hash::DefaultHasher>;

pub(crate) fn filter_impl(arg: FilterArg) -> syn::Result<TokenStream> {
    let rls_sql = arg.sql;

    let rls_name = format_ident!("rls_{}", Hasher::default().hash_one(rls_sql.value()));

    let register_rls_symbol = format!("__preinit__20_register_{rls_name}");

    let generated_describe_function = quote! {
        #[export_name = #register_rls_symbol]
        extern "C" fn __register_rls() {
            spacetimedb::rt::register_row_level_security::<#rls_name>()
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
            #[allow(non_camel_case_types)]
            struct #rls_name;
            impl spacetimedb::rt::RowLevelSecurityInfo for #rls_name {
                const SQL: &'static str = #rls_sql;
            }
        };
    })
}
