namespace SpacetimeDB;

/// <summary>
/// A row-level security filter,
/// which can be registered using the <c>[SpacetimeDB.ClientVisibilityFilter]</c> attribute.
///
/// Currently, the only valid value for a filter is a <c>Filter.Sql</c>.
/// This is a filter written as a SQL query. Rows that match this query will be made visible to clients.
///
/// The query must be of the form `SELECT * FROM table` or `SELECT table.* from table`,
/// followed by any number of `JOIN` clauses and a `WHERE` clause.
/// If the query includes any `JOIN`s, it must be in the form `SELECT table.* FROM table`.
/// In any case, the query must select all of the columns from a single table, and nothing else.
///
/// SQL queries are not checked for syntactic or semantic validity
/// until they are processed by the SpacetimeDB host.
/// This means that errors in queries used as <c>[SpacetimeDB.ClientVisibilityFilter]</c> rules
/// will be reported during <c>spacetime publish</c>, not at compile time.
/// </summary>
// Note: _Unused is needed because C# doesn't support single-element named tuples :/
[Type]
public partial record Filter : TaggedEnum<(string Sql, Unit _Unused)> { }
