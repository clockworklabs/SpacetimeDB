use crate::check::{Relvars, SchemaView};
use crate::expr::{
    Expr, FieldProject, LeftDeepJoin, ProjectList, ProjectName, RelExpr, Relvar,
    VariableLengthPath, MAX_VARIABLE_LENGTH_HOPS,
};
use spacetimedb_cypher_parser::ast::{
    CmpOp, CypherExpr, CypherLiteral, CypherQuery, Direction, PathLength, Pattern, ReturnClause,
};
use spacetimedb_lib::{AlgebraicType, AlgebraicValue};
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_schema::schema::TableOrViewSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};
use std::sync::Arc;
use thiserror::Error;

const VERTEX_TABLE: &str = "Vertex";
const EDGE_TABLE: &str = "Edge";

const VERTEX_ID_COL: &str = "Id";
const VERTEX_LABEL_COL: &str = "Label";

const EDGE_START_ID_COL: &str = "StartId";
const EDGE_END_ID_COL: &str = "EndId";
const EDGE_TYPE_COL: &str = "EdgeType";

#[derive(Debug, Error)]
pub enum CypherTranslateError {
    #[error("Unbounded variable-length paths are not supported (use [*min..max])")]
    UnboundedVariableLengthPath,
    #[error("Variable-length path max depth {0} exceeds limit {MAX_VARIABLE_LENGTH_HOPS}")]
    VariableLengthPathTooDeep(u32),
    #[error("Variable-length path min ({0}) must be >= 1 and <= max ({1})")]
    InvalidPathBounds(u32, u32),
    #[error("NULL literals are not yet supported")]
    NullLiteral,
    #[error("Empty MATCH clause")]
    EmptyPattern,
    #[error("Table `{0}` not found in schema")]
    TableNotFound(String),
    #[error("Column `{0}` not found in table `{1}`")]
    ColumnNotFound(String, String),
    #[error("Variable `{0}` is not in scope")]
    UnresolvedVariable(String),
    #[error("Nested property access is not supported")]
    NestedPropertyAccess,
    #[error("Unsupported RETURN expression")]
    UnsupportedReturn,
    #[error("Unsupported relationship direction")]
    UnsupportedDirection,
    #[error("Unsupported comparison operator")]
    UnsupportedCmpOp,
    #[error("Unsupported literal type")]
    UnsupportedLiteral,
    #[error("Unsupported expression type")]
    UnsupportedExpr,
}

type Result<T> = std::result::Result<T, CypherTranslateError>;

/// Translate a parsed Cypher query into a `RelExpr`-based logical plan.
///
/// The schema must contain `Vertex` and `Edge` tables matching the
/// userland graph model (Id, Label, Properties / Id, StartId, EndId,
/// EdgeType, Properties).
///
/// If the pattern contains undirected edges, the result is expanded into
/// multiple directed variants (outgoing + incoming) at the pattern level,
/// producing a UNION of plans.
///
/// If the pattern contains a variable-length path, each fixed-depth
/// expansion similarly produces a UNION of plans.
pub fn translate_cypher(query: &CypherQuery, tx: &impl SchemaView) -> Result<ProjectList> {
    // 1. Expand undirected edges in patterns into directed variants.
    let pattern_variants = expand_undirected_patterns(&query.match_clause.patterns);

    // 2. Translate each variant independently (same vars scope, fresh counter).
    let mut all_plans: Vec<(RelExpr, Relvars)> = Vec::new();
    let mut var_counter: usize = 0;

    for patterns in &pattern_variants {
        let mut vars = Relvars::default();
        let base = translate_match(patterns, tx, &mut vars, &mut var_counter)?;

        let filtered = match query.where_clause {
            Some(ref expr) => {
                let filter = translate_expr(expr, &vars)?;
                RelExpr::Select(Box::new(base), filter)
            }
            None => base,
        };

        // Expand any VariableLengthJoins within this variant.
        let expanded = expand_vlj(filtered);
        for e in expanded {
            all_plans.push((e, vars.clone()));
        }
    }

    // 3. Merge all plans with a single RETURN translation.
    if all_plans.len() == 1 {
        let (expr, vars) = all_plans.into_iter().next().unwrap();
        translate_return(&query.return_clause, expr, &vars)
    } else {
        translate_return_union_multi(&query.return_clause, all_plans)
    }
}

/// Recursively expand any `VariableLengthJoin` nodes in the expression tree
/// into multiple fixed-depth plans. Filters, joins, and other wrappers are
/// distributed over each expansion (UNION semantics).
fn expand_vlj(expr: RelExpr) -> Vec<RelExpr> {
    match expr {
        RelExpr::VariableLengthJoin(ref vlp) => vlp.expand(),
        RelExpr::Select(inner, filter) => {
            let expanded = expand_vlj(*inner);
            expanded
                .into_iter()
                .map(|e| RelExpr::Select(Box::new(e), filter.clone()))
                .collect()
        }
        RelExpr::EqJoin(LeftDeepJoin { lhs, rhs }, lf, rf) => {
            let expanded = expand_vlj(*lhs);
            expanded
                .into_iter()
                .map(|e| {
                    RelExpr::EqJoin(
                        LeftDeepJoin {
                            lhs: Box::new(e),
                            rhs: rhs.clone(),
                        },
                        lf.clone(),
                        rf.clone(),
                    )
                })
                .collect()
        }
        RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs }) => {
            let expanded = expand_vlj(*lhs);
            expanded
                .into_iter()
                .map(|e| {
                    RelExpr::LeftDeepJoin(LeftDeepJoin {
                        lhs: Box::new(e),
                        rhs: rhs.clone(),
                    })
                })
                .collect()
        }
        other => vec![other],
    }
}

/// Expand any undirected edges in the given patterns into the cartesian product
/// of directed variants (Outgoing and Incoming).
///
/// Each pattern is cloned; edges with `Direction::Undirected` are replaced with
/// `Outgoing` in half the variants and `Incoming` in the other half.
fn expand_undirected_patterns(patterns: &[Pattern]) -> Vec<Vec<Pattern>> {
    let mut variants: Vec<Vec<Pattern>> = vec![vec![]];

    for pattern in patterns {
        // Collect indices of undirected edges in this pattern
        let undirected_indices: Vec<usize> = pattern
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.direction == Direction::Undirected)
            .map(|(i, _)| i)
            .collect();

        if undirected_indices.is_empty() {
            // No undirected edges: append pattern to all existing variants
            for variant in &mut variants {
                variant.push(pattern.clone());
            }
        } else {
            // Cartesian product: each undirected edge doubles the variant count
            let n = 1usize << undirected_indices.len();
            let mut new_variants: Vec<Vec<Pattern>> = Vec::with_capacity(variants.len() * n);

            for variant in &variants {
                for mask in 0..n {
                    let mut new_pattern = pattern.clone();
                    for (bit_idx, &edge_idx) in undirected_indices.iter().enumerate() {
                        let is_outgoing = (mask >> bit_idx) & 1 == 0;
                        new_pattern.edges[edge_idx].direction = if is_outgoing {
                            Direction::Outgoing
                        } else {
                            Direction::Incoming
                        };
                    }
                    let mut new_variant = variant.clone();
                    new_variant.push(new_pattern);
                    new_variants.push(new_variant);
                }
            }
            variants = new_variants;
        }
    }

    variants
}

fn anon_var(counter: &mut usize) -> RawIdentifier {
    let name = format!("_anon_{counter}");
    *counter += 1;
    RawIdentifier::new(name)
}

fn lookup_schema(tx: &impl SchemaView, name: &str) -> Result<Arc<TableOrViewSchema>> {
    tx.schema(name)
        .ok_or_else(|| CypherTranslateError::TableNotFound(name.to_owned()))
}

fn resolve_field(
    alias: &RawIdentifier,
    schema: &TableOrViewSchema,
    col_name: &str,
) -> Result<FieldProject> {
    let col = schema
        .get_column_by_name_or_alias(col_name)
        .ok_or_else(|| CypherTranslateError::ColumnNotFound(col_name.to_owned(), alias.to_string()))?;
    Ok(FieldProject {
        table: alias.clone(),
        field: col.col_pos.idx(),
        ty: col.col_type.clone(),
    })
}

fn string_eq_filter(
    alias: &RawIdentifier,
    schema: &TableOrViewSchema,
    col_name: &str,
    value: &str,
) -> Result<Expr> {
    let field = resolve_field(alias, schema, col_name)?;
    Ok(Expr::BinOp(
        BinOp::Eq,
        Box::new(Expr::Field(field)),
        Box::new(Expr::str(value.to_owned().into_boxed_str())),
    ))
}

/// Resolve a `PathLength` AST node into concrete (min, max) hop bounds.
fn resolve_path_bounds(length: &PathLength) -> Result<(u32, u32)> {
    let (min, max) = match length {
        PathLength::Unbounded => return Err(CypherTranslateError::UnboundedVariableLengthPath),
        PathLength::Exact(n) => (*n, *n),
        PathLength::Range { min, max } => {
            let lo = min.unwrap_or(1);
            let hi = max.ok_or(CypherTranslateError::UnboundedVariableLengthPath)?;
            (lo, hi)
        }
        // PathLength is #[non_exhaustive]; treat unknown future variants as unbounded
        _ => return Err(CypherTranslateError::UnboundedVariableLengthPath),
    };

    if min < 1 || min > max {
        return Err(CypherTranslateError::InvalidPathBounds(min, max));
    }
    if max > MAX_VARIABLE_LENGTH_HOPS {
        return Err(CypherTranslateError::VariableLengthPathTooDeep(max));
    }

    Ok((min, max))
}

// ── MATCH translation ─────────────────────────────────────────────

fn translate_match(
    patterns: &[Pattern],
    tx: &impl SchemaView,
    vars: &mut Relvars,
    counter: &mut usize,
) -> Result<RelExpr> {
    if patterns.is_empty() {
        return Err(CypherTranslateError::EmptyPattern);
    }

    let mut current: Option<RelExpr> = None;
    let mut all_filters: Vec<Expr> = Vec::new();

    for pattern in patterns {
        let (new_current, filters) = translate_pattern_into(pattern, current, tx, vars, counter)?;
        current = Some(new_current);
        all_filters.extend(filters);
    }

    let mut result = current.unwrap();

    if !all_filters.is_empty() {
        let combined = all_filters
            .into_iter()
            .reduce(|a, b| Expr::LogOp(LogOp::And, Box::new(a), Box::new(b)))
            .unwrap();
        result = RelExpr::Select(Box::new(result), combined);
    }

    Ok(result)
}

/// Translate a single graph pattern, optionally building on top of an existing
/// plan. Returns the combined `RelExpr` and a list of unapplied filter
/// expressions that the caller must wrap in `Select`.
///
/// When `current` is `Some(prev)`:
/// - A new (unbound) first node becomes a cross-join (`LeftDeepJoin`).
/// - A shared first node (already in `vars`) reuses the existing plan.
/// - A shared subsequent node is correlated via a filter on `Vertex.Id`.
fn translate_pattern_into(
    pattern: &Pattern,
    current: Option<RelExpr>,
    tx: &impl SchemaView,
    vars: &mut Relvars,
    counter: &mut usize,
) -> Result<(RelExpr, Vec<Expr>)> {
    let vertex_schema = lookup_schema(tx, VERTEX_TABLE)?;
    let edge_schema = lookup_schema(tx, EDGE_TABLE)?;

    let first = &pattern.nodes[0];
    let first_alias = first
        .variable
        .as_deref()
        .map(RawIdentifier::new)
        .unwrap_or_else(|| anon_var(counter));

    let mut filters: Vec<Expr> = Vec::new();

    let is_shared = vars.contains_key(&first_alias);

    let mut result = if is_shared {
        current.ok_or(CypherTranslateError::EmptyPattern)?
    } else {
        vars.insert(first_alias.clone(), vertex_schema.clone());
        let relvar = Relvar {
            schema: vertex_schema.clone(),
            alias: first_alias.clone(),
            delta: None,
        };
        match current {
            None => RelExpr::RelVar(relvar),
            Some(prev) => RelExpr::LeftDeepJoin(LeftDeepJoin {
                lhs: Box::new(prev),
                rhs: relvar,
            }),
        }
    };

    if let Some(ref label) = first.label {
        filters.push(string_eq_filter(
            &first_alias,
            &vertex_schema,
            VERTEX_LABEL_COL,
            label,
        )?);
    }

    let mut prev_node_alias = first_alias;

    for (i, edge) in pattern.edges.iter().enumerate() {
        let next_node = &pattern.nodes[i + 1];

        let next_alias = next_node
            .variable
            .as_deref()
            .map(RawIdentifier::new)
            .unwrap_or_else(|| anon_var(counter));

        let (edge_col_near, edge_col_far) = match edge.direction {
            Direction::Outgoing | Direction::Undirected => (EDGE_START_ID_COL, EDGE_END_ID_COL),
            Direction::Incoming => (EDGE_END_ID_COL, EDGE_START_ID_COL),
            _ => return Err(CypherTranslateError::UnsupportedDirection),
        };

        if let Some(ref length) = edge.length {
            let (min_hops, max_hops) = resolve_path_bounds(length)?;
            if !vars.contains_key(&next_alias) {
                vars.insert(next_alias.clone(), vertex_schema.clone());
            }

            result = RelExpr::VariableLengthJoin(VariableLengthPath {
                lhs: Box::new(result),
                start_alias: prev_node_alias.clone(),
                end_alias: next_alias.clone(),
                edge_schema: edge_schema.clone(),
                vertex_schema: vertex_schema.clone(),
                rel_type: edge.rel_type.clone(),
                edge_col_near: edge_col_near.to_owned(),
                edge_col_far: edge_col_far.to_owned(),
                vertex_id_col: VERTEX_ID_COL.to_owned(),
                min_hops,
                max_hops,
            });

            if let Some(ref label) = next_node.label {
                filters.push(string_eq_filter(
                    &next_alias,
                    &vertex_schema,
                    VERTEX_LABEL_COL,
                    label,
                )?);
            }
        } else {
            let edge_alias = edge
                .variable
                .as_deref()
                .map(RawIdentifier::new)
                .unwrap_or_else(|| anon_var(counter));

            vars.insert(edge_alias.clone(), edge_schema.clone());

            // prev_node.Id = edge.near_col
            result = RelExpr::EqJoin(
                LeftDeepJoin {
                    lhs: Box::new(result),
                    rhs: Relvar {
                        schema: edge_schema.clone(),
                        alias: edge_alias.clone(),
                        delta: None,
                    },
                },
                resolve_field(&prev_node_alias, &vertex_schema, VERTEX_ID_COL)?,
                resolve_field(&edge_alias, &edge_schema, edge_col_near)?,
            );

            let is_next_shared = vars.contains_key(&next_alias);

            if is_next_shared {
                // Shared variable: correlate via filter on Vertex.Id
                filters.push(Expr::BinOp(
                    BinOp::Eq,
                    Box::new(Expr::Field(resolve_field(
                        &edge_alias,
                        &edge_schema,
                        edge_col_far,
                    )?)),
                    Box::new(Expr::Field(resolve_field(
                        &next_alias,
                        &vertex_schema,
                        VERTEX_ID_COL,
                    )?)),
                ));
            } else {
                vars.insert(next_alias.clone(), vertex_schema.clone());

                // edge.far_col = next_node.Id
                result = RelExpr::EqJoin(
                    LeftDeepJoin {
                        lhs: Box::new(result),
                        rhs: Relvar {
                            schema: vertex_schema.clone(),
                            alias: next_alias.clone(),
                            delta: None,
                        },
                    },
                    resolve_field(&edge_alias, &edge_schema, edge_col_far)?,
                    resolve_field(&next_alias, &vertex_schema, VERTEX_ID_COL)?,
                );
            }

            if let Some(ref rel_type) = edge.rel_type {
                filters.push(string_eq_filter(
                    &edge_alias,
                    &edge_schema,
                    EDGE_TYPE_COL,
                    rel_type,
                )?);
            }
            if let Some(ref label) = next_node.label {
                filters.push(string_eq_filter(
                    &next_alias,
                    &vertex_schema,
                    VERTEX_LABEL_COL,
                    label,
                )?);
            }
        }

        prev_node_alias = next_alias;
    }

    Ok((result, filters))
}

// ── Expression translation ────────────────────────────────────────

fn translate_expr(expr: &CypherExpr, vars: &Relvars) -> Result<Expr> {
    match expr {
        CypherExpr::Lit(lit) => translate_literal(lit),
        CypherExpr::Var(name) => Err(CypherTranslateError::UnresolvedVariable(name.clone())),
        CypherExpr::Prop(base, prop) => {
            let CypherExpr::Var(table_name) = base.as_ref() else {
                return Err(CypherTranslateError::NestedPropertyAccess);
            };
            let alias = RawIdentifier::new(table_name.clone());
            let schema = vars
                .get(&alias)
                .ok_or_else(|| CypherTranslateError::UnresolvedVariable(table_name.clone()))?;
            Ok(Expr::Field(resolve_field(&alias, schema, prop)?))
        }
        CypherExpr::Cmp(lhs, op, rhs) => {
            let a = translate_expr(lhs, vars)?;
            let b = translate_expr(rhs, vars)?;
            let bin_op = match op {
                CmpOp::Eq => BinOp::Eq,
                CmpOp::Ne => BinOp::Ne,
                CmpOp::Lt => BinOp::Lt,
                CmpOp::Gt => BinOp::Gt,
                CmpOp::Lte => BinOp::Lte,
                CmpOp::Gte => BinOp::Gte,
                _ => return Err(CypherTranslateError::UnsupportedCmpOp),
            };
            Ok(Expr::BinOp(bin_op, Box::new(a), Box::new(b)))
        }
        CypherExpr::And(lhs, rhs) => {
            let a = translate_expr(lhs, vars)?;
            let b = translate_expr(rhs, vars)?;
            Ok(Expr::LogOp(LogOp::And, Box::new(a), Box::new(b)))
        }
        CypherExpr::Or(lhs, rhs) => {
            let a = translate_expr(lhs, vars)?;
            let b = translate_expr(rhs, vars)?;
            Ok(Expr::LogOp(LogOp::Or, Box::new(a), Box::new(b)))
        }
        CypherExpr::Not(inner) => {
            let translated = translate_expr(inner, vars)?;
            Ok(Expr::Not(Box::new(translated)))
        }
        _ => Err(CypherTranslateError::UnsupportedExpr),
    }
}

fn translate_literal(lit: &CypherLiteral) -> Result<Expr> {
    match lit {
        CypherLiteral::Integer(v) => Ok(Expr::Value(AlgebraicValue::I64(*v), AlgebraicType::I64)),
        CypherLiteral::Float(v) => Ok(Expr::Value(
            AlgebraicValue::F64((*v).into()),
            AlgebraicType::F64,
        )),
        CypherLiteral::String(s) => Ok(Expr::str(s.clone().into_boxed_str())),
        CypherLiteral::Bool(v) => Ok(Expr::bool(*v)),
        CypherLiteral::Null => Err(CypherTranslateError::NullLiteral),
        _ => Err(CypherTranslateError::UnsupportedLiteral),
    }
}

// ── RETURN translation ────────────────────────────────────────────

/// Translate a RETURN clause over a UNION of expanded paths where each branch
/// may have its own `Relvars` (needed when undirected edge expansion clones
/// the variable map per variant).
fn translate_return_union_multi(
    return_clause: &ReturnClause,
    inputs: Vec<(RelExpr, Relvars)>,
) -> Result<ProjectList> {
    match return_clause {
        ReturnClause::All => {
            let names: Vec<_> = inputs
                .into_iter()
                .map(|(expr, _)| ProjectName::None(expr))
                .collect();
            Ok(ProjectList::Name(names))
        }
        ReturnClause::Items(items) => {
            if items.len() == 1 {
                if let CypherExpr::Var(ref name) = items[0].expr {
                    let alias = items[0]
                        .alias
                        .as_ref()
                        .map(|a| RawIdentifier::new(a.clone()))
                        .unwrap_or_else(|| RawIdentifier::new(name.clone()));
                    let names: Vec<_> = inputs
                        .into_iter()
                        .map(|(expr, _)| ProjectName::Some(expr, alias.clone()))
                        .collect();
                    return Ok(ProjectList::Name(names));
                }
            }

            // Multi-item returns: build projections using the first variant's vars.
            // All variants share the same user-defined variables.
            let first_vars = &inputs[0].1;
            let mut projections = Vec::new();
            for item in items {
                match &item.expr {
                    CypherExpr::Prop(base, prop) => {
                        let CypherExpr::Var(table_name) = base.as_ref() else {
                            return Err(CypherTranslateError::NestedPropertyAccess);
                        };
                        let alias_id = RawIdentifier::new(table_name.clone());
                        let schema = first_vars
                            .get(&alias_id)
                            .ok_or_else(|| {
                                CypherTranslateError::UnresolvedVariable(table_name.clone())
                            })?;
                        let field = resolve_field(&alias_id, schema, prop)?;
                        let out_name = item
                            .alias
                            .as_ref()
                            .map(|a| RawIdentifier::new(a.clone()))
                            .unwrap_or_else(|| RawIdentifier::new(format!("{table_name}.{prop}")));
                        projections.push((out_name, field));
                    }
                    CypherExpr::Var(name) => {
                        let alias_id = RawIdentifier::new(name.clone());
                        let schema = first_vars.get(&alias_id).ok_or_else(|| {
                            CypherTranslateError::UnresolvedVariable(name.clone())
                        })?;
                        for col in schema.public_columns() {
                            let field = FieldProject {
                                table: alias_id.clone(),
                                field: col.col_pos.idx(),
                                ty: col.col_type.clone(),
                            };
                            let col_alias =
                                RawIdentifier::new(format!("{}.{}", name, col.col_name));
                            projections.push((col_alias, field));
                        }
                    }
                    _ => return Err(CypherTranslateError::UnsupportedReturn),
                }
            }
            let exprs: Vec<RelExpr> = inputs.into_iter().map(|(expr, _)| expr).collect();
            Ok(ProjectList::List(exprs, projections))
        }
        _ => Err(CypherTranslateError::UnsupportedReturn),
    }
}

fn translate_return(
    return_clause: &ReturnClause,
    input: RelExpr,
    vars: &Relvars,
) -> Result<ProjectList> {
    match return_clause {
        ReturnClause::All => Ok(ProjectList::Name(vec![ProjectName::None(input)])),
        ReturnClause::Items(items) => {
            // Single bare variable (e.g. `RETURN a`) keeps the table-level
            // binding via ProjectList::Name, preserving the full row context
            // for downstream consumers. Multi-item returns require explicit
            // column-level projection via ProjectList::List instead.
            if items.len() == 1 {
                if let CypherExpr::Var(ref name) = items[0].expr {
                    let alias = items[0]
                        .alias
                        .as_ref()
                        .map(|a| RawIdentifier::new(a.clone()))
                        .unwrap_or_else(|| RawIdentifier::new(name.clone()));
                    return Ok(ProjectList::Name(vec![ProjectName::Some(input, alias)]));
                }
            }

            let mut projections = Vec::new();
            for item in items {
                match &item.expr {
                    CypherExpr::Prop(base, prop) => {
                        let CypherExpr::Var(table_name) = base.as_ref() else {
                            return Err(CypherTranslateError::NestedPropertyAccess);
                        };
                        let alias_id = RawIdentifier::new(table_name.clone());
                        let schema = vars.get(&alias_id).ok_or_else(|| {
                            CypherTranslateError::UnresolvedVariable(table_name.clone())
                        })?;
                        let field = resolve_field(&alias_id, schema, prop)?;
                        let out_name = item
                            .alias
                            .as_ref()
                            .map(|a| RawIdentifier::new(a.clone()))
                            .unwrap_or_else(|| RawIdentifier::new(format!("{table_name}.{prop}")));
                        projections.push((out_name, field));
                    }
                    CypherExpr::Var(name) => {
                        let alias_id = RawIdentifier::new(name.clone());
                        let schema = vars.get(&alias_id).ok_or_else(|| {
                            CypherTranslateError::UnresolvedVariable(name.clone())
                        })?;
                        for col in schema.public_columns() {
                            let field = FieldProject {
                                table: alias_id.clone(),
                                field: col.col_pos.idx(),
                                ty: col.col_type.clone(),
                            };
                            let col_alias =
                                RawIdentifier::new(format!("{}.{}", name, col.col_name));
                            projections.push((col_alias, field));
                        }
                    }
                    _ => return Err(CypherTranslateError::UnsupportedReturn),
                }
            }
            Ok(ProjectList::List(vec![input], projections))
        }
        _ => Err(CypherTranslateError::UnsupportedReturn),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::test_utils::{build_module_def, SchemaViewer};
    use crate::expr::{LeftDeepJoin, RelExpr};
    use spacetimedb_cypher_parser::ast::*;
    use spacetimedb_lib::{AlgebraicType, ProductType};
    use spacetimedb_primitives::TableId;
    use spacetimedb_sql_parser::ast::BinOp;

    fn tx() -> SchemaViewer {
        SchemaViewer::new(
            build_module_def(vec![
                (
                    "Vertex",
                    ProductType::from([
                        ("Id", AlgebraicType::U64),
                        ("Label", AlgebraicType::String),
                        ("Properties", AlgebraicType::String),
                    ]),
                ),
                (
                    "Edge",
                    ProductType::from([
                        ("Id", AlgebraicType::U64),
                        ("StartId", AlgebraicType::U64),
                        ("EndId", AlgebraicType::U64),
                        ("EdgeType", AlgebraicType::String),
                        ("Properties", AlgebraicType::String),
                    ]),
                ),
            ]),
            vec![("Vertex", TableId(0)), ("Edge", TableId(1))],
        )
    }

    fn node(var: Option<&str>, label: Option<&str>) -> NodePattern {
        NodePattern {
            variable: var.map(String::from),
            label: label.map(String::from),
            properties: vec![],
        }
    }

    fn rel(var: Option<&str>, rel_type: Option<&str>, dir: Direction) -> RelPattern {
        RelPattern {
            variable: var.map(String::from),
            rel_type: rel_type.map(String::from),
            length: None,
            direction: dir,
        }
    }

    fn query(pattern: Pattern, where_clause: Option<CypherExpr>, ret: ReturnClause) -> CypherQuery {
        CypherQuery {
            match_clause: MatchClause {
                patterns: vec![pattern],
            },
            where_clause,
            return_clause: ret,
        }
    }

    fn multi_query(
        patterns: Vec<Pattern>,
        where_clause: Option<CypherExpr>,
        ret: ReturnClause,
    ) -> CypherQuery {
        CypherQuery {
            match_clause: MatchClause { patterns },
            where_clause,
            return_clause: ret,
        }
    }

    fn single_pattern(nodes: Vec<NodePattern>, edges: Vec<RelPattern>) -> Pattern {
        Pattern { nodes, edges }
    }

    // ── Golden tests ──────────────────────────────────────────────

    #[test]
    fn single_node_no_label() {
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name, got {result:?}");
        };
        assert_eq!(names.len(), 1);
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected ProjectName::None");
        };
        assert!(
            matches!(expr, RelExpr::RelVar(r) if r.alias.as_ref() == "a"),
            "expected RelVar(a), got {expr:?}"
        );
    }

    #[test]
    fn single_node_with_label() {
        let q = query(
            single_pattern(vec![node(Some("a"), Some("Person"))], vec![]),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected ProjectName::None");
        };
        let RelExpr::Select(inner, filter) = expr else {
            panic!("expected Select for label filter, got {expr:?}");
        };
        assert!(matches!(inner.as_ref(), RelExpr::RelVar(r) if r.alias.as_ref() == "a"));

        let Expr::BinOp(BinOp::Eq, lhs, rhs) = filter else {
            panic!("expected Eq filter, got {filter:?}");
        };
        assert!(matches!(lhs.as_ref(), Expr::Field(f) if f.table.as_ref() == "a"));
        assert!(matches!(rhs.as_ref(), Expr::Value(AlgebraicValue::String(s), _) if s.as_ref() == "Person"));
    }

    #[test]
    fn single_hop_outgoing() {
        // (a)-[r]->(b)
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![rel(Some("r"), None, Direction::Outgoing)],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Outermost is EqJoin(…, r.EndId, b.Id)
        let RelExpr::EqJoin(outer_join, lhs_field, rhs_field) = expr else {
            panic!("expected outer EqJoin, got {expr:?}");
        };
        assert_eq!(lhs_field.table.as_ref(), "r");
        assert_eq!(rhs_field.table.as_ref(), "b");
        assert_eq!(outer_join.rhs.alias.as_ref(), "b");

        // Inner is EqJoin(RelVar(a), Edge(r), a.Id, r.StartId)
        let RelExpr::EqJoin(ref inner_join, ref lhs2, ref rhs2) = *outer_join.lhs else {
            panic!("expected inner EqJoin");
        };
        assert_eq!(lhs2.table.as_ref(), "a");
        assert_eq!(rhs2.table.as_ref(), "r");
        assert_eq!(inner_join.rhs.alias.as_ref(), "r");
        assert!(matches!(*inner_join.lhs, RelExpr::RelVar(ref r) if r.alias.as_ref() == "a"));
    }

    #[test]
    fn single_hop_incoming() {
        // (a)<-[r]-(b) → a.Id = r.EndId, r.StartId = b.Id
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![rel(Some("r"), None, Direction::Incoming)],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Outer: EqJoin(…, r.StartId, b.Id)
        let RelExpr::EqJoin(_, lhs_field, rhs_field) = expr else {
            panic!("expected outer EqJoin");
        };
        assert_eq!(lhs_field.table.as_ref(), "r", "edge far col for incoming should be StartId");
        assert_eq!(rhs_field.table.as_ref(), "b");

        // Inner: EqJoin(RelVar(a), Edge(r), a.Id, r.EndId)
        let RelExpr::EqJoin(outer_join, ..) = expr else {
            unreachable!();
        };
        let RelExpr::EqJoin(_, ref inner_lhs, ref inner_rhs) = *outer_join.lhs else {
            panic!("expected inner EqJoin");
        };
        assert_eq!(inner_lhs.table.as_ref(), "a");
        assert_eq!(inner_rhs.table.as_ref(), "r", "edge near col for incoming should be EndId");
    }

    #[test]
    fn single_hop_with_labels_and_type() {
        // (a:Person)-[r:KNOWS]->(b:Person)
        let q = query(
            single_pattern(
                vec![node(Some("a"), Some("Person")), node(Some("b"), Some("Person"))],
                vec![rel(Some("r"), Some("KNOWS"), Direction::Outgoing)],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Outermost should be Select with combined label/type filters
        let RelExpr::Select(inner, filter) = expr else {
            panic!("expected Select wrapper for label/type filters, got {expr:?}");
        };
        // The inner is the EqJoin chain
        assert!(matches!(inner.as_ref(), RelExpr::EqJoin(..)));

        // Filter should be AND-chain of three conditions
        fn count_ands(e: &Expr) -> usize {
            match e {
                Expr::LogOp(LogOp::And, a, b) => count_ands(a) + count_ands(b),
                _ => 1,
            }
        }
        assert_eq!(count_ands(filter), 3, "expected 3 filter clauses (a.Label, r.EdgeType, b.Label)");
    }

    #[test]
    fn multi_hop() {
        // (a)-[r]->(b)-[s]->(c)
        let q = query(
            single_pattern(
                vec![
                    node(Some("a"), None),
                    node(Some("b"), None),
                    node(Some("c"), None),
                ],
                vec![
                    rel(Some("r"), None, Direction::Outgoing),
                    rel(Some("s"), None, Direction::Outgoing),
                ],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Count EqJoin depth: should be 4 (a-r, r-b, b-s, s-c)
        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                _ => 0,
            }
        }
        assert_eq!(count_joins(expr), 4, "2-hop pattern should produce 4 EqJoins");
    }

    #[test]
    fn where_clause_property_comparison() {
        // MATCH (a) WHERE a.Label = 'Alice' RETURN *
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            Some(CypherExpr::Cmp(
                Box::new(CypherExpr::Prop(
                    Box::new(CypherExpr::Var("a".into())),
                    "Label".into(),
                )),
                CmpOp::Eq,
                Box::new(CypherExpr::Lit(CypherLiteral::String("Alice".into()))),
            )),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };
        let RelExpr::Select(_, filter) = expr else {
            panic!("expected Select, got {expr:?}");
        };
        assert!(matches!(filter, Expr::BinOp(BinOp::Eq, ..)));
    }

    #[test]
    fn unbounded_variable_length_path_rejected() {
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Unbounded),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let err = translate_cypher(&q, &tx()).unwrap_err();
        assert!(
            matches!(err, CypherTranslateError::UnboundedVariableLengthPath),
            "expected UnboundedVariableLengthPath error, got {err:?}"
        );
    }

    #[test]
    fn unbounded_range_max_rejected() {
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Range {
                        min: Some(1),
                        max: None,
                    }),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let err = translate_cypher(&q, &tx()).unwrap_err();
        assert!(
            matches!(err, CypherTranslateError::UnboundedVariableLengthPath),
            "expected UnboundedVariableLengthPath, got {err:?}"
        );
    }

    #[test]
    fn variable_length_too_deep_rejected() {
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Range {
                        min: Some(1),
                        max: Some(100),
                    }),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let err = translate_cypher(&q, &tx()).unwrap_err();
        assert!(
            matches!(err, CypherTranslateError::VariableLengthPathTooDeep(100)),
            "expected VariableLengthPathTooDeep, got {err:?}"
        );
    }

    #[test]
    fn variable_length_invalid_bounds_rejected() {
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Range {
                        min: Some(5),
                        max: Some(3),
                    }),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let err = translate_cypher(&q, &tx()).unwrap_err();
        assert!(
            matches!(err, CypherTranslateError::InvalidPathBounds(5, 3)),
            "expected InvalidPathBounds, got {err:?}"
        );
    }

    #[test]
    fn variable_length_exact_2_hops() {
        // (a)-[*2]->(b) should expand to exactly 1 plan with 4 EqJoins
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Exact(2)),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name, got {result:?}");
        };
        // Exact(2) → single expansion
        assert_eq!(names.len(), 1, "exact depth should produce 1 plan");

        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected ProjectName::None");
        };
        // 2 hops → 4 EqJoins (each hop = 2 EqJoins: prev→edge, edge→next)
        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }
        assert_eq!(count_joins(expr), 4, "2-hop VLJ expansion should have 4 EqJoins");
    }

    #[test]
    fn variable_length_range_produces_union() {
        // (a)-[*1..3]->(b) RETURN * → 3 plans (depths 1, 2, 3)
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Range {
                        min: Some(1),
                        max: Some(3),
                    }),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name, got {result:?}");
        };
        assert_eq!(names.len(), 3, "range [1..3] should produce 3 plans");

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }

        let depths: Vec<usize> = names
            .iter()
            .map(|pn| {
                let expr = match pn {
                    ProjectName::None(e) => e,
                    ProjectName::Some(e, _) => e,
                };
                count_joins(expr)
            })
            .collect();
        assert_eq!(depths, vec![2, 4, 6], "join counts for depths 1/2/3");
    }

    #[test]
    fn variable_length_with_rel_type_filter() {
        // (a)-[:KNOWS*1..2]->(b) → each expansion should have Select filters
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: Some("KNOWS".into()),
                    length: Some(PathLength::Range {
                        min: Some(1),
                        max: Some(2),
                    }),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        assert_eq!(names.len(), 2);

        fn count_selects(e: &RelExpr) -> usize {
            match e {
                RelExpr::Select(inner, _) => 1 + count_selects(inner),
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => count_selects(lhs),
                _ => 0,
            }
        }

        let ProjectName::None(ref e1) = names[0] else { panic!("expected None") };
        let ProjectName::None(ref e2) = names[1] else { panic!("expected None") };

        // Depth 1: 1 edge → 1 Select for EdgeType
        assert_eq!(count_selects(e1), 1, "1-hop path with rel_type should have 1 Select");
        // Depth 2: 2 edges → 2 Selects for EdgeType
        assert_eq!(count_selects(e2), 2, "2-hop path with rel_type should have 2 Selects");
    }

    #[test]
    fn variable_length_incoming_direction() {
        // (a)<-[*1]--(b) → should use EndId as near, StartId as far
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Exact(1)),
                    direction: Direction::Incoming,
                }],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        assert_eq!(names.len(), 1);
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Outer EqJoin: edge.StartId = b.Id (far col for incoming = StartId)
        let RelExpr::EqJoin(_, lhs_f, rhs_f) = expr else {
            panic!("expected outer EqJoin, got {expr:?}");
        };
        // The edge far field for incoming is StartId (index 1)
        assert_eq!(lhs_f.field, 1, "edge far col for incoming should be StartId (index 1)");
        // b.Id is index 0
        assert_eq!(rhs_f.field, 0, "vertex Id should be index 0");
    }

    #[test]
    fn variable_length_end_node_alias_bound() {
        // (a)-[*2]->(b) RETURN b
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Exact(2)),
                    direction: Direction::Outgoing,
                }],
            ),
            None,
            ReturnClause::Items(vec![ReturnItem {
                expr: CypherExpr::Var("b".into()),
                alias: None,
            }]),
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        assert_eq!(names.len(), 1);
        let ProjectName::Some(_, ref alias) = names[0] else {
            panic!("expected ProjectName::Some");
        };
        assert_eq!(alias.as_ref(), "b");
    }

    #[test]
    fn variable_length_with_where_clause() {
        // (a)-[*1..2]->(b) WHERE b.Label = 'Person'
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![RelPattern {
                    variable: None,
                    rel_type: None,
                    length: Some(PathLength::Range {
                        min: Some(1),
                        max: Some(2),
                    }),
                    direction: Direction::Outgoing,
                }],
            ),
            Some(CypherExpr::Cmp(
                Box::new(CypherExpr::Prop(
                    Box::new(CypherExpr::Var("b".into())),
                    "Label".into(),
                )),
                CmpOp::Eq,
                Box::new(CypherExpr::Lit(CypherLiteral::String("Person".into()))),
            )),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        // 2 plans (depths 1 and 2), each wrapped with a WHERE filter
        assert_eq!(names.len(), 2);

        // Each plan should have a top-level Select (the WHERE clause)
        for (i, pn) in names.iter().enumerate() {
            let ProjectName::None(expr) = pn else {
                panic!("expected None at index {i}");
            };
            assert!(
                matches!(expr, RelExpr::Select(..)),
                "depth {i} should be wrapped in Select for WHERE, got {expr:?}"
            );
        }
    }

    #[test]
    fn return_single_var() {
        // MATCH (a) RETURN a
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            None,
            ReturnClause::Items(vec![ReturnItem {
                expr: CypherExpr::Var("a".into()),
                alias: None,
            }]),
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        let ProjectName::Some(_, ref alias) = names[0] else {
            panic!("expected ProjectName::Some");
        };
        assert_eq!(alias.as_ref(), "a");
    }

    #[test]
    fn return_property_access() {
        // MATCH (a) RETURN a.Label
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            None,
            ReturnClause::Items(vec![ReturnItem {
                expr: CypherExpr::Prop(
                    Box::new(CypherExpr::Var("a".into())),
                    "Label".into(),
                ),
                alias: None,
            }]),
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::List(_, ref fields) = result else {
            panic!("expected List, got {result:?}");
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0.as_ref(), "a.Label");
        assert_eq!(fields[0].1.table.as_ref(), "a");
    }

    #[test]
    fn return_aliased_property() {
        // MATCH (a) RETURN a.Label AS name
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            None,
            ReturnClause::Items(vec![ReturnItem {
                expr: CypherExpr::Prop(
                    Box::new(CypherExpr::Var("a".into())),
                    "Label".into(),
                ),
                alias: Some("name".into()),
            }]),
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::List(_, ref fields) = result else {
            panic!("expected List");
        };
        assert_eq!(fields[0].0.as_ref(), "name");
    }

    #[test]
    fn anonymous_variables_generated() {
        // (:Person)-[:KNOWS]->(:Person)
        let q = query(
            single_pattern(
                vec![node(None, Some("Person")), node(None, Some("Person"))],
                vec![rel(None, Some("KNOWS"), Direction::Outgoing)],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx());
        assert!(result.is_ok(), "anonymous variables should be auto-generated: {}", result.unwrap_err());
    }

    #[test]
    fn not_literal_bool() {
        // MATCH (a) WHERE NOT true RETURN a
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            Some(CypherExpr::Not(Box::new(CypherExpr::Lit(
                CypherLiteral::Bool(true),
            )))),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };
        let RelExpr::Select(_, filter) = expr else {
            panic!("expected Select, got {expr:?}");
        };
        assert!(matches!(filter, Expr::Not(_)), "expected Expr::Not, got {filter:?}");
    }

    #[test]
    fn not_comparison() {
        // MATCH (a) WHERE NOT a.Label = 'Person' RETURN a
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            Some(CypherExpr::Not(Box::new(CypherExpr::Cmp(
                Box::new(CypherExpr::Prop(
                    Box::new(CypherExpr::Var("a".into())),
                    "Label".into(),
                )),
                CmpOp::Eq,
                Box::new(CypherExpr::Lit(CypherLiteral::String("Person".into()))),
            )))),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };
        let RelExpr::Select(_, filter) = expr else {
            panic!("expected Select, got {expr:?}");
        };
        assert!(matches!(filter, Expr::Not(_)), "expected Expr::Not, got {filter:?}");
    }

    #[test]
    fn not_nested_in_and() {
        // MATCH (a) WHERE a.Id = 1 AND NOT a.Label = 'Bot' RETURN a
        let q = query(
            single_pattern(vec![node(Some("a"), None)], vec![]),
            Some(CypherExpr::And(
                Box::new(CypherExpr::Cmp(
                    Box::new(CypherExpr::Prop(
                        Box::new(CypherExpr::Var("a".into())),
                        "Id".into(),
                    )),
                    CmpOp::Eq,
                    Box::new(CypherExpr::Lit(CypherLiteral::Integer(1))),
                )),
                Box::new(CypherExpr::Not(Box::new(CypherExpr::Cmp(
                    Box::new(CypherExpr::Prop(
                        Box::new(CypherExpr::Var("a".into())),
                        "Label".into(),
                    )),
                    CmpOp::Eq,
                    Box::new(CypherExpr::Lit(CypherLiteral::String("Bot".into()))),
                )))),
            )),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx());
        assert!(result.is_ok(), "NOT in AND should translate: {}", result.unwrap_err());
    }

    #[test]
    fn empty_match_rejected() {
        let q = CypherQuery {
            match_clause: MatchClause { patterns: vec![] },
            where_clause: None,
            return_clause: ReturnClause::All,
        };
        let err = translate_cypher(&q, &tx()).unwrap_err();
        assert!(matches!(err, CypherTranslateError::EmptyPattern));
    }

    // ── Multi-MATCH tests ─────────────────────────────────────────

    #[test]
    fn multi_match_independent_cross_join() {
        // MATCH (a:Person) MATCH (b:Company) RETURN *
        let q = multi_query(
            vec![
                single_pattern(vec![node(Some("a"), Some("Person"))], vec![]),
                single_pattern(vec![node(Some("b"), Some("Company"))], vec![]),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name, got {result:?}");
        };
        assert_eq!(names.len(), 1);
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected ProjectName::None");
        };

        // Should be: Select(LeftDeepJoin(RelVar(a), RelVar(b)), <filters>)
        let RelExpr::Select(inner, _filter) = expr else {
            panic!("expected Select for label filters, got {expr:?}");
        };
        assert!(
            matches!(inner.as_ref(), RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs })
                if matches!(lhs.as_ref(), RelExpr::RelVar(r) if r.alias.as_ref() == "a")
                && rhs.alias.as_ref() == "b"),
            "expected LeftDeepJoin(a, b), got {inner:?}"
        );
    }

    #[test]
    fn multi_match_independent_no_labels() {
        // MATCH (a) MATCH (b) RETURN *
        let q = multi_query(
            vec![
                single_pattern(vec![node(Some("a"), None)], vec![]),
                single_pattern(vec![node(Some("b"), None)], vec![]),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };
        // No labels → no Select, just LeftDeepJoin
        assert!(
            matches!(expr, RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs })
                if matches!(lhs.as_ref(), RelExpr::RelVar(r) if r.alias.as_ref() == "a")
                && rhs.alias.as_ref() == "b"),
            "expected LeftDeepJoin(a, b), got {expr:?}"
        );
    }

    #[test]
    fn multi_match_three_independent() {
        // MATCH (a) MATCH (b) MATCH (c) RETURN *
        let q = multi_query(
            vec![
                single_pattern(vec![node(Some("a"), None)], vec![]),
                single_pattern(vec![node(Some("b"), None)], vec![]),
                single_pattern(vec![node(Some("c"), None)], vec![]),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };
        // LeftDeepJoin(LeftDeepJoin(a, b), c)
        let RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs }) = expr else {
            panic!("expected outer LeftDeepJoin, got {expr:?}");
        };
        assert_eq!(rhs.alias.as_ref(), "c");
        assert!(
            matches!(lhs.as_ref(), RelExpr::LeftDeepJoin(LeftDeepJoin { lhs: inner, rhs: inner_rhs })
                if matches!(inner.as_ref(), RelExpr::RelVar(r) if r.alias.as_ref() == "a")
                && inner_rhs.alias.as_ref() == "b"),
            "expected inner LeftDeepJoin(a, b), got {lhs:?}"
        );
    }

    #[test]
    fn multi_match_shared_variable_correlated() {
        // MATCH (a)-[r]->(b) MATCH (b)-[s]->(c) RETURN *
        // b is shared → second pattern reuses the existing plan (no duplicate RelVar)
        let q = multi_query(
            vec![
                single_pattern(
                    vec![node(Some("a"), None), node(Some("b"), None)],
                    vec![rel(Some("r"), None, Direction::Outgoing)],
                ),
                single_pattern(
                    vec![node(Some("b"), None), node(Some("c"), None)],
                    vec![rel(Some("s"), None, Direction::Outgoing)],
                ),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }
        // Pattern 1: 2 EqJoins (a→r, r→b)
        // Pattern 2: b is shared first node (reused), 2 EqJoins (b→s, s→c)
        // Total: 4 EqJoins, no Select (no label filters)
        assert_eq!(count_joins(expr), 4, "shared-variable pattern should have 4 EqJoins");
    }

    #[test]
    fn multi_match_shared_first_node_reused() {
        // MATCH (a) MATCH (a)-[r]->(b) RETURN *
        // a is shared → second pattern should not create a new RelVar for a
        let q = multi_query(
            vec![
                single_pattern(vec![node(Some("a"), None)], vec![]),
                single_pattern(
                    vec![node(Some("a"), None), node(Some("b"), None)],
                    vec![rel(Some("r"), None, Direction::Outgoing)],
                ),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Plan should be: EqJoin(EqJoin(RelVar(a), edge(r)), vertex(b))
        // No LeftDeepJoin because a is shared (reused from pattern 1)
        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }
        assert_eq!(count_joins(expr), 2, "shared first-node should produce 2 EqJoins (edge+vertex)");
    }

    #[test]
    fn multi_match_with_where_clause() {
        // MATCH (a:Person) MATCH (b:Company) WHERE a.Label = 'Alice' RETURN *
        let q = multi_query(
            vec![
                single_pattern(vec![node(Some("a"), Some("Person"))], vec![]),
                single_pattern(vec![node(Some("b"), Some("Company"))], vec![]),
            ],
            Some(CypherExpr::Cmp(
                Box::new(CypherExpr::Prop(
                    Box::new(CypherExpr::Var("a".into())),
                    "Label".into(),
                )),
                CmpOp::Eq,
                Box::new(CypherExpr::Lit(CypherLiteral::String("Alice".into()))),
            )),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx());
        assert!(result.is_ok(), "multi-MATCH with WHERE should translate: {}", result.unwrap_err());
    }

    #[test]
    fn multi_match_with_edge_and_independent_node() {
        // MATCH (a)-[r]->(b) MATCH (c) RETURN *
        let q = multi_query(
            vec![
                single_pattern(
                    vec![node(Some("a"), None), node(Some("b"), None)],
                    vec![rel(Some("r"), None, Direction::Outgoing)],
                ),
                single_pattern(vec![node(Some("c"), None)], vec![]),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Should be: LeftDeepJoin(EqJoin(EqJoin(a, r), b), c)
        let RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs }) = expr else {
            panic!("expected LeftDeepJoin for cross-join with c, got {expr:?}");
        };
        assert_eq!(rhs.alias.as_ref(), "c");

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                _ => 0,
            }
        }
        assert_eq!(count_joins(lhs), 2, "first pattern should have 2 EqJoins");
    }

    #[test]
    fn multi_match_return_single_var() {
        // MATCH (a:Person) MATCH (b:Company) RETURN a
        let q = multi_query(
            vec![
                single_pattern(vec![node(Some("a"), Some("Person"))], vec![]),
                single_pattern(vec![node(Some("b"), Some("Company"))], vec![]),
            ],
            None,
            ReturnClause::Items(vec![ReturnItem {
                expr: CypherExpr::Var("a".into()),
                alias: None,
            }]),
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        assert_eq!(names.len(), 1);
        let ProjectName::Some(_, ref alias) = names[0] else {
            panic!("expected ProjectName::Some");
        };
        assert_eq!(alias.as_ref(), "a");
    }

    #[test]
    fn multi_match_shared_non_first_node() {
        // MATCH (a)-[r]->(b) MATCH (c)-[s]->(b) RETURN *
        // b appears as a non-first node in pattern 2 → equi-join filter
        let q = multi_query(
            vec![
                single_pattern(
                    vec![node(Some("a"), None), node(Some("b"), None)],
                    vec![rel(Some("r"), None, Direction::Outgoing)],
                ),
                single_pattern(
                    vec![node(Some("c"), None), node(Some("b"), None)],
                    vec![rel(Some("s"), None, Direction::Outgoing)],
                ),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name");
        };
        let ProjectName::None(ref expr) = names[0] else {
            panic!("expected None");
        };

        // Pattern 1: 2 EqJoins (a→r, r→b)
        // Pattern 2: c is new → LeftDeepJoin, 1 EqJoin (c→s),
        //   b is shared non-first → filter s.EndId = b.Id (no new RelVar)
        // Total: 3 EqJoins + 1 LeftDeepJoin, wrapped in Select for filter
        assert!(
            matches!(expr, RelExpr::Select(..)),
            "expected Select wrapper for shared non-first node correlation, got {expr:?}"
        );

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, .. }) => count_joins(lhs),
                _ => 0,
            }
        }
        assert_eq!(count_joins(expr), 3, "shared non-first node should have 3 EqJoins");
    }

    // ── Undirected edge tests ─────────────────────────────────────

    #[test]
    fn single_hop_undirected_produces_union() {
        // (a)-[r]-(b) RETURN * → 2 plans (outgoing + incoming)
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![rel(Some("r"), None, Direction::Undirected)],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name, got {result:?}");
        };
        assert_eq!(names.len(), 2, "undirected edge should produce 2 plans");

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }

        let ProjectName::None(ref e1) = names[0] else { panic!("expected None") };
        let ProjectName::None(ref e2) = names[1] else { panic!("expected None") };

        // Each plan should have 2 EqJoins (a→r, r→b)
        assert_eq!(count_joins(e1), 2, "plan 1 should have 2 EqJoins");
        assert_eq!(count_joins(e2), 2, "plan 2 should have 2 EqJoins");

        // Verify direction swapping: one plan uses StartId as near, the other uses EndId
        fn find_edge_near_col(e: &RelExpr) -> Option<usize> {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, _, rhs) => {
                    if rhs.table.as_ref() == "r" {
                        Some(rhs.field)
                    } else {
                        find_edge_near_col(lhs)
                    }
                }
                RelExpr::Select(inner, _) => find_edge_near_col(inner),
                _ => None,
            }
        }

        let near1 = find_edge_near_col(e1).expect("plan 1 should have edge near col");
        let near2 = find_edge_near_col(e2).expect("plan 2 should have edge near col");

        // StartId = index 1, EndId = index 2 (in Edge table: Id(0), StartId(1), EndId(2), EdgeType(3), Properties(4))
        let cols = [near1, near2];
        assert!(cols.contains(&1), "one plan should use StartId (1) as near");
        assert!(cols.contains(&2), "one plan should use EndId (2) as near");
    }

    #[test]
    fn undirected_with_rel_type() {
        // (a)-[r:KNOWS]-(b) RETURN * → 2 plans, each with EdgeType filter
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![rel(Some("r"), Some("KNOWS"), Direction::Undirected)],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        assert_eq!(names.len(), 2, "undirected with rel_type should produce 2 plans");

        fn count_selects(e: &RelExpr) -> usize {
            match e {
                RelExpr::Select(inner, _) => 1 + count_selects(inner),
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => count_selects(lhs),
                _ => 0,
            }
        }

        let ProjectName::None(ref e1) = names[0] else { panic!("expected None") };
        let ProjectName::None(ref e2) = names[1] else { panic!("expected None") };

        // Each plan should have 1 Select for the EdgeType filter
        assert_eq!(count_selects(e1), 1, "plan 1 should have 1 Select for EdgeType");
        assert_eq!(count_selects(e2), 1, "plan 2 should have 1 Select for EdgeType");
    }

    #[test]
    fn undirected_return_single_var() {
        // (a)-[r]-(b) RETURN a → 2 plans, each returning a
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![rel(Some("r"), None, Direction::Undirected)],
            ),
            None,
            ReturnClause::Items(vec![ReturnItem {
                expr: CypherExpr::Var("a".into()),
                alias: None,
            }]),
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        assert_eq!(names.len(), 2, "undirected should produce 2 plans");

        for (i, pn) in names.iter().enumerate() {
            let ProjectName::Some(_, alias) = pn else {
                panic!("expected ProjectName::Some at index {i}");
            };
            assert_eq!(alias.as_ref(), "a", "plan {i} should return a");
        }
    }

    #[test]
    fn undirected_with_where_clause() {
        // (a)-[r]-(b) WHERE a.Label = 'Person' RETURN *
        let q = query(
            single_pattern(
                vec![node(Some("a"), None), node(Some("b"), None)],
                vec![rel(Some("r"), None, Direction::Undirected)],
            ),
            Some(CypherExpr::Cmp(
                Box::new(CypherExpr::Prop(
                    Box::new(CypherExpr::Var("a".into())),
                    "Label".into(),
                )),
                CmpOp::Eq,
                Box::new(CypherExpr::Lit(CypherLiteral::String("Person".into()))),
            )),
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        assert_eq!(names.len(), 2, "undirected with WHERE should produce 2 plans");

        for (i, pn) in names.iter().enumerate() {
            let ProjectName::None(expr) = pn else {
                panic!("expected None at index {i}");
            };
            assert!(
                matches!(expr, RelExpr::Select(..)),
                "plan {i} should be wrapped in Select for WHERE, got {expr:?}"
            );
        }
    }

    #[test]
    fn multi_match_undirected() {
        // MATCH (a)-[r]-(b) MATCH (c) RETURN *
        let q = multi_query(
            vec![
                single_pattern(
                    vec![node(Some("a"), None), node(Some("b"), None)],
                    vec![rel(Some("r"), None, Direction::Undirected)],
                ),
                single_pattern(vec![node(Some("c"), None)], vec![]),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        // Undirected pattern produces 2 variants, each cross-joined with (c)
        assert_eq!(names.len(), 2, "multi-match with undirected should produce 2 plans");
    }

    #[test]
    fn multi_hop_with_undirected() {
        // (a)-[r]-(b)-[s]->(c) → 2 plans (undirected edge expanded)
        let q = query(
            single_pattern(
                vec![
                    node(Some("a"), None),
                    node(Some("b"), None),
                    node(Some("c"), None),
                ],
                vec![
                    rel(Some("r"), None, Direction::Undirected),
                    rel(Some("s"), None, Direction::Outgoing),
                ],
            ),
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected Name, got {result:?}");
        };
        assert_eq!(
            names.len(),
            2,
            "single undirected hop in multi-hop should produce 2 plans"
        );

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }

        let ProjectName::None(ref e1) = names[0] else { panic!("expected None") };
        let ProjectName::None(ref e2) = names[1] else { panic!("expected None") };

        // a-r-b-s-c = 4 EqJoins per plan
        assert_eq!(count_joins(e1), 4, "plan 1 should have 4 EqJoins");
        assert_eq!(count_joins(e2), 4, "plan 2 should have 4 EqJoins");
    }

    #[test]
    fn multi_match_variable_length_then_fixed_edge() {
        // MATCH (a)-[*1..2]->(b) MATCH (b)-[s]->(c) RETURN *
        // Variable-length path ending at b, then fixed edge from b to c.
        // b is shared first-node of pattern 2 → reused from vars.
        let q = multi_query(
            vec![
                single_pattern(
                    vec![node(Some("a"), None), node(Some("b"), None)],
                    vec![RelPattern {
                        variable: None,
                        rel_type: None,
                        length: Some(PathLength::Range {
                            min: Some(1),
                            max: Some(2),
                        }),
                        direction: Direction::Outgoing,
                    }],
                ),
                single_pattern(
                    vec![node(Some("b"), None), node(Some("c"), None)],
                    vec![rel(Some("s"), None, Direction::Outgoing)],
                ),
            ],
            None,
            ReturnClause::All,
        );
        let result = translate_cypher(&q, &tx()).unwrap();
        let ProjectList::Name(names) = &result else {
            panic!("expected ProjectList::Name, got {result:?}");
        };
        // Range [1..2] → 2 fixed-depth plans, each with pattern 2 appended
        assert_eq!(names.len(), 2, "VLJ range [1..2] plus fixed edge should produce 2 plans");

        fn count_joins(e: &RelExpr) -> usize {
            match e {
                RelExpr::EqJoin(LeftDeepJoin { lhs, .. }, ..) => 1 + count_joins(lhs),
                RelExpr::Select(inner, _) => count_joins(inner),
                _ => 0,
            }
        }

        let ProjectName::None(ref e1) = names[0] else { panic!("expected None") };
        let ProjectName::None(ref e2) = names[1] else { panic!("expected None") };

        // Depth 1: VLJ expands to 2 EqJoins, plus pattern 2 adds 2 (b→s, s→c) = 4 total
        assert_eq!(count_joins(e1), 4, "depth 1 should have 4 EqJoins");
        // Depth 2: VLJ expands to 4 EqJoins, plus pattern 2 adds 2 = 6 total
        assert_eq!(count_joins(e2), 6, "depth 2 should have 6 EqJoins");
    }
}
