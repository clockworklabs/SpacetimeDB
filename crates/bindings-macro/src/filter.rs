use std::hash::{DefaultHasher, Hash, Hasher};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{ParseStream, Parser};

fn parse_sql(input: ParseStream) -> syn::Result<String> {
    use spacetimedb_sql_parser::parser::sub;

    let lookahead = input.lookahead1();
    let sql = if lookahead.peek(syn::LitStr) {
        let s = input.parse::<syn::LitStr>()?;
        // Checks the query is syntactically valid
        let _ = sub::parse_subscription(&s.value()).map_err(|e| syn::Error::new(s.span(), format_args!("{e}")))?;

        s.value()
    } else {
        return Err(lookahead.error());
    };

    Ok(sql)
}

pub(crate) fn filter_impl(input: TokenStream) -> syn::Result<TokenStream> {
    let rls_sql = parse_sql.parse2(input)?;

    let mut hasher = DefaultHasher::new();
    rls_sql.hash(&mut hasher);
    let rls_name = format_ident!("rls_{}", hasher.finish());

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
        };
        #[allow(non_camel_case_types)]
        struct #rls_name { _never: ::core::convert::Infallible }
        impl spacetimedb::rt::RowLevelSecurityInfo for #rls_name {
            const SQL: &'static str = #rls_sql;
        }
    })
}
