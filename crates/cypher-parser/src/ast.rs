/// A complete openCypher query: `MATCH pattern [WHERE expr] RETURN projection`.
#[derive(Debug, Clone, PartialEq)]
pub struct CypherQuery {
    pub match_clause: MatchClause,
    pub where_clause: Option<CypherExpr>,
    pub return_clause: ReturnClause,
}

/// `MATCH pattern {, pattern}`.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchClause {
    pub patterns: Vec<Pattern>,
}

/// A graph pattern: a chain of alternating nodes and relationships.
///
/// Invariant: `nodes.len() == edges.len() + 1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub nodes: Vec<NodePattern>,
    pub edges: Vec<RelPattern>,
}

/// `(variable:Label {key: value, …})` — all parts optional.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub label: Option<String>,
    pub properties: Vec<(String, CypherLiteral)>,
}

/// `-[variable:TYPE *range]->` / `<-[…]-` / `-[…]-`.
#[derive(Debug, Clone, PartialEq)]
pub struct RelPattern {
    pub variable: Option<String>,
    pub rel_type: Option<String>,
    pub length: Option<PathLength>,
    pub direction: Direction,
}

/// Relationship direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Direction {
    /// `-->` or `-[…]->`
    Outgoing,
    /// `<--` or `<-[…]-`
    Incoming,
    /// `--` or `-[…]-`
    Undirected,
}

/// Variable-length path range in `[*min..max]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathLength {
    /// `[*]` — unbounded
    Unbounded,
    /// `[*n]` — exactly n hops
    Exact(u32),
    /// `[*min..max]`
    Range { min: Option<u32>, max: Option<u32> },
}

/// `RETURN * | RETURN expr [AS alias] {, …}`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ReturnClause {
    /// `RETURN *`
    All,
    /// `RETURN item {, item}`
    Items(Vec<ReturnItem>),
}

/// A single item in a RETURN clause.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnItem {
    pub expr: CypherExpr,
    pub alias: Option<String>,
}

/// Scalar expression used in WHERE and RETURN clauses.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CypherExpr {
    /// A literal constant.
    Lit(CypherLiteral),
    /// An unqualified variable reference.
    Var(String),
    /// Property access: `expr.prop`.
    Prop(Box<CypherExpr>, String),
    /// Binary comparison: `expr op expr`.
    Cmp(Box<CypherExpr>, CmpOp, Box<CypherExpr>),
    /// Boolean AND.
    And(Box<CypherExpr>, Box<CypherExpr>),
    /// Boolean OR.
    Or(Box<CypherExpr>, Box<CypherExpr>),
    /// Boolean NOT.
    Not(Box<CypherExpr>),
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Lte,
    Gte,
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eq => write!(f, "="),
            Self::Ne => write!(f, "<>"),
            Self::Lt => write!(f, "<"),
            Self::Gt => write!(f, ">"),
            Self::Lte => write!(f, "<="),
            Self::Gte => write!(f, ">="),
        }
    }
}

/// A literal constant in Cypher.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CypherLiteral {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}
