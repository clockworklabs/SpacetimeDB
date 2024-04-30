//! Abstract Virtual Machine for execution of end-user logic
//!
//! It optimizes the code & include a more general "query planner"
//!
//! The execution is split in 3 "phases":
//!
//! 1- AST formation
//!
//! Generate the AST (that could be invalid according to the semantics).
//!
//! This step is outside the [vm] and can be done, for example, by the SQL layer.
//!
//! Use [dsl] to build the [expr:Expr] that build the AST.
//!
//! 2- AST validation
//!
//! Calling [eval::optimize] verify the code has the correct semantics (ie: It checks types, schemas, functions are valid, etc.),
//! and "desugar" the code in a more optimal form for later execution.
//!
//! This build [expr::Expr] that is what could be stored in the database, ie: Is like bytecode.
//!
//! 3-  Execution
//!
//! Run the AST build from [expr::Expr]. It assumes is correct.
//!

pub use spacetimedb_lib::operator;

pub mod errors;
pub mod eval;
pub mod expr;
pub mod iterators;
pub mod ops;
pub mod program;
pub mod rel_ops;
pub mod relation;
