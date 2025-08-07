/// A row-level security filter,
/// which can be registered using the [`macro@crate::client_visibility_filter`] attribute.
#[non_exhaustive]
pub enum Filter {
    /// A SQL query. Rows that match this query will be made visible to clients.
    ///
    /// The query must be of the form `SELECT * FROM table` or `SELECT table.* from table`,
    /// followed by any number of `JOIN` clauses and a `WHERE` clause.
    /// If the query includes any `JOIN`s, it must be in the form `SELECT table.* FROM table`.
    /// In any case, the query must select all of the columns from a single table, and nothing else.
    ///
    /// SQL queries are not checked for syntactic or semantic validity
    /// until they are processed by the SpacetimeDB host.
    /// This means that errors in queries used as [`macro@crate::client_visibility_filter`] rules
    /// will be reported during `spacetime publish`, not at compile time.
    Sql(&'static str),
}

impl Filter {
    #[doc(hidden)]
    pub fn sql_text(&self) -> &'static str {
        let Filter::Sql(sql) = self;
        sql
    }
}
