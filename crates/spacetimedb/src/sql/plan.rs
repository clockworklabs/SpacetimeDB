// TODO(cloutiertyler): we could do this the swift parsing way in which
// we always generate a plan, but it may contain errors
#[derive(Clone, Debug)]
pub enum PlanError {
    Unsupported { feature: String, issue_no: Option<usize> },
    UnknownTable { table: String },
    UnknownColumn { table: Option<String>, column: String },
    _SubqueriesDisallowed { context: String },
    _UnknownParameter(usize),
    _Parser(String),
    _Unstructured(String),
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
