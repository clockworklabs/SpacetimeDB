# SQL Support

SpacetimeDB supports two subsets of SQL:
One for queries issued through the [cli] or [http] api.
Another for subscriptions issued via the [sdk] or WebSocket api.

## Subscriptions

```ebnf
SELECT projection FROM relation [ WHERE predicate ]
```

The subscription language is strictly a query language.
Its sole purpose is to replicate a subset of the rows in the database,
and to **automatically** update them in realtime as the database changes.

There is no context for manually updating this view.
Hence data manipulation commands like `INSERT` and `DELETE` are not supported.

> NOTE: Because subscriptions are evaluated in realtime,
> performance is critical, and as a result,
> additional restrictions are applied over ad hoc queries.
> These restrictions are highlighted below.

### SELECT

```ebnf
SELECT ( '*' | table '.' '*' )
```

The `SELECT` clause determines the table that is being subscribed to.
Since the subscription api is purely a replication api,
a query may only return rows from a single table,
and it must return the entire row.
Individual column projections are not allowed.

A `*` projection is allowed when the table is unambiguous,
otherwise it must be qualified with the appropriate table name.

#### Examples

```sql
-- Subscribe to all rows of a table
SELECT * FROM Inventory

-- Qualify the `*` projection with the table
SELECT item.* from Inventory item

-- Subscribe to all customers who have orders totaling more than $1000
SELECT customer.*
FROM Customers customer JOIN Orders o ON customer.id = o.customer_id
WHERE o.amount > 1000

-- INVALID: Must return `Customers` or `Orders`, but not both
SELECT *
FROM Customers customer JOIN Orders o ON customer.id = o.customer_id
WHERE o.amount > 1000
```

### FROM

```ebnf
FROM table [ [AS] alias ] [ [INNER] JOIN table [ [AS] alias ] ON column '=' column ]
```

While you can only subscribe to rows from a single table,
you may reference two tables in the `FROM` clause using a `JOIN`.
A `JOIN` selects all combinations of rows from its input tables,
and `ON` determines which combinations are considered.

Subscriptions do not support joins of more than two tables.

For any column referenced in `ON` clause of a `JOIN`,
it must be qualified with the appropriate table name or alias.

In order for a `JOIN` to be evaluated efficiently,
subscriptions require an index to be defined on both join columns.

#### Example

```sql
-- Subscribe to all orders of products with less than 10 items in stock.
-- Must have an index on the `product_id` column of the `Orders` table,
-- as well as the `id` column of the `Product` table.
SELECT o.*
FROM Orders o JOIN Inventory product ON o.product_id = product.id
WHERE product.quantity < 10

-- Subscribe to all products that have at least one purchase
SELECT product.*
FROM Orders o JOIN Inventory product ON o.product_id = product.id

-- INVALID: Must qualify the column names referenced in `ON`
SELECT product.* FROM Orders JOIN Inventory product ON product_id = id
```

### WHERE

```ebnf
predicate
    = expr
    | predicate AND predicate
    | predicate OR  predicate
    ;

expr
    = literal
    | column
    | expr op expr
    ;

op
    = '='
    | '<'
    | '>'
    | '<' '='
    | '>' '='
    | '!' '='
    | '<' '>'
    ;

literal
    = INTEGER
    | STRING
    | HEX
    | TRUE
    | FALSE
    ;
```

While the `SELECT` clause determines the table,
the `WHERE` clause determines the rows in the subscription.

Arithmetic expressions are not supported.

#### Examples

```sql
-- Find products that sell for more than $X
SELECT * FROM Inventory WHERE price > {X}

-- Find products that sell for more than $X and have fewer than Y items in stock
SELECT * FROM Inventory WHERE price > {X} AND amount < {Y}
```

## Query and DML (Data Manipulation Language)

### Statements

- [SELECT](#select-1)
- [INSERT](#insert)
- [DELETE](#delete)
- [UPDATE](#update)
- [SET](#set)
- [SHOW](#show)

### SELECT

```ebnf
SELECT projection FROM relation [ WHERE predicate ]
```

The query languge is a strict superset of the subscription language.
The main differences are seen in column projections and [joins](#from-clause).

The subscription api only supports `*` projections,
but the query api supports individual column projections.

The subscription api limits the number of tables you can join,
and enforces index constraints on the join columns,
but the query language has no such constraints or limitations.

#### SELECT Clause

```ebnf
projection
    = '*'
    | table '.' '*'
    | projExpr { ',' projExpr }
    ;

projExpr
    = column [ [ AS ] alias ]
    ;
```

The `SELECT` clause determines the columns that are returned.

##### Examples

```sql
-- Select the items in my inventory
SELECT * FROM Inventory;

-- Select the names and prices of the items in my inventory
SELECT item_name, price FROM Inventory
```

#### FROM Clause

```ebnf
FROM table [ [AS] alias ] { [INNER] JOIN table [ [AS] alias ] ON predicate }
```

Unlike [subscriptions](#from), the query api supports joining more than two tables.

##### Examples

```sql
-- Find all customers who ordered a particular product and when they ordered it
SELECT customer.first_name, customer.last_name, o.date
FROM Customers customer
JOIN Orders o ON customer.id = o.customer_id
JOIN Inventory product ON o.product_id = product.id
WHERE product.name = {product_name}
```

#### WHERE Clause

See [Subscriptions](#where).

### INSERT

```ebnf
INSERT INTO table [ '(' column { ',' column } ')' ] VALUES '(' literal { ',' literal } ')'
```

#### Examples

```sql
-- Inserting one row
INSERT INTO Inventory (item_id, item_name) VALUES (1, 'health1');

-- Inserting two rows
INSERT INTO Inventory (item_id, item_name) VALUES (1, 'health1'), (2, 'health2');
```

### DELETE

```ebnf
DELETE FROM table [ WHERE predicate ]
```

Deletes all rows from a table.
If `WHERE` is specified, only the matching rows are deleted.

`DELETE` does not support joins.

#### Examples

```sql
-- Delete all rows
DELETE FROM Inventory;

-- Delete all rows with a specific item_id
DELETE FROM Inventory WHERE item_id = 1;
```

### UPDATE

```ebnf
UPDATE table SET [ '(' assignment { ',' assignment } ')' ] [ WHERE predicate ]
```

Updates column values of existing rows in a table.
The columns are identified by the `assignment` defined as `column '=' literal`.
The column values are updated for all rows that match the `WHERE` condition.
The rows are updated after the `WHERE` condition is evaluated for all rows.

`UPDATE` does not support joins.

#### Examples

```sql
-- Update the item_name for all rows with a specific item_id
UPDATE Inventory SET item_name = 'new name' WHERE item_id = 1;
```

### SET

> WARNING: The `SET` statement is experimental.
> Compatibility with future versions of SpacetimeDB is not guaranteed.

```ebnf
SET var ( TO | '=' ) literal
```

Updates the value of a system variable.

### SHOW

> WARNING: The `SHOW` statement is experimental.
> Compatibility with future versions of SpacetimeDB is not guaranteed.

```ebnf
SHOW var
```

Returns the value of a system variable.

## System Variables

> WARNING: System variables are experimental.
> Compatibility with future versions of SpacetimeDB is not guaranteed.

- `row_limit`

    ```sql
    -- Reject queries that scan more than 10K rows
    SET row_limit = 10000
    ```

## Data types

The set of data types that SpacetimeDB supports is defined by SATS,
the Spacetime Algebraic Type System.

Spacetime SQL however does not support all of SATS,
specifically in the way of product and sum types.
The language itself does not provide a way to construct them,
nore does it provide any scalar operators for them.
Nevertheless rows containing them can be returned to clients.

## Literals

```ebnf
literal = INTEGER | FLOAT | STRING | HEX | TRUE | FALSE ;
```

The following describes how to construct literal values for SATS data types in Spacetime SQL.

### Booleans

Booleans are represented using the canonical atoms `true` or `false`.

### Integers

```ebnf
INTEGER
    = [ '+' | '-' ] NUM
    | [ '+' | '-' ] NUM 'E' [ '+' ] NUM
    ;

NUM
    = DIGIT { DIGIT }
    ;

DIGIT
    = 0..9
    ;
```

SATS supports multple fixed width integer types.
The concrete type of a literal is inferred from the context.

#### Examples

```sql
-- All products that sell for more than $1000
SELECT * FROM Inventory WHERE price > 1000
SELECT * FROM Inventory WHERE price > 1e3
SELECT * FROM Inventory WHERE price > 1E3
```

### Floats

```ebnf
FLOAT
    = [ '+' | '-' ] [ NUM ] '.' NUM
    | [ '+' | '-' ] [ NUM ] '.' NUM 'E' [ '+' | '-' ] NUM
    ;
```

SATS supports both 32 and 64 bit floating point types.
The concrete type of a literal is inferred from the context.

#### Examples

```sql
-- All measurements where the temperature is greater than 105.3
SELECT * FROM Measurements WHERE temperature > 105.3
SELECT * FROM Measurements WHERE temperature > 1053e-1
SELECT * FROM Measurements WHERE temperature > 1053E-1
```

### Strings

```ebnf
STRING
    = "'" { "''" | CHAR } "'"
    ;
```

`CHAR` is defined as a `utf-8` encoded unicode character.

#### Examples

```sql
SELECT * FROM Customers WHERE first_name = 'John'
```

### Hex

```ebnf
HEX
    = 'X' "'" { HEXIT } "'"
    | '0' 'x' { HEXIT }
    ;

HEXIT
    = DIGIT | a..f | A..F
    ;
```

Hex literals can represent [Identity], [Address], or binary types.
The type is ultimately inferred from the context.

#### Examples

```sql
SELECT * FROM Program WHERE hash_value = 0xABCD1234
```

## Identifiers

```ebnf
identifier
    = LATIN { LATIN | DIGIT | '_' }
    | '"' { '""' | CHAR } '"'
    ;

LATIN
    = a..z | A..Z
    ;
```

Identifiers are tokens that identify database objects like tables or columns.
Spacetime SQL supports both quoted and unquoted identifiers.
Both types of identifiers are case sensitive.
Use quoted identifiers to avoid conflict with reserved SQL keywords,
or if your table or column contains non-alphanumeric characters.

### Example

```sql
-- `ORDER` is a sql keyword and therefore needs to be quoted
SELECT * FROM "Order"

-- A table containing `$` needs to be quoted as well
SELECT * FROM "Balance$"
```

## Appendix

Common production rules that have been used throughout this document.

```ebnf
table
    = identifier
    ;

alias
    = identifier
    ;

var
    = identifier
    ;

column
    = identifier
    | identifier '.' identifier
    ;
```


[sdk]:       /docs/sdks/rust/index.md#subscribe-to-queries
[http]:      /docs/http/database#databasesqlname_or_address-post
[cli]:       /docs/cli-reference.md#spacetime-sql

[Identity]: /docs/index.md#identity
[Address]:  /docs/index.md#address
