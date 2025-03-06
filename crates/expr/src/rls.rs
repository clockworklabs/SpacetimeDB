use std::rc::Rc;

use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::TableId;
use spacetimedb_sql_parser::ast::BinOp;

use crate::{
    check::{parse_and_type_sub, SchemaView},
    expr::{Expr, FieldProject, LeftDeepJoin, ProjectList, ProjectName, RelExpr, Relvar},
};

/// The main driver of RLS resolution for subscription queries.
/// Mainly a wrapper around [resolve_views_for_expr].
pub fn resolve_views_for_sub(
    tx: &impl SchemaView,
    expr: ProjectName,
    auth: &AuthCtx,
    has_param: &mut bool,
) -> anyhow::Result<Vec<ProjectName>> {
    // RLS does not apply to the database owner
    if auth.caller == auth.owner {
        return Ok(vec![expr]);
    }

    let Some(return_name) = expr.return_name().map(|name| name.to_owned().into_boxed_str()) else {
        anyhow::bail!("Could not determine return type during RLS resolution")
    };

    // Unwrap the underlying `RelExpr`
    let expr = match expr {
        ProjectName::None(expr) | ProjectName::Some(expr, _) => expr,
    };

    resolve_views_for_expr(
        tx,
        expr,
        // Do not ignore the return table when checking for RLS rules
        None,
        // Resolve list is empty as we are not yet resolving any RLS rules
        Rc::new(ResolveList::None),
        has_param,
        &mut 0,
        auth,
    )
    .map(|fragments| {
        fragments
            .into_iter()
            // The expanded fragments could be join trees,
            // so wrap each of them in an outer project.
            .map(|expr| ProjectName::Some(expr, return_name.clone()))
            .collect()
    })
}

/// The main driver of RLS resolution for sql queries.
/// Mainly a wrapper around [resolve_views_for_expr].
pub fn resolve_views_for_sql(tx: &impl SchemaView, expr: ProjectList, auth: &AuthCtx) -> anyhow::Result<ProjectList> {
    // RLS does not apply to the database owner
    if auth.caller == auth.owner {
        return Ok(expr);
    }
    // The subscription language is a subset of the sql language.
    // Use the subscription helper if this is a compliant expression.
    // Use the generic resolver otherwise.
    let resolve_for_sub = |expr| resolve_views_for_sub(tx, expr, auth, &mut false);
    let resolve_for_sql = |expr| {
        resolve_views_for_expr(
            // Use all default values
            tx,
            expr,
            None,
            Rc::new(ResolveList::None),
            &mut false,
            &mut 0,
            auth,
        )
    };
    match expr {
        ProjectList::Limit(expr, n) => Ok(ProjectList::Limit(Box::new(resolve_views_for_sql(tx, *expr, auth)?), n)),
        ProjectList::Name(exprs) => Ok(ProjectList::Name(
            exprs
                .into_iter()
                .map(resolve_for_sub)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        )),
        ProjectList::List(exprs, fields) => Ok(ProjectList::List(
            exprs
                .into_iter()
                .map(resolve_for_sql)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
            fields,
        )),
        ProjectList::Agg(exprs, agg, ty) => Ok(ProjectList::Agg(
            exprs
                .into_iter()
                .map(resolve_for_sql)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
            agg,
            ty,
        )),
    }
}

/// A list for detecting cycles during RLS resolution.
enum ResolveList {
    None,
    Some(TableId, Rc<ResolveList>),
}

impl ResolveList {
    fn new(table_id: TableId, list: Rc<Self>) -> Rc<Self> {
        Rc::new(Self::Some(table_id, list))
    }

    fn contains(&self, table_id: &TableId) -> bool {
        match self {
            Self::None => false,
            Self::Some(id, suffix) if id != table_id => suffix.contains(table_id),
            Self::Some(..) => true,
        }
    }
}

/// The main utility responsible for view resolution.
///
/// But what is view resolution and why do we need it?
///
/// A view is a named query that can be referenced as though it were just a regular table.
/// In SpacetimeDB, Row Level Security (RLS) is implemented using views.
/// We must resolve/expand these views in order to guarantee the correct access controls.
///
/// Before we discuss the implementation, a quick word on `return_table_id`.
///
/// Why do we care about it?
/// What does it mean for it to be `None`?
///
/// If this IS NOT a user query, it must be a view definition.
/// In SpacetimeDB this means we're expanding an RLS filter.
/// RLS filters cannot be self-referential, meaning that within a filter,
/// we cannot recursively expand references to its return table.
///
/// However, a `None` value implies that this expression is a user query,
/// and so we should attempt to expand references to the return table.
///
/// Now back to the implementation.
///
/// Take the following join tree as an example:
/// ```text
///     x
///    / \
///   x   c
///  / \
/// a   b
/// ```
///
/// Let's assume b is a view with the following structure:
/// ```text
///     x
///    / \
///   x   f
///  / \
/// d   e
/// ```
///
/// Logically we just want to expand the tree like so:
/// ```text
///     x
///    / \
///   x   c
///  / \
/// a   x
///    / \
///   x   f
///  / \
/// d   e
/// ```
///
/// However the join trees at this level are left deep.
/// To maintain this invariant, the correct expansion would be:
/// ```text
///         x
///        / \
///       x   c
///      / \
///     x   f
///    / \
///   x   e
///  / \
/// a   d
/// ```
///
/// That is, the subtree whose root is the left sibling of the node being expanded,
/// i.e. the subtree rooted at `a` in the above example,
/// must be pushed below the leftmost leaf node of the view expansion.
fn resolve_views_for_expr(
    tx: &impl SchemaView,
    view: RelExpr,
    return_table_id: Option<TableId>,
    resolving: Rc<ResolveList>,
    has_param: &mut bool,
    suffix: &mut usize,
    auth: &AuthCtx,
) -> anyhow::Result<Vec<RelExpr>> {
    let is_return_table = |relvar: &Relvar| return_table_id.is_some_and(|id| relvar.schema.table_id == id);

    // Collect the table ids queried by this view.
    // Ignore the id of the return table, since RLS views cannot be recursive.
    let mut names = vec![];
    view.visit(&mut |expr| match expr {
        RelExpr::RelVar(rhs)
        | RelExpr::LeftDeepJoin(LeftDeepJoin { rhs, .. })
        | RelExpr::EqJoin(LeftDeepJoin { rhs, .. }, ..)
            if !is_return_table(rhs) =>
        {
            names.push((rhs.schema.table_id, rhs.alias.clone()));
        }
        _ => {}
    });

    // Are we currently resolving any of them?
    if let Some(table_id) = names
        .iter()
        .map(|(table_id, _)| table_id)
        .find(|table_id| resolving.contains(table_id))
    {
        anyhow::bail!("Discovered cyclic dependency when resolving RLS rules for table id `{table_id}`");
    }

    let return_name = |expr: &ProjectName| {
        expr.return_name()
            .map(|name| name.to_owned())
            .ok_or_else(|| anyhow::anyhow!("Could not resolve table reference in RLS filter"))
    };

    let mut view_def_fragments = vec![];

    for (table_id, alias) in names {
        let mut view_fragments = vec![];

        for sql in tx.rls_rules_for_table(table_id)? {
            // Parse and type check the RLS filter
            let (expr, is_parameterized) = parse_and_type_sub(&sql, tx, auth)?;

            // Are any of the RLS rules parameterized?
            *has_param = *has_param || is_parameterized;

            // We need to know which relvar is being returned for alpha-renaming
            let return_name = return_name(&expr)?;

            // Resolve views within the RLS filter itself
            let fragments = resolve_views_for_expr(
                tx,
                expr.unwrap(),
                Some(table_id),
                ResolveList::new(table_id, resolving.clone()),
                has_param,
                suffix,
                auth,
            )?;

            // Run alpha conversion on each view definition
            alpha_rename_fragments(
                // The revlar returned from the inner expression
                &return_name,
                // Its corresponding alias in the outer expression
                &alias,
                fragments,
                &mut view_fragments,
                suffix,
            );
        }

        if !view_fragments.is_empty() {
            view_def_fragments.push((table_id, alias, view_fragments));
        }
    }

    /// After we collect all the necessary view definitions and run alpha conversion,
    /// this function handles the actual replacement of the view with its definition.
    fn expand_views(expr: RelExpr, view_def_fragments: &[(TableId, Box<str>, Vec<RelExpr>)], out: &mut Vec<RelExpr>) {
        match view_def_fragments {
            [] => out.push(expr),
            [(table_id, alias, fragments), view_def_fragments @ ..] => {
                for fragment in fragments {
                    let expanded = expand_leaf(expr.clone(), *table_id, alias, fragment);
                    expand_views(expanded, view_def_fragments, out);
                }
            }
        }
    }

    let mut resolved = vec![];
    expand_views(view, &view_def_fragments, &mut resolved);
    Ok(resolved)
}

/// This is the main driver of alpha conversion.
///
/// For each expression that we alpha convert,
/// we append a unique suffix to the names in that expression,
/// with the one exception being the name of the return table.
/// The return table is aliased in the outer expression,
/// and so we use the same alias in the inner expression.
///
/// Ex.
///
/// Let `v` be a view defined as:
/// ```sql
/// SELECT r.* FROM r JOIN s ON r.id = s.id
/// ```
///
/// Take the following user query:
/// ```sql
/// SELECT t.* FROM v JOIN t ON v.id = t.id WHERE v.x = 0
/// ```
///
/// After alpha conversion, the expansion becomes:
/// ```sql
/// SELECT t.*
/// FROM r AS v
/// JOIN s AS s_1 ON v.id = s_1.id
/// JOIN t AS t   ON t.id = v.id WHERE v.x = 0
/// ```
fn alpha_rename_fragments(
    return_name: &str,
    outer_alias: &str,
    inputs: Vec<RelExpr>,
    output: &mut Vec<RelExpr>,
    suffix: &mut usize,
) {
    for mut fragment in inputs {
        *suffix += 1;
        alpha_rename(&mut fragment, &mut |name: &str| {
            if name == return_name {
                return outer_alias.to_owned().into_boxed_str();
            }
            (name.to_owned() + "_" + &suffix.to_string()).into_boxed_str()
        });
        output.push(fragment);
    }
}

/// When expanding a view, we must do an alpha conversion on the view definition.
/// This involves renaming the table aliases before replacing the view reference.
fn alpha_rename(expr: &mut RelExpr, f: &mut impl FnMut(&str) -> Box<str>) {
    /// Helper for renaming a relvar
    fn rename(relvar: &mut Relvar, f: &mut impl FnMut(&str) -> Box<str>) {
        relvar.alias = f(&relvar.alias);
    }
    /// Helper for renaming a field reference
    fn rename_field(field: &mut FieldProject, f: &mut impl FnMut(&str) -> Box<str>) {
        field.table = f(&field.table);
    }
    expr.visit_mut(&mut |expr| match expr {
        RelExpr::RelVar(rhs) | RelExpr::LeftDeepJoin(LeftDeepJoin { rhs, .. }) => {
            rename(rhs, f);
        }
        RelExpr::EqJoin(LeftDeepJoin { rhs, .. }, a, b) => {
            rename(rhs, f);
            rename_field(a, f);
            rename_field(b, f);
        }
        RelExpr::Select(_, expr) => {
            expr.visit_mut(&mut |expr| {
                if let Expr::Field(field) = expr {
                    rename_field(field, f);
                }
            });
        }
    });
}

/// Extends a left deep join tree with another.
///
/// Ex.
///
/// Assume `expr` is given by:
/// ```text
///     x
///    / \
///   x   f
///  / \
/// d   e
/// ```
///
/// Assume `with` is given by:
/// ```text
///     x
///    / \
///   x   c
///  / \
/// a   b
/// ```
///
/// This function extends `expr` by pushing `with` to the left-most leaf node:
/// ```text
///           x
///          / \
///         x   f
///        / \
///       x   e
///      / \
///     x   d
///    / \
///   x   c
///  / \
/// a   b
/// ```
fn extend_lhs(expr: RelExpr, with: RelExpr) -> RelExpr {
    match expr {
        RelExpr::RelVar(rhs) => RelExpr::LeftDeepJoin(LeftDeepJoin {
            lhs: Box::new(with),
            rhs,
        }),
        RelExpr::Select(input, expr) => RelExpr::Select(Box::new(extend_lhs(*input, with)), expr),
        RelExpr::LeftDeepJoin(join) => RelExpr::LeftDeepJoin(LeftDeepJoin {
            lhs: Box::new(extend_lhs(*join.lhs, with)),
            ..join
        }),
        RelExpr::EqJoin(join, a, b) => RelExpr::EqJoin(
            LeftDeepJoin {
                lhs: Box::new(extend_lhs(*join.lhs, with)),
                ..join
            },
            a,
            b,
        ),
    }
}

/// Replaces the leaf node determined by `table_id` and `alias` with the subtree `with`.
/// Ensures the expanded tree stays left deep.
fn expand_leaf(expr: RelExpr, table_id: TableId, alias: &str, with: &RelExpr) -> RelExpr {
    let ok = |relvar: &Relvar| relvar.schema.table_id == table_id && relvar.alias.as_ref() == alias;
    match expr {
        RelExpr::RelVar(relvar, ..) if ok(&relvar) => with.clone(),
        RelExpr::RelVar(..) => expr,
        RelExpr::Select(input, expr) => RelExpr::Select(Box::new(expand_leaf(*input, table_id, alias, with)), expr),
        RelExpr::LeftDeepJoin(join) if ok(&join.rhs) => extend_lhs(with.clone(), *join.lhs),
        RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs }) => RelExpr::LeftDeepJoin(LeftDeepJoin {
            lhs: Box::new(expand_leaf(*lhs, table_id, alias, with)),
            rhs,
        }),
        RelExpr::EqJoin(join, a, b) if ok(&join.rhs) => RelExpr::Select(
            Box::new(extend_lhs(with.clone(), *join.lhs)),
            Expr::BinOp(BinOp::Eq, Box::new(Expr::Field(a)), Box::new(Expr::Field(b))),
        ),
        RelExpr::EqJoin(LeftDeepJoin { lhs, rhs }, a, b) => RelExpr::EqJoin(
            LeftDeepJoin {
                lhs: Box::new(expand_leaf(*lhs, table_id, alias, with)),
                rhs,
            },
            a,
            b,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions as pretty;

    use spacetimedb_lib::{identity::AuthCtx, AlgebraicType, AlgebraicValue, Identity, ProductType};
    use spacetimedb_primitives::TableId;
    use spacetimedb_schema::{
        def::ModuleDef,
        schema::{Schema, TableSchema},
    };
    use spacetimedb_sql_parser::ast::BinOp;

    use crate::{
        check::{parse_and_type_sub, test_utils::build_module_def, SchemaView},
        expr::{Expr, FieldProject, LeftDeepJoin, ProjectName, RelExpr, Relvar},
    };

    use super::resolve_views_for_sub;

    pub struct SchemaViewer(pub ModuleDef);

    impl SchemaView for SchemaViewer {
        fn table_id(&self, name: &str) -> Option<TableId> {
            match name {
                "users" => Some(TableId(0)),
                "admins" => Some(TableId(1)),
                "player" => Some(TableId(2)),
                _ => None,
            }
        }

        fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableSchema>> {
            match table_id.idx() {
                0 => Some((TableId(0), "users")),
                1 => Some((TableId(1), "admins")),
                2 => Some((TableId(2), "player")),
                _ => None,
            }
            .and_then(|(table_id, name)| {
                self.0
                    .table(name)
                    .map(|def| Arc::new(TableSchema::from_module_def(&self.0, def, (), table_id)))
            })
        }

        fn rls_rules_for_table(&self, table_id: TableId) -> anyhow::Result<Vec<Box<str>>> {
            match table_id {
                TableId(0) => Ok(vec!["select * from users where identity = :sender".into()]),
                TableId(1) => Ok(vec!["select * from admins where identity = :sender".into()]),
                TableId(2) => Ok(vec![
                    "select player.* from player join users u on player.id = u.id".into(),
                    "select player.* from player join admins".into(),
                ]),
                _ => Ok(vec![]),
            }
        }
    }

    fn module_def() -> ModuleDef {
        build_module_def(vec![
            (
                "users",
                ProductType::from([("identity", AlgebraicType::identity()), ("id", AlgebraicType::U64)]),
            ),
            (
                "admins",
                ProductType::from([("identity", AlgebraicType::identity()), ("id", AlgebraicType::U64)]),
            ),
            (
                "player",
                ProductType::from([("id", AlgebraicType::U64), ("level_num", AlgebraicType::U64)]),
            ),
        ])
    }

    /// Parse, type check, and resolve RLS rules
    fn resolve(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> anyhow::Result<Vec<ProjectName>> {
        let (expr, _) = parse_and_type_sub(sql, tx, auth)?;
        resolve_views_for_sub(tx, expr, auth, &mut false)
    }

    #[test]
    fn test_rls_for_owner() -> anyhow::Result<()> {
        let tx = SchemaViewer(module_def());
        let auth = AuthCtx::new(Identity::ONE, Identity::ONE);
        let sql = "select * from users";
        let resolved = resolve(sql, &tx, &auth)?;

        let users_schema = tx.schema("users").unwrap();

        pretty::assert_eq!(
            resolved,
            vec![ProjectName::None(RelExpr::RelVar(Relvar {
                schema: users_schema,
                alias: "users".into(),
                delta: None,
            }))]
        );

        Ok(())
    }

    #[test]
    fn test_rls_for_non_owner() -> anyhow::Result<()> {
        let tx = SchemaViewer(module_def());
        let auth = AuthCtx::new(Identity::ZERO, Identity::ONE);
        let sql = "select * from users";
        let resolved = resolve(sql, &tx, &auth)?;

        let users_schema = tx.schema("users").unwrap();

        pretty::assert_eq!(
            resolved,
            vec![ProjectName::Some(
                RelExpr::Select(
                    Box::new(RelExpr::RelVar(Relvar {
                        schema: users_schema,
                        alias: "users".into(),
                        delta: None,
                    })),
                    Expr::BinOp(
                        BinOp::Eq,
                        Box::new(Expr::Field(FieldProject {
                            table: "users".into(),
                            field: 0,
                            ty: AlgebraicType::identity(),
                        })),
                        Box::new(Expr::Value(Identity::ONE.into(), AlgebraicType::identity()))
                    )
                ),
                "users".into()
            )]
        );

        Ok(())
    }

    #[test]
    fn test_multiple_rls_rules_for_table() -> anyhow::Result<()> {
        let tx = SchemaViewer(module_def());
        let auth = AuthCtx::new(Identity::ZERO, Identity::ONE);
        let sql = "select * from player where level_num = 5";
        let resolved = resolve(sql, &tx, &auth)?;

        let users_schema = tx.schema("users").unwrap();
        let admins_schema = tx.schema("admins").unwrap();
        let player_schema = tx.schema("player").unwrap();

        pretty::assert_eq!(
            resolved,
            vec![
                ProjectName::Some(
                    RelExpr::Select(
                        Box::new(RelExpr::Select(
                            Box::new(RelExpr::Select(
                                Box::new(RelExpr::LeftDeepJoin(LeftDeepJoin {
                                    lhs: Box::new(RelExpr::RelVar(Relvar {
                                        schema: player_schema.clone(),
                                        alias: "player".into(),
                                        delta: None,
                                    })),
                                    rhs: Relvar {
                                        schema: users_schema.clone(),
                                        alias: "u_2".into(),
                                        delta: None,
                                    },
                                })),
                                Expr::BinOp(
                                    BinOp::Eq,
                                    Box::new(Expr::Field(FieldProject {
                                        table: "u_2".into(),
                                        field: 0,
                                        ty: AlgebraicType::identity(),
                                    })),
                                    Box::new(Expr::Value(Identity::ONE.into(), AlgebraicType::identity())),
                                ),
                            )),
                            Expr::BinOp(
                                BinOp::Eq,
                                Box::new(Expr::Field(FieldProject {
                                    table: "player".into(),
                                    field: 0,
                                    ty: AlgebraicType::U64,
                                })),
                                Box::new(Expr::Field(FieldProject {
                                    table: "u_2".into(),
                                    field: 1,
                                    ty: AlgebraicType::U64,
                                })),
                            ),
                        )),
                        Expr::BinOp(
                            BinOp::Eq,
                            Box::new(Expr::Field(FieldProject {
                                table: "player".into(),
                                field: 1,
                                ty: AlgebraicType::U64,
                            })),
                            Box::new(Expr::Value(AlgebraicValue::U64(5), AlgebraicType::U64)),
                        ),
                    ),
                    "player".into(),
                ),
                ProjectName::Some(
                    RelExpr::Select(
                        Box::new(RelExpr::Select(
                            Box::new(RelExpr::LeftDeepJoin(LeftDeepJoin {
                                lhs: Box::new(RelExpr::RelVar(Relvar {
                                    schema: player_schema.clone(),
                                    alias: "player".into(),
                                    delta: None,
                                })),
                                rhs: Relvar {
                                    schema: admins_schema.clone(),
                                    alias: "admins_4".into(),
                                    delta: None,
                                },
                            })),
                            Expr::BinOp(
                                BinOp::Eq,
                                Box::new(Expr::Field(FieldProject {
                                    table: "admins_4".into(),
                                    field: 0,
                                    ty: AlgebraicType::identity(),
                                })),
                                Box::new(Expr::Value(Identity::ONE.into(), AlgebraicType::identity())),
                            ),
                        )),
                        Expr::BinOp(
                            BinOp::Eq,
                            Box::new(Expr::Field(FieldProject {
                                table: "player".into(),
                                field: 1,
                                ty: AlgebraicType::U64,
                            })),
                            Box::new(Expr::Value(AlgebraicValue::U64(5), AlgebraicType::U64)),
                        ),
                    ),
                    "player".into(),
                ),
            ]
        );

        Ok(())
    }
}
