#pragma once

#include <string>

namespace SpacetimeDB {

/// A row-level security filter,
/// which can be registered using the SPACETIMEDB_CLIENT_VISIBILITY_FILTER macro.
///
/// Currently, the only valid value for a filter is a Filter::Sql.
/// This is a filter written as a SQL query. Rows that match this query will be made visible to clients.
///
/// The query must be of the form `SELECT * FROM table` or `SELECT table.* from table`,
/// followed by any number of `JOIN` clauses and a `WHERE` clause.
/// If the query includes any `JOIN`s, it must be in the form `SELECT table.* FROM table`.
/// In any case, the query must select all of the columns from a single table, and nothing else.
///
/// SQL queries are not checked for syntactic or semantic validity
/// until they are processed by the SpacetimeDB host.
/// This means that errors in queries used as SPACETIMEDB_CLIENT_VISIBILITY_FILTER rules
/// will be reported during `spacetime publish`, not at compile time.
class Filter {
private:
    const char* sql_text_;

public:
    /// Create a SQL-based client visibility filter
    static Filter Sql(const char* sql) {
        return Filter(sql);
    }

    /// Get the SQL text for this filter
    const char* sql_text() const {
        return sql_text_;
    }

private:
    explicit Filter(const char* sql) : sql_text_(sql) {}
};

} // namespace SpacetimeDB