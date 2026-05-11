use spacetimedb_data_structures::map::HashSet;
use spacetimedb_lib::{query::Delta, AlgebraicType, AlgebraicValue};
use spacetimedb_primitives::{TableId, ViewId};
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_schema::{identifier::Identifier, schema::TableOrViewSchema};
use spacetimedb_sql_parser::ast::{BinOp, LogOp};
use std::sync::Arc;

/// Maximum allowed hop depth for variable-length path expansion.
pub const MAX_VARIABLE_LENGTH_HOPS: u32 = 16;

pub trait CollectViews {
    fn collect_views(&self, views: &mut HashSet<ViewId>);
}

impl<T: CollectViews> CollectViews for Arc<T> {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        self.as_ref().collect_views(views);
    }
}

impl<T: CollectViews> CollectViews for Vec<T> {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        for item in self {
            item.collect_views(views);
        }
    }
}

/// A projection is the root of any relational expression.
/// This type represents a projection that returns relvars.
///
/// For example:
///
/// ```sql
/// select * from t
/// ```
///
/// and
///
/// ```sql
/// select t.* from t join s ...
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum ProjectName {
    None(RelExpr),
    Some(RelExpr, RawIdentifier),
}

impl CollectViews for ProjectName {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        match self {
            Self::None(expr) | Self::Some(expr, _) => expr.collect_views(views),
        }
    }
}

impl ProjectName {
    /// Unwrap the outer projection, returning the inner expression
    pub fn unwrap(self) -> RelExpr {
        match self {
            Self::None(expr) | Self::Some(expr, _) => expr,
        }
    }

    /// What is the name of the return table?
    /// This is either the table name itself or its alias.
    pub fn return_name(&self) -> Option<&RawIdentifier> {
        match self {
            Self::None(input) => input.return_name(),
            Self::Some(_, name) => Some(name),
        }
    }

    /// The [`TableOrViewSchema`] of the returned rows.
    /// Note this expression returns rows from a relvar.
    /// Hence it this method should never return [None].
    pub fn return_table(&self) -> Option<&TableOrViewSchema> {
        match self {
            Self::None(input) => input.return_table(),
            Self::Some(input, alias) => input.find_table_schema(alias),
        }
    }

    /// The [TableId] of the returned rows.
    /// Note this expression returns rows from a relvar.
    /// Hence it this method should never return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        match self {
            Self::None(input) => input.return_table_id(),
            Self::Some(input, alias) => input.find_table_id(alias),
        }
    }

    /// Iterate over the returned column names and types
    pub fn for_each_return_field(&self, mut f: impl FnMut(&Identifier, &AlgebraicType)) {
        if let Some(schema) = self.return_table() {
            for schema in schema.public_columns() {
                f(&schema.col_name, &schema.col_type);
            }
        }
    }
}

/// A projection is the root of any relational expression.
/// This type represents a projection that returns fields.
///
/// For example:
///
/// ```sql
/// select a, b from t
/// ```
///
/// and
///
/// ```sql
/// select t.a as x from t join s ...
/// ```
///
/// Note that RLS takes a single expression and produces a list of expressions.
/// Hence why these variants take lists rather than single expressions.
///
/// Why does RLS take an expression and produce a list?
///
/// There may be multiple RLS rules associated to a single table.
/// Semantically these rules represent a UNION over that table,
/// and this corresponds to a UNION in the original expression.
///
/// TODO: We should model the UNION explicitly in the physical plan.
///
/// Ex.
///
/// Let's say we have the following rules for the `users` table:
/// ```rust
/// use spacetimedb::client_visibility_filter;
/// use spacetimedb::Filter;
///
/// #[client_visibility_filter]
/// const USER_FILTER: Filter = Filter::Sql(
///     "SELECT users.* FROM users WHERE identity = :sender"
/// );
///
/// #[client_visibility_filter]
/// const ADMIN_FILTER: Filter = Filter::Sql(
///     "SELECT users.* FROM users JOIN admins"
/// );
/// ```
///
/// The user query
/// ```sql
/// SELECT * FROM users WHERE level > 5
/// ```
///
/// essentially resolves to
/// ```sql
/// SELECT users.*
/// FROM users
/// WHERE identity = :sender AND level > 5
///
/// UNION ALL
///
/// SELECT users.*
/// FROM users JOIN admins
/// WHERE users.level > 5
/// ```
#[derive(Debug)]
pub enum ProjectList {
    Name(Vec<ProjectName>),
    List(Vec<RelExpr>, Vec<(RawIdentifier, FieldProject)>),
    Limit(Box<ProjectList>, u64),
    Agg(Vec<RelExpr>, AggType, RawIdentifier, AlgebraicType),
}

#[derive(Debug)]
pub enum AggType {
    Count,
}

impl CollectViews for ProjectList {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        match self {
            Self::Limit(proj, _) => {
                proj.collect_views(views);
            }
            Self::Name(exprs) => {
                for expr in exprs {
                    expr.collect_views(views);
                }
            }
            Self::List(exprs, _) | Self::Agg(exprs, ..) => {
                for expr in exprs {
                    expr.collect_views(views);
                }
            }
        }
    }
}

impl ProjectList {
    /// Does this expression project a single relvar?
    /// If so, we return it's [`TableOrViewSchema`].
    /// If not, it projects a list of columns, so we return [None].
    pub fn return_table(&self) -> Option<&TableOrViewSchema> {
        match self {
            Self::Name(project) => project.first().and_then(|expr| expr.return_table()),
            Self::Limit(input, _) => input.return_table(),
            Self::List(..) | Self::Agg(..) => None,
        }
    }

    /// Does this expression project a single relvar?
    /// If so, we return it's [TableId].
    /// If not, it projects a list of columns, so we return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        match self {
            Self::Name(project) => project.first().and_then(|expr| expr.return_table_id()),
            Self::Limit(input, _) => input.return_table_id(),
            Self::List(..) | Self::Agg(..) => None,
        }
    }

    /// Iterate over the projected column names and types
    pub fn for_each_return_field(&self, mut f: impl FnMut(&RawIdentifier, &AlgebraicType)) {
        match self {
            Self::Name(input) => {
                input
                    .first()
                    .inspect(|expr| expr.for_each_return_field(|n, at| f(n.as_raw(), at)));
            }
            Self::Limit(input, _) => {
                input.for_each_return_field(f);
            }
            Self::List(_, fields) => {
                for (name, FieldProject { ty, .. }) in fields {
                    f(name, ty);
                }
            }
            Self::Agg(_, _, name, ty) => f(name, ty),
        }
    }
}

/// A logical relational expression
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelExpr {
    /// A relvar or table reference
    RelVar(Relvar),
    /// A logical select for filter
    Select(Box<RelExpr>, Expr),
    /// A left deep binary cross product
    LeftDeepJoin(LeftDeepJoin),
    /// A left deep binary equi-join
    EqJoin(LeftDeepJoin, FieldProject, FieldProject),
    /// A bounded variable-length path traversal (e.g. `(a)-[*1..3]->(b)`)
    VariableLengthJoin(VariableLengthPath),
}

/// A bounded variable-length path traversal.
///
/// Represents `(start)-[*min..max]->(end)` in Cypher. At execution time,
/// this is expanded into a UNION of fixed-depth EqJoin chains (one per
/// hop count from `min_hops` to `max_hops`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableLengthPath {
    /// The base expression up to (and including) the start node
    pub lhs: Box<RelExpr>,
    /// Alias for the start vertex in the path
    pub start_alias: RawIdentifier,
    /// Alias for the end vertex (bound in the Cypher query)
    pub end_alias: RawIdentifier,
    /// Schema of the Edge table
    pub edge_schema: Arc<TableOrViewSchema>,
    /// Schema of the Vertex table
    pub vertex_schema: Arc<TableOrViewSchema>,
    /// Optional relationship type filter (e.g. `:KNOWS`)
    pub rel_type: Option<String>,
    /// Edge column that joins to the near (start-side) vertex Id
    pub edge_col_near: String,
    /// Edge column that joins to the far (end-side) vertex Id
    pub edge_col_far: String,
    /// Column name for vertex Id
    pub vertex_id_col: String,
    /// Minimum hops (inclusive, >= 1)
    pub min_hops: u32,
    /// Maximum hops (inclusive, <= MAX_VARIABLE_LENGTH_HOPS)
    pub max_hops: u32,
}

/// A table reference
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relvar {
    /// The table schema of this relvar
    pub schema: Arc<TableOrViewSchema>,
    /// The name of this relvar
    pub alias: RawIdentifier,
    /// Does this relvar represent a delta table?
    pub delta: Option<Delta>,
}

impl CollectViews for RelExpr {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        self.visit(&mut |expr| {
            if let Self::RelVar(Relvar { schema, .. }) = expr
                && let Some(info) = &schema.view_info
            {
                views.insert(info.view_id);
            }
        });
    }
}

impl VariableLengthPath {
    /// Expand this variable-length path into a list of fixed-depth `RelExpr`
    /// trees, one per hop count from `min_hops` to `max_hops`. Each tree is
    /// a chain of EqJoins identical to what the fixed-depth Cypher translator
    /// produces. The end node always uses `self.end_alias`.
    pub fn expand(&self) -> Vec<RelExpr> {
        let mut results = Vec::with_capacity((self.max_hops - self.min_hops + 1) as usize);

        for depth in self.min_hops..=self.max_hops {
            results.push(self.build_fixed_depth(depth));
        }

        results
    }

    fn build_fixed_depth(&self, depth: u32) -> RelExpr {
        let mut current = (*self.lhs).clone();
        let mut prev_alias = self.start_alias.clone();
        let mut anon_counter: usize = 0;

        for hop in 0..depth {
            let edge_alias = RawIdentifier::new(format!("_vlp_e{}", anon_counter));
            anon_counter += 1;

            let is_last = hop == depth - 1;
            let next_alias = if is_last {
                self.end_alias.clone()
            } else {
                let a = RawIdentifier::new(format!("_vlp_v{}", anon_counter));
                anon_counter += 1;
                a
            };

            let (vid_idx, vid_ty) = self.resolve_vertex_id_col();
            let (near_idx, near_ty) = self.resolve_edge_near_col();

            let prev_id = FieldProject {
                table: prev_alias.clone(),
                field: vid_idx,
                ty: vid_ty.clone(),
            };
            let edge_near = FieldProject {
                table: edge_alias.clone(),
                field: near_idx,
                ty: near_ty,
            };

            current = RelExpr::EqJoin(
                LeftDeepJoin {
                    lhs: Box::new(current),
                    rhs: Relvar {
                        schema: self.edge_schema.clone(),
                        alias: edge_alias.clone(),
                        delta: None,
                    },
                },
                prev_id,
                edge_near,
            );

            let (far_idx, far_ty) = self.resolve_edge_far_col();
            let next_id = FieldProject {
                table: next_alias.clone(),
                field: vid_idx,
                ty: vid_ty,
            };

            current = RelExpr::EqJoin(
                LeftDeepJoin {
                    lhs: Box::new(current),
                    rhs: Relvar {
                        schema: self.vertex_schema.clone(),
                        alias: next_alias.clone(),
                        delta: None,
                    },
                },
                FieldProject {
                    table: edge_alias.clone(),
                    field: far_idx,
                    ty: far_ty,
                },
                next_id,
            );

            if let Some(ref rel_type) = self.rel_type {
                let (et_idx, et_ty) = self.resolve_edge_type_col();
                let edge_type_field = FieldProject {
                    table: edge_alias,
                    field: et_idx,
                    ty: et_ty,
                };
                current = RelExpr::Select(
                    Box::new(current),
                    Expr::BinOp(
                        BinOp::Eq,
                        Box::new(Expr::Field(edge_type_field)),
                        Box::new(Expr::str(rel_type.clone().into_boxed_str())),
                    ),
                );
            }

            prev_alias = next_alias;
        }

        current
    }

    fn resolve_vertex_id_col(&self) -> (usize, AlgebraicType) {
        let col = self
            .vertex_schema
            .get_column_by_name_or_alias(&self.vertex_id_col)
            .expect("VariableLengthPath: vertex_id_col not found in vertex schema");
        (col.col_pos.idx(), col.col_type.clone())
    }

    fn resolve_edge_near_col(&self) -> (usize, AlgebraicType) {
        let col = self
            .edge_schema
            .get_column_by_name_or_alias(&self.edge_col_near)
            .expect("VariableLengthPath: edge_col_near not found in edge schema");
        (col.col_pos.idx(), col.col_type.clone())
    }

    fn resolve_edge_far_col(&self) -> (usize, AlgebraicType) {
        let col = self
            .edge_schema
            .get_column_by_name_or_alias(&self.edge_col_far)
            .expect("VariableLengthPath: edge_col_far not found in edge schema");
        (col.col_pos.idx(), col.col_type.clone())
    }

    fn resolve_edge_type_col(&self) -> (usize, AlgebraicType) {
        let col = self
            .edge_schema
            .get_column_by_name_or_alias("EdgeType")
            .expect("VariableLengthPath: EdgeType column not found in edge schema");
        (col.col_pos.idx(), col.col_type.clone())
    }
}

impl RelExpr {
    /// Walk the expression tree and call `f` on each node
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::Select(lhs, _)
            | Self::LeftDeepJoin(LeftDeepJoin { lhs, .. })
            | Self::EqJoin(LeftDeepJoin { lhs, .. }, ..)
            | Self::VariableLengthJoin(VariableLengthPath { lhs, .. }) => {
                lhs.visit(f);
            }
            Self::RelVar(..) => {}
        }
    }

    /// Walk the expression tree and call `f` on each node
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::Select(lhs, _)
            | Self::LeftDeepJoin(LeftDeepJoin { lhs, .. })
            | Self::EqJoin(LeftDeepJoin { lhs, .. }, ..)
            | Self::VariableLengthJoin(VariableLengthPath { lhs, .. }) => {
                lhs.visit_mut(f);
            }
            Self::RelVar(..) => {}
        }
    }

    /// The number of fields this expression returns
    pub fn nfields(&self) -> usize {
        match self {
            Self::RelVar(..) => 1,
            Self::LeftDeepJoin(join) | Self::EqJoin(join, ..) => join.lhs.nfields() + 1,
            Self::Select(input, _) => input.nfields(),
            Self::VariableLengthJoin(vlp) => vlp.lhs.nfields() + 1,
        }
    }

    /// Does this expression return this field?
    pub fn has_field(&self, field: &str) -> bool {
        match self {
            Self::RelVar(Relvar { alias, .. }) => alias.as_ref() == field,
            Self::LeftDeepJoin(join) | Self::EqJoin(join, ..) => {
                join.rhs.alias.as_ref() == field || join.lhs.has_field(field)
            }
            Self::Select(input, _) => input.has_field(field),
            Self::VariableLengthJoin(vlp) => {
                vlp.end_alias.as_ref() == field || vlp.lhs.has_field(field)
            }
        }
    }

    /// Return the [`TableOrViewSchema`] for a relvar in the expression
    pub fn find_table_schema(&self, alias: &str) -> Option<&TableOrViewSchema> {
        match self {
            Self::RelVar(relvar) if relvar.alias.as_ref() == alias => Some(&relvar.schema),
            Self::Select(input, _) => input.find_table_schema(alias),
            Self::EqJoin(LeftDeepJoin { rhs, .. }, ..) if rhs.alias.as_ref() == alias => Some(&rhs.schema),
            Self::EqJoin(LeftDeepJoin { lhs, .. }, ..) => lhs.find_table_schema(alias),
            Self::LeftDeepJoin(LeftDeepJoin { rhs, .. }) if rhs.alias.as_ref() == alias => Some(&rhs.schema),
            Self::LeftDeepJoin(LeftDeepJoin { lhs, .. }) => lhs.find_table_schema(alias),
            Self::VariableLengthJoin(vlp) if vlp.end_alias.as_ref() == alias => Some(&vlp.vertex_schema),
            Self::VariableLengthJoin(vlp) => vlp.lhs.find_table_schema(alias),
            _ => None,
        }
    }

    /// Return the [TableId] for a relvar in the expression
    pub fn find_table_id(&self, alias: &str) -> Option<TableId> {
        self.find_table_schema(alias).map(|schema| schema.table_id)
    }

    /// Does this expression return a single relvar?
    /// If so, return it's [`TableOrViewSchema`], otherwise return [None].
    pub fn return_table(&self) -> Option<&TableOrViewSchema> {
        match self {
            Self::RelVar(Relvar { schema, .. }) => Some(schema),
            Self::Select(input, _) => input.return_table(),
            _ => None,
        }
    }

    /// Does this expression return a single relvar?
    /// If so, return it's [TableId], otherwise return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        self.return_table().map(|schema| schema.table_id)
    }

    /// Does this expression return a single relvar?
    /// If so, return its name or equivalently its alias.
    pub fn return_name(&self) -> Option<&RawIdentifier> {
        match self {
            Self::RelVar(Relvar { alias, .. }) => Some(alias),
            Self::Select(input, _) => input.return_name(),
            _ => None,
        }
    }
}

/// A left deep binary cross product
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftDeepJoin {
    /// The lhs is recursive
    pub lhs: Box<RelExpr>,
    /// The rhs is a relvar
    pub rhs: Relvar,
}

/// A typed scalar expression
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A binary expression
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    /// A binary logic expression
    LogOp(LogOp, Box<Expr>, Box<Expr>),
    /// A unary NOT expression
    Not(Box<Expr>),
    /// A typed literal expression
    Value(AlgebraicValue, AlgebraicType),
    /// A field projection
    Field(FieldProject),
}

impl Expr {
    /// Walk the expression tree and call `f` on each node
    pub fn visit(&self, f: &impl Fn(&Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) | Self::LogOp(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Self::Not(inner) => inner.visit(f),
            Self::Value(..) | Self::Field(..) => {}
        }
    }

    /// Walk the expression tree and call `f` on each node
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) | Self::LogOp(_, a, b) => {
                a.visit_mut(f);
                b.visit_mut(f);
            }
            Self::Not(inner) => inner.visit_mut(f),
            Self::Value(..) | Self::Field(..) => {}
        }
    }

    /// A literal boolean value
    pub const fn bool(v: bool) -> Self {
        Self::Value(AlgebraicValue::Bool(v), AlgebraicType::Bool)
    }

    /// A literal string value
    pub const fn str(v: Box<str>) -> Self {
        Self::Value(AlgebraicValue::String(v), AlgebraicType::String)
    }

    /// The [AlgebraicType] of this scalar expression
    pub fn ty(&self) -> &AlgebraicType {
        match self {
            Self::BinOp(..) | Self::LogOp(..) | Self::Not(..) => &AlgebraicType::Bool,
            Self::Value(_, ty) | Self::Field(FieldProject { ty, .. }) => ty,
        }
    }
}

/// A typed qualified field projection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProject {
    pub table: RawIdentifier,
    pub field: usize,
    pub ty: AlgebraicType,
}
