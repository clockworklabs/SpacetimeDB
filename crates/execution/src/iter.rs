use std::borrow::Cow;

use spacetimedb_lib::{AlgebraicValue, ProductValue};
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
    /// A non-unique (constraint) index join iterator
    IxJoin(IxJoin<IndexJoin<'a, Project>, IndexJoin<'a, Concat<'a>>>),
    /// A unique (constraint) index join iterator
    UniqueIxJoin(IxJoin<UniqueIndexJoin<'a, Project>, UniqueIndexJoin<'a, Concat<'a>>>),
    /// A tuple at a time filter iterator
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
            Self::CrossJoin(iter) => {
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
                            // Returns (n+1)-tuples,
                            // if the rhs returns n-tuples.
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
                            // Returns (n+1)-tuples,
                            // if the lhs returns n-tuples.
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
                            // Returns (n+m)-tuples,
                            // if the lhs returns n-tuples,
                            // if the rhs returns m-tuples.
                            lhs.append(&mut rhs);
                            Tuple::Join(lhs)
                        }
                    }
                })
            }
        }
    }
}

/// An iterator for a unique (constraint) index join
pub enum IxJoin<SingleCol, MultiCol> {
    /// A single column left semijoin.
    /// Returns tuples from the lhs.
    SemiLhs(SingleCol),
    /// A single column right semijoin.
    /// Returns rows from the index side.
    SemiRhs(SingleCol),
    /// A multi-column left semijoin.
    /// Returns tuples from the lhs.
    MultiColSemiLhs(MultiCol),
    /// A multi-column right semijoin.
    /// Returns rows from the index side.
    MultiColSemiRhs(MultiCol),
    /// A multi-column index join.
    /// If the lhs returns n-tuples,
    /// this returns (n+1)-tuples.
    MultiCol(MultiCol),
    /// A single column index join.
    /// If the lhs returns n-tuples,
    /// this returns (n+1)-tuples.
    Eq(SingleCol),
}

impl<'a, P, Q> Iterator for IxJoin<P, Q>
where
    P: Iterator<Item = (Tuple<'a>, RowRef<'a>)>,
    Q: Iterator<Item = (Tuple<'a>, RowRef<'a>)>,
{
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let proj_left_deep_join = |(tuple, ptr)| {
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
                    // Returns an n+1 tuple
                    rows.push(Row::Ptr(ptr));
                    Tuple::Join(rows)
                }
            }
        };
        match self {
            Self::SemiLhs(iter) => {
                // A left semijoin
                iter.next().map(|(t, _)| t)
            }
            Self::SemiRhs(iter) => {
                // A right semijoin
                iter.next().map(|(_, ptr)| ptr).map(Row::Ptr).map(Tuple::Row)
            }
            Self::MultiColSemiLhs(iter) => {
                // A left semijoin
                iter.next().map(|(t, _)| t)
            }
            Self::MultiColSemiRhs(iter) => {
                // A right semijoin
                iter.next().map(|(_, ptr)| ptr).map(Row::Ptr).map(Tuple::Row)
            }
            Self::MultiCol(iter) => {
                // Appends the rhs to the lhs
                iter.next().map(proj_left_deep_join)
            }
            Self::Eq(iter) => {
                // Appends the rhs to the lhs
                iter.next().map(proj_left_deep_join)
            }
        }
    }
}

pub trait FieldProject {
    fn eval<'a>(&self, tuple: &'a Tuple) -> Cow<'a, AlgebraicValue>;
}

/// A unique (constraint) index join iterator
pub struct UniqueIndexJoin<'a, FieldProject> {
    /// The lhs of the join
    input: Box<Iter<'a>>,
    /// The rhs index
    index: &'a BTreeIndex,
    /// A handle to the datastore
    table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs index key projection
    projection: FieldProject,
}

impl<'a, P> Iterator for UniqueIndexJoin<'a, P>
where
    P: FieldProject,
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

/// A non-unique (constraint) index join iterator
pub struct IndexJoin<'a, FieldProject> {
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
    projection: FieldProject,
}

impl<'a, P> Iterator for IndexJoin<'a, P>
where
    P: FieldProject,
{
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
                    Some(self.index.seek(self.projection.eval(&tuple).as_ref())).and_then(|mut cursor| {
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

/// A single field/column projection evaluator
pub struct Project {
    op: ProjOpCode,
}

impl FieldProject for Project {
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

/// A multi-column projection evaluator.
/// It concatenates a sequence of field projections.
pub struct Concat<'a> {
    ops: &'a [ProjOpCode],
}

impl FieldProject for Concat<'_> {
    fn eval<'a>(&self, tuple: &'a Tuple) -> Cow<'a, AlgebraicValue> {
        Cow::Owned(AlgebraicValue::Product(ProductValue::from_iter(
            self.ops
                .iter()
                .copied()
                .map(|op| Project { op })
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
