use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream, Parser};
use syn::spanned::Spanned;
use syn::{parse_quote, BinOp, Expr, ExprBinary, ExprLit, ExprUnary, Ident, Member, Token, Type, UnOp};

struct ClosureArg {
    // only ident for now as we want to do scope analysis and for now this makes things easier
    row_name: Ident,
    table_ty: Type,
}

impl Parse for ClosureArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<Token![|]>()?;
        let row_name = input.parse()?;
        input.parse::<Token![:]>()?;
        let table_ty = input.parse()?;
        input.parse::<Token![|]>()?;
        Ok(Self { row_name, table_ty })
    }
}

impl ClosureArg {
    fn expr_as_table_field<'e>(&self, expr: &'e Expr) -> syn::Result<&'e Ident> {
        match expr {
            Expr::Field(field)
                if match field.base.as_ref() {
                    Expr::Path(path) => path.path.is_ident(&self.row_name),
                    _ => false,
                } =>
            {
                match &field.member {
                    Member::Named(ident) => Ok(ident),
                    Member::Unnamed(index) => Err(syn::Error::new_spanned(index, "unnamed members are not allowed")),
                }
            }
            _ => Err(syn::Error::new_spanned(expr, "expected table field access")),
        }
    }

    fn make_rhs(&self, e: &mut Expr) -> syn::Result<()> {
        match e {
            // support `E::A`, `foobar`, etc. - any path except the `row` argument
            Expr::Path(path) if !path.path.is_ident(&self.row_name) => Ok(()),
            // support any field of a valid RHS expression - this makes it work like
            // Rust 2021 closures where `|| foo.bar.baz` captures only `foo.bar.baz`
            Expr::Field(field) => self.make_rhs(&mut field.base),
            // string literals need to be converted to their owned version for serialization
            Expr::Lit(ExprLit {
                lit: syn::Lit::Str(_), ..
            }) => {
                *e = parse_quote!(#e.to_owned());
                Ok(())
            }
            // other literals can be inlined into the AST as-is
            Expr::Lit(_) => Ok(()),
            // unary expressions can be also hoisted out to AST builder, in particular this
            // is important to support negative literals like `-123`
            Expr::Unary(ExprUnary { expr: arg, .. }) => self.make_rhs(arg),
            Expr::Group(group) => self.make_rhs(&mut group.expr),
            Expr::Paren(paren) => self.make_rhs(&mut paren.expr),
            _ => Err(syn::Error::new_spanned(
                e,
                "this expression is not supported in the right-hand side of the comparison",
            )),
        }
    }

    fn handle_cmp(&self, expr: &ExprBinary) -> syn::Result<TokenStream> {
        let left = self.expr_as_table_field(&expr.left)?;

        let mut right = expr.right.clone();
        self.make_rhs(&mut right)?;

        let table_ty = &self.table_ty;

        let lhs_field = quote_spanned!(left.span()=> <#table_ty as spacetimedb::spacetimedb_lib::filter::Table>::FieldIndex::#left as u8);

        let rhs = quote_spanned!(right.span()=> spacetimedb::spacetimedb_lib::filter::Rhs::Value(
            std::convert::identity::<<#table_ty as spacetimedb::query::FieldAccess::<{#lhs_field}>>::Field>(#right).into()
        ));

        let op = match expr.op {
            BinOp::Lt(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::Lt),
            BinOp::Le(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::LtEq),
            BinOp::Eq(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::Eq),
            BinOp::Ne(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::NotEq),
            BinOp::Ge(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::GtEq),
            BinOp::Gt(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::Gt),
            _ => unreachable!(),
        };

        Ok(
            quote_spanned!(expr.span()=> spacetimedb::spacetimedb_lib::filter::Expr::Cmp(spacetimedb::spacetimedb_lib::filter::Cmp {
                op: #op,
                args: spacetimedb::spacetimedb_lib::filter::CmpArgs {
                    lhs_field: #lhs_field,
                    rhs: #rhs,
                },
            })),
        )
    }

    fn handle_logic(&self, expr: &ExprBinary) -> syn::Result<TokenStream> {
        let op = match expr.op {
            BinOp::And(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpLogic::And),
            BinOp::Or(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpLogic::Or),
            _ => unreachable!(),
        };

        let left = self.handle_expr(&expr.left)?;
        let right = self.handle_expr(&expr.right)?;

        Ok(
            quote_spanned!(expr.span()=> spacetimedb::spacetimedb_lib::filter::Expr::Logic(spacetimedb::spacetimedb_lib::filter::Logic {
                lhs: Box::new(#left),
                op: #op,
                rhs: Box::new(#right),
            })),
        )
    }

    fn handle_binop(&self, expr: &ExprBinary) -> syn::Result<TokenStream> {
        match expr.op {
            BinOp::Lt(_) | BinOp::Le(_) | BinOp::Eq(_) | BinOp::Ne(_) | BinOp::Ge(_) | BinOp::Gt(_) => {
                self.handle_cmp(expr)
            }
            BinOp::And(_) | BinOp::Or(_) => self.handle_logic(expr),
            _ => Err(syn::Error::new_spanned(expr.op, "unsupported binary operator")),
        }
    }

    fn handle_unop(&self, expr: &ExprUnary) -> syn::Result<TokenStream> {
        let op = match expr.op {
            UnOp::Not(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpUnary::Not),
            _ => return Err(syn::Error::new_spanned(expr.op, "unsupported unary operator")),
        };

        let arg = self.handle_expr(&expr.expr)?;

        Ok(
            quote_spanned!(expr.span()=> spacetimedb::spacetimedb_lib::filter::Expr::Unary(spacetimedb::spacetimedb_lib::filter::Unary {
                op: #op,
                arg: Box::new(#arg),
            })),
        )
    }

    fn handle_expr(&self, expr: &Expr) -> syn::Result<TokenStream> {
        Ok(match expr {
            Expr::Binary(expr) => self.handle_binop(expr)?,
            Expr::Unary(expr) => self.handle_unop(expr)?,
            Expr::Group(group) => self.handle_expr(&group.expr)?,
            Expr::Paren(paren) => self.handle_expr(&paren.expr)?,
            expr => return Err(syn::Error::new_spanned(expr, "unsupported expression")),
        })
    }
}

fn handle_closure(arg: &ClosureArg, body: &Expr) -> syn::Result<TokenStream> {
    let table_ty = &arg.table_ty;
    let expr = arg.handle_expr(&body)?;

    Ok(quote_spanned!(body.span()=> {
        <#table_ty as spacetimedb::TableType>::iter_filtered(#expr)
    }))
}

pub(crate) fn query_impl(input: TokenStream) -> TokenStream {
    let parser = |input: ParseStream| {
        let arg = input.parse::<ClosureArg>()?;
        let body = input.parse::<Expr>();

        let result = body.and_then(|body| handle_closure(&arg, &body));

        let output = result.unwrap_or_else(|error| {
            let error = error.into_compile_error();
            let table_ty = &arg.table_ty;
            quote!(({
                #error
                // if the error was just in the body, but we know the table for the query,
                // still inform type inference that this expression will be a TableIter<$tablety>,
                // so that the rest of their code doesn't lose all type info in their IDE
                <#table_ty as spacetimedb::TableType>::iter()
            }))
        });

        Ok(output)
    };
    parser.parse2(input).unwrap_or_else(syn::Error::into_compile_error)
}
