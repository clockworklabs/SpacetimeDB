use std::collections::HashMap;
use std::{borrow::Cow, ops::Bound, sync::Arc};

use derive_more::From;

use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};
use spacetimedb_table::table::RowRef;

use crate::rules::{
    ComputePositions, ConjunctionToIxScan, EqToIxScan, HashToIxJoin, PushConjunction, PushEqFilter, RewriteRule,
    UniqueHashJoinRule, UniqueIxJoinRule,
};

/// Table aliases are replaced with labels in the physical plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, From)]
pub struct Label(pub usize);

/// Physical plans always terminate with a projection.
/// This type of projection returns row ids.
///
/// It can represent:
///
/// ```sql
/// select * from t
/// ```
///
/// and
///
/// ```sql
/// select t.* from t join ...
/// ```
///
/// but not
///
/// ```sql
/// select a from t
/// ```
#[derive(Debug)]
pub enum ProjectPlan {
    None(PhysicalPlan),
    Name(PhysicalPlan, Label, Option<usize>),
}

impl ProjectPlan {
    pub fn optimize(self) -> Self {
        match self {
            Self::None(plan) => Self::None(plan.optimize(vec![])),
            Self::Name(plan, label, _) => {
                let plan = plan.optimize(vec![label]);
                let n = plan.nfields();
                let pos = plan.label_pos(&label);
                match n {
                    1 => Self::None(plan),
                    _ => Self::Name(plan, label, pos),
                }
            }
        }
    }
}

/// Physical plans always terminate with a projection.
/// This type can project fields within a table.
///
/// That is, it can represent:
///
/// ```sql
/// select a from t
/// ```
///
/// as well as
///
/// ```sql
/// select t.a, s.b from t join s ...
/// ```
#[derive(Debug)]
pub enum ProjectListPlan {
    Name(ProjectPlan),
    List(PhysicalPlan, Vec<(Box<str>, TupleField)>),
}

impl ProjectListPlan {
    pub fn optimize(self) -> Self {
        match self {
            Self::Name(plan) => Self::Name(plan.optimize()),
            Self::List(plan, fields) => Self::List(
                plan.optimize(
                    fields
                        .iter()
                        .map(|(_, TupleField { label, .. })| label)
                        .copied()
                        .collect(),
                ),
                fields,
            ),
        }
    }
}

/// Query operators return tuples of rows.
/// And this type refers to a field of a row within a tuple.
///
/// Note that from the perspective of the optimizer,
/// tuple elements have names or labels,
/// so as to preserve query semantics across rewrites.
///
/// However from the perspective of the query engine,
/// tuple elements are entirely positional.
/// Hence the need for both `label` and `label_pos`.
///
/// The former is consistent across rewrites.
/// The latter is only computed once after optimization.
#[derive(Debug, PartialEq, Eq)]
pub struct TupleField {
    pub label: Label,
    pub label_pos: Option<usize>,
    pub field_pos: usize,
}

/// A physical plan represents a concrete evaluation strategy.
#[derive(Debug, PartialEq, Eq)]
pub enum PhysicalPlan {
    /// Scan a table row by row, returning row ids
    TableScan(Arc<TableSchema>, Label),
    /// Fetch row ids from an index
    IxScan(IxScan, Label),
    /// An index join + projection
    IxJoin(IxJoin, Semi),
    /// An hash join + projection
    HashJoin(HashJoin, Semi),
    /// A nested loop join
    NLJoin(Box<PhysicalPlan>, Box<PhysicalPlan>),
    /// A tuple-at-a-time filter
    Filter(Box<PhysicalPlan>, PhysicalExpr),
}

impl PhysicalPlan {
    /// Walks the plan tree and calls `f` on every op
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::IxJoin(IxJoin { lhs: input, .. }, _) | Self::Filter(input, _) => {
                input.visit(f);
            }
            Self::NLJoin(lhs, rhs) | Self::HashJoin(HashJoin { lhs, rhs, .. }, _) => {
                lhs.visit(f);
                rhs.visit(f);
            }
            _ => {}
        }
    }

    /// Walks the plan tree and calls `f` on every op
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::IxJoin(IxJoin { lhs: input, .. }, _) | Self::Filter(input, _) => {
                input.visit_mut(f);
            }
            Self::NLJoin(lhs, rhs) | Self::HashJoin(HashJoin { lhs, rhs, .. }, _) => {
                lhs.visit_mut(f);
                rhs.visit_mut(f);
            }
            _ => {}
        }
    }

    /// Is there any subplan where `f` returns true?
    pub fn any(&self, f: &impl Fn(&Self) -> bool) -> bool {
        let mut ok = false;
        self.visit(&mut |plan| {
            ok = ok || f(plan);
        });
        ok
    }

    /// Applies `f` recursively to all subplans
    pub fn map(self, f: &impl Fn(Self) -> Self) -> Self {
        match f(self) {
            Self::Filter(input, expr) => Self::Filter(Box::new(input.map(f)), expr),
            Self::NLJoin(lhs, rhs) => Self::NLJoin(Box::new(lhs.map(f)), Box::new(rhs.map(f))),
            Self::HashJoin(join, semi) => Self::HashJoin(
                HashJoin {
                    lhs: Box::new(join.lhs.map(f)),
                    rhs: Box::new(join.rhs.map(f)),
                    ..join
                },
                semi,
            ),
            Self::IxJoin(join, semi) => Self::IxJoin(
                IxJoin {
                    lhs: Box::new(join.lhs.map(f)),
                    ..join
                },
                semi,
            ),
            plan => plan,
        }
    }

    /// Applies `f` to a subplan if `ok` returns a match.
    /// Recurses until an `ok` match is found.
    pub fn map_if<Info>(self, f: impl FnOnce(Self, Info) -> Self, ok: impl Fn(&Self) -> Option<Info>) -> Self {
        if let Some(info) = ok(&self) {
            return f(self, info);
        }
        let matches = |plan: &PhysicalPlan| {
            // Does `ok` match a subplan?
            plan.any(&|plan| ok(plan).is_some())
        };
        match self {
            Self::NLJoin(lhs, rhs) if matches(&lhs) => {
                // Replace the lhs subtree
                Self::NLJoin(Box::new(lhs.map_if(f, ok)), rhs)
            }
            Self::NLJoin(lhs, rhs) if matches(&rhs) => {
                // Replace the rhs subtree
                Self::NLJoin(lhs, Box::new(rhs.map_if(f, ok)))
            }
            Self::HashJoin(join, semi) if matches(&join.lhs) => Self::HashJoin(
                HashJoin {
                    lhs: Box::new(join.lhs.map_if(f, ok)),
                    ..join
                },
                semi,
            ),
            Self::HashJoin(join, semi) if matches(&join.rhs) => Self::HashJoin(
                HashJoin {
                    rhs: Box::new(join.rhs.map_if(f, ok)),
                    ..join
                },
                semi,
            ),
            Self::IxJoin(join, semi) if matches(&join.lhs) => Self::IxJoin(
                IxJoin {
                    lhs: Box::new(join.lhs.map_if(f, ok)),
                    ..join
                },
                semi,
            ),
            Self::Filter(input, expr) if matches(&input) => {
                // Replace the input only if there is a match
                Self::Filter(Box::new(input.map_if(f, ok)), expr)
            }
            _ => self,
        }
    }

    /// Applies a rewrite rule once to this plan.
    /// Updates indicator variable if plan was modified.
    pub fn apply_once<R: RewriteRule<Plan = PhysicalPlan>>(self, ok: &mut bool) -> Self {
        if let Some(info) = R::matches(&self) {
            *ok = true;
            return R::rewrite(self, info);
        }
        self
    }

    /// Recursively apply a rule to all subplans until a fixedpoint is reached.
    pub fn apply_rec<R: RewriteRule<Plan = PhysicalPlan>>(self) -> Self {
        let mut ok = false;
        let plan = self.map_if(
            |plan, info| {
                ok = true;
                R::rewrite(plan, info)
            },
            R::matches,
        );
        if ok {
            return plan.apply_rec::<R>();
        }
        plan
    }

    /// Repeatedly apply a rule until a fixedpoint is reached.
    /// It does not apply rule recursively to subplans.
    pub fn apply_until<R: RewriteRule<Plan = PhysicalPlan>>(self) -> Self {
        let mut ok = false;
        let plan = self.apply_once::<R>(&mut ok);
        if ok {
            return plan.apply_until::<R>();
        }
        plan
    }

    /// Optimize a plan using the following rewrites:
    ///
    /// 1. Canonicalize the plan
    /// 2. Push filters to the leaves
    /// 3. Turn filters into index scans if possible
    /// 4. Determine index and semijoins
    /// 5. Compute positions for tuple labels
    pub fn optimize(self, reqs: Vec<Label>) -> Self {
        self.map(&Self::canonicalize)
            .apply_until::<PushConjunction>()
            .apply_until::<PushEqFilter>()
            .apply_rec::<EqToIxScan>()
            .apply_rec::<ConjunctionToIxScan>()
            .apply_rec::<HashToIxJoin>()
            .apply_rec::<UniqueIxJoinRule>()
            .apply_rec::<UniqueHashJoinRule>()
            .introduce_semijoins(reqs)
            .apply_rec::<ComputePositions>()
    }

    /// The rewriter assumes a canonicalized plan.
    /// And this means:
    ///
    /// 1. Literals are always on the rhs of a sargable predicate.
    /// 2. Nested ANDs and ORs are flattened.
    /// 3. The lhs(rhs) expr corresponds to the lhs(rhs) of an equijoin.
    ///
    /// Examples:
    ///
    /// 1. Move values to rhs
    /// ```sql
    /// select * from a where 3 = a.x
    /// ```
    ///
    /// ... to ..
    ///
    /// ```sql
    /// select * from a where a.x = 3
    /// ```
    ///
    /// 2. Flatten ANDs and ORs
    /// ```sql
    /// select * from a where (a.x = 3 and a.y = 4) and a.z = 5
    /// ```
    ///
    /// ... to ..
    ///
    /// ```sql
    /// select * from a where a.x = 3 and a.y = 4 and a.z = 5
    /// ```
    ///
    /// 3. Canonicalize equijoin
    /// ```sql
    /// select a.* from a join b on b.id = a.id
    /// ```
    ///
    /// ... to ...
    ///
    /// ```sql
    /// select a.* from a join b on a.id = b.id
    /// ```
    fn canonicalize(self) -> Self {
        match self {
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field,
                    rhs_field,
                    unique,
                },
                semi,
            ) if rhs.has_label(&lhs_field.label) || lhs.has_label(&rhs_field.label) => Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field: rhs_field,
                    rhs_field: lhs_field,
                    unique,
                },
                semi,
            ),
            Self::Filter(input, expr) => {
                let move_value_to_rhs = |expr| match expr {
                    PhysicalExpr::BinOp(op, value, expr)
                        if matches!(&*value, PhysicalExpr::Value(_)) && matches!(&*expr, PhysicalExpr::Field(..)) =>
                    {
                        PhysicalExpr::BinOp(op, expr, value)
                    }
                    _ => expr,
                };
                // Flatten ANDs and ORs, and move values to rhs
                Self::Filter(input, expr.flatten().map(&move_value_to_rhs))
            }
            _ => self,
        }
    }

    /// Introduce semijoins in the plan.
    ///
    /// Example:
    ///
    /// p:  project
    /// x:  join
    /// sx: semijoin
    ///
    /// ```text
    ///    p(c)
    ///     |
    ///     x
    ///    / \
    ///   x   c
    ///  / \
    /// a   b
    ///
    /// ... to ...
    ///
    ///    p(c)
    ///     |
    ///     x
    ///    / \
    ///  p(b) c
    ///   |
    ///   x
    ///  / \
    /// a   b
    ///
    /// ... to ..
    ///
    ///     sx
    ///    /  \
    ///   sx   c
    ///  /  \
    /// a    b
    /// ```
    ///
    /// ```sql
    /// select c.*
    /// from (select * from a where a.x = 3) a
    /// join b on a.id = b.id
    /// join c on b.id = c.id
    /// ```
    ///
    /// ... to ...
    ///
    /// ```sql
    /// select c.*
    /// from (
    ///   select b.*
    ///   from (select * from a where a.x = 3) a
    ///   join b on a.id = b.id
    /// ) b
    /// join c on b.id = c.id
    /// ```
    fn introduce_semijoins(self, mut reqs: Vec<Label>) -> Self {
        impl PhysicalPlan {
            fn append_required_label(&self, reqs: &mut Vec<Label>, label: Label) {
                if !reqs.contains(&label) && self.has_label(&label) {
                    reqs.push(label);
                }
            }
        }
        match self {
            Self::Filter(input, expr) => {
                expr.visit(&mut |expr| {
                    if let PhysicalExpr::Field(TupleField { label: var, .. }) = expr {
                        if !reqs.contains(var) {
                            reqs.push(*var);
                        }
                    }
                });
                Self::Filter(Box::new(input.introduce_semijoins(reqs)), expr)
            }
            Self::NLJoin(lhs, rhs) => {
                let mut lhs_reqs = vec![];
                let mut rhs_reqs = vec![];

                for var in reqs {
                    lhs.append_required_label(&mut lhs_reqs, var);
                    rhs.append_required_label(&mut rhs_reqs, var);
                }
                let lhs = lhs.introduce_semijoins(lhs_reqs);
                let rhs = rhs.introduce_semijoins(rhs_reqs);
                let lhs = Box::new(lhs);
                let rhs = Box::new(rhs);
                Self::NLJoin(lhs, rhs)
            }
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field: lhs_field @ TupleField { label: u, .. },
                    rhs_field: rhs_field @ TupleField { label: v, .. },
                    unique,
                },
                Semi::All,
            ) => {
                let semi = reqs
                    .iter()
                    .all(|label| lhs.has_label(label))
                    .then_some(Semi::Lhs)
                    .or_else(|| reqs.iter().all(|label| rhs.has_label(label)).then_some(Semi::Rhs))
                    .unwrap_or(Semi::All);
                let mut lhs_reqs = vec![u];
                let mut rhs_reqs = vec![v];
                for var in reqs {
                    lhs.append_required_label(&mut lhs_reqs, var);
                    rhs.append_required_label(&mut rhs_reqs, var);
                }
                let lhs = lhs.introduce_semijoins(lhs_reqs);
                let rhs = rhs.introduce_semijoins(rhs_reqs);
                let lhs = Box::new(lhs);
                let rhs = Box::new(rhs);
                Self::HashJoin(
                    HashJoin {
                        lhs,
                        rhs,
                        lhs_field,
                        rhs_field,
                        unique,
                    },
                    semi,
                )
            }
            Self::IxJoin(join, Semi::All) if reqs.len() == 1 && join.rhs_label == reqs[0] => {
                let lhs = join.lhs.introduce_semijoins(vec![join.lhs_field.label]);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Rhs)
            }
            Self::IxJoin(join, Semi::All) if reqs.iter().all(|var| *var != join.rhs_label) => {
                if !reqs.contains(&join.lhs_field.label) {
                    reqs.push(join.lhs_field.label);
                }
                let lhs = join.lhs.introduce_semijoins(reqs);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Lhs)
            }
            Self::IxJoin(join, Semi::All) => {
                let reqs = reqs.into_iter().filter(|label| label != &join.rhs_label).collect();
                let lhs = join.lhs.introduce_semijoins(reqs);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::All)
            }
            _ => self,
        }
    }

    // Does this plan return distinct values for these columns?
    pub(crate) fn returns_distinct_values(&self, label: &Label, cols: &ColSet) -> bool {
        match self {
            // Is there a unique constraint for these cols?
            Self::TableScan(schema, var) => var == label && schema.is_unique(cols),
            // Is there a unique constraint for these cols + the index cols?
            Self::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(col, _),
                    ..
                },
                var,
            ) => {
                var == label
                    && schema.is_unique(&ColSet::from_iter(
                        cols.iter()
                            .chain(prefix.iter().map(|(col_id, _)| *col_id))
                            .chain(vec![*col]),
                    ))
            }
            // If the table in question is on the lhs,
            // and if the lhs returns distinct values,
            // we need the rhs to return at most one row when probed.
            // But this is a unique index join,
            // so by definition this requirement is satisfied.
            Self::IxJoin(IxJoin { lhs, unique: true, .. }, _) if lhs.has_label(label) => {
                lhs.returns_distinct_values(label, cols)
            }
            // If the table in question is on the rhs,
            // and if the rhs returns distinct values,
            // we must not probe the rhs for the same value more than once.
            // Hence the lhs must be distinct w.r.t the probe field.
            Self::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    lhs_field:
                        TupleField {
                            label: lhs_label,
                            field_pos: lhs_field_pos,
                            ..
                        },
                    ..
                },
                _,
            ) => {
                lhs.returns_distinct_values(lhs_label, &ColSet::from(ColId(*lhs_field_pos as u16)))
                    && rhs.is_unique(cols)
            }
            // If the table in question is on the lhs,
            // and if the lhs returns distinct values,
            // we need the rhs to return at most one row when probed.
            // Hence the rhs must be distinct w.r.t the probe field.
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    rhs_field:
                        TupleField {
                            label: rhs_label,
                            field_pos: rhs_field_pos,
                            ..
                        },
                    ..
                },
                _,
            ) if lhs.has_label(label) => {
                lhs.returns_distinct_values(label, cols)
                    && rhs.returns_distinct_values(rhs_label, &ColSet::from(ColId(*rhs_field_pos as u16)))
            }
            // If the table in question is on the rhs,
            // and if the rhs returns distinct values,
            // we must not probe the rhs for the same value more than once.
            // Hence the lhs must be distinct w.r.t the probe field.
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field:
                        TupleField {
                            label: lhs_label,
                            field_pos: lhs_field_pos,
                            ..
                        },
                    ..
                },
                _,
            ) => {
                rhs.returns_distinct_values(label, cols)
                    && lhs.returns_distinct_values(lhs_label, &ColSet::from(ColId(*lhs_field_pos as u16)))
            }
            // For the columns in question,
            // the base table may not return distinct values,
            // but given the necessary equality conditions,
            // the filter can return distinct values for them.
            Self::Filter(input, expr) => {
                let mut cols: Vec<_> = cols.iter().collect();
                expr.visit(&mut |plan| {
                    if let PhysicalExpr::BinOp(BinOp::Eq, expr, value) = plan {
                        if let (PhysicalExpr::Field(proj), PhysicalExpr::Value(..)) = (&**expr, &**value) {
                            if proj.label == *label {
                                cols.push(proj.field_pos.into());
                            }
                        }
                    }
                });
                input.returns_distinct_values(label, &ColSet::from_iter(cols))
            }
            _ => false,
        }
    }

    /// Does this plan introduce this label?
    fn has_label(&self, label: &Label) -> bool {
        self.any(&|plan| match plan {
            Self::TableScan(_, var) | Self::IxScan(_, var) | Self::IxJoin(IxJoin { rhs_label: var, .. }, _) => {
                var == label
            }
            _ => false,
        })
    }

    /// How many fields do the tuples returned by this plan have?
    fn nfields(&self) -> usize {
        match self {
            Self::TableScan(..) | Self::IxScan(..) | Self::IxJoin(_, Semi::Rhs) => 1,
            Self::Filter(input, _) => input.nfields(),
            Self::IxJoin(join, Semi::Lhs) => join.lhs.nfields(),
            Self::IxJoin(join, Semi::All) => join.lhs.nfields() + 1,
            Self::HashJoin(join, Semi::Rhs) => join.rhs.nfields(),
            Self::HashJoin(join, Semi::Lhs) => join.lhs.nfields(),
            Self::HashJoin(join, Semi::All) => join.lhs.nfields() + join.rhs.nfields(),
            Self::NLJoin(lhs, rhs) => lhs.nfields() + rhs.nfields(),
        }
    }

    /// What is the position of this label in the return tuple?
    pub(crate) fn label_pos(&self, label: &Label) -> Option<usize> {
        match self {
            Self::TableScan(_, var) | Self::IxScan(_, var) if var == label => Some(0),
            Self::IxJoin(join, Semi::Rhs) if &join.rhs_label == label => Some(0),
            Self::TableScan(..) | Self::IxScan(..) | Self::IxJoin(_, Semi::Rhs) => None,
            Self::Filter(input, _) => input.label_pos(label),
            Self::NLJoin(lhs, rhs) => lhs
                .label_pos(label)
                .or_else(|| rhs.label_pos(label).map(|pos| pos + lhs.nfields())),
            Self::IxJoin(join, Semi::Lhs) => join.lhs.label_pos(label),
            Self::IxJoin(IxJoin { lhs, rhs_label, .. }, Semi::All) if rhs_label == label => Some(lhs.nfields()),
            Self::IxJoin(IxJoin { lhs, .. }, Semi::All) => lhs.label_pos(label),
            Self::HashJoin(HashJoin { rhs, .. }, Semi::Rhs) => rhs.label_pos(label),
            Self::HashJoin(HashJoin { lhs, .. }, Semi::Lhs) => lhs.label_pos(label),
            Self::HashJoin(HashJoin { lhs, rhs, .. }, Semi::All) => lhs
                .label_pos(label)
                .or_else(|| rhs.label_pos(label).map(|pos| pos + lhs.nfields())),
        }
    }
}

/// Fetch and return row ids from a btree index
#[derive(Debug, PartialEq, Eq)]
pub struct IxScan {
    /// The table on which this index is defined
    pub schema: Arc<TableSchema>,
    /// The index id
    pub index_id: IndexId,
    /// An equality prefix for multi-column scans
    pub prefix: Vec<(ColId, AlgebraicValue)>,
    /// The index argument
    pub arg: Sarg,
}

/// An index [S]earch [arg]ument
#[derive(Debug, PartialEq, Eq)]
pub enum Sarg {
    Eq(ColId, AlgebraicValue),
    Range(BinOp, ColId, Bound<AlgebraicValue>, Bound<AlgebraicValue>),
}

/// A join of two relations on a single equality condition.
/// It builds a hash table for the rhs and streams the lhs.
#[derive(Debug, PartialEq, Eq)]
pub struct HashJoin {
    pub lhs: Box<PhysicalPlan>,
    pub rhs: Box<PhysicalPlan>,
    pub lhs_field: TupleField,
    pub rhs_field: TupleField,
    pub unique: bool,
}

/// An index join is a left deep join tree,
/// where the lhs is a relation,
/// and the rhs is a relvar or base table,
/// whose rows are fetched using an index.
#[derive(Debug, PartialEq, Eq)]
pub struct IxJoin {
    /// The lhs input used to probe the index
    pub lhs: Box<PhysicalPlan>,
    /// The rhs indexed table
    pub rhs: Arc<TableSchema>,
    /// The rhs relvar label
    pub rhs_label: Label,
    /// The index id
    pub rhs_index: IndexId,
    /// The index field
    pub rhs_field: ColId,
    /// Is the index a unique constraint index?
    pub unique: bool,
    /// The expression for computing probe values.
    /// Values are projected from the lhs,
    /// and used to probe the index on the rhs.
    pub lhs_field: TupleField,
}

/// Is this a semijoin?
/// If so, which side is projected?
#[derive(Debug, PartialEq, Eq)]
pub enum Semi {
    Lhs,
    Rhs,
    All,
}

/// A physical scalar expression
#[derive(Debug, PartialEq, Eq)]
pub enum PhysicalExpr {
    /// An n-ary logic expression
    LogOp(LogOp, Vec<PhysicalExpr>),
    /// A binary expression
    BinOp(BinOp, Box<PhysicalExpr>, Box<PhysicalExpr>),
    /// A constant algebraic value
    Value(AlgebraicValue),
    /// A field projection expression
    Field(TupleField),
}

/// A trait for projecting values from a tuple.
/// This is needed because not all tuples are created equal.
/// Some operators return [RowRef]s.
/// Some joins return tuples of combined [RowRef]s.
pub trait ProjectField {
    fn project(&self, field: &TupleField) -> AlgebraicValue;
}

impl ProjectField for RowRef<'_> {
    fn project(&self, field: &TupleField) -> AlgebraicValue {
        self.read_col(field.field_pos).unwrap()
    }
}

impl PhysicalExpr {
    /// Walks the expression tree and calls `f` on every subexpression
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Self::LogOp(_, exprs) => {
                for expr in exprs {
                    expr.visit(f);
                }
            }
            _ => {}
        }
    }

    /// Is there any subplan where `f` returns true?
    pub fn any(&self, f: impl Fn(&Self) -> bool) -> bool {
        let mut ok = false;
        self.visit(&mut |plan| {
            ok = ok || f(plan);
        });
        ok
    }

    /// Applies the transformation `f` to all subplans
    pub fn map(self, f: &impl Fn(Self) -> Self) -> Self {
        match f(self) {
            value @ Self::Value(..) => value,
            field @ Self::Field(..) => field,
            Self::BinOp(op, a, b) => Self::BinOp(op, Box::new(a.map(f)), Box::new(b.map(f))),
            Self::LogOp(op, exprs) => Self::LogOp(op, exprs.into_iter().map(|expr| expr.map(f)).collect()),
        }
    }

    /// Evaluate this boolean expression over `row`
    pub fn eval_bool(&self, row: &impl ProjectField) -> bool {
        self.eval(row).as_bool().copied().unwrap_or(false)
    }

    /// Evaluate this expression over `row`
    fn eval(&self, row: &impl ProjectField) -> Cow<'_, AlgebraicValue> {
        fn eval_bin_op(op: BinOp, a: &AlgebraicValue, b: &AlgebraicValue) -> bool {
            match op {
                BinOp::Eq => a == b,
                BinOp::Ne => a != b,
                BinOp::Lt => a < b,
                BinOp::Lte => a <= b,
                BinOp::Gt => a > b,
                BinOp::Gte => a >= b,
            }
        }
        let into = |b| Cow::Owned(AlgebraicValue::Bool(b));
        match self {
            Self::BinOp(op, a, b) => into(eval_bin_op(*op, &a.eval(row), &b.eval(row))),
            Self::LogOp(LogOp::And, exprs) => exprs
                .iter()
                .all(|expr| expr.eval_bool(row))
                .then(|| AlgebraicValue::Bool(true))
                .map(Cow::Owned)
                .unwrap_or_else(|| into(false)),
            Self::LogOp(LogOp::Or, exprs) => exprs
                .iter()
                .any(|expr| expr.eval_bool(row))
                .then(|| AlgebraicValue::Bool(true))
                .map(Cow::Owned)
                .unwrap_or_else(|| into(false)),
            Self::Field(field) => row
                .project(field)
                .as_bool()
                .copied()
                .map(into)
                .unwrap_or_else(|| into(false)),
            Self::Value(v) => v.as_bool().copied().map(into).unwrap_or_else(|| into(false)),
        }
    }

    /// Flatten nested ANDs and ORs
    fn flatten(self) -> Self {
        match self {
            Self::LogOp(op, exprs) => Self::LogOp(
                op,
                exprs
                    .into_iter()
                    .map(Self::flatten)
                    .flat_map(|expr| match expr {
                        Self::LogOp(nested, exprs) if nested == op => exprs,
                        _ => vec![expr],
                    })
                    .collect(),
            ),
            Self::BinOp(op, a, b) => Self::BinOp(op, Box::new(a.flatten()), Box::new(b.flatten())),
            Self::Field(..) | Self::Value(..) => self,
        }
    }

    /// Compute the positions of all tuple labels
    pub(crate) fn label_positions(&mut self, plan: &PhysicalPlan) {
        match self {
            Self::Field(field @ TupleField { label_pos: None, .. }) => {
                field.label_pos = plan.label_pos(&field.label);
            }
            Self::BinOp(_, a, b) => {
                a.label_positions(plan);
                b.label_positions(plan);
            }
            Self::LogOp(_, exprs) => {
                for expr in exprs {
                    expr.label_positions(plan);
                }
            }
            _ => {}
        }
    }
}

/// A physical context for the result of a query compilation.
#[derive(Debug)]
pub struct PhysicalCtx<'a> {
    pub plan: ProjectListPlan,
    pub sql: &'a str,
    // A map from table names to their labels
    pub vars: HashMap<String, usize>,
    pub source: StatementSource,
    pub planning_time: std::time::Duration,
}

impl<'a> PhysicalCtx<'a> {
    pub fn optimize(self) -> Self {
        Self {
            plan: self.plan.optimize(),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::compile::{compile, compile_sub};
    use crate::plan::TupleField;
    use crate::printer::Explain;
    use expect_test::{expect, Expect};
    use pretty_assertions::assert_eq;
    use spacetimedb_expr::check::{compile_sql_sub, parse_and_type_sub, SchemaView};
    use spacetimedb_expr::statement::compile_sql_stmt;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::AlgebraicType;
    use spacetimedb_primitives::{ColList, TableId};
    use spacetimedb_schema::def::{BTreeAlgorithm, ConstraintData, IndexAlgorithm, UniqueConstraintData};
    use spacetimedb_schema::schema::{ColumnSchema, ConstraintSchema, IndexSchema};
    use std::sync::Arc;

    struct SchemaViewer {
        schemas: Vec<Arc<TableSchema>>,
        optimize: bool,
        show_source: bool,
        show_schema: bool,
        show_timings: bool,
    }

    impl SchemaViewer {
        fn new(schemas: Vec<Arc<TableSchema>>) -> Self {
            Self {
                schemas,
                optimize: false,
                show_source: false,
                show_schema: false,
                show_timings: false,
            }
        }

        fn optimize(mut self) -> Self {
            self.optimize = true;
            self
        }
        fn with_source(mut self) -> Self {
            self.show_source = true;
            self
        }
        fn with_schema(mut self) -> Self {
            self.show_schema = true;
            self
        }
        // TODO: Remove when we integrate it.
        #[allow(dead_code)]
        fn with_timings(mut self) -> Self {
            self.show_timings = true;
            self
        }
    }

    impl SchemaView for SchemaViewer {
        fn table_id(&self, name: &str) -> Option<TableId> {
            self.schemas
                .iter()
                .find(|schema| schema.table_name.as_ref() == name)
                .map(|schema| schema.table_id)
        }

        fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableSchema>> {
            self.schemas.iter().find(|schema| schema.table_id == table_id).cloned()
        }
    }

    fn schema(
        table_id: TableId,
        table_name: &str,
        columns: &[(&str, AlgebraicType)],
        indexes: &[&[usize]],
        unique: &[&[usize]],
        primary_key: Option<usize>,
    ) -> TableSchema {
        TableSchema::new(
            table_id,
            table_name.to_owned().into_boxed_str(),
            columns
                .iter()
                .enumerate()
                .map(|(i, (name, ty))| ColumnSchema {
                    table_id,
                    col_name: (*name).to_owned().into_boxed_str(),
                    col_pos: i.into(),
                    col_type: ty.clone(),
                })
                .collect(),
            indexes
                .iter()
                .enumerate()
                .map(|(i, cols)| IndexSchema {
                    table_id,
                    index_id: i.into(),
                    index_name: "".to_owned().into_boxed_str(),
                    index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm {
                        columns: ColList::from_iter(cols.iter().copied()),
                    }),
                })
                .collect(),
            unique
                .iter()
                .enumerate()
                .map(|(i, cols)| ConstraintSchema {
                    table_id,
                    constraint_id: i.into(),
                    constraint_name: "".to_owned().into_boxed_str(),
                    data: ConstraintData::Unique(UniqueConstraintData {
                        columns: ColSet::from_iter(cols.iter().copied()),
                    }),
                })
                .collect(),
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            primary_key.map(ColId::from),
        )
    }

    //TODO: This test are not ported until we integrate the changes that allow correctly report the labels.

    /// No rewrites applied to a simple table scan
    #[test]
    fn table_scan_noop() {
        let t_id = TableId(1);

        let t = Arc::new(schema(
            t_id,
            "t",
            &[("id", AlgebraicType::U64), ("x", AlgebraicType::U64)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer::new(vec![t.clone()]);
        let sql = "select * from t";

        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_sub(lp).optimize();

        match pp {
            ProjectPlan::None(PhysicalPlan::TableScan(schema, _)) => {
                assert_eq!(schema.table_id, t_id);
            }
            proj => panic!("unexpected project: {:#?}", proj),
        };
    }

    /// No rewrites applied to a table scan + filter
    #[test]
    fn filter_noop() {
        let t_id = TableId(1);

        let t = Arc::new(schema(
            t_id,
            "t",
            &[("id", AlgebraicType::U64), ("x", AlgebraicType::U64)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer::new(vec![t.clone()]);

        let sql = "select * from t where x = 5";

        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_sub(lp).optimize();

        match pp {
            ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*field, PhysicalExpr::Field(TupleField { field_pos: 1, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U64(5))));

                match *input {
                    PhysicalPlan::TableScan(schema, _) => {
                        assert_eq!(schema.table_id, t_id);
                    }
                    plan => panic!("unexpected plan: {:#?}", plan),
                }
            }
            proj => panic!("unexpected project: {:#?}", proj),
        };
    }

    /// Given the following operator notation:
    ///
    /// x:  join
    /// p:  project
    /// s:  select
    /// ix: index scan
    /// rx: right index semijoin
    ///
    /// This test takes the following logical plan:
    ///
    /// ```text
    ///       p(b)
    ///        |
    ///        x
    ///       / \
    ///      x   b
    ///     / \
    ///    x   l
    ///   / \
    /// s(u) l
    ///  |
    ///  u
    /// ```
    ///
    /// And turns it into the following physical plan:
    ///
    /// ```text
    ///         rx
    ///        /  \
    ///       rx   b
    ///      /  \
    ///     rx   l
    ///    /  \
    /// ix(u)  l
    /// ```
    #[test]
    fn index_semijoins_1() {
        let u_id = TableId(1);
        let l_id = TableId(2);
        let b_id = TableId(3);

        let u = Arc::new(schema(
            u_id,
            "u",
            &[("identity", AlgebraicType::U64), ("entity_id", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0], &[1]],
            Some(0),
        ));

        let l = Arc::new(schema(
            l_id,
            "l",
            &[("entity_id", AlgebraicType::U64), ("chunk", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        ));

        let b = Arc::new(schema(
            b_id,
            "b",
            &[("entity_id", AlgebraicType::U64), ("misc", AlgebraicType::U64)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer::new(vec![u.clone(), l.clone(), b.clone()]).optimize();

        let sql = "
            select b.*
            from u
            join l as p on u.entity_id = p.entity_id
            join l as q on p.chunk = q.chunk
            join b on q.entity_id = b.entity_id
            where u.identity = 5
        ";

        check_sub(
            &db,
            sql,
            expect![[r#"
Index Join: Rhs
  -> Index Join: Rhs
      -> Index Join: Rhs
          -> Index Scan using Index id 0: (identity) on u
            -> Index Cond: (u.identity = U64(5))
        -> Inner Unique: true
        -> Index Cond: (u.entity_id = p.entity_id)
    -> Inner Unique: false
    -> Index Cond: (p.chunk = q.chunk)
  Inner Unique: true
  Index Cond: (q.entity_id = b.entity_id)
  Output: b.entity_id, b.misc"#]],
        );

        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_sub(lp).optimize();

        // Plan:
        //         rx
        //        /  \
        //       rx   b
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match pp {
            ProjectPlan::None(plan) => plan,
            proj => panic!("unexpected project: {:#?}", proj),
        };

        // Plan:
        //         rx
        //        /  \
        //       rx   b
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: true,
                    lhs_field: TupleField { field_pos: 0, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, b_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //       rx
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(1),
                    unique: false,
                    lhs_field: TupleField { field_pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, l_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //     rx
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: true,
                    lhs_field: TupleField { field_pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, l_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan: ix(u)
        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(ColId(0), AlgebraicValue::U64(5)),
                    ..
                },
                _,
            ) => {
                assert!(prefix.is_empty());
                assert_eq!(schema.table_id, u_id);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        }
    }

    /// Given the following operator notation:
    ///
    /// x:  join
    /// p:  project
    /// s:  select
    /// ix: index scan
    /// rx: right index semijoin
    /// rj: right hash semijoin
    ///
    /// This test takes the following logical plan:
    ///
    /// ```text
    ///         p(p)
    ///          |
    ///          x
    ///         / \
    ///        x   p
    ///       / \
    ///      x   s(w)
    ///     / \   |
    ///    x   w  w
    ///   / \
    /// s(m) m
    ///  |
    ///  m
    /// ```
    ///
    /// And turns it into the following physical plan:
    ///
    /// ```text
    ///           rx
    ///          /  \
    ///         rj   p
    ///        /  \
    ///       rx  ix(w)
    ///      /  \
    ///     rx   w
    ///    /  \
    /// ix(m)  m
    /// ```
    #[test]
    fn index_semijoins_2() {
        let m_id = TableId(1);
        let w_id = TableId(2);
        let p_id = TableId(3);

        let m = Arc::new(schema(
            m_id,
            "m",
            &[("employee", AlgebraicType::U64), ("manager", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        ));

        let w = Arc::new(schema(
            w_id,
            "w",
            &[("employee", AlgebraicType::U64), ("project", AlgebraicType::U64)],
            &[&[0], &[1], &[0, 1]],
            &[&[0, 1]],
            None,
        ));

        let p = Arc::new(schema(
            p_id,
            "p",
            &[("id", AlgebraicType::U64), ("name", AlgebraicType::String)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer::new(vec![m.clone(), w.clone(), p.clone()]);

        let sql = "
            select p.*
            from m
            join m as n on m.manager = n.manager
            join w as u on n.employee = u.employee
            join w as v on u.project = v.project
            join p on p.id = v.project
            where 5 = m.employee and 5 = v.employee
        ";

        check_sub(
            &db,
            sql,
            expect![[r#"
Hash Join: All
  -> Hash Join: All
      -> Hash Join: All
          -> Hash Join: All
              -> Seq Scan on m
              -> Seq Scan on n
            -> Inner Unique: false
            -> Hash Cond: (m.manager = n.manager)
          -> Seq Scan on u
        -> Inner Unique: false
        -> Hash Cond: (n.employee = u.employee)
      -> Seq Scan on v
    -> Inner Unique: false
    -> Hash Cond: (u.project = v.project)
  -> Seq Scan on p
  Inner Unique: false
  Hash Cond: (p.id = v.project)
  Filter: (m.employee = U64(5) AND v.employee = U64(5))
  Output: p.id, p.name"#]],
        );

        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_sub(lp).optimize();

        // Plan:
        //           rx
        //          /  \
        //         rj   p
        //        /  \
        //       rx  ix(w)
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match pp {
            ProjectPlan::None(plan) => plan,
            proj => panic!("unexpected project: {:#?}", proj),
        };

        // Plan:
        //           rx
        //          /  \
        //         rj   p
        //        /  \
        //       rx  ix(w)
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: true,
                    lhs_field: TupleField { field_pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, p_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //         rj
        //        /  \
        //       rx  ix(w)
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let (rhs, lhs) = match plan {
            PhysicalPlan::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field: TupleField { field_pos: 1, .. },
                    rhs_field: TupleField { field_pos: 1, .. },
                    unique: true,
                },
                Semi::Rhs,
            ) => (*rhs, *lhs),
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan: ix(w)
        match rhs {
            PhysicalPlan::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(ColId(0), AlgebraicValue::U64(5)),
                    ..
                },
                _,
            ) => {
                assert!(prefix.is_empty());
                assert_eq!(schema.table_id, w_id);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        }

        // Plan:
        //       rx
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match lhs {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: false,
                    lhs_field: TupleField { field_pos: 0, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, w_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //     rx
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(1),
                    unique: false,
                    lhs_field: TupleField { field_pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, m_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan: ix(m)
        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(ColId(0), AlgebraicValue::U64(5)),
                    ..
                },
                _,
            ) => {
                assert!(prefix.is_empty());
                assert_eq!(schema.table_id, m_id);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        }
    }

    fn data() -> SchemaViewer {
        let m_id = TableId(1);
        let w_id = TableId(2);
        let p_id = TableId(3);

        let m = Arc::new(schema(
            m_id,
            "m",
            &[("employee", AlgebraicType::U64), ("manager", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        ));

        let w = Arc::new(schema(
            w_id,
            "w",
            &[("employee", AlgebraicType::U64), ("project", AlgebraicType::U64)],
            &[&[0], &[1], &[0, 1]],
            &[&[0, 1]],
            None,
        ));

        let p = Arc::new(schema(
            p_id,
            "p",
            &[("id", AlgebraicType::U64), ("name", AlgebraicType::String)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        SchemaViewer::new(vec![m.clone(), w.clone(), p.clone()])
    }

    fn sub<'a>(db: &'a SchemaViewer, sql: &'a str) -> PhysicalCtx<'a> {
        let plan = compile_sql_sub(sql, db).unwrap();
        compile(plan)
    }

    fn query<'a>(db: &'a SchemaViewer, sql: &'a str) -> PhysicalCtx<'a> {
        let plan = compile_sql_stmt(sql, db).unwrap();
        compile(plan)
    }

    fn check(db: &SchemaViewer, plan: PhysicalCtx, expect: Expect) {
        let plan = if db.optimize { plan.optimize() } else { plan };

        let explain = Explain::new(&plan);
        let explain = if db.show_source { explain.with_source() } else { explain };
        let explain = if db.show_schema { explain.with_schema() } else { explain };
        let explain = if db.show_timings {
            explain.with_timings()
        } else {
            explain
        };

        let explain = explain.build();
        expect.assert_eq(&explain.to_string());
    }
    fn check_sub(db: &SchemaViewer, sql: &str, expect: Expect) {
        let plan = sub(db, sql);
        check(db, plan, expect);
    }

    fn check_query(db: &SchemaViewer, sql: &str, expect: Expect) {
        let plan = query(db, sql);
        check(db, plan, expect);
    }

    #[test]
    fn plan_metadata() {
        let db = data().with_schema().with_source().optimize();
        check_query(
            &db,
            "SELECT m.* FROM m CROSS JOIN p WHERE m.employee = 1",
            expect![
                r#"
                Query: SELECT m.* FROM m CROSS JOIN p WHERE m.employee = 1
                Nested Loop
                  -> Index Scan using Index id 0: (employee) on m:1
                    -> Index Cond: (m.employee = U64(1))
                  -> Seq Scan on p:2
                  Output: m.employee, m.manager
                -------
                Schema:

                Label m: 1
                  Columns: employee, manager
                  Indexes: Unique(m.employee)
                Label p: 2
                  Columns: id, name
                  Indexes: Unique(p.id)"#
            ],
        );
    }

    #[test]
    fn table_scan() {
        let db = data();
        check_sub(
            &db,
            "SELECT * FROM p",
            expect![
                r#"
                Seq Scan on p
                  Output: p.id, p.name"#
            ],
        );
    }

    #[test]
    fn table_alias() {
        let db = data();
        check_sub(
            &db,
            "SELECT * FROM p as b",
            expect![
                r#"
                Seq Scan on b
                  Output: b.id, b.name"#
            ],
        );
        check_sub(
            &db,
            "select p.*
            from w
            join m as p",
            expect![
                r#"
                Nested Loop
                  -> Seq Scan on w
                  -> Seq Scan on p
                  Output: p.employee, p.manager"#
            ],
        );
    }

    #[test]
    fn table_project() {
        let db = data();
        check_query(
            &db,
            "SELECT id FROM p",
            expect![
                r#"
                Seq Scan on p
                  Output: p.id"#
            ],
        );

        check_query(
            &db,
            "SELECT p.id,m.employee FROM m CROSS JOIN p",
            expect![
                r#"
                Nested Loop
                  -> Seq Scan on m
                  -> Seq Scan on p
                  Output: p.id, m.employee"#
            ],
        );
    }

    #[test]
    fn table_scan_filter() {
        let db = data();

        check_sub(
            &db,
            "SELECT * FROM p WHERE id > 1",
            expect![[r#"
                Seq Scan on p
                  Filter: (p.id > U64(1))
                  Output: p.id, p.name"#]],
        );

        check_query(
            &db,
            "SELECT * FROM p WHERE id = 1 AND id =2 OR name = 'jhon'",
            expect![[r#"
                Seq Scan on p
                  Filter: (p.id = U64(1) AND p.id = U64(2) OR p.name = String("jhon"))
                  Output: p.id, p.name"#]],
        );
    }

    #[test]
    fn index_scan_filter() {
        let db = data().optimize();

        check_sub(
            &db,
            "SELECT m.* FROM m WHERE employee = 1",
            expect![[r#"
                Index Scan using Index id 0: (employee) on m
                  Index Cond: (m.employee = U64(1))
                  Output: m.employee, m.manager"#]],
        );
    }

    #[test]
    fn cross_join() {
        let db = data();

        check_sub(
            &db,
            "SELECT p.* FROM m JOIN p",
            expect![[r#"
                Nested Loop
                  -> Seq Scan on m
                  -> Seq Scan on p
                  Output: p.id, p.name"#]],
        );
    }

    #[test]
    fn hash_join() {
        let db = data();

        check_sub(
            &db,
            "SELECT p.* FROM m JOIN p ON m.employee = p.id where m.employee = 1",
            expect![[r#"
                Hash Join: All
                  -> Seq Scan on m
                  -> Seq Scan on p
                  Inner Unique: false
                  Hash Cond: (m.employee = p.id)
                  Filter: (m.employee = U64(1))
                  Output: p.id, p.name"#]],
        );
    }

    #[test]
    fn semi_join() {
        let db = data().optimize();

        check_sub(
            &db,
            "SELECT p.* FROM m JOIN p ON m.employee = p.id",
            expect![[r#"
                Index Join: Rhs
                  -> Seq Scan on m
                  Inner Unique: true
                  Index Cond: (m.employee = p.id)
                  Output: p.id, p.name"#]],
        );
    }
}
