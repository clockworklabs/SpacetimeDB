use std::ops::{Bound, RangeBounds};

use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_table::{
    blob_store::BlobStore,
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    static_assert_size,
    table::{IndexScanIter, RowRef, Table, TableScanIter},
};

/// A row from a base table in the form of a pointer or product value
#[derive(Clone)]
pub enum Row<'a> {
    Ptr(RowRef<'a>),
    Ref(&'a ProductValue),
}

impl Row<'_> {
    /// Expect a pointer value, panic otherwise
    pub fn expect_ptr(&self) -> &RowRef {
        match self {
            Self::Ptr(ptr) => ptr,
            _ => unreachable!(),
        }
    }

    /// Expect a product value, panic otherwise
    pub fn expect_ref(&self) -> &ProductValue {
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

impl Tuple<'_> {
    /// Expect a row from a base table, panic otherwise
    pub fn expect_row(&self) -> &Row {
        match self {
            Self::Row(row) => row,
            _ => unreachable!(),
        }
    }

    /// Expect a temporary tuple, panic otherwise
    pub fn expect_join(&self) -> &[Row] {
        match self {
            Self::Join(elems) => elems.as_slice(),
            _ => unreachable!(),
        }
    }
}

/// An execution plan for a tuple-at-a-time iterator.
/// As the name suggests it is meant to be cached.
/// Building the iterator should incur minimal overhead.
pub struct CachedIterPlan {
    /// The relational ops
    iter_ops: Box<[IterOp]>,
    /// The expression ops
    expr_ops: Box<[OpCode]>,
    /// The constants referenced by the plan
    constants: Box<[AlgebraicValue]>,
}

static_assert_size!(CachedIterPlan, 48);

impl CachedIterPlan {
    /// Returns an interator over the query ops
    fn ops(&self) -> impl Iterator<Item = IterOp> + '_ {
        self.iter_ops.iter().copied()
    }

    /// Lookup a constant in the plan
    fn constant(&self, i: u16) -> &AlgebraicValue {
        &self.constants[i as usize]
    }
}

/// An opcode for a tuple-at-a-time execution plan
#[derive(Clone, Copy)]
pub enum IterOp {
    /// A table scan opcode takes 1 arg: A [TableId]
    TableScan(TableId),
    /// A delta scan opcode takes 1 arg: A [TableId]
    DeltaScan(TableId),
    /// An index scan opcode takes 2 args:
    /// 1. An [IndexId]
    /// 2. A ptr to an [AlgebraicValue]
    IxScanEq(IndexId, u16),
    /// An index range scan opcode takes 3 args:
    /// 1. An [IndexId]
    /// 2. A ptr to the lower bound
    /// 3. A ptr to the upper bound
    IxScanRange(IndexId, Bound<u16>, Bound<u16>),
    /// Pops its 2 args from the stack
    NLJoin,
    /// An index join opcode takes 2 args:
    /// 1. An [IndexId]
    /// 2. An instruction ptr
    /// 3. A length
    IxJoin(IndexId, usize, u16),
    /// An index join opcode takes 2 args:
    /// 1. An [IndexId]
    /// 2. An instruction ptr
    /// 3. A length
    UniqueIxJoin(IndexId, usize, u16),
    /// A filter opcode takes 2 args:
    /// 1. An instruction ptr
    /// 2. A length
    Filter(usize, u32),
}

static_assert_size!(IterOp, 16);

pub trait Datastore {
    fn delta_scan_iter(&self, table_id: TableId) -> DeltaScanIter;
    fn table_scan_iter(&self, table_id: TableId) -> TableScanIter;
    fn index_scan_iter(&self, index_id: IndexId, range: &impl RangeBounds<AlgebraicValue>) -> IndexScanIter;
    fn get_table_for_index(&self, index_id: &IndexId) -> &Table;
    fn get_index(&self, index_id: &IndexId) -> &BTreeIndex;
    fn get_blob_store(&self) -> &dyn BlobStore;
}

/// An iterator for a delta table
pub struct DeltaScanIter<'a> {
    iter: std::slice::Iter<'a, ProductValue>,
}

impl<'a> Iterator for DeltaScanIter<'a> {
    type Item = &'a ProductValue;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl CachedIterPlan {
    pub fn iter<'a>(&'a self, tx: &'a impl Datastore) -> Iter<'a> {
        let mut stack = vec![];
        for op in self.ops() {
            match op {
                IterOp::TableScan(table_id) => {
                    // Push table scan
                    stack.push(Iter::TableScan(tx.table_scan_iter(table_id)));
                }
                IterOp::DeltaScan(table_id) => {
                    // Push delta scan
                    stack.push(Iter::DeltaScan(tx.delta_scan_iter(table_id)));
                }
                IterOp::IxScanEq(index_id, ptr) => {
                    // Push index scan
                    stack.push(Iter::IndexScan(tx.index_scan_iter(index_id, &self.constant(ptr))));
                }
                IterOp::IxScanRange(index_id, lower, upper) => {
                    // Push range scan
                    let lower = lower.map(|ptr| self.constant(ptr));
                    let upper = upper.map(|ptr| self.constant(ptr));
                    stack.push(Iter::IndexScan(tx.index_scan_iter(index_id, &(lower, upper))));
                }
                IterOp::NLJoin => {
                    // Pop args and push nested loop join
                    let rhs = stack.pop().unwrap();
                    let lhs = stack.pop().unwrap();
                    stack.push(Iter::NLJoin(NestedLoopJoin::new(lhs, rhs)));
                }
                IterOp::IxJoin(index_id, i, n) => {
                    // Pop arg and push index join
                    let input = stack.pop().unwrap();
                    let index = tx.get_index(&index_id);
                    let table = tx.get_table_for_index(&index_id);
                    let blob_store = tx.get_blob_store();
                    let ops = &self.expr_ops[i..i + n as usize];
                    let program = ExprProgram::new(ops, &self.constants);
                    let projection = ProgramEvaluator::from(program);
                    stack.push(Iter::IxJoin(LeftDeepJoin::Eq(IndexJoin::new(
                        input, index, table, blob_store, projection,
                    ))));
                }
                IterOp::UniqueIxJoin(index_id, i, n) => {
                    // Pop arg and push index join
                    let input = stack.pop().unwrap();
                    let index = tx.get_index(&index_id);
                    let table = tx.get_table_for_index(&index_id);
                    let blob_store = tx.get_blob_store();
                    let ops = &self.expr_ops[i..i + n as usize];
                    let program = ExprProgram::new(ops, &self.constants);
                    let projection = ProgramEvaluator::from(program);
                    stack.push(Iter::UniqueIxJoin(LeftDeepJoin::Eq(UniqueIndexJoin::new(
                        input, index, table, blob_store, projection,
                    ))));
                }
                IterOp::Filter(i, n) => {
                    // Pop arg and push filter
                    let input = Box::new(stack.pop().unwrap());
                    let ops = &self.expr_ops[i..i + n as usize];
                    let program = ExprProgram::new(ops, &self.constants);
                    let program = ProgramEvaluator::from(program);
                    stack.push(Iter::Filter(Filter { input, program }));
                }
            }
        }
        stack.pop().unwrap()
    }
}

/// A tuple-at-a-time query iterator.
/// Notice there is no explicit projection operation.
/// This is because for applicable plans,
/// the optimizer can remove intermediate projections,
/// implementing a form of late materialization.
pub enum Iter<'a> {
    /// A [RowRef] table iterator
    TableScan(TableScanIter<'a>),
    /// A [ProductValue] ref iterator
    DeltaScan(DeltaScanIter<'a>),
    /// A [RowRef] index iterator
    IndexScan(IndexScanIter<'a>),
    /// A nested loop join iterator
    NLJoin(NestedLoopJoin<'a>),
    /// A non-unique (constraint) index join iterator
    IxJoin(LeftDeepJoin<IndexJoin<'a>>),
    /// A unique (constraint) index join iterator
    UniqueIxJoin(LeftDeepJoin<UniqueIndexJoin<'a>>),
    /// A tuple-at-a-time filter iterator
    Filter(Filter<'a>),
}

impl<'a> Iterator for Iter<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::TableScan(iter) => {
                // Returns row ids
                iter.next().map(Row::Ptr).map(Tuple::Row)
            }
            Self::DeltaScan(iter) => {
                // Returns product refs
                iter.next().map(Row::Ref).map(Tuple::Row)
            }
            Self::IndexScan(iter) => {
                // Returns row ids
                iter.next().map(Row::Ptr).map(Tuple::Row)
            }
            Self::IxJoin(iter) => {
                // Returns row ids for semijoins, (n+1)-tuples otherwise
                iter.next()
            }
            Self::UniqueIxJoin(iter) => {
                // Returns row ids for semijoins, (n+1)-tuples otherwise
                iter.next()
            }
            Self::Filter(iter) => {
                // Filter is a passthru
                iter.next()
            }
            Self::NLJoin(iter) => {
                iter.next().map(|t| {
                    match t {
                        // A leaf join
                        //   x
                        //  / \
                        // a   b
                        (Tuple::Row(u), Tuple::Row(v)) => {
                            // Returns a 2-tuple
                            Tuple::Join(vec![u, v])
                        }
                        // A right deep join
                        //   x
                        //  / \
                        // a   x
                        //    / \
                        //   b   c
                        (Tuple::Row(r), Tuple::Join(mut rows)) => {
                            // Returns an (n+1)-tuple
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
                            // Returns an (n+1)-tuple
                            rows.push(r);
                            Tuple::Join(rows)
                        }
                        // A bushy join
                        //      x
                        //     / \
                        //    /   \
                        //   x     x
                        //  / \   / \
                        // a   b c   d
                        (Tuple::Join(mut lhs), Tuple::Join(mut rhs)) => {
                            // Returns an (n+m)-tuple
                            lhs.append(&mut rhs);
                            Tuple::Join(lhs)
                        }
                    }
                })
            }
        }
    }
}

/// An iterator for a left deep join tree
pub enum LeftDeepJoin<Iter> {
    /// A standard join
    Eq(Iter),
    /// A semijoin that returns the lhs
    SemiLhs(Iter),
    /// A semijion that returns the rhs
    SemiRhs(Iter),
}

impl<'a, Iter> Iterator for LeftDeepJoin<Iter>
where
    Iter: Iterator<Item = (Tuple<'a>, RowRef<'a>)>,
{
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::SemiLhs(iter) => {
                // Return the lhs tuple
                iter.next().map(|(t, _)| t)
            }
            Self::SemiRhs(iter) => {
                // Return the rhs row
                iter.next().map(|(_, ptr)| ptr).map(Row::Ptr).map(Tuple::Row)
            }
            Self::Eq(iter) => {
                iter.next().map(|(tuple, ptr)| {
                    match (tuple, ptr) {
                        // A leaf join
                        //   x
                        //  / \
                        // a   b
                        (Tuple::Row(u), ptr) => {
                            // Returns a 2-tuple
                            Tuple::Join(vec![u, Row::Ptr(ptr)])
                        }
                        // A left deep join
                        //     x
                        //    / \
                        //   x   c
                        //  / \
                        // a   b
                        (Tuple::Join(mut rows), ptr) => {
                            // Returns an (n+1)-tuple
                            rows.push(Row::Ptr(ptr));
                            Tuple::Join(rows)
                        }
                    }
                })
            }
        }
    }
}

/// A unique (constraint) index join iterator
pub struct UniqueIndexJoin<'a> {
    /// The lhs of the join
    input: Box<Iter<'a>>,
    /// The rhs index
    index: &'a BTreeIndex,
    /// A handle to the datastore
    table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs index key projection
    projection: ProgramEvaluator<'a>,
}

impl<'a> UniqueIndexJoin<'a> {
    fn new(
        input: Iter<'a>,
        index: &'a BTreeIndex,
        table: &'a Table,
        blob_store: &'a dyn BlobStore,
        projection: ProgramEvaluator<'a>,
    ) -> Self {
        Self {
            input: Box::new(input),
            index,
            table,
            blob_store,
            projection,
        }
    }
}

impl<'a> Iterator for UniqueIndexJoin<'a> {
    type Item = (Tuple<'a>, RowRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.input.find_map(|tuple| {
            self.index
                .seek(&self.projection.eval(&tuple))
                .next()
                .and_then(|ptr| self.table.get_row_ref(self.blob_store, ptr))
                .map(|ptr| (tuple, ptr))
        })
    }
}

/// A non-unique (constraint) index join iterator
pub struct IndexJoin<'a> {
    /// The lhs of the join
    input: Box<Iter<'a>>,
    /// The current tuple from the lhs
    tuple: Option<Tuple<'a>>,
    /// The rhs index
    index: &'a BTreeIndex,
    /// The current cursor for the rhs index
    index_cursor: Option<BTreeIndexRangeIter<'a>>,
    /// A handle to the datastore
    table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs index key projection
    projection: ProgramEvaluator<'a>,
}

impl<'a> IndexJoin<'a> {
    fn new(
        input: Iter<'a>,
        index: &'a BTreeIndex,
        table: &'a Table,
        blob_store: &'a dyn BlobStore,
        projection: ProgramEvaluator<'a>,
    ) -> Self {
        Self {
            input: Box::new(input),
            tuple: None,
            index,
            index_cursor: None,
            table,
            blob_store,
            projection,
        }
    }
}

impl<'a> Iterator for IndexJoin<'a> {
    type Item = (Tuple<'a>, RowRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.tuple
            .as_ref()
            .and_then(|tuple| {
                self.index_cursor.as_mut().and_then(|cursor| {
                    cursor.next().and_then(|ptr| {
                        self.table
                            .get_row_ref(self.blob_store, ptr)
                            .map(|ptr| (tuple.clone(), ptr))
                    })
                })
            })
            .or_else(|| {
                self.input.find_map(|tuple| {
                    Some(self.index.seek(&self.projection.eval(&tuple))).and_then(|mut cursor| {
                        cursor.next().and_then(|ptr| {
                            self.table.get_row_ref(self.blob_store, ptr).map(|ptr| {
                                self.tuple = Some(tuple.clone());
                                self.index_cursor = Some(cursor);
                                (tuple, ptr)
                            })
                        })
                    })
                })
            })
    }
}

/// A nested loop join returns the cross product of its inputs
pub struct NestedLoopJoin<'a> {
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

impl<'a> NestedLoopJoin<'a> {
    fn new(lhs: Iter<'a>, rhs: Iter<'a>) -> Self {
        Self {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            build: vec![],
            lhs_row: None,
            rhs_ptr: 0,
        }
    }
}

impl<'a> Iterator for NestedLoopJoin<'a> {
    type Item = (Tuple<'a>, Tuple<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        for t in self.rhs.as_mut() {
            self.build.push(t);
        }
        match self.build.get(self.rhs_ptr) {
            Some(v) => {
                self.rhs_ptr += 1;
                self.lhs_row.as_ref().map(|u| (u.clone(), v.clone()))
            }
            None => {
                self.rhs_ptr = 1;
                self.lhs_row = self.lhs.next();
                self.lhs_row
                    .as_ref()
                    .zip(self.build.first())
                    .map(|(u, v)| (u.clone(), v.clone()))
            }
        }
    }
}

/// A tuple-at-a-time filter iterator
pub struct Filter<'a> {
    input: Box<Iter<'a>>,
    program: ProgramEvaluator<'a>,
}

impl<'a> Iterator for Filter<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.input
            .find(|tuple| self.program.eval(tuple).as_bool().is_some_and(|ok| *ok))
    }
}

/// An opcode for a stack-based expression evaluator
#[derive(Clone, Copy)]
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
    /// ||
    Concat(u16),
    /// r.0 : [Row::Ptr]
    PtrProj(u16),
    /// r.0 : [Row::Ref]
    RefProj(u16),
    /// r.0.1 : [Row::Ptr]
    TupPtrProj(u16),
    /// r.0.1 : [Row::Ref]
    TupRefProj(u16),
}

static_assert_size!(OpCode, 4);

/// A program for evaluating a scalar expression
pub struct ExprProgram<'a> {
    /// The instructions or opcodes
    ops: &'a [OpCode],
    /// The constants in the original expression
    constants: &'a [AlgebraicValue],
}

impl<'a> ExprProgram<'a> {
    fn new(ops: &'a [OpCode], constants: &'a [AlgebraicValue]) -> Self {
        Self { ops, constants }
    }

    /// Returns an interator over the opcodes
    fn ops(&self) -> impl Iterator<Item = OpCode> + '_ {
        self.ops.iter().copied()
    }

    /// Lookup a constant in the plan
    fn constant(&self, i: u16) -> AlgebraicValue {
        self.constants[i as usize].clone()
    }
}

/// An evaluator for an [ExprProgram]
pub struct ProgramEvaluator<'a> {
    program: ExprProgram<'a>,
    stack: Vec<AlgebraicValue>,
}

impl<'a> From<ExprProgram<'a>> for ProgramEvaluator<'a> {
    fn from(program: ExprProgram<'a>) -> Self {
        Self { program, stack: vec![] }
    }
}

impl ProgramEvaluator<'_> {
    pub fn eval(&mut self, tuple: &Tuple) -> AlgebraicValue {
        for op in self.program.ops() {
            match op {
                OpCode::Const(i) => {
                    self.stack.push(self.program.constant(i));
                }
                OpCode::Eq => {
                    let r = self.stack.pop().unwrap();
                    let l = self.stack.pop().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l == r));
                }
                OpCode::Ne => {
                    let r = self.stack.pop().unwrap();
                    let l = self.stack.pop().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l != r));
                }
                OpCode::Lt => {
                    let r = self.stack.pop().unwrap();
                    let l = self.stack.pop().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l < r));
                }
                OpCode::Gt => {
                    let r = self.stack.pop().unwrap();
                    let l = self.stack.pop().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l > r));
                }
                OpCode::Lte => {
                    let r = self.stack.pop().unwrap();
                    let l = self.stack.pop().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l <= r));
                }
                OpCode::Gte => {
                    let r = self.stack.pop().unwrap();
                    let l = self.stack.pop().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l >= r));
                }
                OpCode::And => {
                    let r = *self.stack.pop().unwrap().as_bool().unwrap();
                    let l = *self.stack.pop().unwrap().as_bool().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l && r));
                }
                OpCode::Or => {
                    let r = *self.stack.pop().unwrap().as_bool().unwrap();
                    let l = *self.stack.pop().unwrap().as_bool().unwrap();
                    self.stack.push(AlgebraicValue::Bool(l || r));
                }
                OpCode::Concat(n) => {
                    let mut elems = Vec::with_capacity(n as usize);
                    // Pop args off stack
                    for _ in 0..n {
                        elems.push(self.stack.pop().unwrap());
                    }
                    // Concat and push on stack
                    self.stack.push(AlgebraicValue::Product(ProductValue::from_iter(
                        elems.into_iter().rev(),
                    )));
                }
                OpCode::PtrProj(i) => {
                    self.stack.push(
                        tuple
                            // Read field from row ref
                            .expect_row()
                            .expect_ptr()
                            .read_col(i as usize)
                            .unwrap(),
                    );
                }
                OpCode::RefProj(i) => {
                    self.stack.push(
                        tuple
                            // Read field from product ref
                            .expect_row()
                            .expect_ref()
                            .elements[i as usize]
                            .clone(),
                    );
                }
                OpCode::TupPtrProj(i) => {
                    let idx = *self
                        // Pop index off stack
                        .stack
                        .pop()
                        .unwrap()
                        .as_u16()
                        .unwrap();
                    self.stack.push(
                        tuple
                            // Read field from row ref
                            .expect_join()[idx as usize]
                            .expect_ptr()
                            .read_col(i as usize)
                            .unwrap(),
                    );
                }
                OpCode::TupRefProj(i) => {
                    let idx = *self
                        // Pop index off stack
                        .stack
                        .pop()
                        .unwrap()
                        .as_u16()
                        .unwrap();
                    self.stack.push(
                        tuple
                            // Read field from product ref
                            .expect_join()[idx as usize]
                            .expect_ptr()
                            .read_col(i as usize)
                            .unwrap(),
                    );
                }
            }
        }
        self.stack.pop().unwrap()
    }
}
