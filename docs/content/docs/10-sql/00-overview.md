---
title: SQL Reference
---

SpacetimeDB supports a subset of SQL as a query language. Developers can evaluate SQL queries against a Spacetime database via the `spacetime sql` command-line tool and the [`/database/sql/:name_or_address POST` HTTP endpoint](/docs/http/database#database-sql-name-or-address-post). Client developers also write SQL queries when subscribing to events in the [WebSocket API](/docs/ws/overview#messages-client-to-server-subscribe) or via an SDK `subscribe` function.

SpacetimeDB aims to support much of the [SQL 2016 standard](https://www.iso.org/standard/63555.html), and in particular aims to be compatible with [PostgreSQL](https://www.postgresql.org/).

SpacetimeDB 0.6 implements a relatively small subset of SQL. Future SpacetimeDB versions will implement additional SQL features.

## Types

| Type                                                                   | Description                            |
| ---------------------------------------------------------------------- | -------------------------------------- |
| [Nullable types](#data-types-nullable-types)                           | Types which may not hold a value.      |
| [Logic types](#data-types-logic-types)                                 | Booleans, i.e. `true` and `false`.     |
| [Integer types](#data-types-numeric-types-integer-types)               | Numbers without fractional components. |
| [Floating-point types](#data-types-numeric-types-floating-point-types) | Numbers with fractional components.    |
| [Text types](#data-types-text-types)                                   | UTF-8 encoded text.                    |

### Definition statements

| Statement                            | Description                          |
| ------------------------------------ | ------------------------------------ |
| [CREATE TABLE](#syntax-create-table) | Create a new table.                  |
| [DROP TABLE](#syntax-drop-table)     | Remove a table, discarding all rows. |

### Query statements

| Statement                 | Description                                                                                  |
| ------------------------- | -------------------------------------------------------------------------------------------- |
| [FROM](#queries-from)     | A source of data, like a table or a value.                                                   |
| [JOIN](#queries-join)     | Combine several data sources.                                                                |
| [SELECT](#queries-select) | Select specific rows and columns from a data source, and optionally compute a derived value. |
| [DELETE](#queries-delete) | Delete specific rows from a table.                                                           |
| [INSERT](#queries-insert) | Insert rows into a table.                                                                    |
| [UPDATE](#queries-update) | Update specific rows in a table.                                                             |

## Data types

SpacetimeDB is built on the Spacetime Algebraic Type System, or SATS. SATS is a richer, more expressive type system than the one included in the SQL language.

Because SATS is a richer type system than SQL, some SATS types cannot cleanly correspond to SQL types. In particular, the SpacetimeDB SQL interface is unable to construct or compare instances of product and sum types. As such, SpacetimeDB SQL must largely restrict themselves to interacting with columns of builtin types.

Most SATS builtin types map cleanly to SQL types.

### Nullable types

SpacetimeDB types, by default, do not permit `NULL` as a value. Nullable types are encoded in SATS using a sum type which corresponds to [Rust's `Option`](https://doc.rust-lang.org/stable/std/option/enum.Option.html). In SQL, such types can be written by adding the constraint `NULL`, like `INT NULL`.

### Logic types

| SQL       | SATS   | Example         |
| --------- | ------ | --------------- |
| `BOOLEAN` | `Bool` | `true`, `false` |

### Numeric types

#### Integer types

An integer is a number without a fractional component.

Adding the `UNSIGNED` constraint to an integer type allows only positive values. This allows representing a larger positive range without increasing the width of the integer.

| SQL                 | SATS  | Example | Min    | Max   |
| ------------------- | ----- | ------- | ------ | ----- |
| `TINYINT`           | `I8`  | 1       | -(2⁷)  | 2⁷-1  |
| `TINYINT UNSIGNED`  | `U8`  | 1       | 0      | 2⁸-1  |
| `SMALLINT`          | `I16` | 1       | -(2¹⁵) | 2¹⁵-1 |
| `SMALLINT UNSIGNED` | `U16` | 1       | 0      | 2¹⁶-1 |
| `INT`, `INTEGER`    | `I32` | 1       | -(2³¹) | 2³¹-1 |
| `INT UNSIGNED`      | `U32` | 1       | 0      | 2³²-1 |
| `BIGINT`            | `I64` | 1       | -(2⁶³) | 2⁶³-1 |
| `BIGINT UNSIGNED`   | `U64` | 1       | 0      | 2⁶⁴-1 |

#### Floating-point types

SpacetimeDB supports single- and double-precision [binary IEEE-754 floats](https://en.wikipedia.org/wiki/IEEE_754).

| SQL               | SATS  | Example | Min                      | Max                     |
| ----------------- | ----- | ------- | ------------------------ | ----------------------- |
| `REAL`            | `F32` | 1.0     | -3.40282347E+38          | 3.40282347E+38          |
| `DOUBLE`, `FLOAT` | `F64` | 1.0     | -1.7976931348623157E+308 | 1.7976931348623157E+308 |

### Text types

SpacetimeDB supports a single string type, `String`. SpacetimeDB strings are UTF-8 encoded.

| SQL                                             | SATS     | Example | Notes                |
| ----------------------------------------------- | -------- | ------- | -------------------- |
| `CHAR`, `VARCHAR`, `NVARCHAR`, `TEXT`, `STRING` | `String` | 'hello' | Always UTF-8 encoded |

> SpacetimeDB SQL currently does not support length contraints like `CHAR(10)`.

## Syntax

### Comments

SQL line comments begin with `--`.

```sql
-- This is a comment
```

### Expressions

We can express different, composable, values that are universally called `expressions`.

An expression is one of the following:

#### Literals

| Example   | Description |
| --------- | ----------- |
| `1`       | An integer. |
| `1.0`     | A float.    |
| `'hello'` | A string.   |
| `true`    | A boolean.  |

#### Binary operators

| Example | Description         |
| ------- | ------------------- |
| `1 > 2` | Integer comparison. |
| `1 + 2` | Integer addition.   |

#### Logical expressions

Any expression which returns a boolean, i.e. `true` or `false`, is a logical expression.

| Example          | Description                                                  |
| ---------------- | ------------------------------------------------------------ |
| `1 > 2`          | Integer comparison.                                          |
| `1 + 2 == 3`     | Equality comparison between a constant and a computed value. |
| `true AND false` | Boolean and.                                                 |
| `true OR false`  | Boolean or.                                                  |
| `NOT true`       | Boolean inverse.                                             |

#### Function calls

| Example         | Description                                        |
| --------------- | -------------------------------------------------- |
| `lower('JOHN')` | Apply the function `lower` to the string `'JOHN'`. |

#### Table identifiers

| Example       | Description               |
| ------------- | ------------------------- |
| `inventory`   | Refers to a table.        |
| `"inventory"` | Refers to the same table. |

#### Column references

| Example                    | Description                                             |
| -------------------------- | ------------------------------------------------------- |
| `inventory_id`             | Refers to a column.                                     |
| `"inventory_id"`           | Refers to the same column.                              |
| `"inventory.inventory_id"` | Refers to the same column, explicitly naming its table. |

#### Wildcards

Special "star" expressions which select all the columns of a table.

| Example       | Description                                             |
| ------------- | ------------------------------------------------------- |
| `*`           | Refers to all columns of a table identified by context. |
| `inventory.*` | Refers to all columns of the `inventory` table.         |

#### Parenthesized expressions

Sub-expressions can be enclosed in parentheses for grouping and to override operator precedence.

| Example       | Description             |
| ------------- | ----------------------- |
| `1 + (2 / 3)` | One plus a fraction.    |
| `(1 + 2) / 3` | A sum divided by three. |

### `CREATE TABLE`

A `CREATE TABLE` statement creates a new, initially empty table in the database.

The syntax of the `CREATE TABLE` statement is:

> **CREATE TABLE** _table_name_ (_column_name_ _data_type_, ...);

![create-table](/images/syntax/create_table.svg)

#### Examples

Create a table `inventory` with two columns, an integer `inventory_id` and a string `name`:

```sql
CREATE TABLE inventory (inventory_id INTEGER, name TEXT);
```

Create a table `player` with two integer columns, an `entity_id` and an `inventory_id`:

```sql
CREATE TABLE player (entity_id INTEGER, inventory_id INTEGER);
```

Create a table `location` with three columns, an integer `entity_id` and floats `x` and `z`:

```sql
CREATE TABLE location (entity_id INTEGER, x REAL, z REAL);
```

### `DROP TABLE`

A `DROP TABLE` statement removes a table from the database, deleting all its associated rows, indexes, constraints and sequences.

To empty a table of rows without destroying the table, use [`DELETE`](#queries-delete).

The syntax of the `DROP TABLE` statement is:

> **DROP TABLE** _table_name_;

![drop-table](/images/syntax/drop_table.svg)

Examples:

```sql
DROP TABLE inventory;
```

## Queries

### `FROM`

A `FROM` clause derives a data source from a table name.

The syntax of the `FROM` clause is:

> **FROM** _table_name_ _join_clause_?;

![from](/images/syntax/from.svg)

#### Examples

Select all rows from the `inventory` table:

```sql
SELECT * FROM inventory;
```

### `JOIN`

A `JOIN` clause combines two data sources into a new data source.

Currently, SpacetimeDB SQL supports only inner joins, which return rows from two data sources where the values of two columns match.

The syntax of the `JOIN` clause is:

> **JOIN** _table_name_ **ON** _expr_ = _expr_;

![join](/images/syntax/join.svg)

### Examples

Select all players rows who have a corresponding location:

```sql
SELECT player.* FROM player
 JOIN location
 ON location.entity_id = player.entity_id;
```

Select all inventories which have a corresponding player, and where that player has a corresponding location:

```sql
SELECT inventory.* FROM inventory
 JOIN player
 ON inventory.inventory_id = player.inventory_id
 JOIN location
 ON player.entity_id = location.entity_id;
```

### `SELECT`

A `SELECT` statement returns values of particular columns from a data source, optionally filtering the data source to include only rows which satisfy a `WHERE` predicate.

The syntax of the `SELECT` command is:

> **SELECT** _column_expr_ > **FROM** _from_expr_
> {**WHERE** _expr_}?

![sql-select](/images/syntax/select.svg)

#### Examples

Select all columns of all rows from the `inventory` table:

```sql
SELECT * FROM inventory;
SELECT inventory.* FROM inventory;
```

Select only the `inventory_id` column of all rows from the `inventory` table:

```sql
SELECT inventory_id FROM inventory;
SELECT inventory.inventory_id FROM inventory;
```

An optional `WHERE` clause can be added to filter the data source using a [logical expression](#syntax-expressions-logical-expressions). The `SELECT` will return only the rows from the data source for which the expression returns `true`.

#### Examples

Select all columns of all rows from the `inventory` table, with a filter that is always true:

```sql
SELECT * FROM inventory WHERE 1 = 1;
```

Select all columns of all rows from the `inventory` table with the `inventory_id` 1:

```sql
SELECT * FROM inventory WHERE inventory_id = 1;
```

Select only the `name` column of all rows from the `inventory` table with the `inventory_id` 1:

```sql
SELECT name FROM inventory WHERE inventory_id = 1;
```

Select all columns of all rows from the `inventory` table where the `inventory_id` is 2 or greater:

```sql
SELECT * FROM inventory WHERE inventory_id > 1;
```

### `INSERT`

An `INSERT INTO` statement inserts new rows into a table.

One can insert one or more rows specified by value expressions.

The syntax of the `INSERT INTO` statement is:

> **INSERT INTO** _table_name_ (_column_name_, ...) **VALUES** (_expr_, ...), ...;

![sql-insert](/images/syntax/insert.svg)

#### Examples

Insert a single row:

```sql
INSERT INTO inventory (inventory_id, name) VALUES (1, 'health1');
```

Insert two rows:

```sql
INSERT INTO inventory (inventory_id, name) VALUES (1, 'health1'), (2, 'health2');
```

### UPDATE

An `UPDATE` statement changes the values of a set of specified columns in all rows of a table, optionally filtering the table to update only rows which satisfy a `WHERE` predicate.

Columns not explicitly modified with the `SET` clause retain their previous values.

If the `WHERE` clause is absent, the effect is to update all rows in the table.

The syntax of the `UPDATE` statement is

> **UPDATE** _table_name_ **SET** > _column_name_ = _expr_, ...
> {_WHERE expr_}?;

![sql-update](/images/syntax/update.svg)

#### Examples

Set the `name` column of all rows from the `inventory` table with the `inventory_id` 1 to `'new name'`:

```sql
UPDATE inventory
  SET name = 'new name'
  WHERE inventory_id = 1;
```

### DELETE

A `DELETE` statement deletes rows that satisfy the `WHERE` clause from the specified table.

If the `WHERE` clause is absent, the effect is to delete all rows in the table. In that case, the result is a valid empty table.

The syntax of the `DELETE` statement is

> **DELETE** _table_name_
> {**WHERE** _expr_}?;

![sql-delete](/images/syntax/delete.svg)

#### Examples

Delete all the rows from the `inventory` table with the `inventory_id` 1:

```sql
DELETE FROM inventory WHERE inventory_id = 1;
```

Delete all rows from the `inventory` table, leaving it empty:

```sql
DELETE FROM inventory;
```
