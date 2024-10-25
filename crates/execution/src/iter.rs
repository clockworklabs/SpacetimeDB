use std::borrow::Cow;

use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_table::{
    blob_store::BlobStore,
    btree_index::BTreeIndex,
    static_assert_size,
    table::{IndexScanIter, RowRef, Table, TableScanIter},
};

/// A row from a base table in the form of a pointer or product value
#[derive(Clone)]
pub enum Row<'a> {
    Ptr(RowRef<'a>),
    Ref(&'a ProductValue),
}

impl<'a> Row<'a> {
    /// Expect a pointer value, panic otherwise
    pub fn expect_ptr(&self) -> &'a RowRef {
        match self {
            Self::Ptr(ptr) => ptr,
            _ => unreachable!(),
        }
    }

    /// Expect a product value, panic otherwise
    pub fn expect_ref(&self) -> &'a ProductValue {
        match self {
            Self::Ref(r) => r,
            _ => unreachable!(),
        }
    }
}

static_assert_size!(Row, 32);

/// A tuple returned by a query iterator
#[derive(Clone)]
pub enum Tuple<'a> {
    /// A row from a base table
    Row(Row<'a>),
    /// A temporary constructed by a query operator
    Join(Vec<Row<'a>>),
}

static_assert_size!(Tuple, 40);

impl<'a> Tuple<'a> {
    /// Expect a row from a base table, panic otherwise
    pub fn expect_row(&self) -> &'a Row {
        match self {
            Self::Row(row) => row,
            _ => unreachable!(),
        }
    }

    /// Expect a temporary tuple, panic otherwise
    pub fn expect_join(&'a self) -> &'a [Row<'a>] {
        match self {
            Self::Join(elems) => elems.as_slice(),
            _ => unreachable!(),
        }
    }
}

/// A tuple at a time query iterator.
/// Notice there is no explicit projection operation.
/// This is because for applicable plans,
/// the optimizer can remove intermediate projections,
/// implementing a form of late materialization.
pub enum Iter<'a> {
    /// A [RowRef] table iterator
    TableScan(TableScanIter<'a>),
    /// A [RowRef] index iterator
    IndexScan(IndexScanIter<'a>),
    /// A cross product iterator
    CrossJoin(CrossJoinIter<'a>),
    /// A unique single column index join iterator
    UniqueIxJoin(UniqueIxJoin<'a, ProjEvaluator>),
    /// A unique multi-column index join iterator
    UniqueMultiColIxJoin(UniqueIxJoin<'a, MultiColProjEvaluator<'a>>),
    /// A unique single column index semijoin iterator.
    /// Returns tuples from the streaming side (lhs).
    UniqueIxSemiLhs(UniqueIxSemiLhs<'a, ProjEvaluator>),
    /// A unique multi-column index semijoin iterator.
    /// Returns tuples from the streaming side (lhs).
    UniqueMultiColIxSemiLhs(UniqueIxSemiLhs<'a, MultiColProjEvaluator<'a>>),
    /// A unique single column index semijoin iterator.
    /// Returns [RowRef]s from the index (rhs).
    UniqueIxSemiRhs(UniqueIxSemiRhs<'a, ProjEvaluator>),
    /// A unique multi-column index semijoin iterator.
    /// Returns [RowRef]s from the index (rhs).
    UniqueMultiColIxSemiRhs(UniqueIxSemiRhs<'a, MultiColProjEvaluator<'a>>),
    /// A tuple at a time filter
    Filter(Filter<'a>),
}

impl<'a> Iterator for Iter<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::TableScan(iter) => {
                // Table scans return row ids
                iter.next().map(Row::Ptr).map(Tuple::Row)
            }
            Self::IndexScan(iter) => {
                // Index scans return row ids
                iter.next().map(Row::Ptr).map(Tuple::Row)
            }
            Self::CrossJoin(iter) => {
                iter.next().map(|t| {
                    match t {
                        // A leaf join
                        //   x
                        //  / \
                        // a   b
                        (Tuple::Row(u), Tuple::Row(v)) => Tuple::Join(vec![u, v]),
                        // A right deep join
                        //   x
                        //  / \
                        // a   x
                        //    / \
                        //   b   c
                        (Tuple::Row(r), Tuple::Join(mut rows)) => {
                            let mut pointers = vec![r];
                            pointers.append(&mut rows);
                            Tuple::Join(pointers)
                        }
                        // A left deep join
                        //     x
                        //    / \
                        //   x   c
                        //  / \
                        // a   b
                        (Tuple::Join(mut rows), Tuple::Row(r)) => {
                            rows.push(r);
                            Tuple::Join(rows)
                        }
                        // A bushy join
                        //      x
                        //    /   \
                        //   x     x
                        //  / \   / \
                        // a   b c   d
                        (Tuple::Join(mut lhs), Tuple::Join(mut rhs)) => {
                            lhs.append(&mut rhs);
                            Tuple::Join(lhs)
                        }
                    }
                })
            }
            Self::UniqueIxJoin(iter) => {
                iter.next().map(|t| {
                    match t {
                        // A leaf join
                        //   x
                        //  / \
                        // a   b
                        (Tuple::Row(u), ptr) => Tuple::Join(vec![u, Row::Ptr(ptr)]),
                        // A left deep join
                        //     x
                        //    / \
                        //   x   c
                        //  / \
                        // a   b
                        (Tuple::Join(mut rows), ptr) => {
                            rows.push(Row::Ptr(ptr));
                            Tuple::Join(rows)
                        }
                    }
                })
            }
            Self::UniqueMultiColIxJoin(iter) => {
                iter.next().map(|t| {
                    match t {
                        // A leaf join
                        //   x
                        //  / \
                        // a   b
                        (Tuple::Row(u), ptr) => Tuple::Join(vec![u, Row::Ptr(ptr)]),
                        // A left deep join
                        //     x
                        //    / \
                        //   x   c
                        //  / \
                        // a   b
                        (Tuple::Join(mut rows), ptr) => {
                            rows.push(Row::Ptr(ptr));
                            Tuple::Join(rows)
                        }
                    }
                })
            }
            Self::UniqueIxSemiLhs(iter) => {
                // Left index semijoins return tuples from the lhs
                iter.next()
            }
            Self::UniqueIxSemiRhs(iter) => {
                // Right index semijions return row ids from the index
                iter.next().map(|ptr| Tuple::Row(Row::Ptr(ptr)))
            }
            Self::UniqueMultiColIxSemiLhs(iter) => {
                // Left index semijoins return tuples from the lhs
                iter.next()
            }
            Self::UniqueMultiColIxSemiRhs(iter) => {
                // Right index semijions return row ids from the index
                iter.next().map(|ptr| Tuple::Row(Row::Ptr(ptr)))
            }
            Self::Filter(iter) => {
                // Filter is a passthru
                iter.next()
            }
        }
    }
}

/// A cross join returns the cross product of its two inputs.
/// It materializes the rhs and streams the lhs.
pub struct CrossJoinIter<'a> {
    /// The lhs input
    lhs: Box<Iter<'a>>,
    /// The rhs input
    rhs: Box<Iter<'a>>,
    /// The materialized rhs
    build: Vec<Tuple<'a>>,
    /// The current lhs tuple
    lhs_row: Option<Tuple<'a>>,
    /// The current rhs tuple
    rhs_ptr: usize,
}

impl<'a> Iterator for CrossJoinIter<'a> {
    type Item = (Tuple<'a>, Tuple<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        // Materialize the rhs on the first call
        if self.build.is_empty() {
            self.build = self.rhs.as_mut().collect();
            self.lhs_row = self.lhs.next();
            self.rhs_ptr = 0;
        }
        // Reset the rhs pointer
        if self.rhs_ptr == self.build.len() {
            self.lhs_row = self.lhs.next();
            self.rhs_ptr = 0;
        }
        self.lhs_row.as_ref().map(|lhs_tuple| {
            self.rhs_ptr += 1;
            (lhs_tuple.clone(), self.build[self.rhs_ptr - 1].clone())
        })
    }
}

pub trait TupleProjector {
    fn eval<'a>(&self, tuple: &'a Tuple) -> Cow<'a, AlgebraicValue>;
}

/// A unique index join has the same signature as that of a cross join.
/// It is an index join where the index is a unique index.
/// A primary key index is one such example.
pub struct UniqueIxJoin<'a, P> {
    /// The lhs of the join
    input: Box<Iter<'a>>,
    /// The rhs index
    index: &'a BTreeIndex,
    /// A handle to the datastore
    table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs column projector
    projection: P,
}

impl<'a, P> Iterator for UniqueIxJoin<'a, P>
where
    P: TupleProjector,
{
    type Item = (Tuple<'a>, RowRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.input.find_map(|tuple| {
            self.index
                .seek(self.projection.eval(&tuple).as_ref())
                .next()
                .and_then(|ptr| self.table.get_row_ref(self.blob_store, ptr))
                .map(|ptr| (tuple, ptr))
        })
    }
}

/// This iterator implements a unique index join,
/// followed by a projection of the lhs.
pub struct UniqueIxSemiLhs<'a, P> {
    /// The lhs of the join
    input: Box<Iter<'a>>,
    /// The rhs index
    index: &'a BTreeIndex,
    /// The lhs column projector
    projection: P,
}

impl<'a, P> Iterator for UniqueIxSemiLhs<'a, P>
where
    P: TupleProjector,
{
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.input
            .find(|tuple| self.index.contains_any(self.projection.eval(tuple).as_ref()))
    }
}

/// This iterator implements a unique index join,
/// followed by a projection of the rhs.
pub struct UniqueIxSemiRhs<'a, P> {
    /// The lhs of the join
    input: Box<Iter<'a>>,
    /// The rhs index
    index: &'a BTreeIndex,
    /// A handle to the datastore
    table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs column projector
    projection: P,
}

impl<'a, P> Iterator for UniqueIxSemiRhs<'a, P>
where
    P: TupleProjector,
{
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.find_map(|tuple| {
            self.index
                .seek(self.projection.eval(&tuple).as_ref())
                .next()
                .and_then(|ptr| self.table.get_row_ref(self.blob_store, ptr))
        })
    }
}

/// A tuple at a time filter iterator
pub struct Filter<'a> {
    input: Box<Iter<'a>>,
    predicate: ExprProgram<'a>,
}

impl<'a> Iterator for Filter<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.find(|tuple| {
            ExprEvaluator {
                val_stack: vec![],
                row_stack: vec![],
            }
            .eval(&self.predicate, tuple)
            .as_bool()
            .copied()
            .unwrap_or(false)
        })
    }
}

/// An opcode for a tuple projection operation
#[derive(Clone, Copy)]
pub enum ProjOpCode {
    /// r.0 applied to a [Row::Ptr]
    Ptr(u16),
    /// r.0 applied to a [Row::Ref]
    Ref(u16),
    /// r.0.1 applied to a [Tuple::Join] -> [Row::Ptr]
    TupToPtr(u8, u16),
    /// r.0.1 applied to a [Tuple::Join] -> [Row::Ref]
    TupToRef(u8, u16),
}

static_assert_size!(ProjOpCode, 4);

/// A single column projection evaluator
pub struct ProjEvaluator {
    op: ProjOpCode,
}

impl TupleProjector for ProjEvaluator {
    fn eval<'a>(&self, tuple: &'a Tuple) -> Cow<'a, AlgebraicValue> {
        match self.op {
            ProjOpCode::Ptr(i) => tuple
                .expect_row()
                .expect_ptr()
                .read_col(i as usize)
                .map(Cow::Owned)
                .unwrap(),
            ProjOpCode::Ref(i) => tuple
                .expect_row()
                .expect_ref()
                .elements
                .get(i as usize)
                .map(Cow::Borrowed)
                .unwrap(),
            ProjOpCode::TupToPtr(i, j) => tuple
                .expect_join()
                .get(i as usize)
                .unwrap()
                .expect_ptr()
                .read_col(j as usize)
                .map(Cow::Owned)
                .unwrap(),
            ProjOpCode::TupToRef(i, j) => tuple
                .expect_join()
                .get(i as usize)
                .unwrap()
                .expect_ref()
                .elements
                .get(j as usize)
                .map(Cow::Borrowed)
                .unwrap(),
        }
    }
}

/// A multi-column projection evaluator
pub struct MultiColProjEvaluator<'a> {
    ops: &'a [ProjOpCode],
}

impl TupleProjector for MultiColProjEvaluator<'_> {
    fn eval<'a>(&self, tuple: &'a Tuple) -> Cow<'a, AlgebraicValue> {
        Cow::Owned(AlgebraicValue::Product(ProductValue::from_iter(
            self.ops
                .iter()
                .copied()
                .map(|op| ProjEvaluator { op })
                .map(|evaluator| evaluator.eval(tuple))
                .map(|v| v.into_owned()),
        )))
    }
}

/// An opcode for a stack-based expression evaluator
pub enum OpCode {
    /// ==
    Eq,
    /// <>
    Ne,
    /// <
    Lt,
    /// >
    Gt,
    /// <=
    Lte,
    /// <=
    Gte,
    /// AND
    And,
    /// OR
    Or,
    /// 5
    Const(u16),
    /// r.0 : [Row::Ptr]
    PtrProj(u16),
    /// r.0 : [Row::Ref]
    RefProj(u16),
    /// r.0 : [Tuple::Join]
    TupProj(u16),
}

static_assert_size!(OpCode, 4);

/// A program for evaluating a scalar expression
pub struct ExprProgram<'a> {
    /// The instructions or opcodes
    ops: &'a [OpCode],
    /// The constants in the original expression
    constants: &'a [AlgebraicValue],
}

/// An evaluator for an [ExprProgram]
pub struct ExprEvaluator<'a> {
    val_stack: Vec<Cow<'a, AlgebraicValue>>,
    row_stack: Vec<&'a Row<'a>>,
}

impl<'a> ExprEvaluator<'a> {
    pub fn eval(&mut self, program: &'a ExprProgram, tuple: &'a Tuple) -> Cow<'a, AlgebraicValue> {
        for op in program.ops.iter() {
            match op {
                OpCode::Const(i) => {
                    self.val_stack.push(Cow::Borrowed(&program.constants[*i as usize]));
                }
                OpCode::Eq => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(l == r)));
                }
                OpCode::Ne => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(l != r)));
                }
                OpCode::Lt => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(l < r)));
                }
                OpCode::Gt => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(l > r)));
                }
                OpCode::Lte => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(l <= r)));
                }
                OpCode::Gte => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(l >= r)));
                }
                OpCode::And => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(
                        *l.as_bool().unwrap() && *r.as_bool().unwrap(),
                    )));
                }
                OpCode::Or => {
                    let r = self.val_stack.pop().unwrap();
                    let l = self.val_stack.pop().unwrap();
                    self.val_stack.push(Cow::Owned(AlgebraicValue::Bool(
                        *l.as_bool().unwrap() || *r.as_bool().unwrap(),
                    )));
                }
                OpCode::PtrProj(i) => {
                    self.val_stack.push(Cow::Owned(
                        self.row_stack
                            .pop()
                            .unwrap()
                            .expect_ptr()
                            .read_col(*i as usize)
                            .unwrap(),
                    ));
                }
                OpCode::RefProj(i) => {
                    self.val_stack.push(Cow::Borrowed(
                        self.row_stack
                            .pop()
                            .unwrap()
                            .expect_ref()
                            .elements
                            .get(*i as usize)
                            .unwrap(),
                    ));
                }
                OpCode::TupProj(i) => {
                    self.row_stack.push(&tuple.expect_join()[*i as usize]);
                }
            }
        }
        self.val_stack.pop().unwrap()
    }
}
