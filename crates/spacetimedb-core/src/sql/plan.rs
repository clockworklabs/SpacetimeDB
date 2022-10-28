use crate::error::DBError;
use thiserror::Error;

// TODO(cloutiertyler): we could do this the swift parsing way in which
// we always generate a plan, but it may contain errors
#[derive(Error, Debug)]
pub enum PlanError {
    #[error("Unsupported feature: `{feature}` issue: `#{issue_no:?}`")]
    Unsupported { feature: String, issue_no: Option<usize> },
    #[error("Unknown table: `{table}`")]
    UnknownTable { table: String },
    #[error("Unknown column: `{table:?}.{column}`")]
    UnknownColumn { table: Option<String>, column: String },
    #[error("Subqueries disallowed: `{context}`")]
    _SubqueriesDisallowed { context: String },
    #[error("Unknown parameter: `{0}`")]
    _UnknownParameter(usize),
    #[error("Parsing error: `{0}`")]
    _Parser(String),
    #[error("Plan error: `{0}`")]
    Unstructured(String),
    #[error("Internal DBError: `{0}`")]
    DatabaseInternal(DBError),
}

#[derive(Debug)]
pub enum Plan {
    Query(QueryPlan),
}

#[derive(Debug)]
pub struct QueryPlan {
    pub source: RelationExpr,
}

#[derive(Debug)]
pub enum RelationExpr {
    GetTable {
        table_id: u32,
    },
    Project {
        input: Box<RelationExpr>,
        col_ids: Vec<u32>,
    },
}
