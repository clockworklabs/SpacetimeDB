use std::sync::Arc;

/// Used to store the source of a SQL query in a way that can be cheaply cloned,
/// without proliferating lifetimes everywhere.
///
/// TODO: if CrudExpr ever gets refactored, this should probably be attached to those.
/// That would be a large refactoring though. It would be nice if we could get
/// more precise spans from sqlparser. We could stick all sorts of other things in here too.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct QueryDebugInfo(Arc<str>);

impl QueryDebugInfo {
    /// Create a new [QueryDebugInfo] from the given source text.
    pub fn from_source<T: AsRef<str>>(source: T) -> Self {
        Self(source.as_ref().into())
    }

    /// Get the source text of the query, if available.
    pub fn source(&self) -> &str {
        &self.0
    }
}
